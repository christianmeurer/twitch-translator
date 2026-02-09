use crate::ingest::{IngestError, IngestItem, Ingestor};
use bytes::Bytes;
use m3u8_rs::Playlist;
use reqwest::Client;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc::Sender;
use url::Url;

#[derive(Clone, Debug)]
pub struct TwitchIngestOptions {
    pub audio_only: bool,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

impl Default for TwitchIngestOptions {
    fn default() -> Self {
        Self {
            audio_only: true,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

#[derive(Clone)]
pub struct TwitchHlsIngestor {
    _twitch_config: crate::config::TwitchConfig,
    input: crate::config::InputSource,
    options: TwitchIngestOptions,
    client: Client,
}

impl TwitchHlsIngestor {
    pub fn new(
        twitch_config: crate::config::TwitchConfig,
        input: crate::config::InputSource,
        options: TwitchIngestOptions,
    ) -> Result<Self, IngestError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| IngestError::Http(e.into()))?;

        Ok(Self {
            _twitch_config: twitch_config,
            input,
            options,
            client,
        })
    }

    async fn get_stream_url(&self) -> Result<Url, IngestError> {
        match &self.input {
            crate::config::InputSource::Url(url) => {
                Url::parse(url).map_err(IngestError::InvalidUrl)
            }
            crate::config::InputSource::Channel(channel) => {
                self.get_channel_stream_url(channel).await
            }
        }
    }

    async fn get_channel_stream_url(&self, channel: &str) -> Result<Url, IngestError> {
        // Twitch Helix API endpoint for getting stream information
        let api_url = format!(
            "https://api.twitch.tv/helix/streams?user_login={}",
            channel
        );

        tracing::info!("Fetching stream info for channel: {}", channel);
        
        let mut request = self.client
            .get(&api_url)
            .header("Client-ID", &self._twitch_config.client_id);

        // Add OAuth token if available
        if let Some(token) = &self._twitch_config.oauth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Twitch API request failed: {}", e);
                IngestError::Http(e)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            tracing::error!("Twitch API error {}: {}", status, error_text);
            // Use our new HttpStatus error variant
            return Err(IngestError::HttpStatus(status.as_u16(), error_text));
        }

        let stream_data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| {
                tracing::error!("Failed to parse Twitch API response: {}", e);
                IngestError::Http(e)
            })?;

        tracing::debug!("Twitch API response: {:?}", stream_data);

        // Extract stream information from the response
        let streams = stream_data["data"]
            .as_array()
            .ok_or_else(|| {
                tracing::error!("Twitch API response missing data field");
                IngestError::TwitchGqlMissingFields
            })?;

        if streams.is_empty() {
            tracing::warn!("Channel '{}' is not live or not found", channel);
            return Err(IngestError::HttpStatus(404, format!("Channel '{}' is not live", channel)));
        }

        // For now, we'll use a placeholder approach since getting actual HLS URLs
        // requires additional API calls or using the Twitch GQL API
        // In production, you would:
        // 1. Use the stream information to get actual HLS URLs
        // 2. Handle different quality variants
        // 3. Implement proper error handling for offline streams
        
        // Extract the user ID from the response
        let user_data = &streams[0];
        let _user_id = user_data["user_id"]
            .as_str()
            .ok_or_else(|| {
                tracing::error!("Twitch API response missing user_id field");
                IngestError::TwitchGqlMissingFields
            })?;

        // Get the stream access token via Twitch GQL API
        let (token, sig) = self.get_stream_access_token(channel).await?;
        
        // Construct the HLS URL with the actual token and signature
        let hls_url = format!(
            "https://usher.ttvnw.net/api/channel/hls/{}.m3u8?client_id={}&token={}&sig={}&allow_audio_only=true&allow_source=true&type=any&p={}", 
            channel, 
            &self._twitch_config.client_id,
            urlencoding::encode(&token),
            urlencoding::encode(&sig),
            rand::random::<u32>()
        );
        
