use bytes::Bytes;
use m3u8_rs::{AlternativeMediaType, MasterPlaylist, MediaPlaylist, Playlist};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, Mutex, Notify};
use url::Url;

use crate::config::{InputSource, TwitchConfig};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestPacket {
    pub received_at: SystemTime,
    pub approx_duration: Duration,
    pub bytes: Vec<u8>,
}

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
}

pub trait Ingestor: Send + Sync {
    fn start(
        &self,
        tx: tokio::sync::mpsc::Sender<IngestPacket>,
    ) -> Pin<Box<dyn Future<Output = Result<(), IngestError>> + Send + 'static>>;
}

#[derive(Clone, Debug)]
pub struct TwitchIngestOptions {
    pub jitter_buffer_segments: usize,
    pub initial_backlog_segments: usize,
    pub min_poll_interval: Duration,
    pub max_poll_interval: Duration,
}

impl Default for TwitchIngestOptions {
    fn default() -> Self {
        Self {
            jitter_buffer_segments: 8,
            initial_backlog_segments: 1,
            min_poll_interval: Duration::from_millis(200),
            max_poll_interval: Duration::from_secs(2),
        }
    }
}

#[derive(Clone)]
pub struct TwitchHlsIngestor {
    client: reqwest::Client,
    twitch: TwitchConfig,
    input: InputSource,
    opts: TwitchIngestOptions,
}

impl TwitchHlsIngestor {
    pub fn new(
        twitch: TwitchConfig,
        input: InputSource,
        opts: TwitchIngestOptions,
    ) -> Result<Self, IngestError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122 Safari/537.36")
            .build()?;

        Ok(Self {
            client,
            twitch,
            input,
            opts,
        })
    }

    pub async fn run(self) -> Result<mpsc::Receiver<IngestItem>, IngestError> {
        let locator = TwitchStreamLocator::new(self.client.clone(), self.twitch.clone());
        let master_url = locator.resolve_master_url(&self.input).await?;

        let (playlist_url, playlist_bytes) =
            fetch_text_bytes(&self.client, master_url.clone()).await?;
        let media_url = HlsVariantSelector::new(self.opts.clone(), self.twitch.hls_audio_only)
            .select_media_url(playlist_url, &playlist_bytes)?;

        let (tx, rx) = mpsc::channel::<IngestItem>(self.opts.jitter_buffer_segments);
        let shutdown = Arc::new(AtomicBool::new(false));
        let buf = Arc::new(JitterBuffer::<SegmentInfo>::new(
            self.opts.jitter_buffer_segments,
        ));

        {
            let client = self.client.clone();
            let buf = Arc::clone(&buf);
            let shutdown = Arc::clone(&shutdown);
            let opts = self.opts.clone();
            tokio::spawn(async move {
                let mut poller = MediaPlaylistPoller::new(client, media_url, opts);
                loop {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    match poller.poll_once().await {
                        Ok(segments) => {
                            for s in segments {
                                buf.push_drop_oldest(s).await;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "media playlist poll failed");
                        }
                    }
                    poller.sleep_until_next().await;
                }
            });
        }

        {
            let client = self.client.clone();
            let buf = Arc::clone(&buf);
            let shutdown = Arc::clone(&shutdown);
            tokio::spawn(async move {
                let fetcher = SegmentFetcher::new(client);
                while !shutdown.load(Ordering::Relaxed) {
                    let Some(seg) = buf.pop().await else {
                        continue;
                    };

                    match fetcher.fetch(seg).await {
                        Ok(item) => {
                            if tx.send(item).await.is_err() {
                                shutdown.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "segment fetch failed");
                        }
                    }
                }
            });
        }

        Ok(rx)
    }
}

