use crate::emotion::ProsodyWindow;
use futures::future::BoxFuture;
use futures::FutureExt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Emotion {
    Neutral,
    Happy,
    Sad,
    Angry,
    Fearful,
    Disgusted,
    Surprised,
}

#[derive(thiserror::Error, Debug)]
pub enum EmotionError {
    #[error("emotion analysis not implemented")]
    NotImplemented,
}

pub trait EmotionAnalyzer: Send + Sync {
    fn analyze_prosody(
        &self,
        prosody: ProsodyWindow,
    ) -> BoxFuture<'_, Result<Emotion, EmotionError>>;

    fn analyze_text(&self, text: String) -> BoxFuture<'_, Result<Emotion, EmotionError>>;
}

pub struct BasicEmotionAnalyzer;

impl BasicEmotionAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BasicEmotionAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl EmotionAnalyzer for BasicEmotionAnalyzer {
    fn analyze_prosody(
        &self,
        _prosody: ProsodyWindow,
    ) -> BoxFuture<'_, Result<Emotion, EmotionError>> {
        async move {
            // TODO: Implement prosody-based emotion analysis
            Ok(Emotion::Neutral)
        }
        .boxed()
    }

    fn analyze_text(&self, _text: String) -> BoxFuture<'_, Result<Emotion, EmotionError>> {
        async move {
            // TODO: Implement text-based emotion analysis
            Ok(Emotion::Neutral)
        }
        .boxed()
    }
}
