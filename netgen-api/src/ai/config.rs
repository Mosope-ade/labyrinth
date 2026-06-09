use std::env;
use thiserror::Error;

/// Which AI provider to use. Controls the default base URL.
/// Switch providers by changing `AI_PROVIDER` + `AI_API_KEY` + `AI_MODEL` — no code change.
#[derive(Debug, Clone, PartialEq)]
pub enum AiProvider {
    /// Google Gemini — OpenAI-compatible endpoint.
    Gemini,
    /// Anthropic Claude — OpenAI-compatible endpoint.
    Anthropic,
    /// OpenAI or any OpenAI-compatible host (Groq, Together, Mistral, Ollama…).
    OpenAi,
    /// Fully custom — `AI_BASE_URL` is required.
    Custom,
}

impl AiProvider {
    /// Default base URL for the provider's chat-completions endpoint.
    pub fn default_base_url(&self) -> &'static str {
        match self {
            AiProvider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            AiProvider::Anthropic => "https://api.anthropic.com/v1",
            AiProvider::OpenAi => "https://api.openai.com/v1",
            AiProvider::Custom => "",
        }
    }

    /// Default model name when `AI_MODEL` is not set.
    pub fn default_model(&self) -> &'static str {
        match self {
            AiProvider::Gemini => "gemini-2.0-flash",
            AiProvider::Anthropic => "claude-sonnet-4-6",
            AiProvider::OpenAi => "gpt-4o",
            AiProvider::Custom => "default",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "gemini" | "google" => AiProvider::Gemini,
            "anthropic" | "claude" => AiProvider::Anthropic,
            "openai" | "gpt" => AiProvider::OpenAi,
            _ => AiProvider::Custom,
        }
    }
}

#[derive(Clone)]
pub struct AiConfig {
    pub provider: AiProvider,
    /// Bearer token — private to prevent accidental logging or serialization.
    api_key: String,
    /// Model identifier, e.g. `gemini-2.0-flash`, `claude-sonnet-4-6`, `gpt-4o`.
    pub model: String,
    /// Full base URL up to (but not including) `/chat/completions`.
    pub base_url: String,
    pub max_tokens: u32,
}

impl std::fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiConfig")
            .field("provider", &self.provider)
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl AiConfig {
    /// Loads configuration from environment variables.
    ///
    /// | Variable       | Required | Default                        |
    /// |----------------|----------|--------------------------------|
    /// | `AI_API_KEY`   | yes      | —                              |
    /// | `AI_PROVIDER`  | no       | `gemini`                       |
    /// | `AI_MODEL`     | no       | provider default               |
    /// | `AI_BASE_URL`  | no*      | provider default (*required for `custom`) |
    /// | `AI_MAX_TOKENS`| no       | `4096`                         |
    pub fn from_env() -> Result<Self, ConfigError> {
        let api_key = env::var("AI_API_KEY")
            .map_err(|_| ConfigError::Missing("AI_API_KEY"))?;

        let provider = env::var("AI_PROVIDER")
            .map(|s| AiProvider::from_str(&s))
            .unwrap_or(AiProvider::Gemini);

        let model = env::var("AI_MODEL")
            .unwrap_or_else(|_| provider.default_model().to_string());

        let base_url = env::var("AI_BASE_URL")
            .unwrap_or_else(|_| provider.default_base_url().to_string());

        if base_url.is_empty() {
            return Err(ConfigError::Missing(
                "AI_BASE_URL (required when AI_PROVIDER=custom)",
            ));
        }

        let max_tokens = env::var("AI_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4096);

        Ok(AiConfig { provider, api_key, model, base_url, max_tokens })
    }

    /// Constructs a config directly — useful for tests and programmatic setup.
    pub fn new(
        api_key: impl Into<String>,
        provider: AiProvider,
        model: impl Into<String>,
    ) -> Self {
        let provider_clone = provider.clone();
        AiConfig {
            base_url: provider_clone.default_base_url().to_string(),
            provider,
            api_key: api_key.into(),
            model: model.into(),
            max_tokens: 4096,
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = max;
        self
    }

    /// Returns the API key. Private field — use only inside the AI client.
    pub(crate) fn api_key(&self) -> &str {
        &self.api_key
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    Missing(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_default_urls_are_nonempty_except_custom() {
        assert!(!AiProvider::Gemini.default_base_url().is_empty());
        assert!(!AiProvider::Anthropic.default_base_url().is_empty());
        assert!(!AiProvider::OpenAi.default_base_url().is_empty());
        assert!(AiProvider::Custom.default_base_url().is_empty());
    }

    #[test]
    fn provider_from_str_parses_known_names() {
        assert_eq!(AiProvider::from_str("gemini"), AiProvider::Gemini);
        assert_eq!(AiProvider::from_str("GEMINI"), AiProvider::Gemini);
        assert_eq!(AiProvider::from_str("google"), AiProvider::Gemini);
        assert_eq!(AiProvider::from_str("anthropic"), AiProvider::Anthropic);
        assert_eq!(AiProvider::from_str("claude"), AiProvider::Anthropic);
        assert_eq!(AiProvider::from_str("openai"), AiProvider::OpenAi);
        assert_eq!(AiProvider::from_str("gpt"), AiProvider::OpenAi);
        assert_eq!(AiProvider::from_str("ollama"), AiProvider::Custom);
    }

    #[test]
    fn provider_default_models_are_nonempty() {
        for p in [AiProvider::Gemini, AiProvider::Anthropic, AiProvider::OpenAi] {
            assert!(!p.default_model().is_empty());
        }
    }

    #[test]
    fn ai_config_new_sets_provider_default_url() {
        let cfg = AiConfig::new("key", AiProvider::Gemini, "gemini-2.0-flash");
        assert!(cfg.base_url.contains("googleapis.com"));
        assert_eq!(cfg.max_tokens, 4096);
        assert_eq!(cfg.api_key(), "key");
    }

    #[test]
    fn ai_config_with_base_url_overrides_default() {
        let cfg = AiConfig::new("key", AiProvider::Gemini, "gemini-2.0-flash")
            .with_base_url("http://localhost:11434/v1");
        assert_eq!(cfg.base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn ai_config_with_max_tokens_overrides_default() {
        let cfg = AiConfig::new("key", AiProvider::OpenAi, "gpt-4o")
            .with_max_tokens(2048);
        assert_eq!(cfg.max_tokens, 2048);
    }

    #[test]
    fn config_error_from_env_missing_key() {
        // Temporarily unset the key if set; use a separate env namespace check
        // We can't rely on env state in tests, so just verify the error type displays correctly
        let err = ConfigError::Missing("AI_API_KEY");
        assert!(err.to_string().contains("AI_API_KEY"));
    }
}
