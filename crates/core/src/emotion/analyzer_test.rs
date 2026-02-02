#[cfg(test)]
mod tests {
    use super::*;
    use crate::emotion::{Emotion, ProsodyFeatures};

    #[test]
    fn test_prosody_emotion_mapping() {
        // Test high energy mapping to excited emotion
        let high_energy = ProsodyFeatures {
            energy_rms: 0.8,
            pitch_mean: 200.0,
            pitch_range: 100.0,
            speech_rate: 200.0,
        };
        
        let emotion = map_prosody_to_emotion(&high_energy);
        // We expect high energy to map to an excited or happy emotion
        assert!(matches!(emotion, Emotion::Happy | Emotion::Excited));
        
        // Test low energy mapping to calm emotion
        let low_energy = ProsodyFeatures {
            energy_rms: 0.2,
            pitch_mean: 150.0,
            pitch_range: 30.0,
            speech_rate: 100.0,
        };
        
        let emotion = map_prosody_to_emotion(&low_energy);
        // We expect low energy to map to a calm or neutral emotion
        assert!(matches!(emotion, Emotion::Calm | Emotion::Neutral));
    }

    #[test]
    fn test_text_emotion_mapping() {
        // Test positive text mapping
        let positive_emotion = map_text_to_emotion("I'm so happy today!");
        assert_eq!(positive_emotion, Emotion::Happy);
        
        // Test negative text mapping
        let negative_emotion = map_text_to_emotion("This is terrible and awful.");
        assert_eq!(negative_emotion, Emotion::Sad);
        
        // Test neutral text mapping
        let neutral_emotion = map_text_to_emotion("The weather is okay today.");
        assert_eq!(neutral_emotion, Emotion::Neutral);
    }

    #[test]
    fn test_combine_emotions() {
        // Test combining prosody and text emotions
        let combined = combine_emotions(Emotion::Happy, Emotion::Excited);
        // When both are positive, we should get the more intense one
        assert_eq!(combined, Emotion::Excited);
        
        let combined = combine_emotions(Emotion::Sad, Emotion::Calm);
        // When both are negative, we should get the more intense one
        assert_eq!(combined, Emotion::Sad);
    }
}