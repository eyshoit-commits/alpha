#![cfg(target_os = "linux")]

use std::fs;
use std::time::Duration;

use bkg_db::Database;
use cave_kernel::{
    verify_signed_line, CaveKernel, CreateSandboxRequest, ExecRequest, KernelConfig,
    ProcessSandboxRuntime, SeccompAction, SeccompConfig,
};
use uuid::Uuid;

#[tokio::test]
async fn audit_log_records_blocked_syscall_and_write_attempts() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let workspace_root = std::env::temp_dir().join(format!("cave-audit-{}", Uuid::new_v4()));
    let audit_path = std::env::temp_dir().join(format!("cave-audit-log-{}.jsonl", Uuid::new_v4()));
    let key = b"kernel-audit-secret";

    let mut config = KernelConfig::default();
    config.workspace_root = workspace_root.clone();
    config.isolation.enable_namespaces = false;
    config.isolation.enable_overlayfs = true;
    config.isolation.enable_seccomp = true;
    config.isolation.enable_cgroups = false;
    let mut seccomp = SeccompConfig::default();
    seccomp.action = SeccompAction::Errno(libc::EPERM);
    seccomp = seccomp.deny("ptrace").deny("chmod");
    config.isolation.seccomp = Some(seccomp);
    config.audit.enabled = true;
    config.audit.hmac_key = Some(key.to_vec());
    config.audit.log_path = audit_path.clone();

    let runtime = ProcessSandboxRuntime::new(config.isolation.clone()).unwrap();
    let kernel = CaveKernel::new(db, runtime, config);

    let sandbox = kernel
        .create_sandbox(CreateSandboxRequest::new("ns", "audit"))
        .await
        .unwrap();
    kernel.start_sandbox(sandbox.id).await.unwrap();

    // Trigger a blocked ptrace syscall via seccomp
    let ptrace_script = r#"import ctypes, sys
ctypes.set_errno(0)
libc = ctypes.CDLL('libc.so.6')
PTRACE_TRACEME = 0
res = libc.ptrace(PTRACE_TRACEME, 0, None, None)
sys.exit(0 if res == 0 else 1)
"#;

    let ptrace_outcome = kernel
        .exec(
            sandbox.id,
            ExecRequest {
                command: "python3".into(),
                args: vec!["-c".into(), ptrace_script.into()],
                stdin: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await
        .unwrap();
    assert_eq!(ptrace_outcome.exit_code, Some(1));

    // Attempt to chmod a file which is denied by seccomp, simulating a blocked write/metadata change
    let chmod_script = r#"import os, sys
path = 'audit-write-test'
with open(path, 'w', encoding='utf-8') as handle:
    handle.write('data')
try:
    os.chmod(path, 0o777)
    sys.exit(0)
except PermissionError:
    sys.exit(1)
"#;

    let chmod_outcome = kernel
        .exec(
            sandbox.id,
            ExecRequest {
                command: "python3".into(),
                args: vec!["-c".into(), chmod_script.into()],
                stdin: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await
        .unwrap();
    assert_eq!(chmod_outcome.exit_code, Some(1));

    kernel.stop_sandbox(sandbox.id).await.unwrap();
    kernel.delete_sandbox(sandbox.id).await.unwrap();

    let contents = fs::read_to_string(&audit_path).unwrap();
    let mut exec_events = Vec::new();
    for line in contents.lines() {
        let event = verify_signed_line(line, key).unwrap();
        if let cave_kernel::AuditEventKind::Exec {
            command,
            exit_code,
            timed_out,
            ..
        } = event.kind
        {
            exec_events.push((command, exit_code, timed_out));
        }
    }

    assert!(
        exec_events
            .iter()
            .any(|(command, code, _)| { command == "python3" && *code == Some(1) }),
        "expected signed exec audit entry for blocked ptrace"
    );
    assert_eq!(exec_events.len(), 2, "expected two exec audit events");
    assert!(exec_events.iter().all(|(_, _, timed_out)| !timed_out));

    let _ = fs::remove_file(&audit_path);
    if workspace_root.exists() {
        let _ = fs::remove_dir_all(&workspace_root);
    }
}
