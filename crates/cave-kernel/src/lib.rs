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
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use audit::{AuditEvent, AuditLogWriter};
use bkg_db::{
    self, Database, ExecutionRecord, NewSandbox, ResourceLimits, SandboxError, SandboxRecord,
    SandboxStatus,
};
use chrono::Utc;
use isolation::{
    add_pid_to_cgroup, apply_seccomp_filter, cleanup_cgroup, cleanup_overlay_dirs, mount_overlay,
    prepare_cgroup, prepare_overlay_dirs, resolve_seccomp_numbers, unmount_overlay, OverlayDirs,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Mutex,
};
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
    pub enable_overlayfs: bool,
    pub enable_seccomp: bool,
    pub bubblewrap_path: Option<PathBuf>,
    pub cgroup_root: Option<PathBuf>,
    pub seccomp_profile_path: Option<PathBuf>,
    pub seccomp_allow_syscalls: Vec<String>,
    pub fallback_to_plain: bool,
}

impl Default for IsolationSettings {
    fn default() -> Self {
        Self {
            enable_namespaces: true,
            enable_cgroups: true,
            enable_overlayfs: cfg!(target_os = "linux"),
            enable_seccomp: cfg!(target_os = "linux"),
            bubblewrap_path: None,
            cgroup_root: Some(PathBuf::from("/sys/fs/cgroup/bkg")),
            seccomp_profile_path: None,
            seccomp_allow_syscalls: Vec::new(),
            fallback_to_plain: true,
        }
    }
}

const DEFAULT_SECCOMP_ALLOWLIST: &[&str] = &[
    "read",
    "write",
    "close",
    "exit",
    "exit_group",
    "futex",
    "sched_yield",
    "nanosleep",
    "clock_gettime",
    "clock_getres",
    "clock_nanosleep",
    "rt_sigaction",
    "rt_sigprocmask",
    "rt_sigreturn",
    "sigaltstack",
    "set_tid_address",
    "set_robust_list",
    "brk",
    "mmap",
    "mprotect",
    "munmap",
    "mremap",
    "prlimit64",
    "getpid",
    "getppid",
    "gettid",
    "getuid",
    "geteuid",
    "getgid",
    "getegid",
    "getrandom",
    "readlink",
    "readlinkat",
    "open",
    "openat",
    "fstat",
    "newfstatat",
    "lseek",
    "stat",
    "lstat",
    "statx",
    "arch_prctl",
    "dup",
    "dup2",
    "dup3",
    "pipe",
    "pipe2",
    "ioctl",
    "uname",
    "access",
    "fcntl",
    "poll",
    "ppoll",
    "select",
    "pselect6",
    "eventfd2",
    "timerfd_create",
    "timerfd_settime",
    "timerfd_gettime",
    "chdir",
    "fchdir",
    "getcwd",
    "splice",
    "tee",
    "vmsplice",
    "writev",
    "readv",
    "pread64",
    "pwrite64",
    "rt_sigtimedwait",
    "wait4",
    "waitid",
    "kill",
    "tkill",
    "tgkill",
    "socket",
    "socketpair",
    "connect",
    "accept",
    "accept4",
    "bind",
    "listen",
    "getsockname",
    "getpeername",
    "getsockopt",
    "setsockopt",
    "shutdown",
    "sendto",
    "sendmsg",
    "sendmmsg",
    "recvfrom",
    "recvmsg",
    "recvmmsg",
    "clone",
    "clone3",
    "execve",
    "execveat",
    "umask",
    "sysinfo",
    "times",
    "gettimeofday",
    "setitimer",
    "getitimer",
    "madvise",
    "prctl",
];

fn build_seccomp_allowlist(settings: &IsolationSettings) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut allowlist = Vec::new();

    for syscall in DEFAULT_SECCOMP_ALLOWLIST {
        if seen.insert(*syscall) {
            allowlist.push((*syscall).to_string());
        }
    }

    for syscall in &settings.seccomp_allow_syscalls {
        if seen.insert(syscall.as_str()) {
            allowlist.push(syscall.clone());
        }
    }

    allowlist
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

#[derive(Debug, Clone)]
struct SeccompContext {
    numbers: Arc<Vec<u32>>,
    profile_path: Option<PathBuf>,
}

#[derive(Debug)]
struct ProcessRuntimeInner {
    isolation: IsolationSettings,
    bubblewrap_path: Option<PathBuf>,
    seccomp: Option<SeccompContext>,
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

