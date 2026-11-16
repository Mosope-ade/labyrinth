//! Bounded cursor-based discovery without private artifact keys or job inputs.
use super::{ApiError, AppState};
use crate::{auth::AuthenticatedActor, jobs::AdmissionError};
use axum::{
    Json,
    extract::{
        Extension, Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PageQuery {
    pub(super) limit: Option<usize>,
    pub(super) after: Option<Uuid>,
}
impl PageQuery {
    pub(super) fn limit(&self) -> Result<usize, ApiError> {
        let limit = self.limit.unwrap_or(25);
        if !(1..=100).contains(&limit) {
            return Err(ApiError::invalid());
        }
        Ok(limit)
    }
}
pub(super) fn page(value: Value, limit: usize, id: &str) -> Result<Response, ApiError> {
    let mut items = value
        .as_array()
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Stored page could not be read",
            )
        })?
        .clone();
    let more = items.len() > limit;
    items.truncate(limit);
    let next = if more {
        items.last().and_then(|v| v.get(id)).cloned()
    } else {
        None
    };
    Ok(Json(json!({"items":items,"next_after":next})).into_response())
}
pub(super) async fn projects(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path(tenant) = path.map_err(|_| ApiError::invalid())?;
    let Query(query) = query.map_err(|_| ApiError::invalid())?;
    let limit = query.limit()?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.list_projects($1,$2,$3,$4)")
        .bind(actor.id())
        .bind(tenant)
        .bind(query.after)
        .bind(limit as i32)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    page(value, limit, "project_id")
}
pub(super) async fn revisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project)) = path.map_err(|_| ApiError::invalid())?;
    let Query(query) = query.map_err(|_| ApiError::invalid())?;
    let limit = query.limit()?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.list_revisions($1,$2,$3,$4,$5)")
        .bind(actor.id())
        .bind(tenant)
        .bind(project)
        .bind(query.after)
        .bind(limit as i32)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    page(value, limit, "revision_id")
}
pub(super) async fn revision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project, revision)) = path.map_err(|_| ApiError::invalid())?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.revision_detail($1,$2,$3,$4)")
        .bind(actor.id())
        .bind(tenant)
        .bind(project)
        .bind(revision)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    Ok(Json(value).into_response())
}
