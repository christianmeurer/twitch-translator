mod analyzer;

use serde::{Deserialize, Serialize};
use std::time::Duration;

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

pub use analyzer::{BasicEmotionAnalyzer, EmotionAnalyzer, EmotionError};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProsodyFeatures {
    pub energy_rms: f32,
    pub pitch_hz: Option<f32>,
    pub speaking_rate: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProsodyWindow {
    pub duration: Duration,
    pub features: ProsodyFeatures,
}
