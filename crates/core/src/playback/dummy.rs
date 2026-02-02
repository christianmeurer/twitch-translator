use crate::playback::{PlaybackError, PlaybackSink};
use crate::tts::TtsAudio;
use futures::future::BoxFuture;
use futures::FutureExt;

#[derive(Clone)]
pub struct DummyPlaybackSink;

impl DummyPlaybackSink {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DummyPlaybackSink {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackSink for DummyPlaybackSink {
    fn play(&self, _audio: TtsAudio) -> BoxFuture<'_, Result<(), PlaybackError>> {
        async move {
            // For a dummy implementation, we'll just return Ok(())
            Ok(())
        }
        .boxed()
    }
}
