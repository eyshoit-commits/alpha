//! Core CAVE kernel orchestration logic.
//!
//! The implementation focuses on the Phase‑0 requirements laid out in the
//! README: provisioning sandboxes, enforcing default resource policies,
//! orchestrating lifecycle transitions and persisting execution audits via the
//! `bkg-db` crate. Real low-level sandboxing (namespaces, seccomp, etc.) will be
//! layered in later; the current runtime is a safe process isolation shim that
//! operates within a prepared workspace directory.

mod audit;
mod isolation;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use audit::{AuditEvent, AuditLogWriter};
use bkg_db::{
    self, Database, ExecutionRecord, NewSandbox, ResourceLimits, SandboxError, SandboxRecord,
    SandboxStatus,
};
use chrono::Utc;
use isolation::{add_pid_to_cgroup, cleanup_cgroup, prepare_cgroup};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt, process::Command, sync::Mutex, task::spawn_blocking};
use tracing::{info, instrument, warn};
use uuid::Uuid;
use which::which;

const DEFAULT_RUNTIME_KIND: &str = "process";

pub use audit::AuditConfig;

/// Logical configuration driving the kernel behaviour.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    pub workspace_root: PathBuf,
    pub default_limits: ResourceLimits,
    pub default_runtime: String,
    pub isolation: IsolationSettings,
    pub audit: AuditConfig,
}

impl KernelConfig {
    pub fn workspace_for(&self, namespace: &str, id: Uuid) -> PathBuf {
        let ns_component = sanitize_component(namespace);
        self.workspace_root.join(ns_component).join(id.to_string())
    }
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("./.cave_workspaces"),
            default_limits: ResourceLimits::default(),
            default_runtime: DEFAULT_RUNTIME_KIND.to_string(),
            isolation: IsolationSettings::default(),
            audit: AuditConfig::default(),
        }
    }
}

/// Configures how the runtime applies host isolation primitives.
#[derive(Debug, Clone)]
pub struct IsolationSettings {
    pub enable_namespaces: bool,
    pub enable_cgroups: bool,
    pub bubblewrap_path: Option<PathBuf>,
    pub cgroup_root: Option<PathBuf>,
    pub fallback_to_plain: bool,
    pub overlay: OverlayConfig,
    pub seccomp: Option<SeccompConfig>,
}

