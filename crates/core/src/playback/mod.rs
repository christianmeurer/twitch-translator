mod dummy;

use crate::tts::TtsAudio;
use futures::future::BoxFuture;

pub use dummy::DummyPlaybackSink;

#[derive(thiserror::Error, Debug)]
pub enum PlaybackError {
    #[error("playback not implemented")]
    NotImplemented,
}

pub trait PlaybackSink: Send + Sync {
    fn play(&self, audio: TtsAudio) -> BoxFuture<'_, Result<(), PlaybackError>>;
}
