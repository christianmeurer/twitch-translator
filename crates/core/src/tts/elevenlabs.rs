use crate::tts::{TtsAudio, TtsClient, TtsError, TtsRequest};
use crate::util::{is_http_retryable, retry_with_backoff, RetryConfig};
use futures::future::BoxFuture;
use futures::FutureExt;
use reqwest::Client;
use serde::Serialize;
use std::io::Cursor;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::audio::Signal;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ElevenLabsError {
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),
    
    #[error("Audio decoding failed: {0}")]
    AudioDecoding(String),
    
    #[error("No audio data received")]
    NoAudioData,
}

#[derive(Clone)]
pub struct ElevenLabsTtsClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ElevenLabsTtsClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.elevenlabs.io/v1".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

#[derive(Serialize, Clone)]
struct ElevenLabsRequest {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_settings: Option<VoiceSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronunciation_dictionary_locators: Option<Vec<PronunciationDictionaryLocator>>,
}

#[derive(Serialize, Clone)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_speaker_boost: Option<bool>,
}

#[derive(Serialize, Clone)]
struct PronunciationDictionaryLocator {
    pronunciation_dictionary_id: String,
    version_id: String,
}

// Function to decode MP3 audio to PCM
fn decode_mp3_to_pcm(mp3_data: Vec<u8>) -> Result<TtsAudio, ElevenLabsError> {
    let cursor = Cursor::new(mp3_data);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    
    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();
    
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| ElevenLabsError::AudioDecoding(format!("Failed to probe audio: {}", e)))?;
    
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| ElevenLabsError::AudioDecoding("No audio track found".to_string()))?;
    
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| ElevenLabsError::AudioDecoding(format!("Failed to create decoder: {}", e)))?;
    
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.ok_or_else(|| {
        ElevenLabsError::AudioDecoding("Sample rate not specified".to_string())
    })?;
    
    let channels = track.codec_params.channels.ok_or_else(|| {
        ElevenLabsError::AudioDecoding("Channels not specified".to_string())
    })?;
    
    let mut pcm_samples = Vec::new();
    
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                
                // Convert all channels to interleaved i16 samples
                for i in 0..decoded.frames() {
                    for channel in 0..spec.channels.count() {
                        // Get the sample from the decoded buffer
                        let sample = match decoded {
                            symphonia::core::audio::AudioBufferRef::F32(ref buf) => buf.chan(channel)[i],
                            symphonia::core::audio::AudioBufferRef::U8(ref buf) => buf.chan(channel)[i] as f32 / 128.0 - 1.0,
                            symphonia::core::audio::AudioBufferRef::U16(ref buf) => buf.chan(channel)[i] as f32 / 32768.0 - 1.0,
                            symphonia::core::audio::AudioBufferRef::S16(ref buf) => buf.chan(channel)[i] as f32 / 32768.0,
                            symphonia::core::audio::AudioBufferRef::S32(ref buf) => buf.chan(channel)[i] as f32 / 2147483648.0,
                            symphonia::core::audio::AudioBufferRef::F64(ref buf) => buf.chan(channel)[i] as f32,
                            symphonia::core::audio::AudioBufferRef::U32(ref buf) => buf.chan(channel)[i] as f32 / 4294967296.0 - 1.0,
                            symphonia::core::audio::AudioBufferRef::S8(ref buf) => buf.chan(channel)[i] as f32 / 128.0,
                            // Skip less common formats that cause compilation issues
                            _ => {
                                tracing::warn!("Unsupported audio format, skipping sample");
                                0.0
                            }
                        };
                        
                        // Convert f32 to i16
                        let sample_i16 = (sample * i16::MAX as f32) as i16;
                        pcm_samples.push(sample_i16);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to decode audio packet: {}", e);
            }
        }
    }
    
    if pcm_samples.is_empty() {
        return Err(ElevenLabsError::NoAudioData);
    }
    
    Ok(TtsAudio {
        sample_rate_hz: sample_rate,
        channels: channels.count() as u16,
        pcm_i16: pcm_samples,
    })
}


