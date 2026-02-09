use crate::playback::{PlaybackError, PlaybackSink};
use crate::tts::TtsAudio;
use futures::future::BoxFuture;
use futures::FutureExt;
use rodio::cpal::traits::DeviceTrait;
use rodio::cpal::traits::HostTrait;
use rodio::source::Source;
use rodio::{OutputStream, OutputStreamBuilder, Sink, StreamError};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// A minimal, poison-tolerant, lazy initializer for a single value.
///
/// Rationale: [`rodio::OutputStream`] must be kept alive for the duration of playback.
/// Opening a new stream per clip causes Rodio to drop the stream every call, producing
/// `Dropping OutputStream, audio playing through this stream will stop` spam and
/// can truncate/blank playback.
struct LazyInit<T> {
    value: Mutex<Option<T>>,
}

impl<T> LazyInit<T> {
    fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    fn get_or_try_init_with<R, E>(
        &self,
        init: impl FnOnce() -> Result<T, E>,
        f: impl FnOnce(&T) -> R,
        invariant_err: impl FnOnce() -> E,
    ) -> Result<R, E> {
        let mut guard = match self.value.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                tracing::warn!(
                    "playback OutputStream cache lock was poisoned; recovering and continuing"
                );
                poisoned.into_inner()
            }
        };

        // NOTE: `init` is used at most once (only when the cache is empty).
        if guard.is_none() {
            *guard = Some(init()?);
        }

        match guard.as_ref() {
            Some(v) => Ok(f(v)),
            None => Err(invariant_err()),
        }
    }
}

struct RateLimitedWarn {
    interval: Duration,
    last: Mutex<Option<Instant>>,
}