impl Ingestor for TwitchHlsIngestor {
    fn start(
        &self,
        tx: tokio::sync::mpsc::Sender<IngestPacket>,
    ) -> Pin<Box<dyn Future<Output = Result<(), IngestError>> + Send + 'static>> {
        let this = self.clone();
        Box::pin(async move {
            // Convert IngestItem stream to IngestPacket stream
            let mut rx = this.run().await?;
            while let Some(item) = rx.recv().await {
                let packet = IngestPacket {
                    received_at: item.fetched_at,
                    approx_duration: item.approx_duration,
                    bytes: item.bytes.to_vec(),
                };
                if tx.send(packet).await.is_err() {
                    break;
                }
            }
            Ok(())
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SegmentInfo {
    sequence: u64,
    url: Url,
    approx_duration: Duration,
}

struct SegmentFetcher {
    client: reqwest::Client,
}

impl SegmentFetcher {
    fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    async fn fetch(&self, seg: SegmentInfo) -> Result<IngestItem, IngestError> {
        let fetched_at = SystemTime::now();
        let resp = self
            .client
            .get(seg.url.clone())
            .send()
            .await?
            .error_for_status()?;
        let bytes = resp.bytes().await?;
        Ok(IngestItem {
            sequence: seg.sequence,
            fetched_at,
            url: seg.url,
            approx_duration: seg.approx_duration,
            bytes,
        })
    }
}

struct JitterBuffer<T> {
    cap: usize,
    inner: Mutex<VecDeque<T>>,
    notify: Notify,
}

impl<T> JitterBuffer<T> {
    fn new(cap: usize) -> Self {
        assert!(cap > 0, "cap must be > 0");
        Self {
            cap,
            inner: Mutex::new(VecDeque::with_capacity(cap)),
            notify: Notify::new(),
        }
    }

    async fn push_drop_oldest(&self, item: T) {
        let mut g = self.inner.lock().await;
        g.push_back(item);
        while g.len() > self.cap {
            let _ = g.pop_front();
        }
        drop(g);
        self.notify.notify_one();
    }

    async fn pop(&self) -> Option<T> {
        loop {
            {
                let mut g = self.inner.lock().await;
                if let Some(v) = g.pop_front() {
                    return Some(v);
                }
            }
            self.notify.notified().await;
        }
    }
}

struct MediaPlaylistPoller {
    client: reqwest::Client,
    url: Url,
    opts: TwitchIngestOptions,
    state: MediaPlaylistState,
    next_sleep: Duration,
}

#[derive(Clone, Debug)]
struct MediaPlaylistState {
    next_sequence: Option<u64>,
    initial_backlog_segments: usize,
}

impl MediaPlaylistPoller {
    fn new(client: reqwest::Client, url: Url, opts: TwitchIngestOptions) -> Self {
        Self {
            client,
            url,
            state: MediaPlaylistState {
                next_sequence: None,
                initial_backlog_segments: opts.initial_backlog_segments,
            },
            next_sleep: opts.min_poll_interval,
            opts,
        }
    }

    async fn poll_once(&mut self) -> Result<Vec<SegmentInfo>, IngestError> {
        let (base, bytes) = fetch_text_bytes(&self.client, self.url.clone()).await?;
        let playlist = parse_playlist(&bytes)?;
        let Playlist::MediaPlaylist(mp) = playlist else {
            return Err(IngestError::ExpectedMediaPlaylist);
        };

        self.next_sleep = compute_poll_interval(
            &mp,
            self.opts.min_poll_interval,
            self.opts.max_poll_interval,
        );
        self.state.extract_new_segments(&mp, &base)
    }

    async fn sleep_until_next(&self) {
        tokio::time::sleep(self.next_sleep).await;
    }
}

impl MediaPlaylistState {
    fn extract_new_segments(
        &mut self,
        mp: &MediaPlaylist,
        base: &Url,
    ) -> Result<Vec<SegmentInfo>, IngestError> {
        let seq0 = mp.media_sequence;
        let n = mp.segments.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        match self.next_sequence {
            None => {
                let backlog = self.initial_backlog_segments.max(1);
                let start_index = n.saturating_sub(backlog);
                self.next_sequence = Some(seq0 + u64::try_from(start_index).unwrap_or(0));
            }
            Some(next) => {
                if next < seq0 {
                    self.next_sequence = Some(seq0);
                }
            }
        }

        let next = self.next_sequence.expect("set above");
        let mut out = Vec::new();
        for (i, seg) in mp.segments.iter().enumerate() {
            let seq = seq0 + u64::try_from(i).unwrap_or(0);
            if seq < next {
                continue;
            }
            let url = base.join(seg.uri.as_str())?;
            let ms = (f64::from(seg.duration).max(0.0) * 1000.0).round() as u64;
            out.push(SegmentInfo {
                sequence: seq,
                url,
                approx_duration: Duration::from_millis(ms),
            });
        }
        if let Some(last) = out.last() {
            self.next_sequence = Some(last.sequence.saturating_add(1));
        }
        Ok(out)
    }
}

#[derive(Clone)]
struct HlsVariantSelector {
    audio_only: bool,
}

impl HlsVariantSelector {
    fn new(_opts: TwitchIngestOptions, audio_only: bool) -> Self {
        Self { audio_only }
    }

    fn select_media_url(&self, base_url: Url, bytes: &[u8]) -> Result<Url, IngestError> {
        let playlist = parse_playlist(bytes)?;
        match playlist {
            Playlist::MediaPlaylist(_) => Ok(base_url),
            Playlist::MasterPlaylist(mp) => self.select_from_master(base_url, &mp),
        }
    }

    fn select_from_master(&self, base_url: Url, mp: &MasterPlaylist) -> Result<Url, IngestError> {
        if self.audio_only {
            if let Some(u) = select_audio_only_from_master(mp) {
                return Ok(base_url.join(u.as_str())?);
            }
        }

        let mut best: Option<(&str, u64)> = None;
        for v in &mp.variants {
            let bw = v.average_bandwidth.unwrap_or(v.bandwidth);
            match best {
                None => best = Some((v.uri.as_str(), bw)),
                Some((_, best_bw)) if bw < best_bw => best = Some((v.uri.as_str(), bw)),
                _ => {}
            }
        }
        let Some((uri, _)) = best else {
            return Err(IngestError::NoUsableVariant);
        };
        Ok(base_url.join(uri)?)
    }
}

fn select_audio_only_from_master(mp: &MasterPlaylist) -> Option<String> {
    mp.alternatives
        .iter()
        .filter(|a| a.media_type == AlternativeMediaType::Audio)
        .filter_map(|a| a.uri.as_ref())
        .find(|u: &&String| u.as_str().contains("audio"))
        .cloned()
        .or_else(|| {
            mp.alternatives
                .iter()
                .filter(|a| a.media_type == AlternativeMediaType::Audio)
                .filter_map(|a| a.uri.as_ref())
                .next()
                .cloned()
        })
}

struct TwitchStreamLocator {
    client: reqwest::Client,
    twitch: TwitchConfig,
}

impl TwitchStreamLocator {
    fn new(client: reqwest::Client, twitch: TwitchConfig) -> Self {
        Self { client, twitch }
    }

    async fn resolve_master_url(&self, input: &InputSource) -> Result<Url, IngestError> {
        match input {
            InputSource::Channel(c) => self.usher_master_url_for_channel(c.as_str()).await,
            InputSource::Url(u) => {
                let parsed = parse_any_url(u.as_str())?;
                if let Some(ch) = extract_channel_from_twitch_url(&parsed) {
                    return self.usher_master_url_for_channel(ch.as_str()).await;
                }
                Ok(parsed)
            }
        }
    }

    async fn usher_master_url_for_channel(&self, channel: &str) -> Result<Url, IngestError> {
        let (token, sig) = self.fetch_playback_access_token(channel).await?;
        Ok(build_usher_master_url(
            channel,
            &token,
            &sig,
            self.twitch.hls_audio_only,
        ))
    }

    async fn fetch_playback_access_token(
        &self,
        channel: &str,
    ) -> Result<(String, String), IngestError> {
        let url = Url::parse("https://gql.twitch.tv/gql")?;
        let mut headers = HeaderMap::new();
        headers.insert(
            "Client-ID",
            HeaderValue::from_str(self.twitch.client_id.as_str())
                .map_err(|_| IngestError::TwitchGqlMissingFields)?,
        );
        if let Some(token) = &self.twitch.oauth_token {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(normalize_oauth_header(token).as_str())
                    .map_err(|_| IngestError::TwitchGqlMissingFields)?,
            );
        }

        let body = serde_json::json!({
            "operationName": "PlaybackAccessToken_Template",
            "variables": {
                "isLive": true,
                "login": channel,
                "isVod": false,
                "vodID": "",
                "playerType": "site"
            },
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "0828119ded94e3c6f6785b25a0f31a6b46c0c8e6d7f32cbb6fba58828a741b2e"
                }
            }
        });

        let resp = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let v: serde_json::Value = resp.json().await?;
        
        // Log the response for debugging
        tracing::debug!(response = ?v, "Twitch GQL response");
        
        let token = v
            .pointer("/data/streamPlaybackAccessToken/value")
            .and_then(|x| x.as_str())
            .map(|s| s.to_owned());
        let sig = v
            .pointer("/data/streamPlaybackAccessToken/signature")
            .and_then(|x| x.as_str())
            .map(|s| s.to_owned());

        match (token, sig) {
            (Some(t), Some(s)) => Ok((t, s)),
            _ => {
                tracing::error!(response = ?v, "Missing required fields in Twitch GQL response");
                Err(IngestError::TwitchGqlMissingFields)
            }
        }
    }
}

