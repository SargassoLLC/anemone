//! Configuration — YAML config + env var overrides. 1:1 with Python config.py.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
}
