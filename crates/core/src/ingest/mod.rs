use bytes::Bytes;
use std::{
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime},
};
use url::Url;

pub mod twitch;
pub use twitch::{TwitchHlsIngestor, TwitchIngestOptions};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IngestItem {
    pub sequence: u64,
    pub fetched_at: SystemTime,
    pub url: Url,
    pub approx_duration: Duration,
    pub bytes: Bytes,
}

#[derive(thiserror::Error, Debug)]
pub enum IngestError {
    #[error("ingest not implemented")]
    NotImplemented,

    #[error("invalid url: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("twitch gql response missing required fields")]
    TwitchGqlMissingFields,

    #[error("hls playlist parse error")]
    HlsParse,

    #[error("expected HLS master playlist")]
    ExpectedMasterPlaylist,

    #[error("expected HLS media playlist")]
    ExpectedMediaPlaylist,

    #[error("no usable variant found")]
    NoUsableVariant,

    #[error("http error {0}: {1}")]
    HttpStatus(u16, String),
}

pub trait Ingestor: Send + Sync {
    fn start(
        &self,
        tx: tokio::sync::mpsc::Sender<IngestItem>,
    ) -> Pin<Box<dyn Future<Output = Result<(), IngestError>> + Send + 'static>>;
}