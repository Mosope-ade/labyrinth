//! Opt-in Gemini transport. No runtime route, stored conversation or automatic job admission.
#[cfg(test)]
mod tests;
mod transport;
mod wire;

use super::{MAX_PROMPT_BYTES, ProviderMetadata, ReviewContext, ReviewedProposal};
use reqwest::{
    Client, Url,
    header::{HeaderMap, HeaderValue},
};
use serde::Serialize;
use std::time::Duration;
use tokio::time::Instant;

const ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1/interactions";
const MAX_REQUEST_BYTES: usize = 65_536;
const MAX_RESPONSE_BYTES: usize = 2 * 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeminiProfile {
    FreeTesting,
    PaidPrivate,
}
impl GeminiProfile {
    fn label(self) -> &'static str {
        match self {
            Self::FreeTesting => "free_testing",
            Self::PaidPrivate => "paid_private",
        }
    }
}
/// Server-owned classification, not model output or a client-supplied permission.
/// This is not an automatic detector of confidential material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptDataClass {
    PublicSynthetic,
    PrivateProject,
}
/// Explicit per-request limits. Global tenant budgets and provider account verification
/// remain the hosting application's responsibility before it calls this transport.
#[derive(Debug, Clone)]
pub struct GeminiSettings {
    pub primary_model: String,
    pub fallback_model: Option<String>,
    pub profile: GeminiProfile,
    pub request_timeout: Duration,
    pub max_output_tokens: u32,
    pub attempt_budget: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeminiErrorCode {
    InvalidConfiguration,
    PolicyDenied,
    InvalidPrompt,
    RequestTooLarge,
    ResponseTooLarge,
    ConnectionFailed,
    DeadlineExceeded,
    ProviderUnavailable,
    RateLimited,
    ModelUnavailable,
    ProviderRejected,
    InvalidResponse,
    UnexpectedModel,
    InvalidProposal,
}
#[derive(Debug, Clone, Serialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub thought_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AttemptRecord {
    pub model: String,
    pub failure: Option<GeminiErrorCode>,
    pub usage: Option<TokenUsage>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModelSwitchNotice {
    pub from_model: String,
    pub to_model: String,
    pub reason: GeminiErrorCode,
    pub message: String,
}
#[derive(Debug)]
pub struct GeminiProposal {
    pub proposal: ReviewedProposal,
    pub model_switch: Option<ModelSwitchNotice>,
    pub attempts: Vec<AttemptRecord>,
}
#[derive(Debug, thiserror::Error)]
#[error("Gemini request failed: {code:?}")]
pub struct GeminiFailure {
    pub code: GeminiErrorCode,
    pub model_switch: Option<ModelSwitchNotice>,
    pub attempts: Vec<AttemptRecord>,
}
pub struct GeminiAdapter {
    client: Client,
    endpoint: Url,
    settings: GeminiSettings,
}
impl std::fmt::Debug for GeminiAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiAdapter")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}
impl GeminiAdapter {
    /// Construction sends nothing. PaidPrivate is an operator assertion, not proof
    /// that the supplied key's Google project has the required billing/privacy terms.
    pub fn new(api_key: &str, settings: GeminiSettings) -> Result<Self, GeminiErrorCode> {
        Self::build(
            api_key,
            settings,
            ENDPOINT
                .parse()
                .map_err(|_| GeminiErrorCode::InvalidConfiguration)?,
            true,
        )
    }
    fn build(
        api_key: &str,
        settings: GeminiSettings,
        endpoint: Url,
        https_only: bool,
    ) -> Result<Self, GeminiErrorCode> {
        let valid_model = |s: &str| {
            s.len() <= 100
                && s.starts_with("gemini-")
                && s.len() > 7
                && s.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b".-".contains(&c))
        };
        if !valid_model(&settings.primary_model)
            || settings
                .fallback_model
                .as_ref()
                .is_some_and(|m| !valid_model(m) || m == &settings.primary_model)
            || settings.request_timeout.is_zero()
            || settings.request_timeout > Duration::from_secs(30)
            || settings.max_output_tokens == 0
            || settings.max_output_tokens > 8192
            || !(1..=2).contains(&settings.attempt_budget)
            || api_key.is_empty()
            || api_key.len() > 256
            || api_key.bytes().any(|c| c.is_ascii_whitespace())
        {
            return Err(GeminiErrorCode::InvalidConfiguration);
        }
        let mut key =
            HeaderValue::from_str(api_key).map_err(|_| GeminiErrorCode::InvalidConfiguration)?;
        key.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("x-goog-api-key", key);
        let client = Client::builder()
            .default_headers(headers)
            .https_only(https_only)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(settings.request_timeout.min(Duration::from_secs(3)))
            .timeout(settings.request_timeout)
            .build()
            .map_err(|_| GeminiErrorCode::InvalidConfiguration)?;
        Ok(Self {
            client,
            endpoint,
            settings,
        })
    }
    /// Caller must authorize project/context and reserve its tenant budget first.
    /// No real account, prompt classification or model certification is inferred here.
    pub async fn propose(
        &self,
        prompt: &str,
        context: ReviewContext,
        data_class: PromptDataClass,
    ) -> Result<GeminiProposal, GeminiFailure> {
        let mut records = Vec::new();
        let mut notice = None;
        let fail = |code, records, notice| GeminiFailure {
            code,
            attempts: records,
            model_switch: notice,
        };
        if self.settings.profile == GeminiProfile::FreeTesting
            && data_class == PromptDataClass::PrivateProject
        {
            return Err(fail(GeminiErrorCode::PolicyDenied, records, notice));
        }
        if prompt.len() > MAX_PROMPT_BYTES || prompt.trim().is_empty() {
            return Err(fail(GeminiErrorCode::InvalidPrompt, records, notice));
        }
        let deadline = Instant::now() + self.settings.request_timeout;
        let mut model = self.settings.primary_model.as_str();
        for attempt in 0..self.settings.attempt_budget {
            records.push(AttemptRecord {
                model: model.into(),
                failure: None,
                usage: None,
            });
            let response = tokio::time::timeout_at(deadline, self.attempt(model, prompt))
                .await
                .unwrap_or({
                    Err(transport::AttemptFailure {
                        code: GeminiErrorCode::DeadlineExceeded,
                        retry_after: None,
                    })
                });
            match response {
                Ok(output) => {
                    records.last_mut().expect("attempt exists").usage = output.usage;
                    let metadata = ProviderMetadata::new("gemini", &output.reported_model)
                        .and_then(|m| m.with_profile(self.settings.profile.label()));
                    let proposal = metadata
                        .and_then(|p| ReviewedProposal::parse(&output.content, prompt, context, p));
                    if Instant::now() >= deadline {
                        records.last_mut().expect("attempt exists").failure =
                            Some(GeminiErrorCode::DeadlineExceeded);
                        return Err(fail(GeminiErrorCode::DeadlineExceeded, records, notice));
                    }
                    return match proposal {
                        Ok(proposal) => Ok(GeminiProposal {
                            proposal,
                            model_switch: notice,
                            attempts: records,
                        }),
                        Err(_) => {
                            records.last_mut().expect("attempt exists").failure =
                                Some(GeminiErrorCode::InvalidProposal);
                            Err(fail(GeminiErrorCode::InvalidProposal, records, notice))
                        }
                    };
                }
                Err(problem) => {
                    records.last_mut().expect("attempt exists").failure = Some(problem.code);
                    let fallback = self.settings.fallback_model.as_deref();
                    if attempt != 0
                        || self.settings.attempt_budget != 2
                        || fallback.is_none()
                        || problem.retry_after.is_none()
                    {
                        return Err(fail(problem.code, records, notice));
                    }
                    let delay = problem.retry_after.expect("recovery checked");
                    if deadline
                        .checked_duration_since(Instant::now())
                        .is_none_or(|left| delay >= left)
                    {
                        return Err(fail(problem.code, records, notice));
                    }
                    if tokio::time::timeout_at(deadline, tokio::time::sleep(delay))
                        .await
                        .is_err()
                    {
                        return Err(fail(GeminiErrorCode::DeadlineExceeded, records, notice));
                    }
                    let next = fallback.expect("fallback checked");
                    notice = Some(ModelSwitchNotice {
                        from_model: model.into(),
                        to_model: next.into(),
                        reason: problem.code,
                        message: format!(
                            "Switched from {model} to {next} because {}.",
                            reason(problem.code)
                        ),
                    });
                    model = next;
                }
            }
        }
        Err(fail(GeminiErrorCode::ProviderUnavailable, records, notice))
    }
}
fn reason(code: GeminiErrorCode) -> &'static str {
    match code {
        GeminiErrorCode::ConnectionFailed => "the provider connection could not be established",
        GeminiErrorCode::RateLimited => "the provider reported a rate limit for this request",
        GeminiErrorCode::ModelUnavailable => {
            "the provider could not find the requested model/resource"
        }
        _ => "the provider reported unavailability",
    }
}