        let seccomp = if isolation.enable_seccomp {
            let allowlist = build_seccomp_allowlist(&isolation);
            let numbers = resolve_seccomp_numbers(&allowlist)
                .with_context(|| "resolving seccomp allowlist to numeric identifiers")?;
            Some(SeccompContext {
                numbers: Arc::new(numbers),
                profile_path: isolation.seccomp_profile_path.clone(),
            })
        } else {
            None
        };

        let inner = ProcessRuntimeInner {
            isolation,
            bubblewrap_path,
            seccomp,
        };

        Ok(Self {
            inner: Arc::new(inner),
        })
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

        let overlay = if self.inner.isolation.enable_overlayfs {
            match prepare_overlay_dirs(workspace).await {
                Ok(Some(dirs)) => Some(dirs),
                Ok(None) => None,
                Err(err) => {
                    warn!(
                        sandbox_id = %sandbox.id,
                        error = %err,
                        "failed to prepare overlay directories; continuing without overlay"
                    );
                    None
                }
            }
        } else {
            None
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

        let instance = ProcessSandboxInstance::new(
            sandbox.id,
            workspace.to_path_buf(),
            sandbox.limits(),
            self.inner.clone(),
            cgroup_path,
            overlay,
            self.inner.seccomp.clone(),
        );
        Ok(Arc::new(instance))
    }

    async fn destroy(&self, sandbox_id: Uuid, workspace: &Path) -> Result<()> {
        if self.inner.isolation.enable_overlayfs {
            if let Err(err) = cleanup_overlay_dirs(workspace).await {
                warn!(
                    sandbox_id = %sandbox_id,
                    error = %err,
                    "failed to cleanup overlay directories"
                );
            }
        }

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
    limits: ResourceLimits,
    exec_lock: Mutex<()>,
    runtime: Arc<ProcessRuntimeInner>,
    cgroup_path: Option<PathBuf>,
    overlay: Option<OverlayDirs>,
    seccomp: Option<SeccompContext>,
}

impl ProcessSandboxInstance {
    fn new(
        sandbox_id: Uuid,
        workspace_root: PathBuf,
        limits: ResourceLimits,
        runtime: Arc<ProcessRuntimeInner>,
        cgroup_path: Option<PathBuf>,
        overlay: Option<OverlayDirs>,
        seccomp: Option<SeccompContext>,
    ) -> Self {
        Self {
            sandbox_id,
            workspace_root,
            limits,
            exec_lock: Mutex::new(()),
            runtime,
            cgroup_path,
            overlay,
            seccomp,
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

        let mut active_workspace = self.workspace_root.as_path();
        let mut overlay_mounted = false;
        if let Some(dirs) = self.overlay.as_ref() {
            match mount_overlay(dirs) {
                Ok(()) => {
                    overlay_mounted = true;
                    active_workspace = dirs.merged.as_path();
                }
                Err(err) => {
                    warn!(sandbox = %self.sandbox_id, error = %err, "failed to mount overlay");
                }
            }
        }

        let mut command = if self.runtime.isolation.enable_namespaces {
            if let Some(bwrap) = self.runtime.bubblewrap_path.as_ref() {
                if self.runtime.isolation.enable_seccomp
                    && self
                        .seccomp
                        .as_ref()
                        .map_or(true, |ctx| ctx.profile_path.is_none())
                {
                    warn!(
                        sandbox = %self.sandbox_id,
                        "seccomp enabled but no profile provided for bubblewrap; skipping bwrap seccomp application"
                    );
                }
                let profile = self
                    .seccomp
                    .as_ref()
                    .and_then(|ctx| ctx.profile_path.as_deref());
                build_bubblewrap_command(
                    bwrap,
                    &request,
                    active_workspace,
                    self.sandbox_id,
                    profile,
                )
            } else {
                build_plain_command(
                    &request,
                    active_workspace,
                    self.sandbox_id,
                    self.seccomp.as_ref().map(|ctx| ctx.numbers.clone()),
                )
            }
        } else {
            build_plain_command(
                &request,
                active_workspace,
                self.sandbox_id,
                self.seccomp.as_ref().map(|ctx| ctx.numbers.clone()),
            )
        };

        command.kill_on_drop(true);
        if request.stdin.is_some() {
            command.stdin(std::process::Stdio::piped());
        }
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let start = Instant::now();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                if overlay_mounted {
                    if let Some(dirs) = self.overlay.as_ref() {
                        if let Err(unmount_err) = unmount_overlay(dirs) {
                            warn!(
                                sandbox = %self.sandbox_id,
                                error = %unmount_err,
                                "failed to unmount overlay after spawn error"
                            );
                        }
                    }
                }
                return Err(err.into());
            }
        };

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

