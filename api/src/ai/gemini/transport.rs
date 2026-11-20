use super::{GeminiAdapter, GeminiErrorCode, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, wire};
use serde::Deserialize;
use std::time::Duration;

pub(super) struct AttemptFailure {
    pub code: GeminiErrorCode,
    // None means no automatic second dispatch, not a missing zero-second delay.
    pub retry_after: Option<Duration>,
}
impl From<GeminiErrorCode> for AttemptFailure {
    fn from(code: GeminiErrorCode) -> Self {
        Self {
            code,
            retry_after: None,
        }
    }
}
impl GeminiAdapter {
    pub(super) async fn attempt(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<wire::WireResponse, AttemptFailure> {
        let body = wire::build_request(model, prompt, self.settings.max_output_tokens)?;
        let body = serde_json::to_vec(&body).map_err(|_| GeminiErrorCode::InvalidConfiguration)?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(GeminiErrorCode::RequestTooLarge.into());
        }
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    return GeminiErrorCode::DeadlineExceeded.into();
                }
                if error.is_connect() {
                    return AttemptFailure {
                        code: GeminiErrorCode::ConnectionFailed,
                        retry_after: Some(Duration::ZERO),
                    };
                }
                GeminiErrorCode::ProviderRejected.into()
            })?;
        let status = response.status();
        // Unsupported HTTP-date Retry-After fails closed rather than being ignored.
        let header_delay = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .map(|v| {
                v.to_str()
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(Duration::from_secs)
            });
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
        {
            return Err(GeminiErrorCode::ResponseTooLarge.into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AttemptFailure::from(GeminiErrorCode::InvalidResponse))?
        {
            if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
                return Err(GeminiErrorCode::ResponseTooLarge.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if status.is_success() {
            return wire::parse_response(&bytes, model).map_err(Into::into);
        }
        if status.is_redirection() {
            return Err(GeminiErrorCode::ProviderRejected.into());
        }
        let error: ErrorEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| GeminiErrorCode::ProviderRejected)?;
        if error.error.code != status.as_u16() {
            return Err(GeminiErrorCode::ProviderRejected.into());
        }
        let code = match (status.as_u16(), error.error.status.as_str()) {
            (404, "NOT_FOUND") => GeminiErrorCode::ModelUnavailable,
            (429, "RESOURCE_EXHAUSTED") => GeminiErrorCode::RateLimited,
            (503, "UNAVAILABLE") => GeminiErrorCode::ProviderUnavailable,
            _ => return Err(GeminiErrorCode::ProviderRejected.into()),
        };
        let mut delay = match header_delay {
            Some(None) => return Err(code.into()),
            Some(Some(delay)) => delay,
            None => Duration::ZERO,
        };
        for detail in error.error.details {
            if detail.kind == "type.googleapis.com/google.rpc.RetryInfo" {
                let Some(parsed) = detail.retry_delay.as_deref().and_then(parse_duration) else {
                    return Err(code.into());
                };
                delay = delay.max(parsed);
            }
        }
        // Explicit, completed Gemini availability errors support configured fallback.
        // This is not a claim that an attempt was unbilled. Unknown failures/timeouts
        // never acquire recovery permission; both attempts consume the call allowance.
        Err(AttemptFailure {
            code,
            retry_after: Some(delay),
        })
    }
}
#[derive(Deserialize)]
struct ErrorEnvelope {
    error: GoogleError,
}
#[derive(Deserialize)]
struct GoogleError {
    code: u16,
    status: String,
    #[serde(default)]
    details: Vec<ErrorDetail>,
}
#[derive(Deserialize)]
struct ErrorDetail {
    #[serde(rename = "@type", default)]
    kind: String,
    #[serde(rename = "retryDelay")]
    retry_delay: Option<String>,
}
fn parse_duration(text: &str) -> Option<Duration> {
    let number = text.strip_suffix('s')?;
    let (seconds, fraction) = number.split_once('.').unwrap_or((number, ""));
    if (number.contains('.') && fraction.is_empty())
        || seconds.is_empty()
        || !seconds.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > 9
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let seconds = seconds.parse::<u64>().ok()?;
    let nanos = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().ok()? * 10_u32.pow(9 - fraction.len() as u32)
    };
    Some(Duration::new(seconds, nanos))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_backoff_duration_is_strict_and_preserves_fractional_seconds() {
        assert_eq!(parse_duration("0s"), Some(Duration::ZERO));
        assert_eq!(parse_duration("1.500s"), Some(Duration::from_millis(1500)));
        assert_eq!(
            parse_duration("0.000000001s"),
            Some(Duration::from_nanos(1))
        );
        for invalid in ["1.s", "-1s", "1.1234567890s", "NaNs", "1", "1e3s"] {
            assert!(parse_duration(invalid).is_none(), "{invalid}");
        }
    }
}
