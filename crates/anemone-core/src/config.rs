//! Configuration — YAML config + env var overrides. 1:1 with Python config.py.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Result of an API key validation attempt.
#[derive(Debug, Clone)]
pub struct KeyValidation {
    /// Whether the key is valid and the endpoint responded successfully.
    pub valid: bool,
    /// Model name echoed back (or the model we sent).
    pub model: String,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u64,
    /// Error message if the key is invalid or the request failed.
    pub error: Option<String>,
}

/// Known provider presets
const PROVIDER_PRESETS: &[(&str, Option<&str>)] = &[
    ("openai", None),
    ("openrouter", Some("https://openrouter.ai/api/v1")),
];

/// Provider-specific API key env vars (checked before OPENAI_API_KEY fallback)
const PROVIDER_KEY_ENV_VARS: &[(&str, &str)] = &[("openrouter", "OPENROUTER_API_KEY")];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// "openai" | "openrouter" | "custom"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// LLM model name
    #[serde(default = "default_model")]
    pub model: String,

    /// API key (set here or via env var)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Base URL for Chat Completions API (auto-set for known providers)
    #[serde(default)]
    pub base_url: Option<String>,

    /// Ollama cloud API key for web search tools
    #[serde(default)]
    pub ollama_api_key: Option<String>,

    /// How often the anemone thinks (seconds between cycles)
    #[serde(default = "default_thinking_pace")]
    pub thinking_pace_seconds: u64,

    /// Rolling window of recent thoughts in context
    #[serde(default = "default_max_thoughts")]
    pub max_thoughts_in_context: usize,

    /// Max output tokens per LLM call
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,

    /// Max tool rounds per think cycle
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: usize,

    /// Accumulated importance before reflecting
    #[serde(default = "default_reflection_threshold")]
    pub reflection_threshold: f64,

    /// How many memories to retrieve per query
    #[serde(default = "default_memory_retrieval_count")]
    pub memory_retrieval_count: usize,

    /// Embedding model name
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    /// Exponential decay rate for recency scoring
    #[serde(default = "default_recency_decay_rate")]
    pub recency_decay_rate: f64,

    /// Environment path (auto-detected from *_box/ directories)
    #[serde(default)]
    pub environment_path: Option<String>,

    /// Resolved project root (set at load time, not serialized from YAML)
    #[serde(skip)]
    pub project_root: PathBuf,
}

fn default_provider() -> String {
    "openai".into()
}
fn default_model() -> String {
    "gpt-4.1".into()
}
fn default_thinking_pace() -> u64 {
    45
}
fn default_max_thoughts() -> usize {
    20
}
fn default_max_output_tokens() -> u32 {
    1000
}
fn default_max_tool_rounds() -> usize {
    15
}
fn default_reflection_threshold() -> f64 {
    50.0
}
fn default_memory_retrieval_count() -> usize {
    3
}
fn default_embedding_model() -> String {
    "text-embedding-3-small".into()
}
fn default_recency_decay_rate() -> f64 {
    0.995
}

impl Config {
    /// Load config from a YAML file with env var overrides.
    /// `config_path` is the path to config.yaml.
    pub fn load(config_path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config: {}", config_path.display()))?;

        let mut config: Config =
            serde_yaml::from_str(&content).context("Failed to parse config.yaml")?;

        // Resolve project root from config file location
        config.project_root = config_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
            .canonicalize()
            .unwrap_or_else(|_| {
                config_path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_path_buf()
            });

        // Provider (env var override)
        if let Ok(p) = std::env::var("ANEMONECLAW_PROVIDER") {
            config.provider = p;
        }

        // Base URL: env var > config > provider preset
        if let Ok(url) = std::env::var("ANEMONECLAW_BASE_URL") {
            config.base_url = Some(url);
        } else if config.base_url.is_none() {
            config.base_url = PROVIDER_PRESETS
                .iter()
                .find(|(p, _)| *p == config.provider)
                .and_then(|(_, url)| url.map(String::from));
        }

        // API key: provider-specific env var > OPENAI_API_KEY > config
        let provider_key_var = PROVIDER_KEY_ENV_VARS
            .iter()
            .find(|(p, _)| *p == config.provider)
            .map(|(_, var)| *var);

        if let Some(var) = provider_key_var {
            if let Ok(key) = std::env::var(var) {
                config.api_key = Some(key);
            }
        }
        if config.api_key.is_none() {
            if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                config.api_key = Some(key);
            }
        }

