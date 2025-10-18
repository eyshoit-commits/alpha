use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use std::{
    collections::{HashMap, HashSet},
    ffi::CString,
    fs as std_fs,
    os::unix::ffi::OsStrExt,
    sync::OnceLock,
};

use anyhow::{Context, Result};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::ResourceLimits;

#[cfg(target_os = "linux")]
use tracing::warn;

#[cfg(target_os = "linux")]
const SECCOMP_RET_ALLOW: u32 = 0x7fff0000;
#[cfg(target_os = "linux")]
const SECCOMP_RET_ERRNO_BASE: u32 = 0x00050000;
#[cfg(target_os = "linux")]
const AUDIT_ARCH_X86_64: u32 = 0xc000003e;
#[cfg(target_os = "linux")]
const SECCOMP_DATA_NR_OFFSET: u32 = 0;
#[cfg(target_os = "linux")]
const SECCOMP_DATA_ARCH_OFFSET: u32 = 4;

#[cfg(target_os = "linux")]
const BPF_LD: u16 = 0x00;
#[cfg(target_os = "linux")]
const BPF_W: u16 = 0x00;
#[cfg(target_os = "linux")]
const BPF_ABS: u16 = 0x20;
#[cfg(target_os = "linux")]
const BPF_JMP: u16 = 0x05;
#[cfg(target_os = "linux")]
const BPF_JEQ: u16 = 0x10;
#[cfg(target_os = "linux")]
const BPF_RET: u16 = 0x06;
#[cfg(target_os = "linux")]
const BPF_K: u16 = 0x00;

#[cfg(target_os = "linux")]
#[inline]
fn seccomp_errno(errno: u32) -> u32 {
    SECCOMP_RET_ERRNO_BASE | (errno & 0x0000ffff)
}

#[cfg(target_os = "linux")]
fn syscall_header_path() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "/usr/include/x86_64-linux-gnu/asm/unistd_64.h"
    }

    #[cfg(target_arch = "aarch64")]
    {
        "/usr/include/aarch64-linux-gnu/asm/unistd.h"
    }

    #[cfg(target_arch = "arm")]
    {
        "/usr/include/arm-linux-gnueabihf/asm/unistd.h"
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "arm")))]
    {
        "/usr/include/asm/unistd.h"
    }
}

#[cfg(target_os = "linux")]
fn parse_syscall_value(
    value: &str,
    expressions: &HashMap<String, String>,
    resolved: &mut HashMap<String, i64>,
    stack: &mut HashSet<String>,
) -> Option<i64> {
    let mut expr = value.trim();
    while expr.starts_with('(') && expr.ends_with(')') && expr.len() > 2 {
        expr = &expr[1..expr.len() - 1];
    }

    if let Some(idx) = expr.find('+') {
        let (left, right) = expr.split_at(idx);
        let left_val = parse_syscall_value(left, expressions, resolved, stack)?;
        let right_val = parse_syscall_value(&right[1..], expressions, resolved, stack)?;
        return Some(left_val + right_val);
    }

    if let Some(name) = expr.strip_prefix("__NR_") {
        return resolve_syscall_macro(name, expressions, resolved, stack);
    }

    if let Some(hex) = expr.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16).ok();
    }

    if let Ok(num) = expr.parse::<i64>() {
        return Some(num);
    }

    None
}

#[cfg(target_os = "linux")]
fn resolve_syscall_macro(
    name: &str,
    expressions: &HashMap<String, String>,
    resolved: &mut HashMap<String, i64>,
    stack: &mut HashSet<String>,
) -> Option<i64> {
    if let Some(&value) = resolved.get(name) {
        return Some(value);
    }

    if !stack.insert(name.to_string()) {
        return None;
    }

    let expr = expressions.get(name)?;
    let value = parse_syscall_value(expr, expressions, resolved, stack)?;
    stack.remove(name);
    resolved.insert(name.to_string(), value);
    Some(value)
}

