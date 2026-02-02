mod basic;

use crate::emotion::ProsodyFeatures;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

pub use basic::BasicTtsClient;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtsRequest {
    pub text: String,
    pub voice: Option<VoiceId>,
    pub prosody: Option<ProsodyFeatures>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtsAudio {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub pcm_i16: Vec<i16>,
}

#[derive(thiserror::Error, Debug)]
pub enum TtsError {
    #[error("tts not implemented")]
    NotImplemented,
}

pub trait TtsClient: Send + Sync {
    fn synthesize(&self, request: TtsRequest) -> BoxFuture<'_, Result<TtsAudio, TtsError>>;
}
