use super::{ApiError, AppState, single_header};
use crate::{auth::AuthenticatedActor, jobs::AdmissionError};
use axum::{
    Json,
    body::Bytes,
    extract::{Extension, State, rejection::BytesRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;
#[derive(Clone, Copy)]
pub struct OnboardingPolicy {
    pub max_active_jobs: i32,
}
impl OnboardingPolicy {
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        match std::env::var("LABYRINTH_ONBOARDING_MAX_ACTIVE_JOBS") {
            Err(std::env::VarError::NotPresent) => Ok(None),
            Ok(value) => value
                .parse::<i32>()
                .ok()
                .filter(|v| (1..=1000).contains(v))
                .map(|max_active_jobs| Some(Self { max_active_jobs }))
                .ok_or("invalid onboarding policy"),
            Err(_) => Err("invalid onboarding policy"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {}
pub(super) async fn identity(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
) -> Result<Response, ApiError> {
    let value: Value = sqlx::query_scalar("SELECT labyrinth.current_identity($1)")
        .bind(actor.id())
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    Ok(Json(value).into_response())
}
pub(super) async fn personal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let body = body.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Onboarding request exceeds its limit",
        )
    })?;
    if body.len() > 1024 {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Onboarding request exceeds 1 KiB",
        ));
    }
    if !single_header(&headers, "content-type")?
        .split(';')
        .next()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Content-Type must be application/json",
        ));
    }
    let _: EmptyRequest = serde_json::from_slice(&body).map_err(|_| ApiError::invalid())?;
    let (Some(onboarding), Some(projects)) = (state.onboarding, state.projects) else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "onboarding_unavailable",
            "Onboarding is not configured",
        ));
    };
    let value: Value = sqlx::query_scalar("SELECT labyrinth.onboard_personal($1,$2,$3)")
        .bind(actor.id())
        .bind(onboarding.max_active_jobs)
        .bind(projects.max_projects)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    Ok((
        if value["replayed"] == true {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(value),
    )
        .into_response())
}
