use std::{env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

mod auth;

use anyhow::{Context, Result};
use auth::{AuthError, AuthService, KeyInfo, KeyScope, ScopeRequirement};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bkg_db::{Database, ExecutionRecord, ResourceLimits, SandboxRecord};
use cave_kernel::{
    AuditConfig, CaveKernel, CreateSandboxRequest, ExecOutcome, ExecRequest, IsolationSettings,
    KernelConfig, KernelError, ProcessSandboxRuntime,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};
use tracing_subscriber::{
    filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_tracing();

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
    let auth = Arc::new(AuthService::new(db.clone()));
    let state = Arc::new(AppState { kernel, db, auth });

    let app = build_router(state.clone()).layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.listen_addr)
        .await
        .context("failed to bind listen socket")?;

    info!(addr = %config.listen_addr, "cave-daemon listening");
    axum::serve(listener, app)
        .await
        .context("HTTP server exited")?;
    Ok(())
}

fn initialize_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let (sampling_rate, warning) = read_sampling_rate();

    if sampling_rate >= 1.0 {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    } else {
        let rate = sampling_rate;
        let sampling_filter = filter_fn(move |metadata| {
            if metadata.is_event() {
                rand::thread_rng().gen_bool(rate)
            } else {
                true
            }
        });

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().with_filter(sampling_filter))
            .init();
    }

    if let Some(message) = warning {
        warn!("{message}");
    }

    info!(sampling_rate, "telemetry sampling configured");
}

fn read_sampling_rate() -> (f64, Option<String>) {
    let raw = env::var("CAVE_OTEL_SAMPLING_RATE").ok();
    parse_sampling_rate(raw.as_deref())
}

fn parse_sampling_rate(raw: Option<&str>) -> (f64, Option<String>) {
    match raw {
        None => (1.0, None),
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return (
                    1.0,
                    Some("CAVE_OTEL_SAMPLING_RATE is empty; defaulting to 1.0".to_string()),
                );
            }

            match trimmed.parse::<f64>() {
                Ok(parsed) => {
                    if (0.0..=1.0).contains(&parsed) {
                        (parsed, None)
                    } else {
                        let clamped = parsed.clamp(0.0, 1.0);
                        (
                            clamped,
                            Some(format!(
                                "CAVE_OTEL_SAMPLING_RATE={} outside 0.0..=1.0; clamped to {}",
                                trimmed, clamped
                            )),
                        )
                    }
                }
                Err(_) => (
                    1.0,
                    Some(format!(
                        "CAVE_OTEL_SAMPLING_RATE='{}' is not a valid float; defaulting to 1.0",
                        trimmed
                    )),
                ),
            }
        }
    }
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
        .route("/api/v1/auth/keys", post(issue_key).get(list_keys))
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
}

impl AppConfig {
    fn from_env() -> Result<Self> {
        let listen_addr = env::var("CAVE_API_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()
            .context("invalid CAVE_API_ADDR")?;

        let db_url = match env::var("BKG_DB_DSN") {
            Ok(url) => url,
            Err(_) => {
                let path = env::var("BKG_DB_PATH").unwrap_or_else(|_| "./bkg.db".to_string());
                format!("sqlite://{}", path)
            }
        };

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

        Ok(Self {
            listen_addr,
            db_url,
            workspace_root,
            default_runtime,
            default_limits,
            isolation,
            audit,
        })
    }
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn metrics() -> impl IntoResponse {
    (
        StatusCode::OK,
        "# metrics placeholder\nbkg_cave_daemon_up 1\n",
    )
}

async fn create_sandbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateSandboxBody>,
) -> Result<Json<SandboxResponse>, ApiError> {
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
    Ok(Json(SandboxResponse::from(record)))
}

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

async fn issue_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateKeyBody>,
) -> Result<Json<IssuedKeyResponse>, ApiError> {
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

    Ok(Json(IssuedKeyResponse {
        token: issued.token,
        info: issued.info,
    }))
}

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

#[derive(Debug, Deserialize)]
struct CreateSandboxBody {
    namespace: String,
    name: String,
    #[serde(default)]
    runtime: Option<String>,
    #[serde(default)]
    limits: Option<CreateSandboxLimits>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct CreateKeyBody {
    scope: CreateKeyScope,
    #[serde(default)]
    rate_limit: Option<u32>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CreateKeyScope {
    Admin,
    Namespace { namespace: String },
}

#[derive(Debug, Serialize)]
struct IssuedKeyResponse {
    token: String,
    info: KeyInfo,
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

#[derive(Debug, Deserialize)]
struct SandboxListQuery {
    namespace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExecutionQuery {
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ExecBody {
    command: String,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    stdin: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

fn mi_bytes(value: u64) -> u64 {
    value * 1024 * 1024
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

fn bool_env(key: &str) -> Option<bool> {
    env::var(key)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::auth::{AuthService, KeyScope};
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use serde_json::json;
    use tempfile::TempDir;
    use tower::Service;

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
}
