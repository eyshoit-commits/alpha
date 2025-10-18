//! Core CAVE kernel orchestration logic.
//!
//! The implementation focuses on the Phase‑0 requirements laid out in the
//! README: provisioning sandboxes, enforcing default resource policies,
//! orchestrating lifecycle transitions and persisting execution audits via the
//! `bkg-db` crate. Real low-level sandboxing (namespaces, seccomp, etc.) will be
//! layered in later; the current runtime is a safe process isolation shim that
//! operates within a prepared workspace directory.

use std::{collections::HashMap, path::{Path, PathBuf}, sync::Arc, time::{Duration, Instant}};

use anyhow::Result;
use async_trait::async_trait;
use bkg_db::{self, Database, ExecutionRecord, NewSandbox, ResourceLimits, SandboxError, SandboxRecord, SandboxStatus};
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt, process::Command, sync::Mutex};
use tracing::{info, instrument, warn};
use uuid::Uuid;

const DEFAULT_RUNTIME_KIND: &str = "process";

/// Logical configuration driving the kernel behaviour.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    pub workspace_root: PathBuf,
    pub default_limits: ResourceLimits,
    pub default_runtime: String,
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
        }
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
        }
    }
}

impl<R> CaveKernel<R>
where
    R: SandboxRuntime,
{
    pub fn new(db: Database, runtime: R, config: KernelConfig) -> Self {
        Self {
            db,
            runtime: Arc::new(runtime),
            config,
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a sandbox entry and enforces namespace uniqueness.
    #[instrument(skip(self, request))]
    pub async fn create_sandbox(&self, request: CreateSandboxRequest) -> Result<SandboxRecord, KernelError> {
        let limits = request.resource_limits.unwrap_or(self.config.default_limits);
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

        let effective_request = request.with_default_timeout(Duration::from_secs(record.timeout_seconds as u64));
        let runtime_request = effective_request.clone();
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

        Ok(outcome)
    }

    /// Stops the runtime instance and updates state tracking.
    #[instrument(skip(self))]
    pub async fn stop_sandbox(&self, id: Uuid) -> Result<(), KernelError> {
        let _record = self
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

        self.db.delete_sandbox(id).await.map_err(KernelError::from)?;

        info!(sandbox_id = %id, "sandbox deleted");
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
    pub async fn recent_executions(&self, id: Uuid, limit: u32) -> Result<Vec<ExecutionRecord>, KernelError> {
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
    async fn spawn(&self, sandbox: &SandboxRecord, workspace: &Path) -> Result<Arc<dyn SandboxInstance>>;
    async fn destroy(&self, sandbox_id: Uuid, workspace: &Path) -> Result<()>;
}

#[async_trait]
pub trait SandboxInstance: Send + Sync + 'static {
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutcome>;
    async fn stop(&self) -> Result<()>;
}

/// Process-based runtime adapter (no real OS sandboxing yet, just workspace scoping).
#[derive(Debug, Clone)]
pub struct ProcessSandboxRuntime;

#[async_trait]
impl SandboxRuntime for ProcessSandboxRuntime {
    async fn spawn(&self, sandbox: &SandboxRecord, workspace: &Path) -> Result<Arc<dyn SandboxInstance>> {
        fs::create_dir_all(workspace).await?;
        let instance = ProcessSandboxInstance::new(sandbox.id, workspace.to_path_buf(), sandbox.limits());
        Ok(Arc::new(instance))
    }

    async fn destroy(&self, _sandbox_id: Uuid, workspace: &Path) -> Result<()> {
        if fs::metadata(workspace).await.is_ok() {
            fs::remove_dir_all(workspace).await?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ProcessSandboxInstance {
    sandbox_id: Uuid,
    workspace: PathBuf,
    limits: ResourceLimits,
    exec_lock: Mutex<()>,
}

impl ProcessSandboxInstance {
    fn new(sandbox_id: Uuid, workspace: PathBuf, limits: ResourceLimits) -> Self {
        Self {
            sandbox_id,
            workspace,
            limits,
            exec_lock: Mutex::new(()),
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

        let mut command = Command::new(&request.command);
        command.args(&request.args);
        command.current_dir(&self.workspace);
        command.env("BKG_SANDBOX_ID", self.sandbox_id.to_string());
        command.kill_on_drop(true);
        if request.stdin.is_some() {
            command.stdin(std::process::Stdio::piped());
        }
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let start = Instant::now();
        let mut child = command.spawn()?;

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
        // No persistent subprocess to shutdown yet; future runtimes may hold state.
        Ok(())
    }
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
        let kernel = CaveKernel::new(db, ProcessSandboxRuntime, KernelConfig::default());

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
}
