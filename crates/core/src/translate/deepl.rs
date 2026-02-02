use crate::config::TargetLang;
use crate::translate::{TranslateError, Translation, Translator};
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

#[derive(Serialize)]
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
            let request = DeepLRequest {
                text: vec![text],
                target_lang: target.as_str().to_uppercase(),
                source_lang: None, // Let DeepL detect the source language
            };

            // Build the URL
            let url = if this.api_key.ends_with(":fx") {
                "https://api-free.deepl.com/v2/translate"
            } else {
                "https://api.deepl.com/v2/translate"
            };

            // Send the request
            let response = this
                .client
                .post(url)
                .header("Authorization", format!("DeepL-Auth-Key {}", this.api_key))
                .json(&request)
                .send()
                .await
                .map_err(|_e| TranslateError::NotImplemented)?; // TODO: Better error handling

            // Check if the request was successful
            if !response.status().is_success() {
                return Err(TranslateError::NotImplemented); // TODO: Better error handling
            }

            // Parse the response
            let deepl_response: DeepLResponse = response
                .json()
                .await
                .map_err(|_e| TranslateError::NotImplemented)?; // TODO: Better error handling

            // Extract the translation
            let translation = deepl_response
                .translations
                .into_iter()
                .next()
                .ok_or(TranslateError::NotImplemented)?; // TODO: Better error handling

            // Create the Translation object
            let result = Translation {
                text: translation.text,
                detected_source_lang: Some(translation.detected_source_language),
            };

            Ok(result)
        }
        .boxed()
    }
}