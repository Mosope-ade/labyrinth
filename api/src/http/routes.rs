use super::{ApiError, AppState, WorkPermit, single_header};
use crate::{
    auth::AuthenticatedActor,
    jobs::{self, AdmissionContext, PreparedAdmission},
};
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
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitRequest {
    intent: Box<RawValue>,
    #[serde(default)]
    options: labyrinth_engine::artifacts::GenerationOptions,
    catalog_sha256: String,
    #[serde(deserialize_with = "required_head")]
    expected_head_revision_id: Option<Uuid>,
}
fn required_head<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Uuid>, D::Error> {
    Option::deserialize(d)
}
pub(super) async fn submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    Extension(permit): Extension<WorkPermit>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project)) = path.map_err(|_| ApiError::invalid())?;
    let body = body.map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Request body exceeds 2 MiB",
            )
        } else {
            ApiError::invalid()
        }
    })?;
    if !single_header(&headers, "content-type")
        .map_err(|_| {
            ApiError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "Content-Type must be application/json",
            )
        })?
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
    let key = single_header(&headers, "idempotency-key")?.to_owned();
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
    {
        return Err(ApiError::invalid());
    }
    let work = permit.clone();
    let request: SubmitRequest = tokio::task::spawn_blocking(move || {
        let _permit = work;
        serde_json::from_slice(&body)
    })
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Request could not be processed",
        )
    })?
    .map_err(|_| ApiError::invalid())?;
    if request.catalog_sha256.len() != 64
        || !request
            .catalog_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApiError::invalid());
    }
    let context = AdmissionContext {
        actor_id: actor.id(),
        tenant_id: tenant,
        project_id: project,
        expected_head: request.expected_head_revision_id,
    };
    let catalog =
        jobs::catalog_for_admission(&state.pool, &context, &request.catalog_sha256).await?;
    let prepared = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PreparedAdmission::new(
            context,
            &key,
            request.intent.get(),
            &request.options,
            &catalog,
        )
    })
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Request could not be processed",
        )
    })??;
    let result = jobs::admit(&state.pool, &prepared).await?;
    let location = format!(
        "/v1/tenants/{tenant}/projects/{project}/jobs/{}",
        result.job_id
    );
    let code = if result.replayed {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((code, [(header::LOCATION, location)], Json(result)).into_response())
}
pub(super) async fn status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project, job)) = path.map_err(|_| ApiError::invalid())?;
    Ok(Json(jobs::status(&state.pool, actor.id(), tenant, project, job).await?).into_response())
}
