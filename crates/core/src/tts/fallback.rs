use crate::tts::{TtsAudio, TtsClient, TtsError, TtsRequest};
use futures::future::BoxFuture;
use futures::FutureExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const RETRY_PRIMARY_INTERVAL: Duration = Duration::from_secs(300);
const LOG_TARGET: &str = "tts::fallback";

#[derive(Clone)]
pub struct FallbackTtsClient<P, L>
where
    P: TtsClient + Clone,
    L: TtsClient + Clone,
{
    primary: P,
    local: L,
    state: Arc<FallbackState>,
}

struct FallbackState {
    quota_exhausted: AtomicBool,
    exhausted_at: Mutex<Option<Instant>>,
}

impl<P, L> FallbackTtsClient<P, L>
where
    P: TtsClient + Clone,
    L: TtsClient + Clone,
{
    pub fn new(primary: P, local: L) -> Self {
        Self {
            primary,
            local,
            state: Arc::new(FallbackState {
                quota_exhausted: AtomicBool::new(false),
                exhausted_at: Mutex::new(None),
            }),
        }
    }

    pub fn is_using_fallback(&self) -> bool {
        self.state.quota_exhausted.load(Ordering::Relaxed)
    }

    pub fn reset_quota_flag(&self) {
        self.state.quota_exhausted.store(false, Ordering::Relaxed);
        if let Ok(mut exhausted_at) = self.state.exhausted_at.try_lock() {
            *exhausted_at = None;
        }
    }

    #[cfg(test)]
    async fn force_fallback(&self) {
        self.state.quota_exhausted.store(true, Ordering::Relaxed);
        *self.state.exhausted_at.lock().await = Some(Instant::now());
    }
}