        // Model (env var override)
        if let Ok(m) = std::env::var("ANEMONECLAW_MODEL") {
            config.model = m;
        }

        // Ollama cloud web search key
        if let Ok(key) = std::env::var("OLLAMA_API_KEY") {
            config.ollama_api_key = Some(key);
        }

        // Validation
        if config.provider == "custom" && config.base_url.is_none() {
            anyhow::bail!(
                "Provider 'custom' requires base_url in config.yaml or ANEMONECLAW_BASE_URL env var"
            );
        }

        Ok(config)
    }

    /// Load config from the default location (project_root/config.yaml)
    pub fn load_from_dir(project_root: &Path) -> Result<Self> {
        let config_path = project_root.join("config.yaml");
        Self::load(&config_path)
    }

    /// Serialize this config to YAML and write it to `config_path`.
    ///
    /// The `project_root` field is skipped automatically (it has `#[serde(skip)]`).
    pub fn save(&self, config_path: &Path) -> Result<()> {
        let yaml = serde_yaml::to_string(self).context("Failed to serialize config to YAML")?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }
        std::fs::write(config_path, yaml)
            .with_context(|| format!("Failed to write config to: {}", config_path.display()))?;
        Ok(())
    }

    /// Make a minimal API call to verify the configured key works.
    ///
    /// Sends a single-token chat completion request and measures latency.
    /// Returns [`KeyValidation`] regardless of success or failure.
    pub async fn validate_key(&self) -> Result<KeyValidation> {
        let api_key = match &self.api_key {
            Some(k) if !k.is_empty() => k.clone(),
            _ => {
                return Ok(KeyValidation {
                    valid: false,
                    model: self.model.clone(),
                    latency_ms: 0,
                    error: Some("No API key configured".into()),
                });
            }
        };

        // Build the endpoint URL: base_url already includes the path prefix for
        // known providers (e.g. https://openrouter.ai/api/v1), so we just append
        // /chat/completions.  For openai the default is https://api.openai.com/v1.
        let base = self
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/');
        let url = format!("{}/chat/completions", base);

        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1
        });

        let client = reqwest::Client::new();
        let start = Instant::now();

        let response = client
            .post(&url)
            .bearer_auth(&api_key)
            .json(&body)
            .send()
            .await;

        let latency_ms = start.elapsed().as_millis() as u64;

        match response {
            Err(e) => Ok(KeyValidation {
                valid: false,
                model: self.model.clone(),
                latency_ms,
                error: Some(format!("Request failed: {}", e)),
            }),
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    // Try to pull the model name from the response body
                    let model = resp
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|v| v["model"].as_str().map(String::from))
                        .unwrap_or_else(|| self.model.clone());
                    Ok(KeyValidation {
                        valid: true,
                        model,
                        latency_ms,
                        error: None,
                    })
                } else {
                    let error_body = resp.text().await.unwrap_or_default();
                    Ok(KeyValidation {
                        valid: false,
                        model: self.model.clone(),
                        latency_ms,
                        error: Some(format!("HTTP {}: {}", status, error_body.trim())),
                    })
                }
            }
        }
    }

    /// Strip line-break characters and trim surrounding whitespace from a secret
    /// string.  Mirrors OpenClaw's `normalizeSecretInput` helper.
    ///
    /// Characters removed: `\r`, `\n`, Unicode line separator (U+2028),
    /// Unicode paragraph separator (U+2029).
    pub fn normalize_secret(input: &str) -> String {
        input
            .replace(['\r', '\n', '\u{2028}', '\u{2029}'], "")
            .trim()
            .to_string()
    }

    /// Resolve the environment path for a given box directory
    pub fn resolve_env_path(&self, box_path: &Path) -> PathBuf {
        if let Some(ref env) = self.environment_path {
            let p = Path::new(env);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.project_root.join(p)
            }
        } else {
            box_path.to_path_buf()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: default_model(),
            api_key: None,
            base_url: None,
            ollama_api_key: None,
            thinking_pace_seconds: default_thinking_pace(),
            max_thoughts_in_context: default_max_thoughts(),
            max_output_tokens: default_max_output_tokens(),
            max_tool_rounds: default_max_tool_rounds(),
            reflection_threshold: default_reflection_threshold(),
            memory_retrieval_count: default_memory_retrieval_count(),
            embedding_model: default_embedding_model(),
            recency_decay_rate: default_recency_decay_rate(),
            environment_path: None,
            project_root: PathBuf::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_config_defaults() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "provider: openai\nmodel: gpt-4.1").unwrap();

        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4.1");
        assert_eq!(config.thinking_pace_seconds, 45);
        assert_eq!(config.reflection_threshold, 50.0);
        assert_eq!(config.recency_decay_rate, 0.995);
    }

    #[test]
    fn test_load_config_custom_values() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            "provider: custom\nmodel: llama3\nbase_url: http://localhost:11434/v1\nthinking_pace_seconds: 10"
        )
        .unwrap();

        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.provider, "custom");
        assert_eq!(config.model, "llama3");
        assert_eq!(
            config.base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(config.thinking_pace_seconds, 10);
    }

    #[test]
    fn test_custom_without_base_url_fails() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "provider: custom\nmodel: llama3").unwrap();

        let result = Config::load(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_save_and_reload() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.yaml");

        // Build a config with non-default values to make sure round-trip works
        let mut original = Config::default();
        original.provider = "openrouter".into();
        original.model = "openai/gpt-4o".into();
        original.thinking_pace_seconds = 30;
        original.reflection_threshold = 75.0;
        // project_root must NOT appear in the saved YAML (it's #[serde(skip)])
        original.project_root = dir.path().to_path_buf();

        original.save(&config_path).expect("save should succeed");

        // The saved file must be valid YAML that load() can parse
        let reloaded = Config::load(&config_path).expect("reload should succeed");

        assert_eq!(reloaded.provider, "openrouter");
        assert_eq!(reloaded.model, "openai/gpt-4o");
        assert_eq!(reloaded.thinking_pace_seconds, 30);
        assert_eq!(reloaded.reflection_threshold, 75.0);

        // Verify project_root was NOT persisted — it should be re-resolved by load()
        let saved_yaml = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            !saved_yaml.contains("project_root"),
            "project_root must not appear in saved YAML"
        );
    }

    #[test]
    fn test_normalize_secret() {
        // Strips \r\n
        assert_eq!(Config::normalize_secret("sk-abc\r\n"), "sk-abc");
        // Strips bare \n
        assert_eq!(Config::normalize_secret("sk-abc\n"), "sk-abc");
        // Strips bare \r
        assert_eq!(Config::normalize_secret("sk-abc\r"), "sk-abc");
        // Strips Unicode line separator (U+2028)
        assert_eq!(Config::normalize_secret("sk-abc\u{2028}"), "sk-abc");
        // Strips Unicode paragraph separator (U+2029)
        assert_eq!(Config::normalize_secret("sk-abc\u{2029}"), "sk-abc");
        // Trims surrounding whitespace
        assert_eq!(Config::normalize_secret("  sk-abc  "), "sk-abc");
        // Embedded newlines removed, not just trimmed
        assert_eq!(Config::normalize_secret("sk\nabc"), "skabc");
        // Clean string unchanged
        assert_eq!(Config::normalize_secret("sk-abc123"), "sk-abc123");
        // Empty string
        assert_eq!(Config::normalize_secret(""), "");
    }
}
