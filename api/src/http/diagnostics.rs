//! Public allowlisted diagnostics; raw worker exception strings never enter responses.
use super::{ApiError, AppState};
use crate::{auth::AuthenticatedActor, jobs::AdmissionError};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::PathRejection},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use uuid::Uuid;
fn diagnostic(code: &str) -> Value {
    let (outcome, message) = match code {
        "invalid_input" => (
            "rejected",
            "Saved input or package validation failed; no package was published.",
        ),
        "engine_rejected" => (
            "rejected",
            "The engine rejected generation; no verified package was published.",
        ),
        "resource_limit" => (
            "inconclusive",
            "Generation reached a resource limit; no feasibility conclusion was reached.",
        ),
        "storage_unavailable" => (
            "unavailable",
            "Private artifact storage was unavailable or failed integrity checks.",
        ),
        "database_unavailable" => (
            "unavailable",
            "Generation persistence was temporarily unavailable.",
        ),
        "process_io" => (
            "unavailable",
            "The supervised engine process could not complete its input/output operation.",
        ),
        "lease_expired" => (
            "inconclusive",
            "Execution ownership expired; no feasibility conclusion was reached.",
        ),
        "stale_head" => (
            "conflict",
            "The project changed before publication; submit against the current revision.",
        ),
        "cancelled" => (
            "cancelled",
            "Generation was cancelled; this attempt published no package.",
        ),
        "succeeded" => ("verified", "The verified package was published."),
        _ => (
            "unknown",
            "This legacy attempt ended without a detailed diagnostic.",
        ),
    };
    json!({"code":if outcome=="unknown"{"attempt_failed"}else{code},"outcome":outcome,"message":message})
}
pub(super) async fn attempts(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project, job)) = path.map_err(|_| ApiError::invalid())?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.job_attempts($1,$2,$3,$4)")
        .bind(actor.id())
        .bind(tenant)
        .bind(project)
        .bind(job)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    let mut items = value.as_array().ok_or_else(ApiError::invalid)?.clone();
    for item in &mut items {
        let code = item
            .as_object_mut()
            .ok_or_else(ApiError::invalid)?
            .remove("code")
            .unwrap_or(Value::Null);
        item["diagnostic"] = code.as_str().map(diagnostic).unwrap_or(Value::Null);
    }
    Ok(Json(json!({"job_id":job,"items":items})).into_response())
}
