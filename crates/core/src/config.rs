use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, SystemTime},
};

pub const DEFAULT_TARGET_LANG: &str = "pt-BR";
pub const DEFAULT_LATENCY_MS: u64 = 1500;
pub const DEFAULT_TWITCH_WEB_CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";
pub const ENV_DEEPL_API_KEY: &str = "DEEPL_API_KEY";
pub const ENV_ELEVENLABS_API_KEY: &str = "ELEVENLABS_API_KEY";
pub const ENV_TWITCH_CLIENT_ID: &str = "TWITCH_CLIENT_ID";
pub const ENV_TWITCH_OAUTH_TOKEN: &str = "TWITCH_OAUTH_TOKEN";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InputSource {
    Channel(String),
    Url(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetLang(pub String);

impl TargetLang {
    pub fn new<S: Into<String>>(value: S) -> Result<Self, ConfigError> {
        let v = value.into();
        if v.trim().is_empty() {
            return Err(ConfigError::EmptyTargetLang);
        }
        Ok(Self(v))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TargetLang {
    fn default() -> Self {
        Self(DEFAULT_TARGET_LANG.to_owned())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new<S: Into<String>>(value: S) -> Result<Self, ConfigError> {
        let v = value.into();
        if v.trim().is_empty() {
            return Err(ConfigError::EmptyApiKey);
        }
        Ok(Self(v))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(**redacted**)")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeys {
    pub deepl: Option<ApiKey>,
    pub elevenlabs: Option<ApiKey>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatencyBudget {
    pub target_ms: u64,
}

impl LatencyBudget {
    pub fn new(target_ms: u64) -> Result<Self, ConfigError> {
        if target_ms == 0 {
            return Err(ConfigError::ZeroLatency);
        }
        Ok(Self { target_ms })
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.target_ms)
    }

    pub fn frames_for_sample_rate(&self, sample_rate_hz: u32) -> u64 {
        let sr = u64::from(sample_rate_hz);
        (self.target_ms.saturating_mul(sr)).saturating_div(1000)
    }
}

impl Default for LatencyBudget {
    fn default() -> Self {
        Self {
            target_ms: DEFAULT_LATENCY_MS,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub input: InputSource,
    pub target_lang: TargetLang,
    pub api_keys: ApiKeys,
    pub latency: LatencyBudget,
    pub twitch: TwitchConfig,
    pub start_time: SystemTime,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TwitchConfig {
    pub client_id: String,
    pub oauth_token: Option<String>,
    pub hls_audio_only: bool,
}

impl Default for TwitchConfig {
    fn default() -> Self {
        Self {
            client_id: DEFAULT_TWITCH_WEB_CLIENT_ID.to_owned(),
            oauth_token: None,
            hls_audio_only: true,
        }
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("target language must not be empty")]
    EmptyTargetLang,
    #[error("api key must not be empty")]
    EmptyApiKey,
    #[error("latency must be > 0 ms")]
    ZeroLatency,
}

pub trait Env {
    fn var(&self, key: &str) -> Option<String>;
}

#[derive(Clone, Debug, Default)]
pub struct StdEnv;

impl Env for StdEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

#[derive(Clone, Debug, Default)]
pub struct MapEnv {
    vars: std::collections::BTreeMap<String, String>,
}

impl MapEnv {
    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_owned(), value.to_owned());
        self
    }
}

impl Env for MapEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
}

pub fn resolve_api_key(
    cli_value: Option<String>,
    env_key: &str,
    env: &impl Env,
) -> Result<Option<ApiKey>, ConfigError> {
    match cli_value {
        Some(v) => Ok(Some(ApiKey::new(v)?)),
        None => match env.var(env_key) {
            Some(v) => Ok(Some(ApiKey::new(v)?)),
            None => Ok(None),
        },
    }
}

pub fn resolve_string_with_default(
    cli_value: Option<String>,
    env_key: &str,
    env: &impl Env,
    default: &str,
) -> String {
    match cli_value {
        Some(v) => v,
        None => env.var(env_key).unwrap_or_else(|| default.to_owned()),
    }
}

pub fn resolve_optional_string(
    cli_value: Option<String>,
    env_key: &str,
    env: &impl Env,
) -> Option<String> {
    match cli_value {
        Some(v) => Some(v),
        None => env.var(env_key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_cli_takes_precedence_over_env() {
        let env = MapEnv::default().with_var(ENV_DEEPL_API_KEY, "env-key");
        let key = resolve_api_key(Some("cli-key".to_owned()), ENV_DEEPL_API_KEY, &env)
            .expect("valid key")
            .expect("present");
        assert_eq!(key.expose(), "cli-key");
    }

    #[test]
    fn api_key_env_used_when_cli_missing() {
        let env = MapEnv::default().with_var(ENV_DEEPL_API_KEY, "env-key");
        let key = resolve_api_key(None, ENV_DEEPL_API_KEY, &env)
            .expect("valid key")
            .expect("present");
        assert_eq!(key.expose(), "env-key");
    }

    #[test]
    fn latency_budget_frames_simple() {
        let b = LatencyBudget::new(1500).expect("nonzero");
        assert_eq!(b.frames_for_sample_rate(48_000), 72_000);
        assert_eq!(b.frames_for_sample_rate(16_000), 24_000);
    }

    #[test]
    fn resolve_string_with_default_cli_takes_precedence() {
        let env = MapEnv::default().with_var(ENV_TWITCH_CLIENT_ID, "env");
        let v =
            resolve_string_with_default(Some("cli".to_owned()), ENV_TWITCH_CLIENT_ID, &env, "def");
        assert_eq!(v, "cli");
    }

    #[test]
    fn resolve_string_with_default_env_used_when_cli_missing() {
        let env = MapEnv::default().with_var(ENV_TWITCH_CLIENT_ID, "env");
        let v = resolve_string_with_default(None, ENV_TWITCH_CLIENT_ID, &env, "def");
        assert_eq!(v, "env");
    }

    #[test]
    fn resolve_string_with_default_default_used_when_both_missing() {
        let env = MapEnv::default();
        let v = resolve_string_with_default(None, ENV_TWITCH_CLIENT_ID, &env, "def");
        assert_eq!(v, "def");
    }
}
