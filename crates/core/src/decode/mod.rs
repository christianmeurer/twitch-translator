use crate::ingest::IngestItem;
use bytes::Bytes;
use ffmpeg_sidecar::{download, paths::ffmpeg_path};
use futures::future::BoxFuture;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PcmSampleType {
    I16,
    F32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PcmFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_type: PcmSampleType,
}

impl PcmFormat {
    pub const fn whisper_f32_mono_16khz() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            sample_type: PcmSampleType::F32,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PcmChunk {
    pub sequence: u64,
    pub started_at: SystemTime,
    pub fetched_at: SystemTime,
    pub format: PcmFormat,
    pub samples: Vec<f32>,
    pub duration_estimate: Duration,
}

#[derive(thiserror::Error, Debug)]
pub enum DecodeError {
    #[error("ffmpeg unavailable: {0}")]
    FfmpegUnavailable(String),

    #[error("ffmpeg failed: {0}")]
    FfmpegFailed(String),

    #[error("invalid pcm output: {0}")]
    InvalidPcm(String),
}

pub type Result<T> = std::result::Result<T, DecodeError>;

#[allow(async_fn_in_trait)]
pub trait AudioDecoder: Send + Sync {
    fn decode_segment(&self, item: IngestItem) -> BoxFuture<'_, Result<PcmChunk>>;
}

#[derive(Clone)]
pub struct Decoder {
    inner: std::sync::Arc<dyn AudioDecoder>,
}

impl Decoder {
    pub fn new(inner: std::sync::Arc<dyn AudioDecoder>) -> Self {
        Self { inner }
    }

    pub async fn decode_segment(&self, item: IngestItem) -> Result<PcmChunk> {
        self.inner.decode_segment(item).await
    }
}

#[derive(Clone, Debug)]
pub struct FfmpegAudioDecoder {
    output_format: PcmFormat,
}

impl Default for FfmpegAudioDecoder {
    fn default() -> Self {
        Self {
            output_format: PcmFormat::whisper_f32_mono_16khz(),
        }
    }
}

impl FfmpegAudioDecoder {
    pub fn new(output_format: PcmFormat) -> Self {
        Self { output_format }
    }

    fn ensure_ffmpeg_available(&self) -> Result<()> {
        download::auto_download().map_err(|e| DecodeError::FfmpegUnavailable(e.to_string()))
    }

    fn parse_f32le_mono(raw: &[u8]) -> Result<Vec<f32>> {
        if !raw.len().is_multiple_of(4usize) {
            return Err(DecodeError::InvalidPcm(format!(
                "f32le byte length must be multiple of 4, got {}",
                raw.len()
            )));
        }
        let mut out = Vec::with_capacity(raw.len() / 4);
        for chunk in raw.chunks_exact(4) {
            out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(out)
    }

    fn duration_from_samples(sample_rate_hz: u32, samples: usize) -> Duration {
        if sample_rate_hz == 0 {
            return Duration::from_secs(0);
        }
        let micros = (u128::from(samples as u64) * 1_000_000u128) / u128::from(sample_rate_hz);
        Duration::from_micros(micros.min(u128::from(u64::MAX)) as u64)
    }

    async fn decode_with_ffmpeg(&self, segment: Bytes) -> Result<Vec<f32>> {
        let fmt = self.output_format;
        if fmt.channels != 1 || fmt.sample_rate != 16_000 || fmt.sample_type != PcmSampleType::F32 {
            return Err(DecodeError::InvalidPcm(
                "only f32 mono 16kHz supported for now".to_owned(),
            ));
        }

        // TODO: optimize to a persistent FFmpeg process to reduce per-segment spawn latency.
        let mut child = tokio::process::Command::new(ffmpeg_path())
            .args([
                "-hide_banner",
                "-nostdin",
                "-loglevel",
                "error",
                "-fflags",
                "nobuffer",
                "-flags",
                "low_delay",
                "-i",
                "pipe:0",
                "-vn",
                "-sn",
                "-dn",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "pipe:1",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            DecodeError::FfmpegFailed("ffmpeg stdin unavailable (pipe not created)".to_owned())
        })?;
        let mut stdout = child.stdout.take().ok_or_else(|| {
            DecodeError::FfmpegFailed("ffmpeg stdout unavailable (pipe not created)".to_owned())
        })?;
        let mut stderr = child.stderr.take().ok_or_else(|| {
            DecodeError::FfmpegFailed("ffmpeg stderr unavailable (pipe not created)".to_owned())
        })?;

        let stdin_task = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&segment).await?;
            stdin.shutdown().await?;
            Ok::<(), std::io::Error>(())
        });