impl Default for IsolationSettings {
    fn default() -> Self {
        Self {
            enable_namespaces: true,
            enable_cgroups: true,
            bubblewrap_path: None,
            cgroup_root: Some(PathBuf::from("/sys/fs/cgroup/bkg")),
            fallback_to_plain: true,
            overlay: OverlayConfig::default(),
            seccomp: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OverlayConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeccompAction {
    Errno(i32),
    KillProcess,
}

impl Default for SeccompAction {
    fn default() -> Self {
        Self::Errno(1)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SeccompConfig {
    pub action: SeccompAction,
    pub deny_syscalls: Vec<String>,
}

impl SeccompConfig {
    pub fn deny(mut self, syscall: impl Into<String>) -> Self {
        self.deny_syscalls.push(syscall.into());
        self
    }
}

/// High-level API exposed by the CAVE kernel.
pub struct CaveKernel<R>
where
    R: SandboxRuntime,
{
    db: Database,
    runtime: Arc<R>,
    config: KernelConfig,
    instances: Arc<RwLock<HashMap<Uuid, Arc<dyn SandboxInstance>>>>,
    audit: Option<Arc<AuditLogWriter>>,
}

impl<R> Clone for CaveKernel<R>
where
    R: SandboxRuntime,
{
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            runtime: self.runtime.clone(),
            config: self.config.clone(),
            instances: self.instances.clone(),
            audit: self.audit.clone(),
        }
    }
}

impl<R> CaveKernel<R>
where
    R: SandboxRuntime,
{
    pub fn new(db: Database, runtime: R, config: KernelConfig) -> Self {
        let audit = if config.audit.enabled {
            match AuditLogWriter::try_new(&config.audit) {
                Ok(writer) => Some(Arc::new(writer)),
                Err(err) => {
                    warn!(error = %err, "failed to initialize audit log writer; disabling audits");
                    None
                }
            }
        } else {
            None
        };

        Self {
            db,
            runtime: Arc::new(runtime),
            config,
            instances: Arc::new(RwLock::new(HashMap::new())),
            audit,
        }
    }

    pub fn config(&self) -> &KernelConfig {
        &self.config
    }

    async fn record_audit(&self, event: AuditEvent) {
        if let Some(writer) = self.audit.clone() {
            if let Err(err) = writer.append(&event).await {
                warn!(
                    sandbox_id = %event.sandbox_id,
                    error = %err,
                    "failed to append audit log entry"
                );
            }
        }
    }

    /// Creates a sandbox entry and enforces namespace uniqueness.
    #[instrument(skip(self, request))]
    pub async fn create_sandbox(
        &self,
        request: CreateSandboxRequest,
    ) -> Result<SandboxRecord, KernelError> {
        let limits = request
            .resource_limits
            .unwrap_or(self.config.default_limits);
        let runtime = request
            .runtime
            .as_deref()
            .unwrap_or(&self.config.default_runtime)
            .to_owned();

        let record = self
            .db
            .create_sandbox(NewSandbox::with_limits(
                &request.namespace,
                &request.name,
                &runtime,
                limits,
            ))
            .await
            .map_err(KernelError::from)?;

        info!(sandbox_id = %record.id, namespace = %record.namespace, "sandbox created");
        self.record_audit(AuditEvent::sandbox_created(
            record.id,
            record.namespace.clone(),
            record.name.clone(),
            record.runtime.clone(),
            record.limits(),
        ))
        .await;
        Ok(record)
    }

    /// Boots (or reboots) the sandbox runtime and transitions lifecycle state.
    #[instrument(skip(self))]
    pub async fn start_sandbox(&self, id: Uuid) -> Result<SandboxRecord, KernelError> {
        let record = self
            .db
            .fetch_sandbox(id)
            .await
            .map_err(KernelError::from)?
            .ok_or(KernelError::NotFound(id))?;

        if self.instances.read().contains_key(&id) {
            return Err(KernelError::AlreadyRunning(id));
        }

        self.db
            .update_status(id, SandboxStatus::Preparing)
            .await
            .map_err(KernelError::from)?;

        let workspace = self.config.workspace_for(&record.namespace, record.id);
        fs::create_dir_all(&workspace)
            .await
            .map_err(|err| KernelError::Io(workspace.clone(), err))?;

        match self.runtime.spawn(&record, &workspace).await {
            Ok(instance) => {
                self.instances.write().insert(id, instance);
                self.db
                    .touch_last_started(id)
                    .await
                    .map_err(KernelError::from)?;
                self.db
                    .update_status(id, SandboxStatus::Running)
                    .await
                    .map_err(KernelError::from)?;
                let updated = self
                    .db
                    .fetch_sandbox(id)
                    .await
                    .map_err(KernelError::from)?
                    .expect("sandbox must exist after start");
                info!(sandbox_id = %id, "sandbox running");
                self.record_audit(AuditEvent::sandbox_started(
                    updated.id,
                    updated.namespace.clone(),
                ))
                .await;
                Ok(updated)
            }
            Err(err) => {
                warn!(sandbox_id = %id, error = %err, "sandbox failed to start");
                self.db
                    .update_status(id, SandboxStatus::Failed)
                    .await
                    .map_err(KernelError::from)?;
                Err(KernelError::Runtime(err))
            }
        }
    }

    /// Executes a command within the sandbox runtime and persists audit logs.
    #[instrument(skip(self, request))]
    pub async fn exec(&self, id: Uuid, request: ExecRequest) -> Result<ExecOutcome, KernelError> {
        let record = self
            .db
            .fetch_sandbox(id)
            .await
            .map_err(KernelError::from)?
            .ok_or(KernelError::NotFound(id))?;

        let instance = self
            .instances
            .read()
            .get(&id)
            .cloned()
            .ok_or(KernelError::NotRunning(id))?;

        let effective_request =
            request.with_default_timeout(Duration::from_secs(record.timeout_seconds as u64));
        let runtime_request = effective_request.clone();
        let command_for_log = effective_request.command.clone();
        let args_for_log = effective_request.args.clone();
        let outcome = instance
            .exec(runtime_request)
            .await
            .map_err(KernelError::Runtime)?;

        let audit_entry = ExecutionRecord {
            sandbox_id: record.id,
            executed_at: Utc::now(),
            command: effective_request.command,
            args: effective_request.args,
            exit_code: outcome.exit_code,
            stdout: outcome.stdout.clone(),
            stderr: outcome.stderr.clone(),
            duration_ms: outcome.duration_ms(),
            timed_out: outcome.timed_out,
        };

        self.db
            .record_execution(audit_entry)
            .await
            .map_err(KernelError::from)?;

        self.record_audit(AuditEvent::sandbox_exec(
            record.id,
            record.namespace.clone(),
            command_for_log,
            args_for_log,
            outcome.exit_code,
            outcome.duration_ms(),
            outcome.timed_out,
        ))
        .await;

        Ok(outcome)
    }

    /// Stops the runtime instance and updates state tracking.
    #[instrument(skip(self))]
    pub async fn stop_sandbox(&self, id: Uuid) -> Result<(), KernelError> {
        let record = self
            .db
            .fetch_sandbox(id)
            .await
            .map_err(KernelError::from)?
            .ok_or(KernelError::NotFound(id))?;

        let instance = self
            .instances
            .write()
            .remove(&id)
            .ok_or(KernelError::NotRunning(id))?;

        instance.stop().await.map_err(KernelError::Runtime)?;
        self.db
            .touch_last_stopped(id)
            .await
            .map_err(KernelError::from)?;
        self.db
            .update_status(id, SandboxStatus::Stopped)
            .await
            .map_err(KernelError::from)?;
        info!(sandbox_id = %id, "sandbox stopped");
        self.record_audit(AuditEvent::sandbox_stopped(
            record.id,
            record.namespace.clone(),
        ))
        .await;
        Ok(())
    }

    /// Deletes the sandbox record and removes associated workspace on disk.
    #[instrument(skip(self))]
    pub async fn delete_sandbox(&self, id: Uuid) -> Result<(), KernelError> {
        if self.instances.read().contains_key(&id) {
            return Err(KernelError::AlreadyRunning(id));
        }

        let record = self
            .db
            .fetch_sandbox(id)
            .await
            .map_err(KernelError::from)?
            .ok_or(KernelError::NotFound(id))?;

        let workspace = self.config.workspace_for(&record.namespace, record.id);
        self.runtime
            .destroy(record.id, &workspace)
            .await
            .map_err(KernelError::Runtime)?;

        self.db
            .delete_sandbox(id)
            .await
            .map_err(KernelError::from)?;

        info!(sandbox_id = %id, "sandbox deleted");
        self.record_audit(AuditEvent::sandbox_deleted(
            record.id,
            record.namespace.clone(),
        ))
        .await;
        Ok(())
    }

    /// Returns the current metadata snapshot from persistence.
    pub async fn get_sandbox(&self, id: Uuid) -> Result<SandboxRecord, KernelError> {
        self.db
            .fetch_sandbox(id)
            .await
            .map_err(KernelError::from)?
            .ok_or(KernelError::NotFound(id))
    }

    /// Fetches the most recent execution audit entries.
    pub async fn recent_executions(
        &self,
        id: Uuid,
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>, KernelError> {
        let _ = self
            .db
            .fetch_sandbox(id)
            .await
            .map_err(KernelError::from)?
            .ok_or(KernelError::NotFound(id))?;

        self.db
            .list_executions(id, limit)
            .await
            .map_err(KernelError::from)
    }
}

#[derive(Debug, Error)]
pub enum KernelError {
    #[error(transparent)]
    Storage(anyhow::Error),
    #[error(transparent)]
    Sandbox(SandboxError),
    #[error("sandbox {0} is already running")]
    AlreadyRunning(Uuid),
    #[error("sandbox {0} is not running")]
    NotRunning(Uuid),
    #[error("sandbox {0} not found")]
    NotFound(Uuid),
    #[error("runtime operation failed: {0}")]
    Runtime(anyhow::Error),
    #[error("failed to manipulate workspace {0}: {1}")]
    Io(PathBuf, std::io::Error),
}

impl From<anyhow::Error> for KernelError {
    fn from(value: anyhow::Error) -> Self {
        match value.downcast::<SandboxError>() {
            Ok(sandbox_err) => KernelError::Sandbox(sandbox_err),
            Err(other) => KernelError::Storage(other),
        }
    }
}

/// Request payload when provisioning a new sandbox.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateSandboxRequest {
    pub namespace: String,
    pub name: String,
    pub runtime: Option<String>,
    pub resource_limits: Option<ResourceLimits>,
}

impl CreateSandboxRequest {
    pub fn new<N: Into<String>, K: Into<String>>(namespace: N, name: K) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
            runtime: None,
            resource_limits: None,
        }
    }
}

/// Execution request routed to runtime instances.
#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub command: String,
    pub args: Vec<String>,
    pub stdin: Option<String>,
    pub timeout: Option<Duration>,
}

