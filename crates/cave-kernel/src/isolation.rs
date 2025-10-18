use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::ResourceLimits;

/// Prepares a dedicated cgroup for the sandbox and applies basic limits.
#[cfg(target_os = "linux")]
pub async fn prepare_cgroup(
    root: &Path,
    sandbox_id: Uuid,
    limits: ResourceLimits,
) -> Result<PathBuf> {
    let group_path = root.join(sandbox_id.to_string());
    fs::create_dir_all(&group_path)
        .await
        .with_context(|| format!("creating cgroup directory at {}", group_path.display()))?;

    write_value(group_path.join("memory.max"), limits.memory_limit_bytes).await?;
    write_value(group_path.join("pids.max"), limits_to_pids(&limits)).await?;

    let cpu_value = cpu_quota_value(limits.cpu_limit_millis);
    write_string(group_path.join("cpu.max"), cpu_value).await?;

    Ok(group_path)
}

/// Registers the child process with its cgroup.
#[cfg(target_os = "linux")]
pub async fn add_pid_to_cgroup(group_path: &Path, pid: u32) -> Result<()> {
    write_string(group_path.join("cgroup.procs"), pid.to_string()).await
}

/// Removes the cgroup directory once the sandbox is torn down.
#[cfg(target_os = "linux")]
pub async fn cleanup_cgroup(root: &Path, sandbox_id: Uuid) -> Result<()> {
    let group_path = root.join(sandbox_id.to_string());
    match fs::remove_dir(&group_path).await {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("removing cgroup {}", group_path.display())),
    }
}

/// No-op implementations for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub async fn prepare_cgroup(
    _root: &Path,
    _sandbox_id: Uuid,
    _limits: ResourceLimits,
) -> Result<PathBuf> {
    Err(anyhow::anyhow!("cgroups are only supported on Linux"))
}

#[cfg(not(target_os = "linux"))]
pub async fn add_pid_to_cgroup(_group_path: &Path, _pid: u32) -> Result<()> {
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub async fn cleanup_cgroup(_root: &Path, _sandbox_id: Uuid) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
async fn write_value(path: PathBuf, value: u64) -> Result<()> {
    write_string(path, value.to_string()).await
}

#[cfg(target_os = "linux")]
async fn write_string(path: PathBuf, value: String) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)
        .await
        .with_context(|| format!("opening {}", path.display()))?;

    file.set_len(0).await?;
    file.write_all(value.as_bytes()).await?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn cpu_quota_value(cpu_millis: u32) -> String {
    // cgroup v2 uses the format "<quota> <period>" in microseconds.
    // We treat 1000 millis as 1 CPU. Default period 100000µs (100ms).
    const PERIOD: u64 = 100_000;
    if cpu_millis == 0 {
        return "max".to_string();
    }

    let quota = ((cpu_millis as u64) * PERIOD) / 1000;
    format!("{} {}", quota.max(1), PERIOD)
}

#[cfg(target_os = "linux")]
fn limits_to_pids(limits: &ResourceLimits) -> u64 {
    // Allow a small multiple of the default timeout to translate into process count; fall back to 64.
    // PIDs are not captured explicitly in ResourceLimits yet, so we set a conservative cap.
    let baseline = 64u64;
    let factor = (limits.timeout_seconds as u64 / 10).max(1);
    baseline * factor
}
