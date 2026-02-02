mod whisper;

use crate::decode::PcmChunk;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub use whisper::WhisperAsrBackend;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TranscriptSegment {
    pub text: String,
    pub audio_duration: Duration,
    pub confidence: Option<f32>,
}

#[derive(thiserror::Error, Debug)]
pub enum AsrError {
    #[error("asr not implemented")]
    NotImplemented,
}

pub trait AsrBackend: Send + Sync {
    fn transcribe(&self, audio: PcmChunk) -> BoxFuture<'_, Result<TranscriptSegment, AsrError>>;
}