impl ExecRequest {
    pub fn with_command<S: Into<String>>(command: S) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            stdin: None,
            timeout: None,
        }
    }

    pub fn with_default_timeout(mut self, default: Duration) -> Self {
        if self.timeout.is_none() {
            self.timeout = Some(default);
        }
        self
    }
}

/// Result of an execution inside the sandbox.
#[derive(Debug, Clone)]
pub struct ExecOutcome {
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration: Duration,
    pub timed_out: bool,
}

impl ExecOutcome {
    pub fn duration_ms(&self) -> u64 {
        self.duration.as_millis() as u64
    }
}

#[async_trait]
pub trait SandboxRuntime: Send + Sync + 'static {
    async fn spawn(
        &self,
        sandbox: &SandboxRecord,
        workspace: &Path,
    ) -> Result<Arc<dyn SandboxInstance>>;
    async fn destroy(&self, sandbox_id: Uuid, workspace: &Path) -> Result<()>;
}

#[async_trait]
pub trait SandboxInstance: Send + Sync + 'static {
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutcome>;
    async fn stop(&self) -> Result<()>;
}

/// Process-based runtime adapter wrapping OS isolation primitives when available.
#[derive(Debug, Clone)]
pub struct ProcessSandboxRuntime {
    inner: Arc<ProcessRuntimeInner>,
}

