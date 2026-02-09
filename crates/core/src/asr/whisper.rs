use crate::asr::{AsrBackend, AsrError, TranscriptSegment};
use crate::decode::PcmChunk;
use futures::future::BoxFuture;
use futures::FutureExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

#[derive(Clone)]
pub struct WhisperAsrBackend {
    _ctx: Arc<WhisperContext>,
    state: Arc<Mutex<WhisperState>>,
}

impl WhisperAsrBackend {
    pub fn new(model_path: &str) -> Result<Self, AsrError> {
        if !std::path::Path::new(model_path).exists() {
            return Err(AsrError::ModelNotFound(model_path.to_string()));
        }

        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(true);

        let ctx = WhisperContext::new_with_params(model_path, ctx_params)
            .map_err(|e| AsrError::ModelLoadError(format!("Load failed: {e:?}")))?;

        let state = ctx
            .create_state()
            .map_err(|e| AsrError::InferenceError(format!("State init failed: {e:?}")))?;

        tracing::info!("Whisper model loaded with Vulkan GPU acceleration.");
        Ok(Self {
            _ctx: Arc::new(ctx),
            state: Arc::new(Mutex::new(state)),
        })
    }
}

impl AsrBackend for WhisperAsrBackend {
    fn transcribe(&self, audio: PcmChunk) -> BoxFuture<'_, Result<TranscriptSegment, AsrError>> {
        async move {
            if audio.samples.is_empty() {
                return Err(AsrError::EmptyAudio);
            }

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_n_threads(4);
            params.set_language(Some("en"));

            let mut state = self.state.lock().await;

            state
                .full(params, &audio.samples)
                .map_err(|e| AsrError::InferenceError(format!("Inference failed: {e:?}")))?;

            let num_segments = state.full_n_segments();
            let mut text = String::new();

            for i in 0..num_segments {
                if let Some(segment) = state.get_segment(i) {
                    if let Ok(segment_text) = segment.to_str() {
                        text.push_str(segment_text);
                        text.push(' ');
                    }
                }
            }

            let duration = Duration::from_secs_f32(audio.samples.len() as f32 / 16000.0);

            Ok(TranscriptSegment {
                text: text.trim().to_string(),
                audio_duration: duration,
                confidence: None,
            })
        }
        .boxed()
    }
}