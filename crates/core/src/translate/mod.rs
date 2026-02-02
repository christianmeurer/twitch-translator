mod deepl;
mod dummy;

use crate::config::TargetLang;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

pub use deepl::DeepLTranslator;
pub use dummy::DummyTranslator;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Translation {
    pub text: String,
    pub detected_source_lang: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum TranslateError {
    #[error("translation not implemented")]
    NotImplemented,
    
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    
    #[error("API error: {0}")]
    Api(String),
}

pub trait Translator: Send + Sync {
    fn translate(
        &self,
        text: String,
        target: TargetLang,
    ) -> BoxFuture<'_, Result<Translation, TranslateError>>;
}