        let stdout_task = child.stdout.take().map(|mut stdout| {
            tokio::spawn(async move {
                let mut buf = Vec::new();
                match stdout.read_to_end(&mut buf).await {
                    Ok(_) => Ok(buf),
                    Err(err) => Err(err),
                }
            })
        });
        let stderr_task = child.stderr.take().map(|mut stderr| {
            tokio::spawn(async move {
                let mut buf = Vec::new();
                match stderr.read_to_end(&mut buf).await {
                    Ok(_) => Ok(buf),
                    Err(err) => Err(err),
                }
            })
        });

        let mut timed_out = false;
        let exit_status = loop {
            match child
                .try_wait()
                .with_context(|| "polling sandbox process status")?
            {
                Some(status) => break status,
                None => {
                    if start.elapsed() >= timeout {
                        timed_out = true;
                        warn!(sandbox = %self.sandbox_id, "execution timed out, terminating process");
                        let _ = child.start_kill();
                        break child
                            .wait()
                            .await
                            .with_context(|| "waiting for sandbox process after timeout")?;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        };

        let duration = start.elapsed();

        let stdout = match stdout_task {
            Some(handle) => match handle.await {
                Ok(Ok(buf)) => {
                    if buf.is_empty() {
                        None
                    } else {
                        Some(String::from_utf8_lossy(&buf).to_string())
                    }
                }
                Ok(Err(err)) => {
                    warn!(sandbox = %self.sandbox_id, error = %err, "failed to read stdout");
                    None
                }
                Err(err) => {
                    warn!(sandbox = %self.sandbox_id, error = %err, "stdout task panicked");
                    None
                }
            },
            None => None,
        };

        let mut stderr = match stderr_task {
            Some(handle) => match handle.await {
                Ok(Ok(buf)) => {
                    if buf.is_empty() {
                        None
                    } else {
                        Some(String::from_utf8_lossy(&buf).to_string())
                    }
                }
                Ok(Err(err)) => {
                    warn!(sandbox = %self.sandbox_id, error = %err, "failed to read stderr");
                    None
                }
                Err(err) => {
                    warn!(sandbox = %self.sandbox_id, error = %err, "stderr task panicked");
                    None
                }
            },
            None => None,
        };

        if timed_out {
            const TIMEOUT_MSG: &str = "execution timed out";
            match &mut stderr {
                Some(existing) => {
                    if !existing.is_empty() {
                        existing.push('\n');
                    }
                    existing.push_str(TIMEOUT_MSG);
                }
                None => stderr = Some(TIMEOUT_MSG.to_string()),
            }
        }

        let result = Ok(ExecOutcome {
            exit_code: exit_status.code(),
            stdout,
            stderr,
            duration,
            timed_out,
        });

        if overlay_mounted {
            if let Some(dirs) = self.overlay.as_ref() {
                if let Err(err) = unmount_overlay(dirs) {
                    warn!(sandbox = %self.sandbox_id, error = %err, "failed to unmount overlay");
                }
            }
        }

        result
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

fn build_plain_command(
    request: &ExecRequest,
    workspace: &Path,
    sandbox_id: Uuid,
    seccomp_numbers: Option<Arc<Vec<u32>>>,
) -> Command {
    let mut command = Command::new(&request.command);
    command.args(&request.args);
    command.current_dir(workspace);
    command.env("BKG_SANDBOX_ID", sandbox_id.to_string());

    #[cfg(target_os = "linux")]
    if let Some(numbers) = seccomp_numbers {
        let numbers = numbers.clone();
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(command.as_std_mut(), move || {
                apply_seccomp_filter(&numbers).map_err(std::io::Error::other)
            });
        }
    }

    command
}

fn build_bubblewrap_command(
    bwrap_path: &Path,
    request: &ExecRequest,
    workspace: &Path,
    sandbox_id: Uuid,
    seccomp_profile: Option<&Path>,
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
    command.arg("--bind").arg(workspace).arg(workspace);
    command.arg("--chdir").arg(workspace);
    command.arg("--tmpfs").arg("/tmp");
    command
        .arg("--setenv")
        .arg("PATH")
        .arg("/usr/bin:/bin:/sbin");
    command
        .arg("--setenv")
        .arg("BKG_SANDBOX_ID")
        .arg(sandbox_id.to_string());

    if let Some(profile) = seccomp_profile {
        command.arg("--seccomp").arg(profile);
    }

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
