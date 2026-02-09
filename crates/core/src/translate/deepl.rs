use crate::config::TargetLang;
use crate::translate::{TranslateError, Translation, Translator};
use crate::util::{is_http_retryable, retry_with_backoff, RetryConfig};
use futures::future::BoxFuture;
use futures::FutureExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct DeepLTranslator {
    client: Client,
    api_key: String,
}

impl DeepLTranslator {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

#[derive(Serialize, Clone)]
struct DeepLRequest {
    text: Vec<String>,
    target_lang: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_lang: Option<String>,
}

#[derive(Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Deserialize)]
struct DeepLTranslation {
    detected_source_language: String,
    text: String,
}

impl Translator for DeepLTranslator {
    fn translate(
        &self,
        text: String,
        target: TargetLang,
    ) -> BoxFuture<'_, Result<Translation, TranslateError>> {
        let this = self.clone();
        async move {
            // Prepare the request
            // For most language codes, we use uppercase, but some have special cases
            let target_lang = match target.as_str().to_lowercase().as_str() {
                "pt-br" => "pt-BR".to_string(),
                "pt-pt" => "pt-PT".to_string(),
                "en-gb" => "en-GB".to_string(),
                "en-us" => "en-US".to_string(),
                _ => target.as_str().to_uppercase(),
            };
            
            let request = DeepLRequest {
                text: vec![text],
                target_lang,
                source_lang: None, // Let DeepL detect the source language
            };

            // Build the URL
            let url = if this.api_key.ends_with(":fx") {
                "https://api-free.deepl.com/v2/translate"
            } else {
                "https://api.deepl.com/v2/translate"
            };

            // Configure retry with exponential backoff
            let retry_config = RetryConfig::default();
            
            // Perform the translation with retry logic
            retry_with_backoff(&retry_config, || {
                let client = this.client.clone();
                let api_key = this.api_key.clone();
                let request_body = request.clone();
                let url_str = url.to_string();
                
                async move {
                    // Send the request
                    let response = client
                        .post(&url_str)
                        .header("Authorization", format!("DeepL-Auth-Key {}", api_key))
                        .json(&request_body)
                        .send()
                        .await
                        .map_err(TranslateError::Network)?;

                    // Check if the request was successful
                    if !response.status().is_success() {
                        let status = response.status();
                        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                        
                        // Check if this error is retryable
                        if is_http_retryable(status.as_u16()) {
                            return Err(TranslateError::Api(format!("HTTP {}: {}", status, error_text)));
                        } else {
                            // Non-retryable error, return immediately
                            return Err(TranslateError::Api(format!("HTTP {}: {}", status, error_text)));
                        }
                    }

                    // Parse the response
                    let deepl_response: DeepLResponse = response
                        .json()
                        .await
                        .map_err(|e| TranslateError::InvalidResponse(format!("Failed to parse JSON: {}", e)))?;

                    // Extract the translation
                    let translation = deepl_response
                        .translations
                        .into_iter()
                        .next()
                        .ok_or_else(|| TranslateError::InvalidResponse("No translations in response".to_string()))?;

                    // Create the Translation object
                    Ok(Translation {
                        text: translation.text,
                        detected_source_lang: Some(translation.detected_source_language),
                    })
                }
            }, |error| {
                // Only retry on API errors with retryable HTTP status codes
                matches!(error, TranslateError::Api(_))
            }).await
        }
        .boxed()
    }
}
