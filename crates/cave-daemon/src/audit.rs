use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::ScopeRequirement,
    server::{require_bearer, ApiError, AppState},
};

use bkg_db::{AuditEventFilters, AuditEventRecord};

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditEventResponse {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub event_type: String,
    pub recorded_at: String,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_valid: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListAuditQuery {
    #[param(example = "namespace-alpha")]
    pub namespace: Option<String>,
    #[param(name = "event_type", example = "model.registered")]
    pub event_type: Option<String>,
    #[param(example = 50)]
    pub limit: Option<u32>,
    #[param(example = "2024-01-01T00:00:00Z")]
    pub since: Option<String>,
    #[param(example = "2024-01-31T00:00:00Z")]
    pub until: Option<String>,
    #[param(example = "api-key-id")]
    pub actor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/audit/events",
    params(ListAuditQuery),
    responses(
        (status = 200, description = "List audit events", body = [AuditEventResponse]),
        (status = 400, description = "Invalid query parameters", body = super::ErrorBody),
        (status = 401, description = "Missing or invalid credentials", body = super::ErrorBody),
        (status = 403, description = "Insufficient permissions", body = super::ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
pub(super) async fn list_audit_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ListAuditQuery>,
) -> Result<Json<Vec<AuditEventResponse>>, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let since = parse_optional_timestamp(params.since.as_deref())?;
    let until = parse_optional_timestamp(params.until.as_deref())?;
    let limit = params.limit.map(|value| value.min(500));

    let filters = AuditEventFilters {
        namespace: params.namespace.as_deref(),
        event_type: params.event_type.as_deref(),
        since,
        until,
        limit,
        actor: params.actor.as_deref(),
    };

    let events = state
        .db
        .list_audit_events(filters)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(AuditEventResponse::from)
        .collect();

    Ok(Json(events))
}

impl From<AuditEventRecord> for AuditEventResponse {
    fn from(record: AuditEventRecord) -> Self {
        Self {
            id: record.id,
            namespace: record.namespace,
            actor: record.actor,
            event_type: record.event_type,
            recorded_at: record.recorded_at.to_rfc3339(),
            payload: record.payload,
            signature_valid: record.signature_valid,
        }
    }
}

fn parse_optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, ApiError> {
    match value {
        Some(raw) => {
            let parsed = DateTime::parse_from_rfc3339(raw)
                .map_err(|_| ApiError::bad_request(format!("invalid RFC3339 timestamp: {raw}")))?
                .with_timezone(&Utc);
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}
