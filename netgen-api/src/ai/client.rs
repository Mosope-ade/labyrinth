use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ai::config::AiConfig;

// ─── Message types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: MessageRole::System, content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: MessageRole::User, content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: MessageRole::Assistant, content: content.into() }
    }
}

// ─── Error ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AiError {
    #[error("HTTP transport error: {0}")]
    Http(String),
    #[error("Provider returned error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("Provider returned an empty response")]
    EmptyResponse,
    #[error("Response deserialization failed: {0}")]
    Deserialization(String),
}

// ─── Wire types for OpenAI chat completions format ───────────────────────────

#[derive(Serialize)]
struct CompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    max_tokens: u32,
    temperature: f32,
}

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
}

#[derive(Deserialize)]
struct CompletionMessage {
    content: String,
}

// ─── AiClient ────────────────────────────────────────────────────────────────

/// HTTP client for any OpenAI-compatible chat completions endpoint.
///
/// Works with Gemini, Claude, GPT-4, Groq, Together, Ollama, and any other
/// provider that exposes `/v1/chat/completions`. Switch providers by changing
/// `AI_PROVIDER`, `AI_API_KEY`, and `AI_MODEL` environment variables.
pub struct AiClient {
    config: AiConfig,
    http: Client,
}

impl AiClient {
    pub fn new(config: AiConfig) -> Self {
        Self { config, http: Client::new() }
    }

    pub fn from_env() -> Result<Self, crate::ai::config::ConfigError> {
        Ok(Self::new(AiConfig::from_env()?))
    }

    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    /// Sends a chat completion request and returns the assistant's text content.
    ///
    /// Uses the OpenAI `/chat/completions` format, which is supported by all
    /// major providers either natively or via their compatibility endpoints.
    pub async fn complete(&self, messages: &[ChatMessage]) -> Result<String, AiError> {
        let endpoint = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let body = CompletionRequest {
            model: &self.config.model,
            messages,
            max_tokens: self.config.max_tokens,
            temperature: 0.3, // low temperature — deterministic intent extraction
        };

        let resp = self
            .http
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", self.config.api_key()))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AiError::Api { status, message: body });
        }

        let parsed: CompletionResponse = resp
            .json()
            .await
            .map_err(|e| AiError::Deserialization(e.to_string()))?;

        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .filter(|s| !s.is_empty())
            .ok_or(AiError::EmptyResponse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::config::{AiConfig, AiProvider};

    fn test_client() -> AiClient {
        AiClient::new(
            AiConfig::new("test-key", AiProvider::Gemini, "gemini-2.0-flash")
                .with_base_url("http://localhost:11434/v1"),
        )
    }

    #[test]
    fn chat_message_constructors_set_correct_roles() {
        assert_eq!(ChatMessage::system("x").role, MessageRole::System);
        assert_eq!(ChatMessage::user("x").role, MessageRole::User);
        assert_eq!(ChatMessage::assistant("x").role, MessageRole::Assistant);
    }

    #[test]
    fn chat_message_content_is_preserved() {
        let msg = ChatMessage::user("design a hospital network");
        assert_eq!(msg.content, "design a hospital network");
    }

    #[test]
    fn chat_message_role_serializes_lowercase() {
        let msg = ChatMessage::user("hi");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
    }

    #[test]
    fn ai_client_config_is_accessible() {
        let client = test_client();
        assert_eq!(client.config().api_key(), "test-key");
        assert_eq!(client.config().model, "gemini-2.0-flash");
    }

    #[test]
    fn ai_error_messages_are_descriptive() {
        let e = AiError::Api { status: 401, message: "Unauthorized".into() };
        assert!(e.to_string().contains("401"));
        assert!(e.to_string().contains("Unauthorized"));

        let e2 = AiError::EmptyResponse;
        assert!(e2.to_string().contains("empty"));
    }
}
