use std::{env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use bkg_db::{Database, ExecutionRecord, ResourceLimits, SandboxRecord};
use cave_kernel::{
    CaveKernel, CreateSandboxRequest, ExecOutcome, ExecRequest, KernelConfig, KernelError,
    ProcessSandboxRuntime,
};
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_tracing();

    let config = AppConfig::from_env()?;

    let db = Database::connect(&config.db_url)
        .await
        .context("failed to open database")?;

    let mut kernel_cfg = KernelConfig::default();
    kernel_cfg.workspace_root = config.workspace_root.clone();
    kernel_cfg.default_runtime = config.default_runtime.clone();
    kernel_cfg.default_limits = config.default_limits;

    let kernel = CaveKernel::new(db.clone(), ProcessSandboxRuntime, kernel_cfg);
    let state = Arc::new(AppState { kernel, db });

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
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
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
        .with_state(state)
}

#[derive(Clone)]
struct AppState {
    kernel: CaveKernel<ProcessSandboxRuntime>,
    db: Database,
}

#[derive(Debug, Clone)]
struct AppConfig {
    listen_addr: SocketAddr,
    db_url: String,
    workspace_root: PathBuf,
    default_runtime: String,
    default_limits: ResourceLimits,
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

        Ok(Self {
            listen_addr,
            db_url,
            workspace_root,
            default_runtime,
            default_limits,
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
    Json(payload): Json<CreateSandboxBody>,
) -> Result<Json<SandboxResponse>, ApiError> {
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
    Query(query): Query<SandboxListQuery>,
) -> Result<Json<Vec<SandboxResponse>>, ApiError> {
    let namespace = query
        .namespace
        .ok_or_else(|| ApiError::bad_request("namespace query parameter is required"))?;

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
    Path(id): Path<Uuid>,
) -> Result<Json<SandboxResponse>, ApiError> {
    let record = state.kernel.get_sandbox(id).await.map_err(ApiError::from)?;
    Ok(Json(SandboxResponse::from(record)))
}

async fn start_sandbox(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<SandboxResponse>, ApiError> {
    let record = state
        .kernel
        .start_sandbox(id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(SandboxResponse::from(record)))
}

async fn exec_sandbox(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ExecBody>,
) -> Result<Json<ExecResponse>, ApiError> {
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
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .kernel
        .stop_sandbox(id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_sandbox(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .kernel
        .delete_sandbox(id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_executions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExecutionQuery>,
) -> Result<Json<Vec<ExecutionResponse>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