        let stdout_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).await?;
            Ok::<Vec<u8>, std::io::Error>(buf)
        });

        let stderr_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            stderr.read_to_end(&mut buf).await?;
            Ok::<Vec<u8>, std::io::Error>(buf)
        });

        let status = child
            .wait()
            .await
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?;

        stdin_task
            .await
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?;

        let stdout_bytes = stdout_task
            .await
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?;

        let stderr_bytes = stderr_task
            .await
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?
            .map_err(|e| DecodeError::FfmpegFailed(e.to_string()))?;

        if !status.success() {
            let stderr_s = String::from_utf8_lossy(&stderr_bytes).trim().to_owned();
            return Err(DecodeError::FfmpegFailed(format!(
                "exit_code={:?} stderr={stderr_s}",
                status.code()
            )));
        }

        Self::parse_f32le_mono(&stdout_bytes)
    }
}

impl AudioDecoder for FfmpegAudioDecoder {
    fn decode_segment(&self, item: IngestItem) -> BoxFuture<'_, Result<PcmChunk>> {
        let this = self.clone();
        async move {
            this.ensure_ffmpeg_available()?;
            let samples = this.decode_with_ffmpeg(item.bytes).await?;
            let duration_estimate =
                Self::duration_from_samples(this.output_format.sample_rate, samples.len());

            Ok(PcmChunk {
                sequence: item.sequence,
                started_at: item.fetched_at,
                fetched_at: item.fetched_at,
                format: this.output_format,
                samples,
                duration_estimate,
            })
        }
        .boxed()
    }
}

pub fn i16_to_f32_pcm(samples: &[i16]) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let scale = 1.0f32 / 32768.0f32;
    samples.iter().map(|&s| f32::from(s) * scale).collect()
}

pub fn duration_from_sample_count(
    sample_rate_hz: u32,
    channels: u16,
    sample_count: usize,
) -> Duration {
    if sample_rate_hz == 0 || channels == 0 {
        return Duration::from_secs(0);
    }
    let frames = sample_count / usize::from(channels);
    FfmpegAudioDecoder::duration_from_samples(sample_rate_hz, frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_to_f32_basic() {
        let v = i16_to_f32_pcm(&[-32768, -1, 0, 1, 32767]);
        assert!((v[0] + 1.0).abs() < 1e-6);
        assert!((v[2] - 0.0).abs() < 1e-6);
        assert!(v[4] <= 1.0);
        assert!(v[4] > 0.9999);
    }

    #[test]
    fn duration_from_sample_count_mono_16k() {
        let d = duration_from_sample_count(16_000, 1, 16_000);
        assert_eq!(d.as_secs(), 1);
    }

    #[test]
    fn parse_f32le_rejects_non_multiple_of_4() {
        let err = FfmpegAudioDecoder::parse_f32le_mono(&[0, 1, 2]).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("multiple of 4"));
    }

    #[test]
    fn parse_f32le_roundtrip() {
        let input = [0.0f32, -0.5f32, 1.0f32];
        let mut raw = Vec::new();
        for f in input {
            raw.extend_from_slice(&f.to_le_bytes());
        }
        let out = FfmpegAudioDecoder::parse_f32le_mono(&raw).unwrap();
        assert_eq!(out.len(), 3);
        for (a, b) in out.iter().zip([0.0f32, -0.5f32, 1.0f32].iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    #[ignore]
    fn ffmpeg_decode_smoke_ignored() {
        // Intentionally ignored: requires ffmpeg presence / download.
        // Kept to allow local manual verification.
    }
}
