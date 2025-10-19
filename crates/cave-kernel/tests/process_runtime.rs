#![cfg(unix)]

use std::time::Duration;

use bkg_db::{ResourceLimits, SandboxRecord, SandboxStatus};
use cave_kernel::{ExecRequest, IsolationSettings, ProcessSandboxRuntime};
use chrono::Utc;
use tokio::fs;
use uuid::Uuid;

fn sandbox_record(limits: ResourceLimits) -> SandboxRecord {
    let now = Utc::now();
    SandboxRecord {
        id: Uuid::new_v4(),
        namespace: "test-ns".to_string(),
        name: "integration".to_string(),
        runtime: "process".to_string(),
        status: SandboxStatus::Provisioned,
        cpu_limit_millis: limits.cpu_limit_millis,
        memory_limit_bytes: limits.memory_limit_bytes,
        disk_limit_bytes: limits.disk_limit_bytes,
        timeout_seconds: limits.timeout_seconds,
        created_at: now,
        updated_at: now,
        last_started_at: None,
        last_stopped_at: None,
    }
}

#[tokio::test]
async fn process_runtime_executes_echo_without_namespaces() -> anyhow::Result<()> {
    let mut isolation = IsolationSettings::default();
    isolation.enable_namespaces = false;
    isolation.enable_overlayfs = false;
    isolation.enable_cgroups = false;
    isolation.seccomp_profile_path = None;

    let runtime = ProcessSandboxRuntime::new(isolation)?;
    let limits = ResourceLimits::default();

    let sandbox = sandbox_record(limits);
    let workspace_dir = tempfile::tempdir()?;
    let workspace_path = workspace_dir.path().join("workspace");

    let instance = runtime.spawn(&sandbox, &workspace_path).await?;
    let outcome = instance
        .exec(ExecRequest {
            command: "/bin/echo".to_string(),
            args: vec!["hello".to_string()],
            stdin: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .await?;

    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome
        .stdout
        .as_deref()
        .map(|stdout| stdout.contains("hello"))
        .unwrap_or(false));
    assert!(!outcome.timed_out);

    runtime.destroy(sandbox.id, &workspace_path).await?;
    assert!(fs::metadata(&workspace_path).await.is_err());

    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn process_runtime_applies_seccomp_allowlist() -> anyhow::Result<()> {
    let mut isolation = IsolationSettings::default();
    isolation.enable_namespaces = false;
    isolation.enable_overlayfs = false;
    isolation.enable_cgroups = false;

    let runtime = ProcessSandboxRuntime::new(isolation)?;
    let limits = ResourceLimits::default();

    let sandbox = sandbox_record(limits);
    let workspace_dir = tempfile::tempdir()?;
    let workspace_path = workspace_dir.path().join("workspace");

    let instance = runtime.spawn(&sandbox, &workspace_path).await?;
    let outcome = instance
        .exec(ExecRequest {
            command: "/bin/true".to_string(),
            args: Vec::new(),
            stdin: None,
            timeout: Some(Duration::from_secs(2)),
        })
        .await?;

    assert_eq!(outcome.exit_code, Some(0));
    assert!(!outcome.timed_out);

    runtime.destroy(sandbox.id, &workspace_path).await?;
    assert!(fs::metadata(&workspace_path).await.is_err());

    Ok(())
}