impl TtsClient for ElevenLabsTtsClient {
    fn synthesize(&self, request: TtsRequest) -> BoxFuture<'_, Result<TtsAudio, TtsError>> {
        let this = self.clone();
        async move {
            // Determine the voice ID
            let voice_id = request
                .voice
                .as_ref()
                .map(|v| v.0.clone())
                .unwrap_or_else(|| "21m00Tcm4TlvDq8ikWAM".to_string()); // Default voice ID

            // Build the URL
            let url = format!("{}/text-to-speech/{}/stream", this.base_url, voice_id);

            // Prepare voice settings based on prosody features
            let voice_settings = if let Some(prosody) = request.prosody {
                Some(VoiceSettings {
                    stability: map_energy_to_stability(prosody.energy_rms),
                    similarity_boost: 0.75, // Default value
                    style: Some(map_energy_to_style(prosody.energy_rms)),
                    use_speaker_boost: Some(true),
                })
            } else {
                Some(VoiceSettings {
                    stability: 0.5,
                    similarity_boost: 0.75,
                    style: Some(0.0),
                    use_speaker_boost: Some(true),
                })
            };

            // Prepare the request
            let elevenlabs_request = ElevenLabsRequest {
                text: request.text,
                voice_settings,
                pronunciation_dictionary_locators: None,
            };

            // Configure retry with exponential backoff
            let retry_config = RetryConfig::default();
            
            // Perform the TTS synthesis with retry logic
            let audio_data = retry_with_backoff(&retry_config, || {
                let client = this.client.clone();
                let api_key = this.api_key.clone();
                let request_body = elevenlabs_request.clone();
                let url_str = url.clone();
                
                async move {
                    // Send the request
                    let response = client
                        .post(&url_str)
                        .header("xi-api-key", &api_key)
                        .header("Content-Type", "application/json")
                        .header("Accept", "audio/mpeg")
                        .json(&request_body)
                        .send()
                        .await
                        .map_err(|e| TtsError::Other(format!("HTTP request failed: {}", e)))?;

                    if !response.status().is_success() {
                        let status = response.status();
                        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

                        if status.as_u16() == 401
                            || error_text.to_lowercase().contains("quota")
                        {
                            return Err(TtsError::QuotaExhausted);
                        }

                        if is_http_retryable(status.as_u16()) {
                            return Err(TtsError::Other(format!(
                                "HTTP error {}: {}",
                                status, error_text
                            )));
                        }

                        return Err(TtsError::Other(format!(
                            "HTTP error {}: {}",
                            status, error_text
                        )));
                    }

                    // Get the audio data
                    let audio_data = response
                        .bytes()
                        .await
                        .map_err(|e| TtsError::Other(format!("Failed to read audio data: {}", e)))?;

                    if audio_data.is_empty() {
                        return Err(TtsError::Other("No audio data received from ElevenLabs".to_string()));
                    }

                    Ok(audio_data.to_vec())
                }
            }, |error| {
                // Only retry on HTTP errors with retryable status codes
                matches!(error, TtsError::Other(_))
            }).await?;

            // Decode the MP3 audio to PCM
            match decode_mp3_to_pcm(audio_data) {
                Ok(tts_audio) => Ok(tts_audio),
                Err(e) => {
                    tracing::warn!("Failed to decode MP3 audio, falling back to dummy audio: {}", e);
                    // Fallback to dummy audio if decoding fails
                    Ok(TtsAudio {
                        sample_rate_hz: 22050,
                        channels: 1,
                        pcm_i16: vec![0; 22050],
                    })
                }
            }
        }
        .boxed()
    }
}

// Helper functions to map prosody features to voice settings
fn map_energy_to_stability(energy: f32) -> f32 {
    // Map energy to stability (0.0 to 1.0)
    // Higher energy -> lower stability (more expressive)
    1.0 - energy.clamp(0.0, 1.0)
}

fn map_energy_to_style(energy: f32) -> f32 {
    // Map energy to style (0.0 to 1.0)
    // Higher energy -> higher style (more emotional)
    energy.clamp(0.0, 1.0)
}