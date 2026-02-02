use crate::{
    asr::AsrBackend,
    config::{ApiKeys, AppConfig, LatencyBudget},
    decode::AudioDecoder,
    ingest::Ingestor,
    playback::PlaybackSink,
    translate::Translator,
    tts::TtsClient,
};

#[derive(thiserror::Error, Debug)]
pub enum PipelineError {
    #[error("pipeline not implemented")]
    NotImplemented,
    #[error("internal channel closed")]
    ChannelClosed,
}

#[derive(Clone, Debug)]
pub struct PipelineConfig {
    pub latency: LatencyBudget,
    pub api_keys: ApiKeys,
}

impl PipelineConfig {
    pub fn from_app(app: &AppConfig) -> Self {
        Self {
            latency: app.latency,
            api_keys: app.api_keys.clone(),
        }
    }
}

pub struct Pipeline<I, D, A, Tr, Ts, P> {
    pub ingest: I,
    pub decode: D,
    pub asr: A,
    pub translate: Tr,
    pub tts: Ts,
    pub playback: P,
    pub config: PipelineConfig,
}

impl<I, D, A, Tr, Ts, P> Pipeline<I, D, A, Tr, Ts, P>
where
    I: Ingestor + Clone + 'static,
    D: AudioDecoder + Clone + 'static,
    A: AsrBackend + Clone + 'static,
    Tr: Translator + Clone + 'static,
    Ts: TtsClient + Clone + 'static,
    P: PlaybackSink + Clone + 'static,
{
    pub async fn run(&self) -> Result<(), PipelineError> {
        // Create channels for communication between components
        let (ingest_tx, mut ingest_rx) =
            tokio::sync::mpsc::channel::<crate::ingest::IngestPacket>(self.channel_capacity());
        let (pcm_tx, mut pcm_rx) =
            tokio::sync::mpsc::channel::<crate::decode::PcmChunk>(self.channel_capacity());
        let (transcript_tx, mut transcript_rx) =
            tokio::sync::mpsc::channel::<crate::asr::TranscriptSegment>(self.channel_capacity());
        let (translation_tx, mut translation_rx) =
            tokio::sync::mpsc::channel::<crate::translate::Translation>(self.channel_capacity());
        let (tts_tx, mut tts_rx) =
            tokio::sync::mpsc::channel::<crate::tts::TtsAudio>(self.channel_capacity());

        // Start the ingestor
        let ingest_task: tokio::task::JoinHandle<Result<(), PipelineError>> = {
            let ingest = self.ingest.clone();
            tokio::spawn(async move {
                ingest.start(ingest_tx).await.map_err(|e| {
                    tracing::error!(error = %e, "ingestor failed");
                    PipelineError::ChannelClosed
                })
            })
        };

        // Start the decoder
        let decode_task = {
            let decode = self.decode.clone();
            tokio::spawn(async move {
                while let Some(packet) = ingest_rx.recv().await {
                    // Convert IngestPacket to IngestItem
                    let item = crate::ingest::IngestItem {
                        sequence: 0, // TODO: Generate proper sequence numbers
                        fetched_at: packet.received_at,
                        url: url::Url::parse("https://example.com/segment.ts").unwrap(), // TODO: Generate proper URLs
                        approx_duration: packet.approx_duration,
                        bytes: bytes::Bytes::from(packet.bytes),
                    };

                    match decode.decode_segment(item).await {
                        Ok(pcm) => {
                            if pcm_tx.send(pcm).await.is_err() {
                                tracing::error!("pcm channel closed");
                                return Err(PipelineError::ChannelClosed);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "decode failed");
                        }
                    }
                }
                Ok(())
            })
        };

        // Start the ASR
        let asr_task = {
            let asr = self.asr.clone();
            tokio::spawn(async move {
                while let Some(pcm) = pcm_rx.recv().await {
                    match asr.transcribe(pcm).await {
                        Ok(transcript) => {
                            if transcript_tx.send(transcript).await.is_err() {
                                tracing::error!("transcript channel closed");
                                return Err(PipelineError::ChannelClosed);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "asr failed");
                        }
                    }
                }
                Ok(())
            })
        };

        // Start the translator
        let translate_task = {
            let translate = self.translate.clone();
            let target_lang = crate::config::TargetLang("English".to_string()); // TODO: Make configurable
            tokio::spawn(async move {
                while let Some(transcript) = transcript_rx.recv().await {
                    match translate
                        .translate(transcript.text, target_lang.clone())
                        .await
                    {
                        Ok(translation) => {
                            if translation_tx.send(translation).await.is_err() {
                                tracing::error!("translation channel closed");
                                return Err(PipelineError::ChannelClosed);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "translation failed");
                        }
                    }
                }
                Ok(())
            })
        };

        // Start the TTS
        let tts_task = {
            let tts = self.tts.clone();
            tokio::spawn(async move {
                while let Some(translation) = translation_rx.recv().await {
                    let request = crate::tts::TtsRequest {
                        text: translation.text,
                        voice: None,
                        prosody: None, // TODO: Add prosody features
                    };
                    match tts.synthesize(request).await {
                        Ok(audio) => {
                            if tts_tx.send(audio).await.is_err() {
                                tracing::error!("tts channel closed");
                                return Err(PipelineError::ChannelClosed);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "tts failed");
                        }
                    }
                }
                Ok(())
            })
        };

        // Start the playback
        let playback_task: tokio::task::JoinHandle<Result<(), PipelineError>> = {
            let playback = self.playback.clone();
            tokio::spawn(async move {
                while let Some(audio) = tts_rx.recv().await {
                    match playback.play(audio).await {
                        Ok(()) => {}
                        Err(e) => {
                            tracing::warn!(error = %e, "playback failed");
                        }
                    }
                }
                Ok(())
            })
        };

        // Wait for all tasks to complete
        let _ = tokio::try_join!(
            ingest_task,
            decode_task,
            asr_task,
            translate_task,
            tts_task,
            playback_task
        )
        .map_err(|_| PipelineError::ChannelClosed)?;

        Ok(())
    }

    pub fn channel_capacity(&self) -> usize {
        let cap = (self.config.latency.target_ms / 250).clamp(2, 32);
        usize::try_from(cap).unwrap_or(8)
    }
}
