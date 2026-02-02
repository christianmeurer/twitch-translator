#![deny(warnings)]

use anyhow::Context;
use clap::{ArgGroup, Parser};
use std::time::SystemTime;
use tracing_subscriber::EnvFilter;
use twitch_translator_core::asr::WhisperAsrBackend;
use twitch_translator_core::config::{
    resolve_api_key, resolve_optional_string, resolve_string_with_default, ApiKeys, AppConfig,
    InputSource, LatencyBudget, StdEnv, TargetLang, TwitchConfig, DEFAULT_LATENCY_MS,
    DEFAULT_TARGET_LANG, DEFAULT_TWITCH_WEB_CLIENT_ID, ENV_DEEPL_API_KEY, ENV_ELEVENLABS_API_KEY,
    ENV_TWITCH_CLIENT_ID, ENV_TWITCH_OAUTH_TOKEN,
};
use twitch_translator_core::decode::FfmpegAudioDecoder;
use twitch_translator_core::ingest::{TwitchHlsIngestor, TwitchIngestOptions};
use twitch_translator_core::pipeline::{Pipeline, PipelineConfig};
use twitch_translator_core::playback::AudioPlaybackSink;
use twitch_translator_core::translate::DeepLTranslator;
use twitch_translator_core::tts::ElevenLabsTtsClient;

#[derive(Parser, Debug)]
#[command(name = "twitch-translator")]
#[command(about = "Low-latency Twitch live translation (ASR->Translate->TTS)")]
#[command(group(
    ArgGroup::new("input")
        .required(true)
        .multiple(false)
        .args(["channel", "url"])
))]
struct Args {
    #[arg(long)]
    channel: Option<String>,

    #[arg(long)]
    url: Option<String>,

    #[arg(long, default_value = DEFAULT_TARGET_LANG)]
    target_lang: String,

    #[arg(long)]
    deepl_api_key: Option<String>,

    #[arg(long)]
    elevenlabs_api_key: Option<String>,

    #[arg(long, default_value_t = DEFAULT_LATENCY_MS)]
    latency_ms: u64,

    #[arg(long, env = ENV_TWITCH_CLIENT_ID, default_value = DEFAULT_TWITCH_WEB_CLIENT_ID)]
    twitch_client_id: String,

    #[arg(long, env = ENV_TWITCH_OAUTH_TOKEN)]
    twitch_oauth_token: Option<String>,

    #[arg(long, default_value_t = true)]
    hls_audio_only: bool,

    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_tracing(&args.log_level)?;

    let env = StdEnv;
    let cfg = build_config(args, &env)?;

    tracing::info!(
        target_lang = %cfg.target_lang.as_str(),
        latency_ms = cfg.latency.target_ms,
        "config loaded"
    );

    run_ingest(cfg).await?;

    Ok(())
}

async fn run_ingest(cfg: AppConfig) -> anyhow::Result<()> {
    // Create pipeline components
    let ingestor = TwitchHlsIngestor::new(
        cfg.twitch.clone(),
        cfg.input.clone(),
        TwitchIngestOptions::default(),
    )?;
    let decoder = FfmpegAudioDecoder::default();
    let asr = WhisperAsrBackend::new(&cfg.asr.model_path)?;
    let translator = if let Some(deepl_key) = cfg.api_keys.deepl.clone() {
        DeepLTranslator::new(deepl_key.expose().to_string())
    } else {
        // Fallback to dummy translator if no API key is provided
        return Err(anyhow::anyhow!("DeepL API key is required for translation"));
    };
    
    let tts = if let Some(elevenlabs_key) = cfg.api_keys.elevenlabs.clone() {
        ElevenLabsTtsClient::new(elevenlabs_key.expose().to_string())
    } else {
        // Fallback to basic TTS if no API key is provided
        return Err(anyhow::anyhow!("ElevenLabs API key is required for TTS"));
    };
    let playback = AudioPlaybackSink::new()?;

    // Create pipeline
    let pipeline_config = PipelineConfig::from_app(&cfg);
    let pipeline = Pipeline {
        ingest: ingestor,
        decode: decoder,
        asr,
        translate: translator,
        tts,
        playback,
        config: pipeline_config,
    };

    // Run pipeline
    pipeline.run().await?;

    Ok(())
}

fn init_tracing(level: &str) -> anyhow::Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(
            level
                .parse()
                .with_context(|| format!("invalid --log-level: {level}"))?,
        )
        .from_env_lossy();

    tracing_subscriber::fmt().with_env_filter(filter).init();
    Ok(())
}

fn build_config(
    args: Args,
    env: &impl twitch_translator_core::config::Env,
) -> anyhow::Result<AppConfig> {
    let input = match (args.channel, args.url) {
        (Some(c), None) => InputSource::Channel(c),
        (None, Some(u)) => InputSource::Url(u),
        _ => anyhow::bail!("exactly one of --channel or --url must be provided"),
    };

    let target_lang = TargetLang::new(args.target_lang)?;
    let latency = LatencyBudget::new(args.latency_ms)?;

    let deepl = resolve_api_key(args.deepl_api_key, ENV_DEEPL_API_KEY, env)?;
    let elevenlabs = resolve_api_key(args.elevenlabs_api_key, ENV_ELEVENLABS_API_KEY, env)?;

    let twitch = TwitchConfig {
        client_id: resolve_string_with_default(
            Some(args.twitch_client_id),
            ENV_TWITCH_CLIENT_ID,
            env,
            DEFAULT_TWITCH_WEB_CLIENT_ID,
        ),
        oauth_token: resolve_optional_string(args.twitch_oauth_token, ENV_TWITCH_OAUTH_TOKEN, env),
        hls_audio_only: args.hls_audio_only,
    };

    Ok(AppConfig {
        input,
        target_lang,
        api_keys: ApiKeys { deepl, elevenlabs },
        latency,
        twitch,
        asr: Default::default(),
        start_time: SystemTime::now(),
    })
}