#[cfg(target_os = "linux")]
fn syscall_numbers() -> Result<&'static HashMap<String, i64>> {
    static TABLE: OnceLock<HashMap<String, i64>> = OnceLock::new();
    if let Some(existing) = TABLE.get() {
        return Ok(existing);
    }

    let map = build_syscall_map()?;
    let _ = TABLE.set(map);
    Ok(TABLE.get().expect("syscall map initialized"))
}

#[cfg(target_os = "linux")]
fn build_syscall_map() -> Result<HashMap<String, i64>> {
    let contents = std_fs::read_to_string(syscall_header_path())
        .with_context(|| "reading Linux syscall definitions")?;
    let mut expressions = HashMap::new();
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("#define __NR_") {
            let mut parts = rest.split_whitespace();
            if let (Some(name), Some(value)) = (parts.next(), parts.next()) {
                expressions.insert(name.to_string(), value.to_string());
            }
        }
    }

    let mut resolved = HashMap::new();
    let mut stack = HashSet::new();
    let keys: Vec<String> = expressions.keys().cloned().collect();
    for name in keys {
        if resolved.contains_key(&name) {
            continue;
        }
        if resolve_syscall_macro(&name, &expressions, &mut resolved, &mut stack).is_none() {
            warn!(syscall = %name, "failed to resolve syscall number");
        }
    }

    Ok(resolved)
}

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

/// Overlay directory layout managed for each sandbox.
#[derive(Debug, Clone)]
pub struct OverlayDirs {
    pub lower: PathBuf,
    pub upper: PathBuf,
    pub work: PathBuf,
    pub merged: PathBuf,
}

impl OverlayDirs {
    pub fn new(workspace: &Path) -> Self {
        Self {
            lower: workspace.join("lower"),
            upper: workspace.join(".overlay").join("upper"),
            work: workspace.join(".overlay").join("work"),
            merged: workspace.join(".overlay").join("merged"),
        }
    }
}

/// Ensures overlay directories exist for the sandbox workspace.
pub async fn prepare_overlay_dirs(workspace: &Path) -> Result<Option<OverlayDirs>> {
    #[cfg(target_os = "linux")]
    {
        let dirs = OverlayDirs::new(workspace);
        fs::create_dir_all(&dirs.lower)
            .await
            .with_context(|| format!("creating overlay lower dir at {}", dirs.lower.display()))?;
        fs::create_dir_all(&dirs.upper)
            .await
            .with_context(|| format!("creating overlay upper dir at {}", dirs.upper.display()))?;
        fs::create_dir_all(&dirs.work)
            .await
            .with_context(|| format!("creating overlay work dir at {}", dirs.work.display()))?;
        fs::create_dir_all(&dirs.merged)
            .await
            .with_context(|| format!("creating overlay merged dir at {}", dirs.merged.display()))?;
        Ok(Some(dirs))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = workspace;
        Ok(None)
    }
}

/// Unmounts and removes overlay directories for the sandbox workspace.
pub async fn cleanup_overlay_dirs(workspace: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let dirs = OverlayDirs::new(workspace);
        if let Err(err) = unmount_overlay(&dirs) {
            warn!(
                workspace = %workspace.display(),
                error = %err,
                "failed to unmount overlay during cleanup"
            );
        }

        if fs::metadata(&dirs.merged).await.is_ok() {
            fs::remove_dir_all(&dirs.merged).await.with_context(|| {
                format!("removing overlay merged dir at {}", dirs.merged.display())
            })?;
        }
        if fs::metadata(&dirs.upper).await.is_ok() {
            fs::remove_dir_all(&dirs.upper).await.with_context(|| {
                format!("removing overlay upper dir at {}", dirs.upper.display())
            })?;
        }
        if fs::metadata(&dirs.work).await.is_ok() {
            fs::remove_dir_all(&dirs.work)
                .await
                .with_context(|| format!("removing overlay work dir at {}", dirs.work.display()))?;
        }
        // lower and root will be deleted when the workspace is removed.
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = workspace;
        Ok(())
    }
}

