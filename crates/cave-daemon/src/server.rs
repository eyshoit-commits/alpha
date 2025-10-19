use std::{env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use crate::auth::{
    AuthError, AuthService, KeyInfo, KeyScope, RotationOutcome, RotationWebhookPayload,
    ScopeRequirement,
};
use crate::middleware::rate_limit::{rate_limit_layer, RateLimitConfig};
use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bkg_db::{
    AuditEventFilter, AuditEventRecord, Database, ExecutionRecord, ModelDownloadJobRecord,
    ModelRecord, ModelStage, NewAuditEvent, NewModel, NewModelDownloadJob, ResourceLimits,
    SandboxRecord,
};
use cave_kernel::{
    AuditConfig, CaveKernel, CreateSandboxRequest, ExecOutcome, ExecRequest, IsolationSettings,
    KernelConfig, KernelError, ProcessSandboxRuntime,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use utoipa::{IntoParams, Modify, OpenApi, ToSchema};
use uuid::Uuid;

pub async fn run() -> Result<()> {
    let config = AppConfig::from_env()?;

    let db = Database::connect(&config.db_url)
        .await
        .context("failed to open database")?;

    let kernel_cfg = KernelConfig {
        workspace_root: config.workspace_root.clone(),
        default_runtime: config.default_runtime.clone(),
        default_limits: config.default_limits,
        isolation: config.isolation.clone(),
        audit: config.audit.clone(),
    };

    let runtime = ProcessSandboxRuntime::new(kernel_cfg.isolation.clone())
        .context("initializing sandbox runtime")?;

    let kernel = CaveKernel::new(db.clone(), runtime, kernel_cfg);
    let auth = Arc::new(AuthService::new(
        db.clone(),
        config.rotation_webhook_secret.clone(),
    ));
    let state = Arc::new(AppState { kernel, db, auth });

    let app = build_router(state.clone())
        .layer(rate_limit_layer(RateLimitConfig::default()))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.listen_addr)
        .await
        .context("failed to bind listen socket")?;

    info!(addr = %config.listen_addr, "cave-daemon listening");
    axum::serve(listener, app)
        .await
        .context("HTTP server exited")?;
    Ok(())
}

fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics))
        .route(
            "/api/v1/sandboxes",
            post(create_sandbox).get(list_sandboxes),
        )
        .route("/api/v1/sandboxes/:id/start", post(start_sandbox))
        .route("/api/v1/sandboxes/:id/exec", post(exec_sandbox))
        .route("/api/v1/sandboxes/:id/stop", post(stop_sandbox))
        .route("/api/v1/sandboxes/:id/status", get(get_sandbox))
        .route("/api/v1/sandboxes/:id/executions", get(list_executions))
        .route("/api/v1/sandboxes/:id", delete(delete_sandbox))
        .route("/api/v1/models", get(list_models).post(register_model))
        .route("/api/v1/models/:id/refresh", post(refresh_model))
        .route("/api/v1/models/:id", delete(delete_model))
        .route("/api/v1/models/:id/jobs", get(list_model_jobs))
        .route("/api/v1/audit/events", get(list_audit_events))
        .route("/api/v1/auth/keys", post(issue_key).get(list_keys))
        .route("/api/v1/auth/keys/rotate", post(rotate_key))
        .route("/api/v1/auth/keys/rotated", post(verify_rotation_webhook))
        .route("/api/v1/auth/keys/:id", delete(revoke_key))
        .with_state(state)
}

#[derive(Clone)]
struct AppState {
    kernel: CaveKernel<ProcessSandboxRuntime>,
    db: Database,
    auth: Arc<AuthService>,
}

#[derive(Debug, Clone)]
struct AppConfig {
    listen_addr: SocketAddr,
    db_url: String,
    workspace_root: PathBuf,
    default_runtime: String,
    default_limits: ResourceLimits,
    isolation: IsolationSettings,
    audit: AuditConfig,
    rotation_webhook_secret: Option<Vec<u8>>,
}

