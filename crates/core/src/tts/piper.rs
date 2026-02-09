use crate::tts::{TtsAudio, TtsClient, TtsError, TtsRequest};
use futures::future::BoxFuture;
use futures::FutureExt;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const PIPER_SAMPLE_RATE: u32 = 22050;
const PIPER_CHANNELS: u16 = 1;
const WAV_HEADER_BYTES: usize = 44;

#[derive(Clone, Debug)]
pub struct PiperTtsClient {
    piper_binary: PathBuf,
    model_path: PathBuf,
}

impl PiperTtsClient {
    #[must_use]
    pub fn new(piper_binary: PathBuf, model_path: PathBuf) -> Self {
        Self {
            piper_binary,
            model_path,
        }
    }
}

impl TtsClient for PiperTtsClient {
    fn synthesize(&self, request: TtsRequest) -> BoxFuture<'_, Result<TtsAudio, TtsError>> {
        let piper_binary = self.piper_binary.clone();
        let model_path = self.model_path.clone();
        let text = request.text;

        async move {
            let mut child = Command::new(&piper_binary)
                .arg("--model")
                .arg(&model_path)
                .arg("--output_raw")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| {
                    let path = piper_binary.display();
                    TtsError::Other(format!("failed to spawn piper at {path}: {e}"))
                })?;

            {
                let stdin = child
                    .stdin
                    .as_mut()
                    .ok_or_else(|| TtsError::Other("failed to open piper stdin".into()))?;
                stdin
                    .write_all(text.as_bytes())
                    .await
                    .map_err(|e| TtsError::Other(format!("piper stdin write failed: {e}")))?;
            }
            child.stdin.take();

            let output = child
                .wait_with_output()
                .await
                .map_err(|e| TtsError::Other(format!("piper process failed: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status;
                return Err(TtsError::Other(format!(
                    "piper exited with {status}: {stderr}"
                )));
            }

            let raw_pcm = &output.stdout;
            if raw_pcm.is_empty() {
                return Err(TtsError::Other("piper produced no audio output".into()));
            }

            let pcm_bytes = if raw_pcm.len() > WAV_HEADER_BYTES && &raw_pcm[..4] == b"RIFF" {
                &raw_pcm[WAV_HEADER_BYTES..]
            } else {
                raw_pcm.as_slice()
            };

            let pcm_i16: Vec<i16> = pcm_bytes
                .chunks_exact(2)
                .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();

            if pcm_i16.is_empty() {
                return Err(TtsError::Other("piper produced empty PCM data".into()));
            }

            Ok(TtsAudio {
                sample_rate_hz: PIPER_SAMPLE_RATE,
                channels: PIPER_CHANNELS,
                pcm_i16,
            })
        }
        .boxed()
    }
}
