//! Audit pipeline for persisting tamper-evident records alongside the SQL store.

use std::{
    ffi::OsStr,
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

const AUDIT_LOG_VERSION: &str = "1.0";

type HmacSha256 = Hmac<Sha256>;

/// Structured payload describing a single audit observation.
#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub entity: String,
    pub action: String,
    pub payload: Value,
}

/// Trait for append-only audit log writers.
pub trait AuditLogWriter: Send + Sync + fmt::Debug {
    fn append(&self, record: &AuditRecord) -> Result<()>;
    fn rotate(&self) -> Result<()>;
}

/// High-level pipeline wrapper that daemon/kernel components can clone and use.
#[derive(Debug, Clone)]
pub struct AuditPipeline {
    writer: Arc<dyn AuditLogWriter>,
}

impl AuditPipeline {
    pub fn new(writer: Arc<dyn AuditLogWriter>) -> Self {
        Self { writer }
    }

    pub fn emit(&self, record: &AuditRecord) -> Result<()> {
        self.writer.append(record)
    }

    pub fn rotate(&self) -> Result<()> {
        self.writer.rotate()
    }
}

/// Blueprint used by configuration loading code to materialise an [`AuditPipeline`].
#[derive(Debug, Clone)]
pub enum AuditPipelineBlueprint {
    Disabled,
    Writer(Arc<dyn AuditLogWriter>),
}

impl Default for AuditPipelineBlueprint {
    fn default() -> Self {
        Self::Disabled
    }
}

impl AuditPipelineBlueprint {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_writer(writer: Arc<dyn AuditLogWriter>) -> Self {
        Self::Writer(writer)
    }

    pub fn from_file_config(config: FileAuditLogConfig) -> Result<Self> {
        let writer = FileAuditLogWriter::new(config)?;
        Ok(Self::Writer(Arc::new(writer)))
    }

    pub fn build(&self) -> Option<AuditPipeline> {
        match self {
            AuditPipelineBlueprint::Disabled => None,
            AuditPipelineBlueprint::Writer(writer) => Some(AuditPipeline::new(writer.clone())),
        }
    }
}

/// Configuration for the JSONL audit writer.
#[derive(Debug, Clone)]
pub struct FileAuditLogConfig {
    pub path: PathBuf,
    pub max_bytes: u64,
    pub hmac_key: Option<Vec<u8>>,
    pub cosign: Option<CosignConfig>,
}

impl Default for FileAuditLogConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("./logs/audit.jsonl"),
            max_bytes: 10 * 1024 * 1024,
            hmac_key: None,
            cosign: None,
        }
    }
}

/// Configuration for signing rotated blobs with `cosign sign-blob`.
#[derive(Debug, Clone)]
pub struct CosignConfig {
    pub binary: PathBuf,
    pub key_path: PathBuf,
    pub signature_dir: Option<PathBuf>,
    pub environment: Vec<(String, String)>,
}

impl CosignConfig {
    fn signature_path(&self, artifact: &Path) -> Result<PathBuf> {
        let file_name = artifact
            .file_name()
            .ok_or_else(|| anyhow!("rotated audit log missing file name"))?;
        let sig_name = format!("{}.sig", file_name.to_string_lossy());
        let base_dir = self
            .signature_dir
            .as_ref()
            .cloned()
            .or_else(|| artifact.parent().map(Path::to_path_buf))
            .ok_or_else(|| anyhow!("unable to determine signature directory"))?;
        Ok(base_dir.join(sig_name))
    }

    fn sign(&self, artifact: &Path) -> Result<()> {
        let signature_path = self.signature_path(artifact)?;
        if let Some(parent) = signature_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating cosign signature dir {}", parent.display()))?;
        }

        let mut command = Command::new(&self.binary);
        command.arg("sign-blob");
        command.arg(artifact.as_os_str());
        command.arg("--key");
        command.arg(&self.key_path);
        command.arg("--output-signature");
        command.arg(&signature_path);

        for (key, value) in &self.environment {
            command.env(key, value);
        }