impl AppConfig {
    fn from_env() -> Result<Self> {
        let listen_addr = env::var("CAVE_API_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()
            .context("invalid CAVE_API_ADDR")?;

        let db_url = env::var("BKG_DB_DSN")
            .or_else(|_| env::var("DATABASE_URL"))
            .context("BKG_DB_DSN or DATABASE_URL must be configured")?;

        let workspace_root = env::var("CAVE_WORKSPACE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./.cave_workspaces"));

        let default_runtime = env::var("CAVE_RUNTIME_DEFAULT").unwrap_or_else(|_| "process".into());

        let base_limits = ResourceLimits::default();
        let default_limits = ResourceLimits {
            cpu_limit_millis: env::var("CAVE_DEFAULT_CPU_MILLIS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(base_limits.cpu_limit_millis),
            memory_limit_bytes: env::var("CAVE_DEFAULT_MEMORY_MIB")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .map(mi_bytes)
                .unwrap_or(base_limits.memory_limit_bytes),
            disk_limit_bytes: env::var("CAVE_DEFAULT_DISK_MIB")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .map(mi_bytes)
                .unwrap_or(base_limits.disk_limit_bytes),
            timeout_seconds: env::var("CAVE_DEFAULT_TIMEOUT_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(base_limits.timeout_seconds),
        };

        let mut isolation = IsolationSettings::default();

        if matches!(bool_env("CAVE_DISABLE_ISOLATION"), Some(true)) {
            isolation.enable_namespaces = false;
            isolation.enable_cgroups = false;
        }

        if let Some(value) = bool_env("CAVE_DISABLE_NAMESPACES") {
            if value {
                isolation.enable_namespaces = false;
            }
        }

        if let Some(value) = bool_env("CAVE_ENABLE_NAMESPACES") {
            if value {
                isolation.enable_namespaces = true;
            }
        }

        if let Some(value) = bool_env("CAVE_DISABLE_CGROUPS") {
            if value {
                isolation.enable_cgroups = false;
            }
        }

        if let Some(value) = bool_env("CAVE_ENABLE_CGROUPS") {
            if value {
                isolation.enable_cgroups = true;
            }
        }

        if matches!(bool_env("CAVE_ISOLATION_NO_FALLBACK"), Some(true)) {
            isolation.fallback_to_plain = false;
        }

        if let Ok(path) = env::var("CAVE_BWRAP_PATH") {
            if !path.is_empty() {
                isolation.bubblewrap_path = Some(PathBuf::from(path));
            }
        }

        if let Some(unshare) = parse_string_list_env("CAVE_BWRAP_UNSHARE") {
            if !unshare.is_empty() {
                isolation.bubblewrap_unshare = unshare;
            }
        }

        if let Some(drop_caps) = parse_string_list_env("CAVE_BWRAP_DROP_CAPS") {
            if !drop_caps.is_empty() {
                isolation.bubblewrap_drop_capabilities = drop_caps;
            }
        }

        if let Some(paths) = parse_path_list_env("CAVE_BWRAP_RO_PATHS") {
            if !paths.is_empty() {
                isolation.bubblewrap_readonly_paths = paths;
            }
        }

        if let Some(paths) = parse_path_list_env("CAVE_BWRAP_DEV_PATHS") {
            if !paths.is_empty() {
                isolation.bubblewrap_dev_paths = paths;
            }
        }

        if let Some(paths) = parse_path_list_env("CAVE_BWRAP_TMPFS_PATHS") {
            if !paths.is_empty() {
                isolation.bubblewrap_tmpfs_paths = paths;
            }
        }

        match env::var("CAVE_BWRAP_UID") {
            Ok(value) if value.trim().is_empty() => isolation.bubblewrap_uid = None,
            Ok(value) => {
                if let Ok(uid) = value.trim().parse::<u32>() {
                    isolation.bubblewrap_uid = Some(uid);
                }
            }
            Err(_) => {}
        }

        match env::var("CAVE_BWRAP_GID") {
            Ok(value) if value.trim().is_empty() => isolation.bubblewrap_gid = None,
            Ok(value) => {
                if let Ok(gid) = value.trim().parse::<u32>() {
                    isolation.bubblewrap_gid = Some(gid);
                }
            }
            Err(_) => {}
        }

        if let Ok(path) = env::var("CAVE_BWRAP_PROC_PATH") {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                isolation.bubblewrap_proc_path = None;
            } else {
                isolation.bubblewrap_proc_path = Some(PathBuf::from(trimmed));
            }
        }

        if let Ok(path) = env::var("CAVE_CGROUP_ROOT") {
            if !path.is_empty() {
                isolation.cgroup_root = Some(PathBuf::from(path));
            }
        }

        let audit_enabled = match bool_env("CAVE_AUDIT_LOG_ENABLED") {
            Some(value) => value,
            None => !matches!(bool_env("CAVE_AUDIT_LOG_DISABLED"), Some(true)),
        };
        let audit_log_path = env::var("CAVE_AUDIT_LOG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./logs/audit.jsonl"));
        let audit_hmac_key = match env::var("CAVE_AUDIT_LOG_HMAC_KEY") {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(
                        STANDARD
                            .decode(trimmed)
                            .context("invalid base64 in CAVE_AUDIT_LOG_HMAC_KEY")?,
                    )
                }
            }
            Err(_) => None,
        };
        let audit = AuditConfig {
            enabled: audit_enabled,
            log_path: audit_log_path,
            hmac_key: audit_hmac_key,
        };

        let rotation_webhook_secret = match env::var("CAVE_ROTATION_WEBHOOK_SECRET") {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(
                        STANDARD
                            .decode(trimmed)
                            .context("invalid base64 in CAVE_ROTATION_WEBHOOK_SECRET")?,
                    )
                }
            }
            Err(_) => None,
        };

        Ok(Self {
            listen_addr,
            db_url,
            workspace_root,
            default_runtime,
            default_limits,
            isolation,
            audit,
            rotation_webhook_secret,
        })
    }
}

#[utoipa::path(
    get,
    path = "/healthz",
    responses((status = 200, description = "Service is healthy"))
)]
async fn healthz() -> StatusCode {
    StatusCode::OK
}

#[utoipa::path(
    get,
    path = "/metrics",
    responses((status = 200, description = "Prometheus metrics", content_type = "text/plain"))
)]
async fn metrics() -> impl IntoResponse {
    (
        StatusCode::OK,
        "# metrics placeholder\nbkg_cave_daemon_up 1\n",
    )
}

#[utoipa::path(
    post,
    path = "/api/v1/sandboxes",
    request_body = CreateSandboxBody,
    responses(
        (status = 201, description = "Sandbox created", body = SandboxResponse),
        (status = 400, description = "Invalid request", body = ErrorBody),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 409, description = "Sandbox already exists", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn create_sandbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateSandboxBody>,
) -> Result<(StatusCode, Json<SandboxResponse>), ApiError> {
    let token = require_bearer(&headers)?;
    state
        .auth
        .authorize(token, ScopeRequirement::Namespace(&payload.namespace))
        .await
        .map_err(ApiError::from)?;

    let limits = payload
        .limits
        .map(|l| l.into_limits(state.kernel.config().default_limits))
        .unwrap_or_else(|| state.kernel.config().default_limits);

    let mut request = CreateSandboxRequest::new(payload.namespace, payload.name);
    request.runtime = payload.runtime;
    request.resource_limits = Some(limits);

    let record = state
        .kernel
        .create_sandbox(request)
        .await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(SandboxResponse::from(record))))
}