#[derive(Debug)]
struct ProcessRuntimeInner {
    isolation: IsolationSettings,
    bubblewrap_path: Option<PathBuf>,
    seccomp_warned: AtomicBool,
}

impl ProcessSandboxRuntime {
    pub fn new(mut isolation: IsolationSettings) -> Result<Self> {
        let bubblewrap_path = if isolation.enable_namespaces {
            if let Some(explicit) = isolation.bubblewrap_path.clone() {
                Some(explicit)
            } else {
                which("bwrap").ok()
            }
        } else {
            None
        };

        let bubblewrap_path = match bubblewrap_path {
            Some(path) => Some(path),
            None if isolation.enable_namespaces && !isolation.fallback_to_plain => {
                return Err(anyhow!(
                    "bubblewrap binary not found and fallback disabled; cannot enable namespaces"
                ));
            }
            None if isolation.enable_namespaces => {
                warn!("bubblewrap not found; falling back to plain process execution");
                isolation.enable_namespaces = false;
                None
            }
            None => None,
        };

        let inner = ProcessRuntimeInner {
            isolation,
            bubblewrap_path,
            seccomp_warned: AtomicBool::new(false),
        };

        Ok(Self {
            inner: Arc::new(inner),
        })
    }
}

impl ProcessRuntimeInner {
    #[cfg(target_os = "linux")]
    fn apply_seccomp(&self, command: &mut Command) -> Result<()> {
        use std::io;

        if let Some(config) = self.isolation.seccomp.as_ref() {
            if config.deny_syscalls.is_empty() {
                return Ok(());
            }

            if self.isolation.enable_namespaces && self.bubblewrap_path.is_some() {
                if !self.seccomp_warned.swap(true, Ordering::Relaxed) {
                    warn!("seccomp profile is not applied when executing via bubblewrap; configure bubblewrap policies instead");
                }
                return Ok(());
            }

            let profile = config.clone();
            unsafe {
                command
                    .pre_exec(move || install_seccomp_filter(&profile).map_err(io::Error::other));
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    fn apply_seccomp(&self, _command: &mut Command) -> Result<()> {
        if self.isolation.seccomp.is_some() && !self.seccomp_warned.swap(true, Ordering::Relaxed) {
            warn!("seccomp filtering requires Linux; skipping enforcement");
        }

        Ok(())
    }
}

#[async_trait]
impl SandboxRuntime for ProcessSandboxRuntime {
    async fn spawn(
        &self,
        sandbox: &SandboxRecord,
        workspace: &Path,
    ) -> Result<Arc<dyn SandboxInstance>> {
        fs::create_dir_all(workspace).await?;

        #[cfg(not(target_os = "linux"))]
        if self.inner.isolation.overlay.enabled {
            return Err(anyhow!("filesystem overlay isolation requires Linux"));
        }

        let persistent_root = if self.inner.isolation.overlay.enabled {
            let root = workspace.join("root");
            fs::create_dir_all(&root).await?;
            root
        } else {
            workspace.to_path_buf()
        };

        let cgroup_path = if self.inner.isolation.enable_cgroups {
            if let Some(root) = self.inner.isolation.cgroup_root.as_ref() {
                match prepare_cgroup(root, sandbox.id, sandbox.limits()).await {
                    Ok(path) => Some(path),
                    Err(err) => {
                        warn!(
                            sandbox_id = %sandbox.id,
                            error = %err,
                            "failed to initialize cgroup; continuing without cgroup limits"
                        );
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        let overlay = OverlayManager::new(
            persistent_root.clone(),
            workspace.join(".overlay"),
            self.inner.isolation.overlay.clone(),
        );

        let instance = ProcessSandboxInstance::new(
            sandbox.id,
            workspace.to_path_buf(),
            persistent_root,
            sandbox.limits(),
            self.inner.clone(),
            cgroup_path,
            overlay,
        );
        Ok(Arc::new(instance))
    }

    async fn destroy(&self, sandbox_id: Uuid, workspace: &Path) -> Result<()> {
        if fs::metadata(workspace).await.is_ok() {
            fs::remove_dir_all(workspace).await?;
        }

        if self.inner.isolation.enable_cgroups {
            if let Some(root) = self.inner.isolation.cgroup_root.as_ref() {
                if let Err(err) = cleanup_cgroup(root, sandbox_id).await {
                    warn!(sandbox_id = %sandbox_id, error = %err, "failed to cleanup cgroup");
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
struct ProcessSandboxInstance {
    sandbox_id: Uuid,
    workspace_root: PathBuf,
    persistent_root: PathBuf,
    limits: ResourceLimits,
    exec_lock: Mutex<()>,
    runtime: Arc<ProcessRuntimeInner>,
    cgroup_path: Option<PathBuf>,
    overlay: OverlayManager,
}

impl ProcessSandboxInstance {
    fn new(
        sandbox_id: Uuid,
        workspace_root: PathBuf,
        persistent_root: PathBuf,
        limits: ResourceLimits,
        runtime: Arc<ProcessRuntimeInner>,
        cgroup_path: Option<PathBuf>,
        overlay: OverlayManager,
    ) -> Self {
        Self {
            sandbox_id,
            workspace_root,
            persistent_root,
            limits,
            exec_lock: Mutex::new(()),
            runtime,
            cgroup_path,
            overlay,
        }
    }
}

#[async_trait]
impl SandboxInstance for ProcessSandboxInstance {
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutcome> {
        let _guard = self.exec_lock.lock().await;
        let timeout = request
            .timeout
            .unwrap_or_else(|| Duration::from_secs(self.limits.timeout_seconds as u64));

        let overlay_mount = self.overlay.prepare().await?;
        let execution_root = overlay_mount.path().to_path_buf();
        let _overlay_guard = overlay_mount;

        let mut command = if self.runtime.isolation.enable_namespaces {
            if let Some(bwrap) = self.runtime.bubblewrap_path.as_ref() {
                build_bubblewrap_command(
                    bwrap,
                    &request,
                    &self.workspace_root,
                    &execution_root,
                    self.sandbox_id,
                )
            } else {
                build_plain_command(
                    &request,
                    &execution_root,
                    &self.persistent_root,
                    self.sandbox_id,
                )
            }
        } else {
            build_plain_command(
                &request,
                &execution_root,
                &self.persistent_root,
                self.sandbox_id,
            )
        };

        command.kill_on_drop(true);
        #[cfg(target_os = "linux")]
        if let Err(err) = self.runtime.apply_seccomp(&mut command) {
            warn!(sandbox = %self.sandbox_id, error = %err, "failed to apply seccomp profile");
        }
        if request.stdin.is_some() {
            command.stdin(std::process::Stdio::piped());
        }
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let start = Instant::now();
        let mut child = command.spawn()?;

        if let Some(path) = self.cgroup_path.as_ref() {
            if let Some(pid) = child.id() {
                if let Err(err) = add_pid_to_cgroup(path, pid).await {
                    warn!(sandbox = %self.sandbox_id, error = %err, "failed to attach process to cgroup");
                }
            }
        }

        if let Some(input) = request.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes()).await?;
            }
        }

        let wait_future = child.wait_with_output();
        tokio::pin!(wait_future);

        match tokio::time::timeout(timeout, wait_future.as_mut()).await {
            Ok(result) => {
                let output = result?;
                let duration = start.elapsed();
                let stdout = if output.stdout.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(&output.stdout).to_string())
                };
                let stderr = if output.stderr.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(&output.stderr).to_string())
                };

                Ok(ExecOutcome {
                    exit_code: output.status.code(),
                    stdout,
                    stderr,
                    duration,
                    timed_out: false,
                })
            }
            Err(_) => {
                warn!(sandbox = %self.sandbox_id, "execution timed out, terminating process");
                Ok(ExecOutcome {
                    exit_code: None,
                    stdout: None,
                    stderr: Some("execution timed out".into()),
                    duration: start.elapsed(),
                    timed_out: true,
                })
            }
        }
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct OverlayManager {
    enabled: bool,
    lower_dir: PathBuf,
    overlay_dir: PathBuf,
}

impl OverlayManager {
    fn new(lower_dir: PathBuf, overlay_dir: PathBuf, config: OverlayConfig) -> Self {
        Self {
            enabled: config.enabled,
            lower_dir,
            overlay_dir,
        }
    }

    #[cfg(target_os = "linux")]
    async fn prepare(&self) -> Result<OverlayMount> {
        if !self.enabled {
            return Ok(OverlayMount::passthrough(self.lower_dir.clone()));
        }

        fs::create_dir_all(&self.overlay_dir).await?;
        let session_dir = self.overlay_dir.join(Uuid::new_v4().to_string());
        fs::create_dir_all(&session_dir).await?;
        let upper = session_dir.join("upper");
        let work = session_dir.join("work");
        let merged = session_dir.join("merged");
        fs::create_dir_all(&upper).await?;
        fs::create_dir_all(&work).await?;
        fs::create_dir_all(&merged).await?;

        let lower = self.lower_dir.clone();
        let upper_clone = upper.clone();
        let work_clone = work.clone();
        let merged_clone = merged.clone();
        spawn_blocking(move || -> Result<()> {
            mount_overlay(&lower, &upper_clone, &work_clone, &merged_clone)?;
            Ok(())
        })
        .await??;

        Ok(OverlayMount::mounted(merged, session_dir))
    }

    #[cfg(not(target_os = "linux"))]
    async fn prepare(&self) -> Result<OverlayMount> {
        if self.enabled {
            Err(anyhow!("filesystem overlay isolation requires Linux"))
        } else {
            Ok(OverlayMount::passthrough(self.lower_dir.clone()))
        }
    }
}

struct OverlayMount {
    path: PathBuf,
    _cleanup: Option<OverlayCleanup>,
}

impl OverlayMount {
    fn passthrough(path: PathBuf) -> Self {
        Self {
            path,
            _cleanup: None,
        }
    }

    fn mounted(path: PathBuf, session_dir: PathBuf) -> Self {
        Self {
            path: path.clone(),
            _cleanup: Some(OverlayCleanup {
                merged: path,
                session_dir,
            }),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

struct OverlayCleanup {
    merged: PathBuf,
    session_dir: PathBuf,
}

impl Drop for OverlayCleanup {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if let Err(err) = unmount_overlay(&self.merged) {
                warn!(path = %self.merged.display(), error = %err, "failed to unmount overlay");
            }
        }

        if let Err(err) = std::fs::remove_dir_all(&self.session_dir) {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(path = %self.session_dir.display(), error = %err, "failed to clean overlay workspace");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn mount_overlay(lower: &Path, upper: &Path, work: &Path, merged: &Path) -> Result<()> {
    use anyhow::Context;
    use std::ffi::CString;

    let data = format!(
        "lowerdir={},upperdir={},workdir={}",
        lower.display(),
        upper.display(),
        work.display()
    );

    let source = CString::new("overlay").expect("static string");
    let fstype = CString::new("overlay").expect("static string");
    let target = path_to_cstring(merged)?;
    let data_c = CString::new(data).context("overlay mount options")?;

    let result = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            0,
            data_c.as_ptr() as *const libc::c_void,
        )
    };

    if result != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("mount overlay at {}", merged.display()));
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn unmount_overlay(path: &Path) -> Result<()> {
    use anyhow::Context;

    let target = path_to_cstring(path)?;
    let result = unsafe { libc::umount2(target.as_ptr(), libc::MNT_DETACH) };
    if result != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("unmount overlay at {}", path.display()));
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn path_to_cstring(path: &Path) -> Result<std::ffi::CString> {
    use anyhow::Context;

    std::ffi::CString::new(path.as_os_str().as_bytes())
        .with_context(|| format!("convert {} to CString", path.display()))
}

#[cfg(target_os = "linux")]
fn install_seccomp_filter(config: &SeccompConfig) -> Result<()> {
    use anyhow::Context;

    if config.deny_syscalls.is_empty() {
        return Ok(());
    }

    let syscalls = config
        .deny_syscalls
        .iter()
        .map(|name| resolve_syscall_number(name))
        .collect::<Result<Vec<_>>>()?;

    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(std::io::Error::last_os_error()).context("prctl(PR_SET_NO_NEW_PRIVS)");
        }
    }

    let mut filter = Vec::with_capacity(syscalls.len() * 2 + 5);
    filter.push(bpf_stmt(
        (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
        SECCOMP_DATA_ARCH_OFFSET,
    ));
    filter.push(bpf_jump(
        (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
        audit_arch(),
        0,
        1,
    ));
    filter.push(bpf_stmt(
        (libc::BPF_RET | libc::BPF_K) as u16,
        libc::SECCOMP_RET_KILL_PROCESS,
    ));
    filter.push(bpf_stmt(
        (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
        SECCOMP_DATA_NR_OFFSET,
    ));

    let deny_action = match config.action {
        SeccompAction::Errno(errno) => libc::SECCOMP_RET_ERRNO | ((errno as u32) & 0x7fff),
        SeccompAction::KillProcess => libc::SECCOMP_RET_KILL_PROCESS,
    };

    for syscall in syscalls {
        filter.push(bpf_jump(
            (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            syscall,
            0,
            1,
        ));
        filter.push(bpf_stmt((libc::BPF_RET | libc::BPF_K) as u16, deny_action));
    }

    filter.push(bpf_stmt(
        (libc::BPF_RET | libc::BPF_K) as u16,
        libc::SECCOMP_RET_ALLOW,
    ));

    let prog = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };

    unsafe {
        if libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &prog as *const _ as *const std::ffi::c_void,
            0,
            0,
        ) != 0
        {
            return Err(std::io::Error::last_os_error()).context("prctl(PR_SET_SECCOMP)");
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn bpf_stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}

#[cfg(target_os = "linux")]
fn bpf_jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

#[cfg(target_os = "linux")]
fn audit_arch() -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        0xC000_003E
    }

    #[cfg(target_arch = "aarch64")]
    {
        0xC000_00B7
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        0xC000_003E
    }
}

#[cfg(target_os = "linux")]
fn resolve_syscall_number(name: &str) -> Result<u32> {
    if let Ok(value) = name.parse::<i64>() {
        if value < 0 {
            return Err(anyhow!("invalid syscall number {}", value));
        }
        return Ok(value as u32);
    }

    let number = match name {
        "clone" => libc::SYS_clone,
        "execve" => libc::SYS_execve,
        "fork" => libc::SYS_fork,
        "kill" => libc::SYS_kill,
        "mount" => libc::SYS_mount,
        "open" => libc::SYS_open,
        "openat" => libc::SYS_openat,
        "pivot_root" => libc::SYS_pivot_root,
        "ptrace" => libc::SYS_ptrace,
        "setgid" => libc::SYS_setgid,
        "setuid" => libc::SYS_setuid,
        "socket" => libc::SYS_socket,
        "umount" => libc::SYS_umount2,
        "unshare" => libc::SYS_unshare,
        "chmod" => libc::SYS_chmod,
        "chown" => libc::SYS_chown,
        "mknod" => libc::SYS_mknod,
        other => {
            return Err(anyhow!("unknown syscall '{}' in seccomp profile", other));
        }
    };

    Ok(number as u32)
}

#[cfg(target_os = "linux")]
const SECCOMP_DATA_NR_OFFSET: u32 = 0;

#[cfg(target_os = "linux")]
const SECCOMP_DATA_ARCH_OFFSET: u32 = 4;

fn build_plain_command(
    request: &ExecRequest,
    workdir: &Path,
    persistent_root: &Path,
    sandbox_id: Uuid,
) -> Command {
    let mut command = Command::new(&request.command);
    command.args(&request.args);
    command.current_dir(workdir);
    command.env("BKG_SANDBOX_ID", sandbox_id.to_string());
    command.env("BKG_SANDBOX_WORKDIR", workdir);
    command.env("BKG_SANDBOX_ROOT", persistent_root);
    command
}

fn build_bubblewrap_command(
    bwrap_path: &Path,
    request: &ExecRequest,
    workspace_root: &Path,
    execution_root: &Path,
    sandbox_id: Uuid,
) -> Command {
    let mut command = Command::new(bwrap_path);
    command.env("BKG_SANDBOX_ID", sandbox_id.to_string());
    command.arg("--die-with-parent");
    command.arg("--new-session");
    command.arg("--unshare-pid");
    command.arg("--unshare-uts");
    command.arg("--unshare-ipc");
    command.arg("--unshare-net");
    command.arg("--unshare-cgroup");
    command.arg("--proc").arg("/proc");

    for path in ro_bind_candidates() {
        if std::path::Path::new(path).exists() {
            command.arg("--ro-bind").arg(path).arg(path);
        }
    }

    command.arg("--dev-bind").arg("/dev").arg("/dev");
    command
        .arg("--bind")
        .arg(execution_root)
        .arg(workspace_root);
    command.arg("--chdir").arg(workspace_root);
    command.arg("--tmpfs").arg("/tmp");
    command
        .arg("--setenv")
        .arg("PATH")
        .arg("/usr/bin:/bin:/sbin");
    command
        .arg("--setenv")
        .arg("BKG_SANDBOX_ID")
        .arg(sandbox_id.to_string());
    command
        .arg("--setenv")
        .arg("BKG_SANDBOX_WORKDIR")
        .arg(workspace_root);
    command
        .arg("--setenv")
        .arg("BKG_SANDBOX_ROOT")
        .arg(workspace_root);

    command.arg("--");
    command.arg(&request.command);
    command.args(&request.args);
    command
}

fn ro_bind_candidates() -> &'static [&'static str] {
    &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"]
}

fn sanitize_component(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_start_exec_stop_flow() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let mut config = KernelConfig::default();
        config.isolation.enable_cgroups = false;
        config.isolation.enable_namespaces = false;
        config.audit.enabled = false;
        let runtime = ProcessSandboxRuntime::new(config.isolation.clone()).unwrap();
        let kernel = CaveKernel::new(db, runtime, config);

        let created = kernel
            .create_sandbox(CreateSandboxRequest::new("ns", "sandbox"))
            .await
            .unwrap();

        let started = kernel.start_sandbox(created.id).await.unwrap();
        assert_eq!(started.status, SandboxStatus::Running);

        let outcome = kernel
            .exec(
                created.id,
                ExecRequest {
                    command: "echo".into(),
                    args: vec!["hello".into()],
                    stdin: None,
                    timeout: Some(Duration::from_secs(5)),
                },
            )
            .await
            .unwrap();

        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.stdout.unwrap().contains("hello"));

        kernel.stop_sandbox(created.id).await.unwrap();
        kernel.delete_sandbox(created.id).await.unwrap();
    }

    #[tokio::test]
    async fn audit_log_records_lifecycle_events() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let mut config = KernelConfig::default();
        config.isolation.enable_cgroups = false;
        config.isolation.enable_namespaces = false;
        config.audit.enabled = true;
        config.audit.log_path =
            std::env::temp_dir().join(format!("cave-kernel-audit-{}.jsonl", Uuid::new_v4()));
        config.audit.hmac_key = None;
        let audit_path = config.audit.log_path.clone();

        let runtime = ProcessSandboxRuntime::new(config.isolation.clone()).unwrap();
        let kernel = CaveKernel::new(db, runtime, config);

        let created = kernel
            .create_sandbox(CreateSandboxRequest::new("ns", "audit"))
            .await
            .unwrap();
        kernel.start_sandbox(created.id).await.unwrap();
        let _ = kernel
            .exec(
                created.id,
                ExecRequest {
                    command: "echo".into(),
                    args: vec!["audit".into()],
                    stdin: None,
                    timeout: Some(Duration::from_secs(2)),
                },
            )
            .await
            .unwrap();
        kernel.stop_sandbox(created.id).await.unwrap();
        kernel.delete_sandbox(created.id).await.unwrap();

        let contents = tokio::fs::read_to_string(&audit_path).await.unwrap();
        tokio::fs::remove_file(&audit_path).await.unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert!(lines.iter().any(|line| line.contains("sandbox_created")));
        assert!(lines.iter().any(|line| line.contains("sandbox_exec")));
        assert!(lines.iter().any(|line| line.contains("sandbox_deleted")));
    }
}
