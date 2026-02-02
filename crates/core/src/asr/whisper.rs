use crate::asr::{AsrBackend, AsrError, TranscriptSegment};
use crate::decode::PcmChunk;
use futures::future::BoxFuture;
use futures::FutureExt;
use std::time::Duration;

#[derive(Clone)]
pub struct WhisperAsrBackend {
    #[allow(dead_code)]
    model_path: String,
}

impl WhisperAsrBackend {
    pub fn new(model_path: &str) -> Result<Self, AsrError> {
        // For now, we'll just store the model path
        // In a real implementation, we would load the model here
        Ok(Self {
            model_path: model_path.to_string(),
        })
    }
}

impl AsrBackend for WhisperAsrBackend {
    fn transcribe(&self, audio: PcmChunk) -> BoxFuture<'_, Result<TranscriptSegment, AsrError>> {
        async move {
            // We assume audio is 16kHz mono based on our decoder
            if audio.format.sample_rate != 16000 || audio.format.channels != 1 {
                return Err(AsrError::NotImplemented); // TODO: Better error for unsupported format
            }

            // For now, we'll return a dummy transcription
            // In a real implementation, we would run the Whisper model here
            let text = "This is a dummy transcription from the Whisper ASR backend.".to_string();
            
            // Calculate duration from audio data
            let duration = Duration::from_secs_f32(audio.samples.len() as f32 / audio.format.sample_rate as f32);
            
            Ok(TranscriptSegment {
                text,
                audio_duration: duration,
                confidence: None, // Whisper doesn't provide overall confidence per segment
            })
        }
        .boxed()
    }
}