        let output = command
            .output()
            .with_context(|| format!("invoking cosign at {}", self.binary.display()))?;
        if !output.status.success() {
            return Err(anyhow!(
                "cosign sign-blob failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }
}

#[derive(Debug)]
struct AuditLogState {
    path: PathBuf,
    max_bytes: u64,
    hmac_key: Option<Vec<u8>>,
    cosign: Option<CosignConfig>,
    current_size: u64,
    file: Option<File>,
}

impl AuditLogState {
    fn open_file(path: &Path) -> Result<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening audit log {}", path.display()))
    }
}

/// Concrete implementation writing signed JSONL records to disk.
#[derive(Debug, Clone)]
pub struct FileAuditLogWriter {
    state: Arc<Mutex<AuditLogState>>,
}

impl FileAuditLogWriter {
    pub fn new(config: FileAuditLogConfig) -> Result<Self> {
        if config.max_bytes == 0 {
            return Err(anyhow!(
                "audit log rotation threshold must be greater than zero"
            ));
        }

        if let Some(parent) = config.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating audit log directory {}", parent.display()))?;
        }

        let file = AuditLogState::open_file(&config.path)?;
        let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);

        let state = AuditLogState {
            path: config.path,
            max_bytes: config.max_bytes,
            hmac_key: config.hmac_key,
            cosign: config.cosign,
            current_size,
            file: Some(file),
        };

        // ensure file is flushed at creation to avoid holding stale handles on Windows
        state
            .file
            .as_ref()
            .unwrap()
            .sync_data()
            .context("syncing audit log on initialization")?;

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }

    fn rotate_locked(state: &mut AuditLogState) -> Result<Option<PathBuf>> {
        if state.current_size == 0 {
            if state.file.is_none() {
                let file = AuditLogState::open_file(&state.path)?;
                state.current_size = file.metadata().map(|m| m.len()).unwrap_or(0);
                state.file = Some(file);
            }
            return Ok(None);
        }

        if let Some(mut file) = state.file.take() {
            file.sync_all()
                .context("syncing audit log before rotation")?;
        }

        let rotated_path = rotated_log_path(&state.path);
        fs::rename(&state.path, &rotated_path)
            .with_context(|| format!("rotating audit log to {}", rotated_path.display()))?;

        let mut new_file = AuditLogState::open_file(&state.path)?;
        new_file
            .set_len(0)
            .context("resetting audit log after rotation")?;
        let new_size = new_file.metadata().map(|m| m.len()).unwrap_or(0);
        state.current_size = new_size;
        state.file = Some(new_file);

        if let Some(cosign) = state.cosign.clone() {
            cosign.sign(&rotated_path)?;
        }

        Ok(Some(rotated_path))
    }
}

impl AuditLogWriter for FileAuditLogWriter {
    fn append(&self, record: &AuditRecord) -> Result<()> {
        let mut guard = self.state.lock();
        let envelope = AuditEnvelope::from_record(record);
        let canonical = serde_json::to_vec(&envelope).context("serializing audit envelope")?;
        let signature = match guard.hmac_key.as_deref() {
            Some(key) => {
                let mut mac =
                    HmacSha256::new_from_slice(key).context("initializing audit log HMAC")?;
                mac.update(&canonical);
                let digest = mac.finalize().into_bytes();
                Some(STANDARD_NO_PAD.encode(digest))
            }
            None => None,
        };

        let line = AuditLogLine {
            envelope,
            hmac: signature,
        };

        let encoded = serde_json::to_vec(&line).context("encoding audit log line")?;
        let entry_size = (encoded.len() + 1) as u64;

        if guard.current_size + entry_size > guard.max_bytes {
            FileAuditLogWriter::rotate_locked(&mut guard)?;
        }

        let file = guard
            .file
            .as_mut()
            .ok_or_else(|| anyhow!("audit log file handle missing"))?;
        file.write_all(&encoded).context("writing audit log line")?;
        file.write_all(b"\n").context("writing audit log newline")?;
        file.flush().context("flushing audit log")?;
        file.sync_data().context("syncing audit log to disk")?;

        guard.current_size += entry_size;
        Ok(())
    }