#[utoipa::path(
    get,
    path = "/api/v1/sandboxes",
    params(SandboxListQuery),
    responses(
        (status = 200, description = "List sandboxes", body = [SandboxResponse]),
        (status = 400, description = "Missing namespace filter", body = ErrorBody),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn list_sandboxes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<SandboxListQuery>,
) -> Result<Json<Vec<SandboxResponse>>, ApiError> {
    let namespace = query
        .namespace
        .ok_or_else(|| ApiError::bad_request("namespace query parameter is required"))?;

    state
        .auth
        .authorize(
            require_bearer(&headers)?,
            ScopeRequirement::Namespace(&namespace),
        )
        .await
        .map_err(ApiError::from)?;

    let records = state
        .db
        .list_sandboxes(&namespace)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(
        records.into_iter().map(SandboxResponse::from).collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/sandboxes/{id}",
    params(("id" = Uuid, Path, description = "Sandbox identifier")),
    responses(
        (status = 200, description = "Sandbox details", body = SandboxResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Sandbox not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn get_sandbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<SandboxResponse>, ApiError> {
    let record = state.kernel.get_sandbox(id).await.map_err(ApiError::from)?;

    state
        .auth
        .authorize(
            require_bearer(&headers)?,
            ScopeRequirement::Namespace(&record.namespace),
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(SandboxResponse::from(record)))
}

#[utoipa::path(
    post,
    path = "/api/v1/sandboxes/{id}/start",
    params(("id" = Uuid, Path, description = "Sandbox identifier")),
    responses(
        (status = 200, description = "Sandbox started", body = SandboxResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Sandbox not found", body = ErrorBody),
        (status = 409, description = "Sandbox already running", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn start_sandbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<SandboxResponse>, ApiError> {
    let meta = state.kernel.get_sandbox(id).await.map_err(ApiError::from)?;
    state
        .auth
        .authorize(
            require_bearer(&headers)?,
            ScopeRequirement::Namespace(&meta.namespace),
        )
        .await
        .map_err(ApiError::from)?;

    let record = state
        .kernel
        .start_sandbox(id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(SandboxResponse::from(record)))
}

#[utoipa::path(
    post,
    path = "/api/v1/sandboxes/{id}/exec",
    params(("id" = Uuid, Path, description = "Sandbox identifier")),
    request_body = ExecBody,
    responses(
        (status = 200, description = "Execution result", body = ExecResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Sandbox not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn exec_sandbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<ExecBody>,
) -> Result<Json<ExecResponse>, ApiError> {
    let meta = state.kernel.get_sandbox(id).await.map_err(ApiError::from)?;
    state
        .auth
        .authorize(
            require_bearer(&headers)?,
            ScopeRequirement::Namespace(&meta.namespace),
        )
        .await
        .map_err(ApiError::from)?;

    let request = ExecRequest {
        command: payload.command,
        args: payload.args.unwrap_or_default(),
        stdin: payload.stdin,
        timeout: payload.timeout_ms.map(Duration::from_millis),
    };

    let outcome = state
        .kernel
        .exec(id, request)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ExecResponse::from(outcome)))
}

#[utoipa::path(
    post,
    path = "/api/v1/sandboxes/{id}/stop",
    params(("id" = Uuid, Path, description = "Sandbox identifier")),
    responses(
        (status = 204, description = "Sandbox stopped"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Sandbox not found", body = ErrorBody),
        (status = 409, description = "Sandbox not running", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn stop_sandbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let meta = state.kernel.get_sandbox(id).await.map_err(ApiError::from)?;
    state
        .auth
        .authorize(
            require_bearer(&headers)?,
            ScopeRequirement::Namespace(&meta.namespace),
        )
        .await
        .map_err(ApiError::from)?;

    state
        .kernel
        .stop_sandbox(id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/sandboxes/{id}",
    params(("id" = Uuid, Path, description = "Sandbox identifier")),
    responses(
        (status = 204, description = "Sandbox deleted"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Sandbox not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn delete_sandbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let meta = state.kernel.get_sandbox(id).await.map_err(ApiError::from)?;
    state
        .auth
        .authorize(
            require_bearer(&headers)?,
            ScopeRequirement::Namespace(&meta.namespace),
        )
        .await
        .map_err(ApiError::from)?;

    state
        .kernel
        .delete_sandbox(id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/sandboxes/{id}/executions",
    params(
        ("id" = Uuid, Path, description = "Sandbox identifier"),
        ExecutionQuery
    ),
    responses(
        (status = 200, description = "Recent executions", body = [ExecutionResponse]),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Sandbox not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn list_executions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<ExecutionQuery>,
) -> Result<Json<Vec<ExecutionResponse>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let meta = state.kernel.get_sandbox(id).await.map_err(ApiError::from)?;
    state
        .auth
        .authorize(
            require_bearer(&headers)?,
            ScopeRequirement::Namespace(&meta.namespace),
        )
        .await
        .map_err(ApiError::from)?;

    let executions = state
        .kernel
        .recent_executions(id, limit)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(
        executions
            .into_iter()
            .map(ExecutionResponse::from)
            .collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/models",
    responses(
        (status = 200, description = "List registered models", body = [ModelResponse]),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn list_models(
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
        (status = 400, description = "Invalid request", body = ErrorBody),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn register_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RegisterModelBody>,
) -> Result<(StatusCode, Json<ModelResponse>), ApiError> {
    let issuer = state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let RegisterModelBody {
        name,
        provider,
        version,
        format,
        source_uri,
        checksum_sha256,
        size_bytes,
        tags,
    } = payload;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("model name cannot be empty"));
    }

    let provider = provider.trim().to_string();
    if provider.is_empty() {
        return Err(ApiError::bad_request("provider cannot be empty"));
    }

    let version = version.trim().to_string();
    if version.is_empty() {
        return Err(ApiError::bad_request("version cannot be empty"));
    }

    let format = format.trim().to_string();
    if format.is_empty() {
        return Err(ApiError::bad_request("format cannot be empty"));
    }

    let source_uri = source_uri.trim().to_string();
    if source_uri.is_empty() {
        return Err(ApiError::bad_request("source_uri cannot be empty"));
    }

    let checksum_sha256 = checksum_sha256.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let mut tags_vec: Vec<String> = tags
        .unwrap_or_default()
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect();
    tags_vec.sort();
    tags_vec.dedup();
    let tags_slice = if tags_vec.is_empty() {
        None
    } else {
        Some(tags_vec.as_slice())
    };

    let checksum_ref = checksum_sha256.as_deref();
    let now = Utc::now();
    let model = state
        .db
        .insert_model(NewModel {
            name: &name,
            provider: &provider,
            version: &version,
            format: &format,
            source_uri: &source_uri,
            checksum_sha256: checksum_ref,
            size_bytes,
            tags: tags_slice,
            stage: ModelStage::Ready,
            last_synced_at: Some(now),
            error_message: None,
        })
        .await
        .map_err(ApiError::internal)?;

    state
        .db
        .insert_model_job(NewModelDownloadJob {
            model_id: model.id,
            stage: ModelStage::Ready,
            progress: 1.0,
            started_at: now,
            finished_at: Some(now),
            error_message: None,
        })
        .await
        .map_err(ApiError::internal)?;

    let actor_id = issuer.id.to_string();
    let audit_payload = json!({
        "model_id": model.id,
        "name": model.name,
        "provider": model.provider,
        "version": model.version,
        "stage": model.stage.as_str(),
    });
    state
        .db
        .record_audit_event(NewAuditEvent {
            namespace: None,
            actor: Some(actor_id.as_str()),
            event_type: "model.registered",
            recorded_at: now,
            payload: &audit_payload,
            signature_valid: None,
        })
        .await
        .map_err(ApiError::internal)?;

    Ok((StatusCode::CREATED, Json(ModelResponse::from(model))))
}

#[utoipa::path(
    post,
    path = "/api/v1/models/{id}/refresh",
    params(("id" = Uuid, Path, description = "Model identifier")),
    responses(
        (status = 200, description = "Model refreshed", body = ModelResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Model not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn refresh_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ModelResponse>, ApiError> {
    let issuer = state
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

    let now = Utc::now();
    state
        .db
        .insert_model_job(NewModelDownloadJob {
            model_id: id,
            stage: ModelStage::Ready,
            progress: 1.0,
            started_at: now,
            finished_at: Some(now),
            error_message: None,
        })
        .await
        .map_err(ApiError::internal)?;

    let updated = state
        .db
        .update_model_stage(id, ModelStage::Ready, Some(now), None)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "model not found"))?;

    let actor_id = issuer.id.to_string();
    let audit_payload = json!({
        "model_id": id,
        "previous_stage": existing.stage.as_str(),
        "stage": ModelStage::Ready.as_str(),
        "name": existing.name,
    });
    state
        .db
        .record_audit_event(NewAuditEvent {
            namespace: None,
            actor: Some(actor_id.as_str()),
            event_type: "model.refreshed",
            recorded_at: now,
            payload: &audit_payload,
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
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Model not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn delete_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let issuer = state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let record = state
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

    let actor_id = issuer.id.to_string();
    let audit_payload = json!({
        "model_id": id,
        "name": record.name,
        "provider": record.provider,
        "version": record.version,
    });
    state
        .db
        .record_audit_event(NewAuditEvent {
            namespace: None,
            actor: Some(actor_id.as_str()),
            event_type: "model.deleted",
            recorded_at: Utc::now(),
            payload: &audit_payload,
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
        (status = 200, description = "List model jobs", body = [ModelJobResponse]),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Model not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn list_model_jobs(
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
        .map_err(ApiError::internal)?;
    Ok(Json(jobs.into_iter().map(ModelJobResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/audit/events",
    params(AuditEventsQuery),
    responses(
        (status = 200, description = "List audit events", body = [AuditEventResponse]),
        (status = 400, description = "Invalid filters", body = ErrorBody),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn list_audit_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<AuditEventsQuery>,
) -> Result<Json<Vec<AuditEventResponse>>, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let since = if let Some(value) = query.since.as_deref() {
        Some(parse_rfc3339(value, "since")?)
    } else {
        None
    };
    let until = if let Some(value) = query.until.as_deref() {
        Some(parse_rfc3339(value, "until")?)
    } else {
        None
    };

    if let (Some(since), Some(until)) = (since, until) {
        if since > until {
            return Err(ApiError::bad_request("since must be before until"));
        }
    }

    let limit = query.limit.unwrap_or(50);
    if limit == 0 {
        return Err(ApiError::bad_request("limit must be greater than zero"));
    }

    let filter = AuditEventFilter {
        namespace: query.namespace.as_deref(),
        event_type: query.event_type.as_deref(),
        limit: Some(limit),
        since,
        until,
    };

    let events = state
        .db
        .list_audit_events(filter)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(
        events.into_iter().map(AuditEventResponse::from).collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/keys",
    request_body = CreateKeyBody,
    responses(
        (status = 201, description = "API key issued", body = IssuedKeyResponse),
        (status = 400, description = "Invalid request", body = ErrorBody),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn issue_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateKeyBody>,
) -> Result<(StatusCode, Json<IssuedKeyResponse>), ApiError> {
    let maybe_token = bearer_optional(&headers)?;
    if state.auth.has_keys().await.map_err(ApiError::internal)? {
        let token = maybe_token
            .ok_or_else(|| ApiError::unauthorized("missing Authorization bearer token"))?;
        state
            .auth
            .authorize(token, ScopeRequirement::Admin)
            .await
            .map_err(ApiError::from)?;
    }

    let scope = match payload.scope {
        CreateKeyScope::Admin => KeyScope::Admin,
        CreateKeyScope::Namespace { namespace } => KeyScope::Namespace { namespace },
    };

    let ttl = payload.ttl_seconds.map(Duration::from_secs);
    let issued = state
        .auth
        .issue_key(scope, payload.rate_limit.unwrap_or(100), ttl)
        .await
        .map_err(ApiError::internal)?;

    Ok((
        StatusCode::CREATED,
        Json(IssuedKeyResponse {
            token: issued.token,
            info: issued.info,
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/keys",
    responses(
        (status = 200, description = "List API keys", body = [KeyInfo]),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn list_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<KeyInfo>>, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let keys = state.auth.list_keys().await.map_err(ApiError::internal)?;
    Ok(Json(keys))
}

#[utoipa::path(
    delete,
    path = "/api/v1/auth/keys/{id}",
    params(("id" = Uuid, Path, description = "API key identifier")),
    responses(
        (status = 204, description = "Key revoked"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Key not found", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn revoke_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    state.auth.revoke(id).await.map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/keys/rotate",
    request_body = RotateKeyBody,
    responses(
        (status = 200, description = "Key rotated", body = RotatedKeyResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 404, description = "Key not found", body = ErrorBody),
        (status = 503, description = "Rotation webhook not configured", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn rotate_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RotateKeyBody>,
) -> Result<Json<RotatedKeyResponse>, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let ttl = payload.ttl_seconds.map(Duration::from_secs);
    let outcome = state
        .auth
        .rotate_key(payload.key_id, payload.rate_limit, ttl)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(RotatedKeyResponse::from(outcome)))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/keys/rotated",
    request_body = RotationWebhookPayload,
    responses(
        (status = 204, description = "Webhook signature verified"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorBody),
        (status = 403, description = "Insufficient permissions", body = ErrorBody),
        (status = 401, description = "Invalid webhook signature", body = ErrorBody)
    ),
    security(("bearerAuth" = []))
)]
async fn verify_rotation_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RotationWebhookPayload>,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .authorize(require_bearer(&headers)?, ScopeRequirement::Admin)
        .await
        .map_err(ApiError::from)?;

    let signature_header = headers
        .get("X-Cave-Webhook-Signature")
        .ok_or_else(|| ApiError::unauthorized("missing X-Cave-Webhook-Signature header"))?;
    let signature = signature_header
        .to_str()
        .map_err(|_| ApiError::unauthorized("invalid webhook signature header encoding"))?
        .trim();

    state
        .auth
        .verify_rotation_signature(&payload, signature)
        .map_err(ApiError::from)?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateSandboxBody {
    namespace: String,
    name: String,
    #[serde(default)]
    runtime: Option<String>,
    #[serde(default)]
    limits: Option<CreateSandboxLimits>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateSandboxLimits {
    #[serde(default)]
    cpu_millis: Option<u32>,
    #[serde(default)]
    memory_mib: Option<u64>,
    #[serde(default)]
    disk_mib: Option<u64>,
    #[serde(default)]
    timeout_seconds: Option<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateKeyBody {
    scope: CreateKeyScope,
    #[serde(default)]
    rate_limit: Option<u32>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct RotateKeyBody {
    key_id: Uuid,
    #[serde(default)]
    rate_limit: Option<u32>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CreateKeyScope {
    Admin,
    Namespace { namespace: String },
}

#[derive(Debug, Serialize, ToSchema)]
struct IssuedKeyResponse {
    token: String,
    info: KeyInfo,
}

#[derive(Debug, Serialize, ToSchema)]
struct RotatedKeyResponse {
    token: String,
    info: KeyInfo,
    previous: KeyInfo,
    webhook: RotationWebhookResponse,
}

#[derive(Debug, Serialize, ToSchema)]
struct RotationWebhookResponse {
    event_id: Uuid,
    signature: String,
    payload: RotationWebhookPayload,
}

impl From<RotationOutcome> for RotatedKeyResponse {
    fn from(outcome: RotationOutcome) -> Self {
        let RotationOutcome {
            new_key,
            previous,
            webhook,
        } = outcome;
        RotatedKeyResponse {
            token: new_key.token,
            info: new_key.info,
            previous,
            webhook: RotationWebhookResponse {
                event_id: webhook.event_id,
                signature: webhook.signature,
                payload: webhook.payload,
            },
        }
    }
}

impl CreateSandboxLimits {
    fn into_limits(self, defaults: ResourceLimits) -> ResourceLimits {
        ResourceLimits {
            cpu_limit_millis: self.cpu_millis.unwrap_or(defaults.cpu_limit_millis),
            memory_limit_bytes: self
                .memory_mib
                .map(mi_bytes)
                .unwrap_or(defaults.memory_limit_bytes),
            disk_limit_bytes: self
                .disk_mib
                .map(mi_bytes)
                .unwrap_or(defaults.disk_limit_bytes),
            timeout_seconds: self.timeout_seconds.unwrap_or(defaults.timeout_seconds),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
struct SandboxListQuery {
    namespace: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
struct ExecutionQuery {
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct ExecBody {
    command: String,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    stdin: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SandboxResponse {
    id: Uuid,
    namespace: String,
    name: String,
    runtime: String,
    status: String,
    limits: SandboxLimits,
    created_at: String,
    updated_at: String,
    last_started_at: Option<String>,
    last_stopped_at: Option<String>,
}

impl From<SandboxRecord> for SandboxResponse {
    fn from(record: SandboxRecord) -> Self {
        let limits = SandboxLimits::from(record.limits());
        let status = record.status.as_str().to_string();
        let created_at = record.created_at.to_rfc3339();
        let updated_at = record.updated_at.to_rfc3339();
        let last_started_at = record.last_started_at.map(|ts| ts.to_rfc3339());
        let last_stopped_at = record.last_stopped_at.map(|ts| ts.to_rfc3339());

        Self {
            id: record.id,
            namespace: record.namespace,
            name: record.name,
            runtime: record.runtime,
            status,
            limits,
            created_at,
            updated_at,
            last_started_at,
            last_stopped_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SandboxLimits {
    cpu_millis: u32,
    memory_mib: u64,
    disk_mib: u64,
    timeout_seconds: u32,
}

impl From<ResourceLimits> for SandboxLimits {
    fn from(limits: ResourceLimits) -> Self {
        Self {
            cpu_millis: limits.cpu_limit_millis,
            memory_mib: bytes_to_mib(limits.memory_limit_bytes),
            disk_mib: bytes_to_mib(limits.disk_limit_bytes),
            timeout_seconds: limits.timeout_seconds,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ExecResponse {
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    duration_ms: u64,
    timed_out: bool,
}

impl From<ExecOutcome> for ExecResponse {
    fn from(outcome: ExecOutcome) -> Self {
        let ExecOutcome {
            exit_code,
            stdout,
            stderr,
            duration,
            timed_out,
        } = outcome;

        Self {
            exit_code,
            stdout,
            stderr,
            duration_ms: duration.as_millis() as u64,
            timed_out,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ExecutionResponse {
    command: String,
    args: Vec<String>,
    executed_at: String,
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    duration_ms: u64,
    timed_out: bool,
}

impl From<ExecutionRecord> for ExecutionResponse {
    fn from(record: ExecutionRecord) -> Self {
        Self {
            command: record.command,
            args: record.args,
            executed_at: record.executed_at.to_rfc3339(),
            exit_code: record.exit_code,
            stdout: record.stdout,
            stderr: record.stderr,
            duration_ms: record.duration_ms,
            timed_out: record.timed_out,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
struct RegisterModelBody {
    name: String,
    provider: String,
    version: String,
    format: String,
    source_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ModelStageResponse {
    Unknown,
    Registered,
    Queued,
    Downloading,
    Verifying,
    Ready,
    Failed,
}

impl From<ModelStage> for ModelStageResponse {
    fn from(stage: ModelStage) -> Self {
        match stage {
            ModelStage::Unknown => ModelStageResponse::Unknown,
            ModelStage::Registered => ModelStageResponse::Registered,
            ModelStage::Queued => ModelStageResponse::Queued,
            ModelStage::Downloading => ModelStageResponse::Downloading,
            ModelStage::Verifying => ModelStageResponse::Verifying,
            ModelStage::Ready => ModelStageResponse::Ready,
            ModelStage::Failed => ModelStageResponse::Failed,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct ModelResponse {
    id: Uuid,
    name: String,
    provider: String,
    version: String,
    format: String,
    source_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum_sha256: Option<String>,
    stage: ModelStageResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_synced_at: Option<String>,
    created_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}

impl From<ModelRecord> for ModelResponse {
    fn from(record: ModelRecord) -> Self {
        let tags = if record.tags.is_empty() {
            None
        } else {
            Some(record.tags)
        };

        Self {
            id: record.id,
            name: record.name,
            provider: record.provider,
            version: record.version,
            format: record.format,
            source_uri: record.source_uri,
            size_bytes: record.size_bytes,
            checksum_sha256: record.checksum_sha256,
            stage: ModelStageResponse::from(record.stage),
            last_synced_at: record.last_synced_at.map(|ts| ts.to_rfc3339()),
            created_at: record.created_at.to_rfc3339(),
            updated_at: record.updated_at.to_rfc3339(),
            tags,
            error_message: record.error_message,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct ModelJobResponse {
    id: Uuid,
    model_id: Uuid,
    stage: ModelStageResponse,
    progress: f32,
    started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}

impl From<ModelDownloadJobRecord> for ModelJobResponse {
    fn from(record: ModelDownloadJobRecord) -> Self {
        Self {
            id: record.id,
            model_id: record.model_id,
            stage: ModelStageResponse::from(record.stage),
            progress: record.progress,
            started_at: record.started_at.to_rfc3339(),
            finished_at: record.finished_at.map(|ts| ts.to_rfc3339()),
            error_message: record.error_message,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct AuditEventResponse {
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    event_type: String,
    recorded_at: String,
    payload: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_valid: Option<bool>,
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

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
struct AuditEventsQuery {
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default, rename = "event_type")]
    event_type: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn internal<E: std::fmt::Display>(err: E) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }
}

impl From<KernelError> for ApiError {
    fn from(err: KernelError) -> Self {
        match err {
            KernelError::Sandbox(inner) => match inner {
                bkg_db::SandboxError::DuplicateSandbox(namespace, name) => ApiError::new(
                    StatusCode::CONFLICT,
                    format!(
                        "sandbox '{}' already exists in namespace '{}'",
                        name, namespace
                    ),
                ),
                bkg_db::SandboxError::NotFound(id) => {
                    ApiError::new(StatusCode::NOT_FOUND, format!("sandbox {} not found", id))
                }
            },
            KernelError::NotFound(id) => {
                ApiError::new(StatusCode::NOT_FOUND, format!("sandbox {} not found", id))
            }
            KernelError::AlreadyRunning(id) => ApiError::new(
                StatusCode::CONFLICT,
                format!("sandbox {} is already running", id),
            ),
            KernelError::NotRunning(id) => ApiError::new(
                StatusCode::CONFLICT,
                format!("sandbox {} is not running", id),
            ),
            KernelError::Runtime(err) => ApiError::internal(err),
            KernelError::Storage(err) => ApiError::internal(err),
            KernelError::Io(path, err) => {
                ApiError::internal(format!("{} ({})", path.display(), err))
            }
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::InvalidToken => ApiError::unauthorized("invalid API key"),
            AuthError::Unauthorized => ApiError::new(
                StatusCode::FORBIDDEN,
                "insufficient permissions for requested scope",
            ),
            AuthError::NotFound => ApiError::new(StatusCode::NOT_FOUND, "key not found"),
            AuthError::WebhookNotConfigured => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "rotation webhook secret is not configured",
            ),
            AuthError::InvalidSignature => ApiError::unauthorized("invalid webhook signature"),
            AuthError::Internal(message) => ApiError::internal(message),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        error!(status = %self.status, message = %self.message, "api error");
        let body = Json(ErrorBody {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct ErrorBody {
    error: String,
}

fn mi_bytes(value: u64) -> u64 {
    value * 1024 * 1024
}

pub mod docs {
    use super::*;
    use utoipa::openapi::security::SecurityRequirement;

    #[derive(OpenApi)]
    #[openapi(
        info(title = "CAVE Daemon API", version = "0.1.0"),
        paths(
            healthz,
            metrics,
            create_sandbox,
            list_sandboxes,
            get_sandbox,
            start_sandbox,
            exec_sandbox,
            stop_sandbox,
            delete_sandbox,
            list_executions,
            list_models,
            register_model,
            refresh_model,
            delete_model,
            list_model_jobs,
            list_audit_events,
            issue_key,
            list_keys,
            rotate_key,
            verify_rotation_webhook,
            revoke_key
        ),
        components(
            schemas(
                CreateSandboxBody,
                CreateSandboxLimits,
                SandboxResponse,
                SandboxLimits,
                ExecBody,
                ExecResponse,
                ExecutionResponse,
                RegisterModelBody,
                ModelResponse,
                ModelStageResponse,
                ModelJobResponse,
                AuditEventResponse,
                AuditEventsQuery,
                ErrorBody,
                CreateKeyBody,
                CreateKeyScope,
                RotateKeyBody,
                IssuedKeyResponse,
                RotatedKeyResponse,
                RotationWebhookResponse,
                KeyInfo,
                KeyScope,
                RotationWebhookPayload
            ),
            security_schemes(
                bearerAuth = (
                    type = "http",
                    scheme = "bearer",
                    bearer_format = "API Token",
                    description = "Bearer token issued via /api/v1/auth/keys"
                )
            )
        ),
        modifiers(&SecurityAddon)
    )]
    pub struct ApiDoc;

    struct SecurityAddon;

    impl Modify for SecurityAddon {
        fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
            openapi
                .security
                .get_or_insert_with(Default::default)
                .push(SecurityRequirement::new("bearerAuth", Vec::<String>::new()));
        }
    }
}

fn bytes_to_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn require_bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    bearer_optional(headers)?
        .ok_or_else(|| ApiError::unauthorized("missing Authorization bearer token"))
}

fn bearer_optional(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let header_value = value
            .to_str()
            .map_err(|_| ApiError::unauthorized("invalid Authorization header encoding"))?;
        if let Some(token) = header_value.strip_prefix("Bearer ") {
            Ok(Some(token.trim()))
        } else {
            Err(ApiError::unauthorized(
                "Authorization header must be a Bearer token",
            ))
        }
    } else {
        Ok(None)
    }
}

fn parse_rfc3339(value: &str, field: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| ApiError::bad_request(format!("invalid {field} timestamp")))
}

fn bool_env(key: &str) -> Option<bool> {
    env::var(key)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

fn parse_string_list_env(key: &str) -> Option<Vec<String>> {
    env::var(key).ok().map(|value| {
        value
            .split(',')
            .filter_map(|item| {
                let trimmed = item.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect::<Vec<_>>()
    })
}

fn parse_path_list_env(key: &str) -> Option<Vec<PathBuf>> {
    parse_string_list_env(key).map(|items| items.into_iter().map(PathBuf::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tempfile::TempDir;
    use tower::Service;

    use crate::auth::{AuthService, KeyScope, RotationWebhookPayload};
    use bkg_db::AuditEventFilter;

    use std::sync::Arc;

    async fn setup_test_app() -> (Arc<AppState>, Router, TempDir) {
        let temp = TempDir::new().expect("tempdir");
        let db_path = temp.path().join(format!("db-{}.sqlite", Uuid::new_v4()));
        let db_url = format!("sqlite://{}", db_path.display());
        let db = Database::connect(&db_url).await.expect("db");

        let isolation = IsolationSettings {
            enable_namespaces: false,
            enable_cgroups: false,
            bubblewrap_path: None,
            cgroup_root: None,
            ..IsolationSettings::default()
        };

        let audit = AuditConfig {
            enabled: false,
            log_path: temp.path().join("audit.jsonl"),
            hmac_key: None,
        };

        let kernel_cfg = KernelConfig {
            workspace_root: temp.path().join("workspaces"),
            default_runtime: "process".to_string(),
            default_limits: ResourceLimits::default(),
            isolation: isolation.clone(),
            audit,
        };

        let runtime = ProcessSandboxRuntime::new(isolation).expect("runtime");
        let kernel = CaveKernel::new(db.clone(), runtime, kernel_cfg);
        let auth = Arc::new(AuthService::new(
            db.clone(),
            Some(b"rotation-secret".to_vec()),
        ));
        let state = Arc::new(AppState {
            kernel,
            db: db.clone(),
            auth,
        });
        let router = build_router(state.clone());
        (state, router, temp)
    }

    #[test]
    fn limit_conversion_roundtrip() {
        let defaults = ResourceLimits::default();
        let limits = CreateSandboxLimits {
            cpu_millis: Some(750),
            memory_mib: Some(1024),
            disk_mib: Some(1024),
            timeout_seconds: Some(90),
        };

        let converted = limits.into_limits(defaults);
        assert_eq!(converted.cpu_limit_millis, 750);
        assert_eq!(converted.memory_limit_bytes, mi_bytes(1024));
        assert_eq!(converted.disk_limit_bytes, mi_bytes(1024));
        assert_eq!(converted.timeout_seconds, 90);
    }

    #[test]
    fn parse_sampling_rate_defaults_to_one() {
        let (rate, warning) = parse_sampling_rate(None);
        assert_eq!(rate, 1.0);
        assert!(warning.is_none());

        let (rate, warning) = parse_sampling_rate(Some("not-a-number"));
        assert_eq!(rate, 1.0);
        assert!(warning
            .unwrap()
            .contains("CAVE_OTEL_SAMPLING_RATE='not-a-number'"));
    }

    #[test]
    fn parse_sampling_rate_clamps_out_of_range() {
        let (rate, warning) = parse_sampling_rate(Some("1.5"));
        assert_eq!(rate, 1.0);
        assert!(warning
            .unwrap()
            .contains("CAVE_OTEL_SAMPLING_RATE=1.5 outside"));

        let (rate, warning) = parse_sampling_rate(Some("-0.3"));
        assert_eq!(rate, 0.0);
        assert!(warning
            .unwrap()
            .contains("CAVE_OTEL_SAMPLING_RATE=-0.3 outside"));
    }

    #[tokio::test]
    async fn register_model_creates_job_and_audit() {
        let (state, mut router, _tmp) = setup_test_app().await;
        let admin = state
            .auth
            .issue_key(KeyScope::Admin, 1000, None)
            .await
            .expect("admin key");

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/models")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", admin.token))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "name": "phi-3",
                    "provider": "huggingface",
                    "version": "1.0.0",
                    "format": "gguf",
                    "source_uri": "https://example.com/models/phi-3.gguf"
                }))
                .unwrap(),
            ))
            .expect("request");

        let response = router.call(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let model_json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(model_json["name"], "phi-3");
        assert_eq!(model_json["stage"], "ready");
        let model_id = Uuid::parse_str(model_json["id"].as_str().unwrap()).unwrap();

        let list_request = Request::builder()
            .method("GET")
            .uri("/api/v1/models")
            .header("authorization", format!("Bearer {}", admin.token))
            .body(Body::empty())
            .expect("list request");
        let list_response = router.call(list_request).await.expect("list response");
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_body = to_bytes(list_response.into_body(), usize::MAX)
            .await
            .expect("list body");
        let models: Vec<Value> = serde_json::from_slice(&list_body).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], model_json["id"]);

        let jobs_request = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/models/{model_id}/jobs"))
            .header("authorization", format!("Bearer {}", admin.token))
            .body(Body::empty())
            .expect("jobs request");
        let jobs_response = router.call(jobs_request).await.expect("jobs response");
        assert_eq!(jobs_response.status(), StatusCode::OK);
        let jobs_body = to_bytes(jobs_response.into_body(), usize::MAX)
            .await
            .expect("jobs body");
        let jobs: Vec<Value> = serde_json::from_slice(&jobs_body).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["stage"], "ready");
        assert_eq!(jobs[0]["progress"].as_f64().unwrap(), 1.0);

        let audit_request = Request::builder()
            .method("GET")
            .uri("/api/v1/audit/events")
            .header("authorization", format!("Bearer {}", admin.token))
            .body(Body::empty())
            .expect("audit request");
        let audit_response = router.call(audit_request).await.expect("audit response");
        assert_eq!(audit_response.status(), StatusCode::OK);
        let audit_body = to_bytes(audit_response.into_body(), usize::MAX)
            .await
            .expect("audit body");
        let events: Vec<Value> = serde_json::from_slice(&audit_body).unwrap();
        assert!(!events.is_empty());
        assert_eq!(events[0]["event_type"], "model.registered");

        let audit_events = state
            .db
            .list_audit_events(AuditEventFilter {
                namespace: None,
                event_type: Some("model.registered"),
                limit: Some(5),
                since: None,
                until: None,
            })
            .await
            .expect("audit events");
        assert_eq!(audit_events.len(), 1);
        assert_eq!(audit_events[0].event_type, "model.registered");
    }

    #[tokio::test]
    async fn model_endpoints_require_admin_scope() {
        let (state, mut router, _tmp) = setup_test_app().await;
        let namespace = state
            .auth
            .issue_key(
                KeyScope::Namespace {
                    namespace: "team-alpha".into(),
                },
                120,
                None,
            )
            .await
            .expect("namespace key");

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/models")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", namespace.token))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "name": "phi-3",
                    "provider": "huggingface",
                    "version": "1.0.0",
                    "format": "gguf",
                    "source_uri": "https://example.com/models/phi-3.gguf"
                }))
                .unwrap(),
            ))
            .expect("request");

        let response = router.call(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let audit_request = Request::builder()
            .method("GET")
            .uri("/api/v1/audit/events")
            .header("authorization", format!("Bearer {}", namespace.token))
            .body(Body::empty())
            .expect("audit request");
        let audit_response = router.call(audit_request).await.expect("audit response");
        assert_eq!(audit_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn rotate_key_succeeds_and_enqueues_event() {
        let (state, mut router, _tmp) = setup_test_app().await;
        let admin = state
            .auth
            .issue_key(KeyScope::Admin, 1000, None)
            .await
            .expect("admin key");
        let original = state
            .auth
            .issue_key(
                KeyScope::Namespace {
                    namespace: "team-alpha".into(),
                },
                120,
                None,
            )
            .await
            .expect("namespace key");

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/keys/rotate")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", admin.token))
            .body(Body::from(
                serde_json::to_vec(&json!({"key_id": original.info.id})).unwrap(),
            ))
            .expect("request");

        let response = router.call(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let rotated_from = body_json["info"]["rotated_from"].as_str().unwrap();
        assert_eq!(rotated_from, original.info.id.to_string());

        let old_authorization = state
            .auth
            .authorize(&original.token, ScopeRequirement::Namespace("team-alpha"))
            .await;
        assert!(matches!(old_authorization, Err(AuthError::InvalidToken)));

        let events = state.db.list_key_rotation_events().await.expect("events");
        assert_eq!(events.len(), 1);
        let new_key_id = Uuid::parse_str(body_json["info"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(events[0].new_key_id, new_key_id);
        assert_eq!(events[0].previous_key_id, original.info.id);
        assert!(!body_json["webhook"]["signature"]
            .as_str()
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn rotate_key_rejects_namespace_scope() {
        let (state, mut router, _tmp) = setup_test_app().await;
        let namespace = state
            .auth
            .issue_key(
                KeyScope::Namespace {
                    namespace: "team-alpha".into(),
                },
                120,
                None,
            )
            .await
            .expect("namespace key");

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/keys/rotate")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", namespace.token))
            .body(Body::from(
                serde_json::to_vec(&json!({"key_id": namespace.info.id})).unwrap(),
            ))
            .expect("request");

        let response = router.call(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn verify_rotation_webhook_requires_signature() {
        let (state, mut router, _tmp) = setup_test_app().await;
        let admin = state
            .auth
            .issue_key(KeyScope::Admin, 1000, None)
            .await
            .expect("admin key");

        let payload = RotationWebhookPayload {
            event: "key.rotated".to_string(),
            key_id: Uuid::new_v4(),
            previous_key_id: Uuid::new_v4(),
            rotated_at: chrono::Utc::now(),
            scope: KeyScope::Admin,
            owner: "admin".into(),
            key_prefix: "demo".into(),
        };

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/keys/rotated")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", admin.token))
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .expect("request");

        let response = router.call(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn verify_rotation_webhook_rejects_invalid_signature() {
        let (state, mut router, _tmp) = setup_test_app().await;
        let admin = state
            .auth
            .issue_key(KeyScope::Admin, 1000, None)
            .await
            .expect("admin key");

        let payload = RotationWebhookPayload {
            event: "key.rotated".to_string(),
            key_id: Uuid::new_v4(),
            previous_key_id: Uuid::new_v4(),
            rotated_at: chrono::Utc::now(),
            scope: KeyScope::Admin,
            owner: "admin".into(),
            key_prefix: "demo".into(),
        };

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/keys/rotated")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", admin.token))
            .header("X-Cave-Webhook-Signature", "invalid-signature")
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .expect("request");

        let response = router.call(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
