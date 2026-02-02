use crate::emotion::{Emotion, ProsodyWindow};
use futures::future::BoxFuture;
use futures::FutureExt;

#[derive(thiserror::Error, Debug)]
pub enum EmotionError {
    #[error("emotion analysis failed")]
    AnalysisFailed,
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
        prosody: ProsodyWindow,
    ) -> BoxFuture<'_, Result<Emotion, EmotionError>> {
        async move {
            // Analyze emotion based on prosody features
            let features = prosody.features;
            
            // Determine emotion based on energy, pitch, and speaking rate
            let emotion = if features.energy_rms > 0.1 {
                if let Some(pitch) = features.pitch_hz {
                    if pitch > 200.0 {
                        Emotion::Happy
                    } else if pitch < 100.0 {
                        Emotion::Sad
                    } else {
                        Emotion::Neutral
                    }
                } else {
                    Emotion::Happy
                }
            } else {
                Emotion::Neutral
            };
            
            Ok(emotion)
        }
        .boxed()
    }

    fn analyze_text(&self, text: String) -> BoxFuture<'_, Result<Emotion, EmotionError>> {
        async move {
            // Simple keyword-based emotion analysis
            let lower_text = text.to_lowercase();
            
            let emotion = if lower_text.contains("happy") || lower_text.contains("joy") || lower_text.contains("excited") {
                Emotion::Happy
            } else if lower_text.contains("sad") || lower_text.contains("depressed") || lower_text.contains("unhappy") {
                Emotion::Sad
            } else if lower_text.contains("angry") || lower_text.contains("mad") || lower_text.contains("furious") {
                Emotion::Angry
            } else if lower_text.contains("scared") || lower_text.contains("afraid") || lower_text.contains("fear") {
                Emotion::Fearful
            } else if lower_text.contains("disgust") || lower_text.contains("disgusting") || lower_text.contains("gross") {
                Emotion::Disgusted
            } else if lower_text.contains("surprise") || lower_text.contains("amazing") || lower_text.contains("wow") {
                Emotion::Surprised
            } else {
                Emotion::Neutral
            };
            
            Ok(emotion)
        }
        .boxed()
    }
}