impl<P, L> TtsClient for FallbackTtsClient<P, L>
where
    P: TtsClient + Clone + Send + Sync + 'static,
    L: TtsClient + Clone + Send + Sync + 'static,
{
    fn synthesize(&self, request: TtsRequest) -> BoxFuture<'_, Result<TtsAudio, TtsError>> {
        async move {
            if self.state.quota_exhausted.load(Ordering::Relaxed) {
                let should_retry = {
                    let exhausted_at = self.state.exhausted_at.lock().await;
                    exhausted_at
                        .map(|t| t.elapsed() >= RETRY_PRIMARY_INTERVAL)
                        .unwrap_or(false)
                };

                if should_retry {
                    tracing::warn!(target: LOG_TARGET, "Retrying ElevenLabs after 5m cooldown...");
                    match self.primary.synthesize(request.clone()).await {
                        Ok(audio) => {
                            self.state.quota_exhausted.store(false, Ordering::Relaxed);
                            *self.state.exhausted_at.lock().await = None;
                            tracing::info!(target: LOG_TARGET, "ElevenLabs recovered, switching back to cloud TTS");
                            return Ok(audio);
                        }
                        Err(TtsError::QuotaExhausted) => {
                            *self.state.exhausted_at.lock().await = Some(Instant::now());
                            return self.local.synthesize(request).await;
                        }
                        Err(e) => {
                            tracing::warn!(target: LOG_TARGET, "ElevenLabs error (not quota), falling back to Piper for this request: {e}");
                            return self.local.synthesize(request).await;
                        }
                    }
                }

                return self.local.synthesize(request).await;
            }

            match self.primary.synthesize(request.clone()).await {
                Ok(audio) => Ok(audio),
                Err(TtsError::QuotaExhausted) => {
                    tracing::warn!(target: LOG_TARGET, "ElevenLabs quota exhausted, switching to local Piper TTS");
                    self.state.quota_exhausted.store(true, Ordering::Relaxed);
                    *self.state.exhausted_at.lock().await = Some(Instant::now());
                    self.local.synthesize(request).await
                }
                Err(e) => {
                    tracing::warn!(target: LOG_TARGET, "ElevenLabs error (not quota), falling back to Piper for this request: {e}");
                    self.local.synthesize(request).await
                }
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::{TtsAudio, TtsRequest};

    #[derive(Clone)]
    struct QuotaClient;

    impl TtsClient for QuotaClient {
        fn synthesize(
            &self,
            _request: TtsRequest,
        ) -> BoxFuture<'_, Result<TtsAudio, TtsError>> {
            async { Err(TtsError::QuotaExhausted) }.boxed()
        }
    }

    #[derive(Clone)]
    struct StubLocalClient;

    impl TtsClient for StubLocalClient {
        fn synthesize(
            &self,
            _request: TtsRequest,
        ) -> BoxFuture<'_, Result<TtsAudio, TtsError>> {
            async {
                Ok(TtsAudio {
                    sample_rate_hz: 22050,
                    channels: 1,
                    pcm_i16: vec![1, 2, 3],
                })
            }
            .boxed()
        }
    }

    #[derive(Clone)]
    struct OkClient;

    impl TtsClient for OkClient {
        fn synthesize(
            &self,
            _request: TtsRequest,
        ) -> BoxFuture<'_, Result<TtsAudio, TtsError>> {
            async {
                Ok(TtsAudio {
                    sample_rate_hz: 44100,
                    channels: 1,
                    pcm_i16: vec![10, 20, 30],
                })
            }
            .boxed()
        }
    }

    #[derive(Clone)]
    struct TransientErrorClient;

    impl TtsClient for TransientErrorClient {
        fn synthesize(
            &self,
            _request: TtsRequest,
        ) -> BoxFuture<'_, Result<TtsAudio, TtsError>> {
            async { Err(TtsError::Other("network timeout".into())) }.boxed()
        }
    }

    fn make_request() -> TtsRequest {
        TtsRequest {
            text: "hello".into(),
            voice: None,
            prosody: None,
        }
    }

    #[tokio::test]
    async fn falls_back_on_quota_exhausted() {
        let client = FallbackTtsClient::new(QuotaClient, StubLocalClient);
        assert!(!client.is_using_fallback());

        let result = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result.sample_rate_hz, 22050);
        assert!(client.is_using_fallback());

        let result2 = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result2.sample_rate_hz, 22050);
    }

    #[tokio::test]
    async fn uses_primary_when_ok() {
        let client = FallbackTtsClient::new(OkClient, StubLocalClient);
        let result = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result.sample_rate_hz, 44100);
        assert!(!client.is_using_fallback());
    }

    #[tokio::test]
    async fn reset_allows_primary_again() {
        let client = FallbackTtsClient::new(OkClient, StubLocalClient);
        client.force_fallback().await;
        assert!(client.is_using_fallback());

        let result = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result.sample_rate_hz, 22050);

        client.reset_quota_flag();
        let result2 = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result2.sample_rate_hz, 44100);
    }

    #[tokio::test]
    async fn falls_back_on_non_quota_error_without_setting_flag() {
        let client = FallbackTtsClient::new(TransientErrorClient, StubLocalClient);
        let result = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result.sample_rate_hz, 22050);
        assert!(!client.is_using_fallback());
    }

    #[tokio::test]
    async fn retry_primary_after_interval_elapsed() {
        let client = FallbackTtsClient::new(OkClient, StubLocalClient);
        client.state.quota_exhausted.store(true, Ordering::Relaxed);
        *client.state.exhausted_at.lock().await =
            Some(Instant::now() - RETRY_PRIMARY_INTERVAL - Duration::from_secs(1));

        let result = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result.sample_rate_hz, 44100);
        assert!(!client.is_using_fallback());
    }

    #[tokio::test]
    async fn no_retry_before_interval_elapsed() {
        let client = FallbackTtsClient::new(OkClient, StubLocalClient);
        client.force_fallback().await;

        let result = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result.sample_rate_hz, 22050);
        assert!(client.is_using_fallback());
    }

    #[tokio::test]
    async fn retry_resets_timer_on_repeated_quota_exhaustion() {
        let client = FallbackTtsClient::new(QuotaClient, StubLocalClient);
        client.state.quota_exhausted.store(true, Ordering::Relaxed);
        let old_time = Instant::now() - RETRY_PRIMARY_INTERVAL - Duration::from_secs(1);
        *client.state.exhausted_at.lock().await = Some(old_time);

        let result = client.synthesize(make_request()).await.unwrap();
        assert_eq!(result.sample_rate_hz, 22050);
        assert!(client.is_using_fallback());

        let exhausted_at = client.state.exhausted_at.lock().await;
        assert!(exhausted_at.unwrap().elapsed() < Duration::from_secs(2));
    }
}
