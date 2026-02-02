use crate::asr::{AsrBackend, AsrError, TranscriptSegment};
use crate::decode::PcmChunk;
use futures::future::BoxFuture;
use futures::FutureExt;

// use mutter::WhisperContext;

#[derive(Clone)]
pub struct WhisperAsrBackend {
    // context: WhisperContext,
}

impl WhisperAsrBackend {
    pub fn new(_model_path: &str) -> Result<Self, AsrError> {
        // let context = WhisperContext::new(model_path)
        //     .map_err(|e| AsrError::NotImplemented)?; // TODO: Better error handling
        // Ok(Self { context })
        Ok(Self {})
    }
}

impl AsrBackend for WhisperAsrBackend {
    fn transcribe(&self, _audio: PcmChunk) -> BoxFuture<'_, Result<TranscriptSegment, AsrError>> {
        // TODO: Implement transcription using mutter
        async move {
            // This is a placeholder implementation
            // We'll need to convert the PcmChunk to a format that mutter can use
            // and then call the whisper transcription API
            Err(AsrError::NotImplemented)
        }
        .boxed()
    }
}