        tracing::info!("Constructed HLS URL for channel '{}'", channel);
        Url::parse(&hls_url).map_err(IngestError::InvalidUrl)
    }

    async fn get_stream_access_token(&self, channel: &str) -> Result<(String, String), IngestError> {
        // Twitch GQL API endpoint
        let gql_url = "https://gql.twitch.tv/gql";
        
        // GraphQL query to get playback access token
        let query = serde_json::json!({
            "query": "query PlaybackAccessToken($login: String!) { streamPlaybackAccessToken(channelName: $login, params: {platform: \"web\", playerType: \"site\"}) { value signature } }",
            "variables": {
                "login": channel
            }
        });

        // Use the standard Twitch web client ID for GQL API
        // This is the client ID used by Twitch's web interface
        let gql_client_id = "kimne78kx3ncx6brgo4mv6wki5h1ko";
        
        let mut request = self.client
            .post(gql_url)
            .header("Client-ID", gql_client_id)
            .header("Content-Type", "application/json");

        // Add OAuth token if available (required for private/age-restricted streams)
        // Note: For public streams, no Authorization header is needed
        if let Some(token) = &self._twitch_config.oauth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .json(&query)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Twitch GQL API request failed: {}", e);
                IngestError::Http(e)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            tracing::error!("Twitch GQL API error {}: {}", status, error_text);
            return Err(IngestError::HttpStatus(status.as_u16(), error_text));
        }

