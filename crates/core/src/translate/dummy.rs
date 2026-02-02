use crate::config::TargetLang;
use crate::translate::{TranslateError, Translation, Translator};
use futures::future::BoxFuture;
use futures::FutureExt;

#[derive(Clone)]
pub struct DummyTranslator;

impl DummyTranslator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DummyTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for DummyTranslator {
    fn translate(
        &self,
        text: String,
        _target: TargetLang,
    ) -> BoxFuture<'_, Result<Translation, TranslateError>> {
        async move {
            // For a dummy implementation, we'll just return the same text
            Ok(Translation {
                text,
                detected_source_lang: Some("en".to_string()),
            })
        }
        .boxed()
    }
}