    fn rotate(&self) -> Result<()> {
        let mut guard = self.state.lock();
        FileAuditLogWriter::rotate_locked(&mut guard)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AuditEnvelope {
    version: &'static str,
    event_id: String,
    timestamp: DateTime<Utc>,
    entity: String,
    action: String,
    payload: Value,
}

impl AuditEnvelope {
    fn from_record(record: &AuditRecord) -> Self {
        Self {
            version: AUDIT_LOG_VERSION,
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            entity: record.entity.clone(),
            action: record.action.clone(),
            payload: record.payload.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AuditLogLine {
    #[serde(flatten)]
    envelope: AuditEnvelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    hmac: Option<String>,
}

fn rotated_log_path(path: &Path) -> PathBuf {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let suffix = Uuid::new_v4();
    let parent = path.parent().map(Path::to_path_buf);
    let rotated_name = match (path.file_stem(), path.extension()) {
        (Some(stem), Some(ext)) => format!(
            "{}-{}-{}.{}",
            stem.to_string_lossy(),
            timestamp,
            suffix,
            ext.to_string_lossy()
        ),
        (Some(stem), None) => format!("{}-{}-{}", stem.to_string_lossy(), timestamp, suffix),
        _ => format!("audit-{}-{}", timestamp, suffix),
    };

    parent
        .unwrap_or_else(|| PathBuf::from("."))
        .join(rotated_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use std::{fs, path::PathBuf};
    use tempfile::tempdir;

    #[test]
    fn writes_signed_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 4096,
            hmac_key: Some(b"super-secret".to_vec()),
            cosign: None,
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record = AuditRecord {
            entity: "api_keys".into(),
            action: "insert".into(),
            payload: serde_json::json!({"id": 1}),
        };

        writer.append(&record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let line: AuditLogLine = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert_eq!(line.envelope.version, AUDIT_LOG_VERSION);
        assert_eq!(line.envelope.entity, "api_keys");
        assert_eq!(line.envelope.action, "insert");
        assert!(Uuid::parse_str(&line.envelope.event_id).is_ok());
        DateTime::parse_from_rfc3339(&line.envelope.timestamp.to_rfc3339()).unwrap();

        let canonical = serde_json::to_vec(&line.envelope).unwrap();
        let mut mac = HmacSha256::new_from_slice(b"super-secret").unwrap();
        mac.update(&canonical);
        let expected = STANDARD_NO_PAD.encode(mac.finalize().into_bytes());
        assert_eq!(line.hmac.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn omits_signature_without_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 4096,
            hmac_key: None,
            cosign: None,
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record = AuditRecord {
            entity: "sandboxes".into(),
            action: "delete".into(),
            payload: serde_json::json!({"id": 42}),
        };
        writer.append(&record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let line: AuditLogLine = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!(line.hmac.is_none());
    }

    #[test]
    fn rotates_when_size_exceeded() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 512,
            hmac_key: None,
            cosign: None,
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record_a = AuditRecord {
            entity: "sandboxes".into(),
            action: "create".into(),
            payload: serde_json::json!({"namespace": "alpha"}),
        };
        let record_b = AuditRecord {
            entity: "sandboxes".into(),
            action: "start".into(),
            payload: serde_json::json!({"namespace": "alpha"}),
        };

        writer.append(&record_a).unwrap();
        writer.append(&record_b).unwrap();

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.starts_with("audit"))
            .collect();
        assert_eq!(entries.len(), 2, "expected base and rotated audit files");

        let rotated_file = entries
            .iter()
            .find(|name| name.starts_with("audit-") && name.ends_with(".jsonl"))
            .expect("rotated audit log present");
        let rotated_contents = std::fs::read_to_string(dir.path().join(rotated_file)).unwrap();
        let rotated_line_count = rotated_contents.lines().count();
        assert_eq!(rotated_line_count, 1);

        let active_contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(active_contents.lines().count(), 1);
    }

    #[test]
    fn cosign_is_invoked_during_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let cosign_script = dir.path().join("cosign.sh");
        let args_log = dir.path().join("cosign_args.log");
        let script = r#"#!/bin/sh
set -eu
echo "$@" > "${COSIGN_ARGS_PATH}"
sig=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output-signature" ]; then
    shift
    sig="$1"
  fi
  shift
done
if [ -n "$sig" ]; then
  echo "signed" > "$sig"
fi
"#;
        std::fs::write(&cosign_script, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&cosign_script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&cosign_script, perms).unwrap();
        }

        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 512,
            hmac_key: None,
            cosign: Some(CosignConfig {
                binary: cosign_script.clone(),
                key_path: PathBuf::from("cosign.key"),
                signature_dir: None,
                environment: vec![(
                    "COSIGN_ARGS_PATH".into(),
                    args_log.to_string_lossy().to_string(),
                )],
            }),
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record_a = AuditRecord {
            entity: "sandboxes".into(),
            action: "create".into(),
            payload: serde_json::json!({"namespace": "alpha"}),
        };
        let record_b = AuditRecord {
            entity: "sandboxes".into(),
            action: "start".into(),
            payload: serde_json::json!({"namespace": "alpha"}),
        };

        writer.append(&record_a).unwrap();
        writer.append(&record_b).unwrap();

        let args_contents = std::fs::read_to_string(&args_log).unwrap();
        assert!(args_contents.contains("sign-blob"));
        assert!(args_contents.contains("--output-signature"));

        let rotated_files: Vec<PathBuf> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(OsStr::to_str)
                    .map(|name| name.starts_with("audit-") && name.ends_with(".jsonl"))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(rotated_files.len(), 1);

        let sig_files: Vec<PathBuf> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(OsStr::to_str)
                    .map(|name| name.ends_with(".sig"))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(sig_files.len(), 1);
        let signature_contents = std::fs::read_to_string(&sig_files[0]).unwrap();
        assert_eq!(signature_contents.trim(), "signed");
    }
}
