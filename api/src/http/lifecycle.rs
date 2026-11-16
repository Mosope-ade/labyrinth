//! Owner-confirmed reversible project deletion; physical erasure is worker-only.
use super::{
    ApiError, AppState,
    discovery::{PageQuery, page},
    single_header,
};
use crate::{auth::AuthenticatedActor, jobs::AdmissionError};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Extension, Path, Query, State,
        rejection::{BytesRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirmation {
    confirm_project: Uuid,
}
async fn change(
    state: AppState,
    actor: Uuid,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
    restore: bool,
) -> Result<Response, ApiError> {
    let Path((tenant, project)) = path.map_err(|_| ApiError::invalid())?;
    if !single_header(&headers, "content-type")?
        .split(';')
        .next()
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Content-Type must be application/json",
        ));
    }
    let bytes = body.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Request exceeds 1 KiB",
        )
    })?;
    if bytes.len() > 1024 {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Request exceeds 1 KiB",
        ));
    }
    let request: Confirmation = serde_json::from_slice(&bytes).map_err(|_| ApiError::invalid())?;
    if request.confirm_project != project {
        return Err(ApiError::invalid());
    }
    let value: Value = sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$4,$5)")
        .bind(actor)
        .bind(tenant)
        .bind(project)
        .bind(request.confirm_project)
        .bind(restore)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    Ok(Json(value).into_response())
}
pub(super) async fn delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    change(state, actor.id(), path, headers, body, false).await
}
pub(super) async fn restore(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    change(state, actor.id(), path, headers, body, true).await
}
pub(super) async fn deleted(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path(tenant) = path.map_err(|_| ApiError::invalid())?;
    let Query(query) = query.map_err(|_| ApiError::invalid())?;
    let limit = query.limit()?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.deleted_projects($1,$2,$3,$4)")
        .bind(actor.id())
        .bind(tenant)
        .bind(query.after)
        .bind(limit as i32)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    page(value, limit, "project_id")
}
