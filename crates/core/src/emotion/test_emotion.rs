#[cfg(test)]
mod tests {
    use crate::emotion::{BasicEmotionAnalyzer, Emotion, EmotionAnalyzer, ProsodyFeatures, ProsodyWindow};
    
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
    
    #[test]
    fn test_combine_emotions() {
        let analyzer = BasicEmotionAnalyzer::new();
        
        // Test combining same emotions
        let combined = futures::executor::block_on(analyzer.combine_emotions(Emotion::Happy, Emotion::Happy)).unwrap();
        assert_eq!(combined, Emotion::Happy);
        
        // Test combining with neutral (should return non-neutral)
        let combined = futures::executor::block_on(analyzer.combine_emotions(Emotion::Neutral, Emotion::Sad)).unwrap();
        assert_eq!(combined, Emotion::Sad);
        
        // Test combining different intensities (should return more intense)
        let combined = futures::executor::block_on(analyzer.combine_emotions(Emotion::Happy, Emotion::Angry)).unwrap();
        assert_eq!(combined, Emotion::Angry);
    }
}