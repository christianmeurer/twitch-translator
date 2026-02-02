use crate::playback::{PlaybackError, PlaybackSink};
use crate::tts::TtsAudio;
use futures::future::BoxFuture;
use futures::FutureExt;
use rodio::source::Source;
use rodio::{OutputStream, Sink};

#[derive(Clone)]
pub struct AudioPlaybackSink;

impl AudioPlaybackSink {
    pub fn new() -> Result<Self, PlaybackError> {
        Ok(Self)
    }
}

impl PlaybackSink for AudioPlaybackSink {
    fn play(&self, audio: TtsAudio) -> BoxFuture<'_, Result<(), PlaybackError>> {
        async move {
            // Create the output stream for each playback
            let (_stream, stream_handle) = OutputStream::try_default()
                .map_err(|_e| PlaybackError::NotImplemented)?; // TODO: Better error handling

            // Create a sink for playback
            let sink = Sink::try_new(&stream_handle)
                .map_err(|_e| PlaybackError::NotImplemented)?; // TODO: Better error handling

            // Convert PCM data to a format that rodio can play
            // For now, we'll create a simple source from the PCM data
            let source = PcmSource::new(
                audio.pcm_i16,
                audio.sample_rate_hz,
                audio.channels,
            );

            // Play the audio
            sink.append(source);
            
            // Wait for the audio to finish playing
            sink.sleep_until_end();

            Ok(())
        }
        .boxed()
    }
}

// A simple PCM audio source for rodio
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
        // Convert i16 to f32
        self.samples.next().map(|s| s as f32 / i16::MAX as f32)
    }
}

impl Source for PcmSource {
    fn current_frame_len(&self) -> Option<usize> {
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