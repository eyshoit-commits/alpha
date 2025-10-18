#![cfg(target_os = "linux")]

use std::fs;
use std::time::Duration;

use bkg_db::Database;
use cave_kernel::{
    CaveKernel, CreateSandboxRequest, ExecRequest, KernelConfig, ProcessSandboxRuntime,
};
use serde_json::Value;
use uuid::Uuid;

#[tokio::test]
async fn seccomp_blocks_ptrace_syscall() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let mut config = KernelConfig::default();
    config.audit.enabled = false;
    config.isolation.enable_namespaces = false;
    config.isolation.enable_cgroups = false;
    config.isolation.enable_overlayfs = true;
    config.isolation.enable_seccomp = true;

    let runtime = ProcessSandboxRuntime::new(config.isolation.clone()).unwrap();
    let kernel = CaveKernel::new(db, runtime, config);

    let sandbox = kernel
        .create_sandbox(CreateSandboxRequest::new("ns", "seccomp"))
        .await
        .unwrap();
    kernel.start_sandbox(sandbox.id).await.unwrap();

    let workspace_root = kernel
        .config()
        .workspace_for(&sandbox.namespace, sandbox.id);

    let script = r#"import ctypes, sys
ctypes.set_errno(0)
libc = ctypes.CDLL('libc.so.6')
PTRACE_TRACEME = 0
res = libc.ptrace(PTRACE_TRACEME, 0, None, None)
sys.exit(0 if res == 0 else 1)
"#;

    let outcome = kernel
        .exec(
            sandbox.id,
            ExecRequest {
                command: "python3".into(),
                args: vec!["-c".into(), script.into()],
                stdin: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await
        .unwrap();

    assert_eq!(outcome.exit_code, Some(1));
    assert!(!outcome.timed_out);

    kernel.stop_sandbox(sandbox.id).await.unwrap();
    kernel.delete_sandbox(sandbox.id).await.unwrap();

    if workspace_root.exists() {
        let _ = fs::remove_dir_all(workspace_root);
    }
}

#[tokio::test]
async fn cgroup_limits_are_written() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let mut config = KernelConfig::default();
    config.audit.enabled = false;
    config.isolation.enable_namespaces = false;
    config.isolation.enable_seccomp = false;
    config.isolation.enable_overlayfs = true;
    config.isolation.enable_cgroups = true;
    let cgroup_root = std::env::temp_dir().join(format!("cave-kernel-cgroup-{}", Uuid::new_v4()));
    config.isolation.cgroup_root = Some(cgroup_root.clone());

    let runtime = ProcessSandboxRuntime::new(config.isolation.clone()).unwrap();
    let kernel = CaveKernel::new(db, runtime, config);

    let sandbox = kernel
        .create_sandbox(CreateSandboxRequest::new("ns", "limits"))
        .await
        .unwrap();
    let started = kernel.start_sandbox(sandbox.id).await.unwrap();

    let group_dir = cgroup_root.join(started.id.to_string());
    let memory_path = group_dir.join("memory.max");
    let cpu_path = group_dir.join("cpu.max");

    let memory_value = tokio::fs::read_to_string(&memory_path).await.unwrap();
    let cpu_value = tokio::fs::read_to_string(&cpu_path).await.unwrap();

    assert_eq!(
        memory_value.trim(),
        started.limits().memory_limit_bytes.to_string()
    );
    assert!(cpu_value.contains(' '));

    kernel.stop_sandbox(sandbox.id).await.unwrap();
    kernel.delete_sandbox(sandbox.id).await.unwrap();

    if cgroup_root.exists() {
        let _ = fs::remove_dir_all(cgroup_root);
    }
}

#[tokio::test]
async fn audit_logs_capture_events_with_hardening() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let mut config = KernelConfig::default();
    config.isolation.enable_namespaces = false;
    config.isolation.enable_overlayfs = true;
    config.isolation.enable_seccomp = true;
    config.isolation.enable_cgroups = false;
    config.audit.enabled = true;
    config.audit.hmac_key = None;
    let audit_path =
        std::env::temp_dir().join(format!("cave-kernel-audit-{}.jsonl", Uuid::new_v4()));
    config.audit.log_path = audit_path.clone();

    let runtime = ProcessSandboxRuntime::new(config.isolation.clone()).unwrap();
    let kernel = CaveKernel::new(db, runtime, config);

    let sandbox = kernel
        .create_sandbox(CreateSandboxRequest::new("ns", "audit"))
        .await
        .unwrap();
    kernel.start_sandbox(sandbox.id).await.unwrap();

    kernel
        .exec(
            sandbox.id,
            ExecRequest {
                command: "echo".into(),
                args: vec!["hardened".into()],
                stdin: None,
                timeout: Some(Duration::from_secs(5)),
            },
        )
        .await
        .unwrap();

    kernel.stop_sandbox(sandbox.id).await.unwrap();
    kernel.delete_sandbox(sandbox.id).await.unwrap();

    let contents = fs::read_to_string(&audit_path).unwrap();
    let mut saw_exec = false;
    let mut lifecycle_events = 0;

    for line in contents.lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        match value.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "sandbox_exec" => saw_exec = true,
            "sandbox_started" | "sandbox_stopped" | "sandbox_created" | "sandbox_deleted" => {
                lifecycle_events += 1;
            }
            _ => {}
        }
    }

    assert!(saw_exec, "expected sandbox_exec audit event");
    assert!(
        lifecycle_events >= 3,
        "expected lifecycle events in audit log"
    );

    let _ = fs::remove_file(audit_path);
}
