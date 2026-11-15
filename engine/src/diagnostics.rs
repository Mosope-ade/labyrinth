//! Stable validation diagnostics shared by the input boundaries.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    InvalidValue,
    DuplicateId,
    UnknownReference,
    Conflict,
    LimitExceeded,
    UnsupportedVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub code: IssueCode,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidationErrors {
    pub issues: Vec<Issue>,
}
impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for issue in &self.issues {
            writeln!(f, "{}: {} ({:?})", issue.path, issue.message, issue.code)?;
        }
        Ok(())
    }
}
impl std::error::Error for ValidationErrors {}

#[derive(Default)]
pub(crate) struct Checks(Vec<Issue>);
impl Checks {
    pub fn require(
        &mut self,
        condition: bool,
        code: IssueCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        if !condition {
            self.0.push(Issue {
                code,
                path: path.into(),
                message: message.into(),
            });
        }
    }
    pub fn finish(self) -> Result<(), ValidationErrors> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors { issues: self.0 })
        }
    }
}

pub const MAX_JSON_BYTES: usize = 1_048_576;
#[derive(Debug)]
pub enum JsonInputError {
    TooLarge,
    Parse(serde_json::Error),
    Invalid(ValidationErrors),
}
impl std::fmt::Display for JsonInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "JSON input exceeds {MAX_JSON_BYTES} bytes"),
            Self::Parse(e) => write!(f, "JSON parse: {e}"),
            Self::Invalid(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for JsonInputError {}
pub(crate) fn decode_json<T: serde::de::DeserializeOwned>(
    input: &str,
) -> Result<T, JsonInputError> {
    if input.len() > MAX_JSON_BYTES {
        return Err(JsonInputError::TooLarge);
    }
    serde_json::from_str(input).map_err(JsonInputError::Parse)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Counterexample {
    pub code: String,
    pub component: String,
    pub detail: String,
    pub required: Option<u64>,
    pub available: Option<u64>,
}
