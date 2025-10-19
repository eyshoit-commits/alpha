use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::ScopeRequirement,
    server::{require_bearer, ApiError, AppState},
};

use bkg_db::{
    ModelDownloadJobRecord, ModelRecord, ModelStage, NewAuditEvent, NewModel, NewModelJob,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterModelBody {
    pub name: String,
    pub provider: String,
    pub version: String,
    pub format: String,
    pub source_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelResponse {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
    pub version: String,
    pub format: String,
    pub source_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub stage: ModelStageDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelJobResponse {
    pub id: Uuid,
    pub model_id: Uuid,
    pub stage: ModelStageDto,
    pub progress: f32,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelStageDto {
    Unknown,
    Registered,
    Queued,
    Downloading,
    Verifying,
    Ready,
    Failed,
}

impl From<ModelStage> for ModelStageDto {
    fn from(stage: ModelStage) -> Self {
        match stage {
            ModelStage::Unknown => ModelStageDto::Unknown,
            ModelStage::Registered => ModelStageDto::Registered,
            ModelStage::Queued => ModelStageDto::Queued,
            ModelStage::Downloading => ModelStageDto::Downloading,
            ModelStage::Verifying => ModelStageDto::Verifying,
            ModelStage::Ready => ModelStageDto::Ready,
            ModelStage::Failed => ModelStageDto::Failed,
        }
    }
}

impl From<ModelRecord> for ModelResponse {
    fn from(record: ModelRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            provider: record.provider,
            version: record.version,
            format: record.format,
            source_uri: record.source_uri,
            checksum_sha256: record.checksum_sha256,
            size_bytes: record.size_bytes,
            stage: ModelStageDto::from(record.stage),
            last_synced_at: record.last_synced_at.map(|ts| ts.to_rfc3339()),
            created_at: record.created_at.to_rfc3339(),
            updated_at: record.updated_at.to_rfc3339(),
            tags: record.tags,
            error_message: record.error_message,
        }
    }
}

impl From<ModelDownloadJobRecord> for ModelJobResponse {
    fn from(record: ModelDownloadJobRecord) -> Self {
        Self {
            id: record.id,
            model_id: record.model_id,
            stage: ModelStageDto::from(record.stage),
            progress: record.progress,
            started_at: record.started_at.to_rfc3339(),
            finished_at: record.finished_at.map(|ts| ts.to_rfc3339()),
            error_message: record.error_message,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/models",
    responses(
        (status = 200, description = "List registered models", body = [ModelResponse]),
        (status = 401, description = "Missing or invalid credentials", body = super::ErrorBody),
        (status = 403, description = "Insufficient permissions", body = super::ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
pub(super) async fn list_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<ModelResponse>>, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let records = state.db.list_models().await.map_err(ApiError::internal)?;
    Ok(Json(records.into_iter().map(ModelResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/models",
    request_body = RegisterModelBody,
    responses(
        (status = 201, description = "Model registered", body = ModelResponse),
        (status = 400, description = "Invalid request", body = super::ErrorBody),
        (status = 401, description = "Missing or invalid credentials", body = super::ErrorBody),
        (status = 403, description = "Insufficient permissions", body = super::ErrorBody),
        (status = 409, description = "Model already exists", body = super::ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
pub(super) async fn register_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RegisterModelBody>,
) -> Result<(StatusCode, Json<ModelResponse>), ApiError> {
    let actor = state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    if state
        .db
        .find_model_by_name_version(&payload.name, &payload.version)
        .await
        .map_err(ApiError::internal)?
        .is_some()
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "model '{}' version '{}' already exists",
                payload.name, payload.version
            ),
        ));
    }

    let tags_ref = payload.tags.as_ref().map(|items| items.as_slice());
    let record = state
        .db
        .create_model(NewModel {
            name: &payload.name,
            provider: &payload.provider,
            version: &payload.version,
            format: &payload.format,
            source_uri: &payload.source_uri,
            checksum_sha256: payload.checksum_sha256.as_deref(),
            size_bytes: payload.size_bytes,
            tags: tags_ref,
            stage: ModelStage::Registered,
            error_message: None,
        })
        .await
        .map_err(ApiError::internal)?;

    state
        .db
        .create_audit_event(NewAuditEvent {
            namespace: None,
            actor: Some(&actor.id.to_string()),
            event_type: "model.registered",
            recorded_at: Utc::now(),
            payload: &json!({
                "model_id": record.id,
                "name": record.name,
                "provider": record.provider,
                "version": record.version,
            }),
            signature_valid: None,
        })
        .await
        .map_err(ApiError::internal)?;

    Ok((StatusCode::CREATED, Json(ModelResponse::from(record))))
}

#[utoipa::path(
    post,
    path = "/api/v1/models/{id}/refresh",
    params(("id" = Uuid, Path, description = "Model identifier")),
    responses(
        (status = 200, description = "Refresh queued", body = ModelResponse),
        (status = 401, description = "Missing or invalid credentials", body = super::ErrorBody),
        (status = 403, description = "Insufficient permissions", body = super::ErrorBody),
        (status = 404, description = "Model not found", body = super::ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
pub(super) async fn refresh_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ModelResponse>, ApiError> {
    let actor = state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let existing = state
        .db
        .fetch_model(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "model not found"))?;

    let updated = state
        .db
        .update_model_stage(id, ModelStage::Queued, None, None)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "model not found"))?;

    state
        .db
        .insert_model_job(NewModelJob {
            model_id: id,
            stage: ModelStage::Queued,
            progress: 0.0,
            started_at: Utc::now(),
            finished_at: None,
            error_message: None,
        })
        .await
        .map_err(ApiError::internal)?;

    state
        .db
        .create_audit_event(NewAuditEvent {
            namespace: None,
            actor: Some(&actor.id.to_string()),
            event_type: "model.refresh_requested",
            recorded_at: Utc::now(),
            payload: &json!({
                "model_id": id,
                "previous_stage": existing.stage.as_str(),
                "next_stage": "queued",
            }),
            signature_valid: None,
        })
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(ModelResponse::from(updated)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/models/{id}",
    params(("id" = Uuid, Path, description = "Model identifier")),
    responses(
        (status = 204, description = "Model deleted"),
        (status = 401, description = "Missing or invalid credentials", body = super::ErrorBody),
        (status = 403, description = "Insufficient permissions", body = super::ErrorBody),
        (status = 404, description = "Model not found", body = super::ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
pub(super) async fn delete_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let actor = state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let model = state
        .db
        .fetch_model(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "model not found"))?;

    if !state
        .db
        .delete_model(id)
        .await
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "model not found"));
    }

    state
        .db
        .create_audit_event(NewAuditEvent {
            namespace: None,
            actor: Some(&actor.id.to_string()),
            event_type: "model.deleted",
            recorded_at: Utc::now(),
            payload: &json!({
                "model_id": id,
                "name": model.name,
                "provider": model.provider,
                "version": model.version,
            }),
            signature_valid: None,
        })
        .await
        .map_err(ApiError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/models/{id}/jobs",
    params(("id" = Uuid, Path, description = "Model identifier")),
    responses(
        (status = 200, description = "Model download jobs", body = [ModelJobResponse]),
        (status = 401, description = "Missing or invalid credentials", body = super::ErrorBody),
        (status = 403, description = "Insufficient permissions", body = super::ErrorBody),
        (status = 404, description = "Model not found", body = super::ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
pub(super) async fn list_model_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ModelJobResponse>>, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    if state
        .db
        .fetch_model(id)
        .await
        .map_err(ApiError::internal)?
        .is_none()
    {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "model not found"));
    }

    let jobs = state
        .db
        .list_model_jobs(id)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(ModelJobResponse::from)
        .collect();

    Ok(Json(jobs))
}