        let gql_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| {
                tracing::error!("Failed to parse Twitch GQL response: {}", e);
                IngestError::Http(e)
            })?;

        tracing::debug!("Twitch GQL response: {:?}", gql_response);
        
        // Check for errors in the response
        if let Some(errors) = gql_response.get("errors") {
            tracing::error!("Twitch GQL API returned errors: {:?}", errors);
            return Err(IngestError::TwitchGqlMissingFields);
        }

        // Extract token and signature from response
        let data = gql_response["data"]
            .as_object()
            .ok_or_else(|| {
                tracing::error!("Twitch GQL response missing data field. Full response: {:?}", gql_response);
                IngestError::TwitchGqlMissingFields
            })?;

        let stream_token = data["streamPlaybackAccessToken"]
            .as_object()
            .ok_or_else(|| {
                tracing::error!("Twitch GQL response missing streamPlaybackAccessToken");
                IngestError::TwitchGqlMissingFields
            })?;

        let token = stream_token["value"]
            .as_str()
            .ok_or_else(|| {
                tracing::error!("Twitch GQL response missing token value");
                IngestError::TwitchGqlMissingFields
            })?
            .to_string();

        let sig = stream_token["signature"]
            .as_str()
            .ok_or_else(|| {
                tracing::error!("Twitch GQL response missing signature");
                IngestError::TwitchGqlMissingFields
            })?
            .to_string();

        tracing::info!("Successfully obtained stream access token for channel '{}'", channel);
        Ok((token, sig))
    }

    async fn fetch_playlist(&self, url: &Url) -> Result<String, IngestError> {
        let response = self.client
            .get(url.as_str())
            .send()
            .await
            .map_err(IngestError::Http)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(IngestError::HttpStatus(status.as_u16(), error_text));
        }

        response
            .text()
            .await
            .map_err(IngestError::Http)
    }

    async fn fetch_media_segment(&self, url: &Url) -> Result<Bytes, IngestError> {
        let response = self.client
            .get(url.as_str())
            .send()
            .await
            .map_err(IngestError::Http)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(IngestError::HttpStatus(status.as_u16(), error_text));
        }

        response
            .bytes()
            .await
            .map_err(IngestError::Http)
    }

    async fn process_playlist(&self, playlist_url: Url, tx: Sender<IngestItem>) -> Result<(), IngestError> {
        let mut sequence = 0u64;
        let mut last_segment_url: Option<Url> = None;
        let mut target_duration;
        let mut media_playlist_url = playlist_url.clone();

        // If we get a master playlist, extract the media playlist URL
        let initial_content = self.fetch_playlist(&media_playlist_url).await?;
        let (_remaining, initial_parsed) = m3u8_rs::parse_playlist(&initial_content.as_bytes())
            .map_err(|e| {
                tracing::error!("HLS initial parse error: {:?}", e);
                tracing::debug!("Initial playlist content: {}", initial_content);
                IngestError::HlsParse
            })?;

        // Handle master playlist by selecting the appropriate variant
        if let Playlist::MasterPlaylist(master) = initial_parsed {
            tracing::info!("Received master playlist with {} variants", master.variants.len());
            
            // Select variant based on audio_only option
            let selected_variant = if self.options.audio_only {
                // Try to find audio-only variant first
                master.variants.iter()
                    .find(|v| v.audio.is_some() || v.codecs.as_ref().map(|c| c.contains("mp4a")).unwrap_or(false))
                    .or_else(|| master.variants.first())
            } else {
                // Select first variant (usually highest quality)
                master.variants.first()
            };

            let variant = selected_variant.ok_or_else(|| {
                tracing::error!("No variants found in master playlist");
                IngestError::HlsParse
            })?;

            media_playlist_url = playlist_url.join(&variant.uri)
                .map_err(IngestError::InvalidUrl)?;
            
            tracing::info!("Selected variant: {} (codecs: {:?})", variant.uri, variant.codecs);
        }

        loop {
            let playlist_content = self.fetch_playlist(&media_playlist_url).await?;
            
            // Parse the HLS playlist
            let (_remaining, parsed) = m3u8_rs::parse_playlist(&playlist_content.as_bytes())
                .map_err(|e| {
                    tracing::error!("HLS parse error: {:?}", e);
                    tracing::debug!("Playlist content: {}", playlist_content);
                    IngestError::HlsParse
                })?;

            match parsed {
                Playlist::MasterPlaylist(_) => {
                    tracing::error!("Received master playlist when expecting media playlist");
                    return Err(IngestError::ExpectedMediaPlaylist);
                }
                Playlist::MediaPlaylist(playlist) => {
                    // Set target duration
                    target_duration = Duration::from_secs(playlist.target_duration);

                    for segment in &playlist.segments {
                        let segment_url = playlist_url
                            .join(&segment.uri)
                            .map_err(IngestError::InvalidUrl)?;

                        // Skip if we've already processed this segment
                        if last_segment_url.as_ref() == Some(&segment_url) {
                            continue;
                        }

                        // Fetch the media segment
                        tracing::debug!("Fetching segment: {}", segment_url);
                        let bytes = self.fetch_media_segment(&segment_url).await?;
                        tracing::debug!("Fetched segment: {} bytes from {}", bytes.len(), segment_url);

                        let ingest_item = IngestItem {
                            sequence,
                            fetched_at: SystemTime::now(),
                            url: segment_url.clone(),
                            approx_duration: Duration::from_secs_f64(segment.duration as f64),
                            bytes,
                        };

                        if tx.send(ingest_item).await.is_err() {
                            return Err(IngestError::NotImplemented);
                        }

                        sequence += 1;
                        last_segment_url = Some(segment_url);
                    }
                }
            }

            // Wait for the target duration before checking for new segments
            tokio::time::sleep(target_duration).await;
        }
    }
}

impl Ingestor for TwitchHlsIngestor {
    fn start(
        &self,
        tx: Sender<IngestItem>,
    ) -> Pin<Box<dyn Future<Output = Result<(), IngestError>> + Send + 'static>> {
        let this = self.clone();
        Box::pin(async move {
            tracing::info!(
                "Starting Twitch HLS ingestor for {:?} (audio_only: {})",
                this.input,
                this.options.audio_only
            );

            let stream_url = this.get_stream_url().await?;
            tracing::info!("Using stream URL: {}", stream_url);

            this.process_playlist(stream_url, tx).await
        })
    }
}