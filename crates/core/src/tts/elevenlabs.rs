use crate::tts::{TtsAudio, TtsClient, TtsError, TtsRequest};
use futures::future::BoxFuture;
use futures::FutureExt;
use reqwest::Client;
use serde::Serialize;

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

#[derive(Serialize)]
struct ElevenLabsRequest {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_settings: Option<VoiceSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronunciation_dictionary_locators: Option<Vec<PronunciationDictionaryLocator>>,
}

#[derive(Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_speaker_boost: Option<bool>,
}

#[derive(Serialize)]
struct PronunciationDictionaryLocator {
    pronunciation_dictionary_id: String,
    version_id: String,
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

            // Send the request
            let response = this
                .client
                .post(&url)
                .header("xi-api-key", &this.api_key)
                .header("Content-Type", "application/json")
                .header("Accept", "audio/mpeg")
                .json(&elevenlabs_request)
                .send()
                .await
                .map_err(|_e| TtsError::NotImplemented)?; // TODO: Better error handling

            // Check if the request was successful
            if !response.status().is_success() {
                return Err(TtsError::NotImplemented); // TODO: Better error handling
            }

            // Get the audio data
            let _audio_data = response
                .bytes()
                .await
                .map_err(|_e| TtsError::NotImplemented)?; // TODO: Better error handling

            // For now, we'll return a dummy TtsAudio object since we're not actually decoding the audio
            // In a real implementation, we would decode the MP3 audio to PCM
            let tts_audio = TtsAudio {
                sample_rate_hz: 22050, // Default sample rate
                channels: 1,           // Mono
                pcm_i16: vec![0; 22050], // Dummy PCM data
            };

            Ok(tts_audio)
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