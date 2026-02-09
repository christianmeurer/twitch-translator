//! Automatic Speech Recognition (ASR) module
//!
//! This module provides traits and implementations for converting audio to text.
//! Currently supports Whisper-based ASR when the `whisper-rs` feature is enabled.

#[cfg(feature = "whisper-rs")]
mod whisper;

use crate::decode::PcmChunk;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "whisper-rs")]
pub use whisper::WhisperAsrBackend;

/// A segment of transcribed text with metadata
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TranscriptSegment {
    /// The transcribed text
    pub text: String,
    /// Duration of the audio segment that was transcribed
    pub audio_duration: Duration,
    /// Confidence score for the transcription (if available)
    pub confidence: Option<f32>,
}

/// Errors that can occur during automatic speech recognition
#[derive(thiserror::Error, Debug)]
pub enum AsrError {
    /// The model file could not be found at the specified path
    #[error("model file not found: {0}")]
    ModelNotFound(String),
    
    /// Failed to load the ASR model
    #[error("failed to load model: {0}")]
    ModelLoadError(String),
    
    /// An error occurred during inference
    #[error("inference failed: {0}")]
    InferenceError(String),
    
    /// The audio format is not supported by the ASR backend
    #[error("unsupported audio format: expected {expected_sample_rate}Hz/{expected_channels}ch, got {actual_sample_rate}Hz/{actual_channels}ch")]
    UnsupportedFormat {
        expected_sample_rate: u32,
        expected_channels: u16,
        actual_sample_rate: u32,
        actual_channels: u16,
    },
    
    /// The provided audio data is empty
    #[error("empty audio data")]
    EmptyAudio,
    
    /// Failed to extract transcription from the model output
    #[error("transcription failed: {0}")]
    TranscriptionFailed(String),
}

/// Trait for automatic speech recognition backends
///
/// Implementations of this trait convert audio data to text transcripts.
/// The trait is async and designed to work with the pipeline architecture.
pub trait AsrBackend: Send + Sync {
    /// Transcribe audio to text
    ///
    /// # Arguments
    ///
    /// * `audio` - The audio chunk to transcribe
    ///
    /// # Returns
    ///
    /// A `TranscriptSegment` containing the transcribed text and metadata
    fn transcribe(&self, audio: PcmChunk) -> BoxFuture<'_, Result<TranscriptSegment, AsrError>>;
}
