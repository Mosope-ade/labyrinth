use super::{
    ApiError, AppState, WorkPermit,
    discovery::{PageQuery, page},
    single_header,
};
use crate::{
    auth::AuthenticatedActor,
    jobs::{self, AdmissionError},
};
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
use serde_json::{Value, json, value::RawValue};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CatalogPage {
    limit: Option<usize>,
    after: Option<String>,
}
fn hash_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) async fn catalogs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<CatalogPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path(tenant) = path.map_err(|_| ApiError::invalid())?;
    let Query(query) = query.map_err(|_| ApiError::invalid())?;
    let limit = query.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) || query.after.as_ref().is_some_and(|h| !hash_valid(h)) {
        return Err(ApiError::invalid());
    }
    let value: Value = sqlx::query_scalar("SELECT labyrinth.list_catalogs($1,$2,$3,$4)")
        .bind(actor.id())
        .bind(tenant)
        .bind(query.after)
        .bind(limit as i32)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    page(value, limit, "sha256")
}
pub(super) async fn catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    Extension(permit): Extension<WorkPermit>,
    path: Result<Path<(Uuid, String)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, hash)) = path.map_err(|_| ApiError::invalid())?;
    if !hash_valid(&hash) {
        return Err(ApiError::invalid());
    }
    let yaml: String = sqlx::query_scalar("SELECT labyrinth.catalog_payload($1,$2,$3)")
        .bind(actor.id())
        .bind(tenant)
        .bind(&hash)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    let value = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let catalog = labyrinth_engine::catalog::load_yaml(&yaml).map_err(|_| ())?;
        if catalog.version().sha256 != hash {
            return Err(());
        }
        Ok::<_, ()>(json!({"catalog_version":catalog.version(),"bundle":catalog.bundle()}))
    })
    .await
    .map_err(|_| internal())?
    .map_err(|_| internal())?;
    Ok(Json(value).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateRequest {
    intent: Box<RawValue>,
    #[serde(default)]
    options: labyrinth_engine::artifacts::GenerationOptions,
}
fn internal() -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Stored data could not be processed",
    )
}
pub(super) async fn validate(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    Extension(permit): Extension<WorkPermit>,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path(tenant) = path.map_err(|_| ApiError::invalid())?;
    let body = body.map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Request exceeds 2 MiB",
            )
        } else {
            ApiError::invalid()
        }
    })?;
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
    // Validation authorizes membership without depending on any catalog contents.
    sqlx::query("SELECT labyrinth.validation_access($1,$2)")
        .bind(actor.id())
        .bind(tenant)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    let value=tokio::task::spawn_blocking(move||{
        let _permit=permit;let request:ValidateRequest=serde_json::from_slice(&body).map_err(|_|ApiError::invalid())?;
        let intent=jobs::resolve_generation_input(request.intent.get(),&request.options)?;
        let requirements:Value=serde_json::from_slice(&labyrinth_engine::artifacts::canonical_requirements(&intent).map_err(|_|internal())?).map_err(|_|internal())?;
        let provenance:Value=serde_json::from_slice(&labyrinth_engine::artifacts::canonical_provenance(&intent).map_err(|_|internal())?).map_err(|_|internal())?;
        Ok::<_,ApiError>(json!({"status":"ready","requirements":requirements,"provenance":provenance,"assurance":"requirements_validated_not_generated"}))
    }).await.map_err(|_|internal())??;
    Ok(Json(value).into_response())
}
pub(super) async fn jobs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project)) = path.map_err(|_| ApiError::invalid())?;
    let Query(query) = query.map_err(|_| ApiError::invalid())?;
    let limit = query.limit()?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.list_jobs($1,$2,$3,$4,$5)")
        .bind(actor.id())
        .bind(tenant)
        .bind(project)
        .bind(query.after)
        .bind(limit as i32)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    page(value, limit, "job_id")
}