impl RateLimitedWarn {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: Mutex::new(None),
        }
    }

    fn should_log(&self) -> bool {
        let mut guard = match self.last.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        let now = Instant::now();
        match *guard {
            None => {
                *guard = Some(now);
                true
            }
            Some(prev) if now.duration_since(prev) >= self.interval => {
                *guard = Some(now);
                true
            }
            Some(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct AudioPlaybackSink {
    output_device_name: Option<String>,
    disabled: Arc<AtomicBool>,
    disabled_details: Arc<OnceLock<String>>,

    // Keep the OutputStream alive across play calls. Clones share a single stream.
    output_stream: Arc<LazyInit<OutputStream>>,
    output_stream_open_attempts: Arc<AtomicUsize>,

    blank_audio_warn: Arc<RateLimitedWarn>,
}

impl AudioPlaybackSink {
    pub fn new() -> Result<Self, PlaybackError> {
        Ok(Self {
            output_device_name: None,
            disabled: Arc::new(AtomicBool::new(false)),
            disabled_details: Arc::new(OnceLock::new()),

            output_stream: Arc::new(LazyInit::new()),
            output_stream_open_attempts: Arc::new(AtomicUsize::new(0)),
            blank_audio_warn: Arc::new(RateLimitedWarn::new(Duration::from_secs(5))),
        })
    }

    pub fn with_output_device_name<S: Into<String>>(mut self, name: S) -> Self {
        self.output_device_name = Some(name.into());
        self
    }

    fn open_output_stream(&self) -> Result<OutputStream, PlaybackError> {
        let attempt = self
            .output_stream_open_attempts
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        tracing::debug!(
            attempt,
            configured_output_device = %self.output_device_name.as_deref().unwrap_or("<default>"),
            "opening Rodio OutputStream"
        );

        match self.output_device_name.as_deref() {
            Some(wanted) => match open_named_output_stream(wanted) {
                Ok(stream) => Ok(stream),
                Err(NamedDeviceStreamError::DeviceNotFound { wanted, available }) => {
                    tracing::warn!(
                        wanted_device = %wanted,
                        available_devices = %format_device_list(&available),
                        "configured output device not found; falling back to default output device"
                    );
                    OutputStreamBuilder::open_default_stream().map_err(|e| {
                        PlaybackError::AudioOutputUnavailable {
                            details: format_stream_error_details(
                                e,
                                Some(wanted.as_str()),
                                "default-device fallback after named device not found",
                            ),
                        }
                    })
                }
                Err(NamedDeviceStreamError::OpenFailed {
                    wanted,
                    error,
                    available,
                }) => {
                    tracing::warn!(
                        wanted_device = %wanted,
                        error = %error,
                        available_devices = %format_device_list(&available),
                        "failed to open configured output device; falling back to default output device"
                    );
                    OutputStreamBuilder::open_default_stream().map_err(|e| {
                        PlaybackError::AudioOutputUnavailable {
                            details: format_stream_error_details(
                                e,
                                Some(wanted.as_str()),
                                "default-device fallback after named device open failed",
                            ),
                        }
                    })
                }
            },
            None => OutputStreamBuilder::open_default_stream().map_err(|e| {
                PlaybackError::AudioOutputUnavailable {
                    details: format_stream_error_details(e, None, "open default output stream"),
                }
            }),
        }
    }

    fn connect_sink(&self) -> Result<Sink, PlaybackError> {
        self.output_stream.get_or_try_init_with(
            || self.open_output_stream(),
            |stream| {
                let mixer = stream.mixer();
                Sink::connect_new(&mixer)
            },
            || PlaybackError::AudioOutputUnavailable {
                details: "internal error: output stream cache invariant violated".to_owned(),
            },
        )
    }
}

impl PlaybackSink for AudioPlaybackSink {
    fn play(&self, audio: TtsAudio) -> BoxFuture<'_, Result<(), PlaybackError>> {
        async move {
            if self.disabled.load(Ordering::Relaxed) {
                return Ok(());
            }

            // "Blank audio" diagnostics: rate-limited warning to avoid log spam.
            // This helps distinguish output issues from silent/invalid PCM being generated.
            if audio.sample_rate_hz == 0
                || audio.channels == 0
                || audio.pcm_i16.is_empty()
                || (usize::from(audio.channels) != 0
                    && audio.pcm_i16.len() % usize::from(audio.channels) != 0)
            {
                if self.blank_audio_warn.should_log() {
                    tracing::warn!(
                        sample_rate_hz = audio.sample_rate_hz,
                        channels = audio.channels,
                        samples_i16 = audio.pcm_i16.len(),
                        "skipping playback due to empty/invalid PCM (rate-limited)"
                    );
                } else {
                    tracing::debug!(
                        sample_rate_hz = audio.sample_rate_hz,
                        channels = audio.channels,
                        samples_i16 = audio.pcm_i16.len(),
                        "skipping playback due to empty/invalid PCM"
                    );
                }
                return Ok(());
            }

            let sink = match self.connect_sink() {
                Ok(s) => s,
                Err(e) => {
                    if let PlaybackError::AudioOutputUnavailable { details } = &e {
                        if details.contains("NoDevice") {
                            self.disabled.store(true, Ordering::Relaxed);
                            let _ = self.disabled_details.set(details.clone());
                        }
                    }
                    return Err(e);
                }
            };

            let source = PcmSource::new(audio.pcm_i16, audio.sample_rate_hz, audio.channels);

            sink.append(source);
            sink.sleep_until_end();

            Ok(())
        }
        .boxed()
    }
}

#[derive(Debug)]
enum NamedDeviceStreamError {
    DeviceNotFound {
        wanted: String,
        available: Vec<String>,
    },
    OpenFailed {
        wanted: String,
        error: StreamError,
        available: Vec<String>,
    },
}

fn normalize_device_name(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn open_named_output_stream(wanted: &str) -> Result<OutputStream, NamedDeviceStreamError> {
    let wanted_norm = normalize_device_name(wanted);

    let host = rodio::cpal::default_host();
    let devices = host.output_devices().ok();
    let mut available: Vec<String> = Vec::new();
    let mut selected = None;

    if let Some(devices) = devices {
        for d in devices {
            let name = d.name().unwrap_or_else(|_| "<unnamed>".to_owned());
            if normalize_device_name(&name) == wanted_norm {
                selected = Some(d);
            }
            available.push(name.to_owned());
        }
    }

    let Some(device) = selected else {
        return Err(NamedDeviceStreamError::DeviceNotFound {
            wanted: wanted.to_owned(),
            available,
        });
    };

    match OutputStreamBuilder::from_device(device).and_then(|b| b.open_stream_or_fallback()) {
        Ok(stream) => Ok(stream),
        Err(error) => Err(NamedDeviceStreamError::OpenFailed {
            wanted: wanted.to_owned(),
            error,
            available,
        }),
    }
}

fn format_device_list(devices: &[String]) -> String {
    if devices.is_empty() {
        return "<unknown>".to_owned();
    }
    devices.join(", ")
}

fn format_stream_error_details(err: StreamError, wanted: Option<&str>, context: &str) -> String {
    let mut s = format!("{context}: {err}");
    if let Some(w) = wanted {
        s.push_str(&format!(" (configured_device={w})"));
    }
    #[cfg(feature = "playback-device-enum")]
    {
        if let Ok(devices) = enumerate_output_device_names() {
            if devices.is_empty() {
                s.push_str("; available_output_devices=<none>");
            } else {
                s.push_str("; available_output_devices=");
                s.push_str(&devices.join(", "));
            }
        }
    }
    s
}

#[cfg(feature = "playback-device-enum")]
pub fn enumerate_output_device_names() -> Result<Vec<String>, PlaybackError> {
    let host = rodio::cpal::default_host();
    let devices = host
        .output_devices()
        .map_err(|e| PlaybackError::AudioOutputUnavailable {
            details: format!("failed to list output devices: {e}"),
        })?;

    let mut out = Vec::new();
    for d in devices {
        let name = d.name().unwrap_or_else(|_| "<unnamed>".to_owned());
        out.push(name);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_device_name_trims_and_is_case_insensitive() {
        assert_eq!(normalize_device_name("  Speakers  "), "speakers");
        assert_eq!(normalize_device_name("HeAdPhOnEs"), "headphones");
    }

    #[test]
    fn format_device_list_handles_empty() {
        assert_eq!(format_device_list(&[]), "<unknown>");
        assert_eq!(
            format_device_list(&["A".to_owned(), "B".to_owned()]),
            "A, B"
        );
    }

    #[test]
    fn blank_audio_warning_is_rate_limited() {
        let limiter = RateLimitedWarn::new(Duration::from_secs(5));

        // First call should log.
        assert!(limiter.should_log());
        // Immediately after, it should not.
        assert!(!limiter.should_log());
    }

    #[test]
    fn lazy_init_runs_init_only_once() {
        let cell: LazyInit<u32> = LazyInit::new();
        let calls = Arc::new(AtomicUsize::new(0));

        let v1 = cell
            .get_or_try_init_with(
                {
                    let calls = Arc::clone(&calls);
                    move || {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Ok(42)
                    }
                },
                |v| *v,
                || (),
            )
            .unwrap();
        let v2 = cell
            .get_or_try_init_with(
                {
                    let calls = Arc::clone(&calls);
                    move || {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Ok(99)
                    }
                },
                |v| *v,
                || (),
            )
            .unwrap();

        assert_eq!(v1, 42);
        assert_eq!(v2, 42);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}

struct PcmSource {
    samples: std::vec::IntoIter<i16>,
    sample_rate: u32,
    channels: u16,
}

impl PcmSource {
    fn new(samples: Vec<i16>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples: samples.into_iter(),
            sample_rate,
            channels,
        }
    }
}

impl Iterator for PcmSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.samples.next().map(|s| s as f32 / i16::MAX as f32)
    }
}

impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}