fn normalize_oauth_header(raw: &str) -> String {
    let s = raw.trim();
    if s.to_ascii_lowercase().starts_with("oauth ") || s.to_ascii_lowercase().starts_with("bearer ")
    {
        return s.to_owned();
    }
    let s = s.strip_prefix("oauth:").unwrap_or(s);
    format!("OAuth {s}")
}

fn build_usher_master_url(channel: &str, token: &str, sig: &str, allow_audio_only: bool) -> Url {
    let p = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
        % 1_000_000_000) as u64;

    let mut url = Url::parse(&format!(
        "https://usher.ttvnw.net/api/channel/hls/{channel}.m3u8"
    ))
    .expect("static base url");

    {
        let mut q = url.query_pairs_mut();
        q.append_pair("p", &p.to_string());
        q.append_pair("player", "twitchweb");
        q.append_pair("allow_source", "true");
        q.append_pair(
            "allow_audio_only",
            if allow_audio_only { "true" } else { "false" },
        );
        q.append_pair("fast_bread", "true");
        q.append_pair("sig", sig);
        q.append_pair("token", token);
    }
    url
}

fn extract_channel_from_twitch_url(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if !host.ends_with("twitch.tv") {
        return None;
    }
    let mut segs = url.path_segments()?;
    let first = segs.next()?.trim();
    if first.is_empty() {
        return None;
    }
    if first.eq_ignore_ascii_case("videos") {
        return None;
    }
    Some(first.to_owned())
}

