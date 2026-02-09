mod audio;
mod dummy;

use crate::tts::TtsAudio;
use futures::future::BoxFuture;

pub use audio::AudioPlaybackSink;
pub use dummy::DummyPlaybackSink;

#[derive(thiserror::Error, Debug)]
pub enum PlaybackError {
    #[error("playback not implemented")]
    NotImplemented,

    #[error("audio output unavailable: {details}")]
    AudioOutputUnavailable { details: String },
}

pub trait PlaybackSink: Send + Sync {
    fn play(&self, audio: TtsAudio) -> BoxFuture<'_, Result<(), PlaybackError>>;
}