/// Mounts overlay filesystem so sandbox processes operate on the merged view.
#[cfg(target_os = "linux")]
pub fn mount_overlay(dirs: &OverlayDirs) -> Result<()> {
    let merged = CString::new(dirs.merged.as_os_str().as_bytes())?;
    let opts = CString::new(format!(
        "lowerdir={},upperdir={},workdir={}",
        dirs.lower.display(),
        dirs.upper.display(),
        dirs.work.display()
    ))?;
    let source = CString::new("overlay")?;
    let fstype = CString::new("overlay")?;

    unsafe {
        if libc::mount(
            source.as_ptr(),
            merged.as_ptr(),
            fstype.as_ptr(),
            0,
            opts.as_ptr() as *const libc::c_void,
        ) != 0
        {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EBUSY) {
                return Err(err)
                    .with_context(|| format!("mounting overlay at {}", dirs.merged.display()));
            }
        }
    }

    Ok(())
}

/// Unmounts the overlay filesystem for the sandbox if it is mounted.
#[cfg(target_os = "linux")]
pub fn unmount_overlay(dirs: &OverlayDirs) -> Result<()> {
    let merged = CString::new(dirs.merged.as_os_str().as_bytes())?;
    unsafe {
        if libc::umount2(merged.as_ptr(), 0) != 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::NotFound
                && err.kind() != std::io::ErrorKind::InvalidInput
            {
                return Err(err)
                    .with_context(|| format!("unmounting overlay at {}", dirs.merged.display()));
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn mount_overlay(_dirs: &OverlayDirs) -> Result<()> {
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn unmount_overlay(_dirs: &OverlayDirs) -> Result<()> {
    Ok(())
}

/// Applies the configured seccomp filter for the sandbox child processes.
#[cfg(target_os = "linux")]
pub fn apply_seccomp_filter(syscalls: &[u32]) -> Result<()> {
    if syscalls.is_empty() {
        return Err(anyhow::anyhow!("seccomp syscall allowlist is empty"));
    }

    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| "setting PR_SET_NO_NEW_PRIVS");
        }
    }

    let mut filters = vec![
        libc::sock_filter {
            code: BPF_LD | BPF_W | BPF_ABS,
            jt: 0,
            jf: 0,
            k: SECCOMP_DATA_ARCH_OFFSET,
        },
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 1,
            jf: 0,
            k: AUDIT_ARCH_X86_64,
        },
        libc::sock_filter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_KILL_PROCESS,
        },
        libc::sock_filter {
            code: BPF_LD | BPF_W | BPF_ABS,
            jt: 0,
            jf: 0,
            k: SECCOMP_DATA_NR_OFFSET,
        },
    ];

    for &nr in syscalls {
        filters.push(libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 1,
            k: nr,
        });
        filters.push(libc::sock_filter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ALLOW,
        });
    }

    filters.push(libc::sock_filter {
        code: BPF_RET | BPF_K,
        jt: 0,
        jf: 0,
        k: seccomp_errno(libc::EPERM as u32),
    });

    let mut program = libc::sock_fprog {
        len: filters.len() as u16,
        filter: filters.as_mut_ptr(),
    };

    let result = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &mut program as *mut libc::sock_fprog,
            0,
            0,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| "loading seccomp filter");
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_seccomp_filter(_syscalls: &[u32]) -> Result<()> {
    Ok(())
}

/// Resolves syscall names to numeric identifiers and sorts/deduplicates them.
#[cfg(target_os = "linux")]
pub fn resolve_seccomp_numbers(syscalls: &[String]) -> Result<Vec<u32>> {
    let table = syscall_numbers()?;
    let mut numbers = Vec::with_capacity(syscalls.len());

    for name in syscalls {
        let sysno = table
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown syscall in allowlist: {name}"))?;
        if sysno < 0 {
            return Err(anyhow::anyhow!(
                "syscall {name} resolved to negative identifier {sysno}"
            ));
        }
        numbers.push(sysno as u32);
    }

    numbers.sort_unstable();
    numbers.dedup();
    Ok(numbers)
}

#[cfg(not(target_os = "linux"))]
pub fn resolve_seccomp_numbers(_syscalls: &[String]) -> Result<Vec<u32>> {
    Ok(Vec::new())
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
        .truncate(true)
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
