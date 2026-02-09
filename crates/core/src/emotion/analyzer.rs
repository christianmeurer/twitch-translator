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
    
    fn combine_emotions(
        &self,
        prosody_emotion: Emotion,
        text_emotion: Emotion,
    ) -> BoxFuture<'_, Result<Emotion, EmotionError>>;
}

pub struct BasicEmotionAnalyzer;

impl BasicEmotionAnalyzer {
    pub fn new() -> Self {
        Self
    }
    
    fn emotion_intensity(&self, emotion: &Emotion) -> i32 {
        match emotion {
            Emotion::Neutral => 0,
            Emotion::Happy | Emotion::Sad => 1,
            Emotion::Angry | Emotion::Fearful | Emotion::Disgusted | Emotion::Surprised => 2,
        }
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
            let emotion = if features.energy_rms > 0.3 {
                if let Some(pitch) = features.pitch_hz {
                    if pitch > 220.0 {
                        if features.energy_rms > 0.6 {
                            Emotion::Happy // Using Happy instead of Excited
                        } else {
                            Emotion::Happy
                        }
                    } else if pitch < 100.0 {
                        if features.energy_rms > 0.5 {
                            Emotion::Angry
                        } else {
                            Emotion::Sad
                        }
                    } else {
                        if features.energy_rms > 0.5 {
                            Emotion::Happy
                        } else {
                            Emotion::Neutral // Using Neutral instead of Calm
                        }
                    }
                } else {
                    if features.energy_rms > 0.6 {
                        Emotion::Happy // Using Happy instead of Excited
                    } else {
                        Emotion::Happy
                    }
                }
            } else if features.energy_rms > 0.1 {
                if let Some(pitch) = features.pitch_hz {
                    if pitch > 200.0 {
                        Emotion::Happy
                    } else if pitch < 100.0 {
                        Emotion::Sad
                    } else {
                        Emotion::Neutral // Using Neutral instead of Calm
                    }
                } else {
                    Emotion::Neutral // Using Neutral instead of Calm
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
            
            let emotion = if lower_text.contains("happy") || lower_text.contains("joy") || lower_text.contains("excited") || lower_text.contains("amazing") || lower_text.contains("wonderful") || lower_text.contains("awesome") || lower_text.contains("thrilled") {
                Emotion::Happy
            } else if lower_text.contains("sad") || lower_text.contains("depressed") || lower_text.contains("unhappy") || lower_text.contains("terrible") || lower_text.contains("awful") {
                Emotion::Sad
            } else if lower_text.contains("angry") || lower_text.contains("mad") || lower_text.contains("furious") || lower_text.contains("hate") {
                Emotion::Angry
            } else if lower_text.contains("scared") || lower_text.contains("afraid") || lower_text.contains("fear") || lower_text.contains("nervous") {
                Emotion::Fearful
            } else if lower_text.contains("disgust") || lower_text.contains("disgusting") || lower_text.contains("gross") || lower_text.contains("disgusted") {
                Emotion::Disgusted
            } else if lower_text.contains("surprise") || lower_text.contains("amazing") || lower_text.contains("wow") || lower_text.contains("incredible") {
                Emotion::Surprised
            } else {
                Emotion::Neutral
            };
            
            Ok(emotion)
        }
        .boxed()
    }
    
    fn combine_emotions(
        &self,
        prosody_emotion: Emotion,
        text_emotion: Emotion,
    ) -> BoxFuture<'_, Result<Emotion, EmotionError>> {
        async move {
            // If both emotions are the same, return that emotion
            if prosody_emotion == text_emotion {
                return Ok(prosody_emotion);
            }
            
            // If one is neutral, return the other
            if prosody_emotion == Emotion::Neutral {
                return Ok(text_emotion);
            }
            if text_emotion == Emotion::Neutral {
                return Ok(prosody_emotion);
            }
            
            // Compare intensities and return the more intense emotion
            let self_intensity = self.emotion_intensity(&prosody_emotion);
            let text_intensity = self.emotion_intensity(&text_emotion);
            
            if self_intensity > text_intensity {
                Ok(prosody_emotion)
            } else if text_intensity > self_intensity {
                Ok(text_emotion)
            } else {
                // If intensities are equal, prefer prosody emotion
                Ok(prosody_emotion)
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emotion::ProsodyFeatures;
    
    #[test]
    fn test_basic_emotion_analyzer() {
        let analyzer = BasicEmotionAnalyzer::new();
        
        // Test text analysis
        let emotion = futures::executor::block_on(analyzer.analyze_text("I am happy today!".to_string())).unwrap();
        assert_eq!(emotion, Emotion::Happy);
        
        let emotion = futures::executor::block_on(analyzer.analyze_text("This is terrible.".to_string())).unwrap();
        assert_eq!(emotion, Emotion::Sad);
        
        let emotion = futures::executor::block_on(analyzer.analyze_text("I'm so angry right now!".to_string())).unwrap();
        assert_eq!(emotion, Emotion::Angry);
        
        let emotion = futures::executor::block_on(analyzer.analyze_text("This is amazing!".to_string())).unwrap();
        assert_eq!(emotion, Emotion::Happy);
        
        let emotion = futures::executor::block_on(analyzer.analyze_text("Just a normal day.".to_string())).unwrap();
        assert_eq!(emotion, Emotion::Neutral);
    }
    
    #[test]
    fn test_prosody_analysis() {
        let analyzer = BasicEmotionAnalyzer::new();
        
        // Test high energy prosody (should be happy)
        let prosody_high = ProsodyWindow {
            duration: std::time::Duration::from_secs(1),
            features: ProsodyFeatures {
                energy_rms: 0.8,
                pitch_hz: Some(250.0),
                speaking_rate: Some(5.0),
            },
        };
        
        let emotion = futures::executor::block_on(analyzer.analyze_prosody(prosody_high)).unwrap();
        assert_eq!(emotion, Emotion::Happy);
        
        // Test low energy prosody (should be neutral)
        let prosody_low = ProsodyWindow {
            duration: std::time::Duration::from_secs(1),
            features: ProsodyFeatures {
                energy_rms: 0.05,
                pitch_hz: Some(100.0),
                speaking_rate: Some(2.0),
            },
        };
        
        let emotion = futures::executor::block_on(analyzer.analyze_prosody(prosody_low)).unwrap();
        assert_eq!(emotion, Emotion::Neutral);
        
        // Test low pitch prosody (should be sad)
        let prosody_low_pitch = ProsodyWindow {
            duration: std::time::Duration::from_secs(1),
            features: ProsodyFeatures {
                energy_rms: 0.8,
                pitch_hz: Some(80.0),
                speaking_rate: Some(4.0),
            },
        };
        
        let emotion = futures::executor::block_on(analyzer.analyze_prosody(prosody_low_pitch)).unwrap();
        assert_eq!(emotion, Emotion::Angry);
    }
}