fn parse_any_url(s: &str) -> Result<Url, IngestError> {
    if let Ok(u) = Url::parse(s) {
        return Ok(u);
    }
    Ok(Url::parse(&format!("https://{s}"))?)
}

fn parse_playlist(bytes: &[u8]) -> Result<Playlist, IngestError> {
    m3u8_rs::parse_playlist_res(bytes).map_err(|_| IngestError::HlsParse)
}

async fn fetch_text_bytes(
    client: &reqwest::Client,
    url: Url,
) -> Result<(Url, Vec<u8>), IngestError> {
    let resp = client.get(url.clone()).send().await?.error_for_status()?;
    let bytes = resp.bytes().await?;
    Ok((url, bytes.to_vec()))
}

fn compute_poll_interval(mp: &MediaPlaylist, min: Duration, max: Duration) -> Duration {
    let target = Duration::from_secs(mp.target_duration);
    let half =
        Duration::from_millis((target.as_millis() / 2).clamp(1, u128::from(u64::MAX)) as u64);
    half.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_selects_audio_only_when_present() {
        let m = r#"#EXTM3U
#EXT-X-VERSION:3
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="audio_only",DEFAULT=YES,AUTOSELECT=YES,URI="audio_only.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,AUDIO="audio"
video_360p30.m3u8
"#;
        let base = Url::parse("https://example.com/master.m3u8").unwrap();
        let sel = HlsVariantSelector::new(TwitchIngestOptions::default(), true);
        let u = sel.select_media_url(base, m.as_bytes()).unwrap();
        assert_eq!(u.as_str(), "https://example.com/audio_only.m3u8");
    }

    #[test]
    fn master_selects_lowest_bandwidth_when_audio_only_disabled() {
        let m = r#"#EXTM3U
#EXT-X-VERSION:3
#EXT-X-STREAM-INF:BANDWIDTH=3000000,RESOLUTION=1280x720
hi.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360
lo.m3u8
"#;
        let base = Url::parse("https://example.com/master.m3u8").unwrap();
        let sel = HlsVariantSelector::new(TwitchIngestOptions::default(), false);
        let u = sel.select_media_url(base, m.as_bytes()).unwrap();
        assert_eq!(u.as_str(), "https://example.com/lo.m3u8");
    }

    fn parse_media(s: &str) -> MediaPlaylist {
        let pl = parse_playlist(s.as_bytes()).unwrap();
        match pl {
            Playlist::MediaPlaylist(mp) => mp,
            _ => panic!("expected media"),
        }
    }

    #[test]
    fn media_poll_detects_new_segments_across_polls() {
        let p1 = r#"#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:2
#EXT-X-MEDIA-SEQUENCE:100
#EXTINF:2.0,
s100.ts
#EXTINF:2.0,
s101.ts
#EXTINF:2.0,
s102.ts
"#;
        let p2 = r#"#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:2
#EXT-X-MEDIA-SEQUENCE:101
#EXTINF:2.0,
s101.ts
#EXTINF:2.0,
s102.ts
#EXTINF:2.0,
s103.ts
"#;

        let base = Url::parse("https://example.com/live/index.m3u8").unwrap();
        let mut st = MediaPlaylistState {
            next_sequence: None,
            initial_backlog_segments: 1,
        };

        let mp1 = parse_media(p1);
        let segs1 = st.extract_new_segments(&mp1, &base).unwrap();
        assert_eq!(segs1.len(), 1);
        assert_eq!(segs1[0].sequence, 102);
        assert_eq!(segs1[0].url.as_str(), "https://example.com/live/s102.ts");

        let mp2 = parse_media(p2);
        let segs2 = st.extract_new_segments(&mp2, &base).unwrap();
        assert_eq!(segs2.len(), 1);
        assert_eq!(segs2[0].sequence, 103);
        assert_eq!(segs2[0].url.as_str(), "https://example.com/live/s103.ts");
    }

    #[test]
    fn media_poll_jumps_forward_if_too_far_behind() {
        let p1 = r#"#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:2
#EXT-X-MEDIA-SEQUENCE:10
#EXTINF:2.0,
a.ts
"#;
        let p2 = r#"#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:2
#EXT-X-MEDIA-SEQUENCE:50
#EXTINF:2.0,
b.ts
"#;
        let base = Url::parse("https://example.com/live/index.m3u8").unwrap();
        let mut st = MediaPlaylistState {
            next_sequence: Some(11),
            initial_backlog_segments: 1,
        };
        let mp1 = parse_media(p1);
        let _ = st.extract_new_segments(&mp1, &base).unwrap();
        let mp2 = parse_media(p2);
        let segs2 = st.extract_new_segments(&mp2, &base).unwrap();
        assert_eq!(segs2.len(), 1);
        assert_eq!(segs2[0].sequence, 50);
        assert_eq!(segs2[0].url.as_str(), "https://example.com/live/b.ts");
    }
}
