use crate::{auth::AuthError, jobs::AdmissionError};
use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}
#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}
pub(super) struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    details: Option<serde_json::Value>,
}
impl ApiError {
    pub(super) fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            details: None,
        }
    }
    pub(super) fn invalid() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Request format is invalid",
        )
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                    details: self.details,
                },
            }),
        )
            .into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                "Bearer".parse().expect("literal header"),
            );
        }
        response
    }
}
impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::InvalidToken => Self::new(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "A valid user access token is required",
            ),
            _ => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication_unavailable",
                "Authentication is temporarily unavailable",
            ),
        }
    }
}
impl From<AdmissionError> for ApiError {
    fn from(e: AdmissionError) -> Self {
        use StatusCode as S;
        match e {
            AdmissionError::InvalidInput(_) => Self::new(
                S::UNPROCESSABLE_ENTITY,
                "invalid_requirements",
                "Requirements or options are invalid or unsupported",
            ),
            AdmissionError::InvalidRequirements(issues) => Self {
                status: S::UNPROCESSABLE_ENTITY,
                code: "invalid_requirements",
                message: "Requirements failed validation",
                details: Some(serde_json::json!(issues)),
            },
            AdmissionError::NeedsClarification(fields) => Self {
                status: S::UNPROCESSABLE_ENTITY,
                code: "needs_clarification",
                message: "Critical requirements need clarification",
                details: Some(serde_json::json!({"fields":fields})),
            },
            AdmissionError::Forbidden => {
                Self::new(S::FORBIDDEN, "forbidden", "Access is not permitted")
            }
            AdmissionError::NotFound => Self::new(
                S::NOT_FOUND,
                "not_found",
                "Resource was not found in this project",
            ),
            AdmissionError::IdempotencyConflict => Self::new(
                S::CONFLICT,
                "idempotency_conflict",
                "Idempotency key was already used for different input",
            ),
            AdmissionError::StaleHead => Self::new(
                S::CONFLICT,
                "stale_head",
                "Project head does not match the precondition",
            ),
            AdmissionError::InvalidBase => {
                Self::new(S::CONFLICT, "invalid_base", "Base revision is incompatible")
            }
            AdmissionError::CatalogUnavailable => Self::new(
                S::UNPROCESSABLE_ENTITY,
                "catalog_unavailable",
                "Catalog cannot be used for this request",
            ),
            AdmissionError::QuotaExceeded => Self::new(
                S::TOO_MANY_REQUESTS,
                "quota_exceeded",
                "Active job quota is exhausted",
            ),
            AdmissionError::Database(_) => Self::new(
                S::SERVICE_UNAVAILABLE,
                "database_unavailable",
                "Job storage is temporarily unavailable; retry submissions with the same idempotency key",
            ),
            AdmissionError::Serialization(_) => Self::new(
                S::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Request could not be processed",
            ),
        }
    }
}
