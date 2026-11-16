use super::{ApiError, AppState, single_header};
use crate::{auth::AuthenticatedActor, jobs::AdmissionError};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Extension, Path, State,
        rejection::{BytesRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone, Copy)]
pub struct ProjectPolicy {
    pub max_projects: i32,
}
impl ProjectPolicy {
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        match std::env::var("LABYRINTH_TENANT_MAX_PROJECTS") {
            Err(std::env::VarError::NotPresent) => Ok(None),
            Ok(value) => value
                .parse::<i32>()
                .ok()
                .filter(|n| (1..=10000).contains(n))
                .map(|max_projects| Some(Self { max_projects }))
                .ok_or("invalid project policy"),
            Err(_) => Err("invalid project policy"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateProject {
    name: String,
}
pub(super) async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path(tenant) = path.map_err(|_| ApiError::invalid())?;
    let body = body.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Project request exceeds its limit",
        )
    })?;
    if body.len() > 8192 {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Project request exceeds 8 KiB",
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
    let key = single_header(&headers, "idempotency-key")?;
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
    {
        return Err(ApiError::invalid());
    }
    let request: CreateProject = serde_json::from_slice(&body).map_err(|_| ApiError::invalid())?;
    if request.name.is_empty()
        || request.name.chars().count() > 200
        || request.name.trim() != request.name
        || request.name.chars().any(char::is_control)
    {
        return Err(ApiError::invalid());
    }
    let policy = state.projects.ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "project_creation_unavailable",
            "Project creation is not configured",
        )
    })?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.create_project($1,$2,$3,$4,$5)")
        .bind(actor.id())
        .bind(tenant)
        .bind(key)
        .bind(&request.name)
        .bind(policy.max_projects)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| match AdmissionError::from(error) {
            AdmissionError::QuotaExceeded => ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "quota_exceeded",
                "Project quota is exhausted",
            ),
            other => ApiError::from(other),
        })?;
    let code = if value["replayed"] == true {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    let id = value["project_id"].as_str().ok_or_else(ApiError::invalid)?;
    Ok((
        code,
        [(
            header::LOCATION,
            format!("/v1/tenants/{tenant}/projects/{id}"),
        )],
        Json(value),
    )
        .into_response())
}
pub(super) async fn detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project)) = path.map_err(|_| ApiError::invalid())?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.project_detail($1,$2,$3)")
        .bind(actor.id())
        .bind(tenant)
        .bind(project)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    Ok(Json(value).into_response())
}
