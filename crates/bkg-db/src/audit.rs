//! Audit pipeline implementation for bkg-db.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// High-level audit record emitted by the database layer.
#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub entity: String,
    pub action: String,
    pub payload: Value,
}

/// Writer contract for audit log sinks.
pub trait AuditLogWriter: Send + Sync + std::fmt::Debug {
    fn append(&self, record: &AuditRecord) -> Result<()>;
    fn rotate(&self) -> Result<()>;
}

/// File-backed audit log writer that emits JSON-Lines records with optional
/// HMAC signing and cosign integration during rotation.
#[derive(Debug)]
pub struct JsonlAuditLogWriter {
    inner: Arc<AuditLogInner>,
}

#[derive(Debug)]
struct AuditLogInner {
    config: AuditLogConfig,
    state: Mutex<AuditLogState>,
}

#[derive(Debug)]
struct AuditLogState {
    file: Option<File>,
    len: u64,
}

/// Configuration for [`JsonlAuditLogWriter`].
#[derive(Debug, Clone)]
pub struct AuditLogConfig {
    pub path: PathBuf,
    pub max_bytes: u64,
    pub hmac_key: Option<Vec<u8>>,
    pub cosign: Option<CosignConfig>,
}

impl Default for AuditLogConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("./logs/audit.jsonl"),
            max_bytes: 16 * 1024 * 1024,
            hmac_key: None,
            cosign: None,
        }
    }
}

/// Controls optional cosign signing after log rotation.
#[derive(Debug, Clone)]
pub struct CosignConfig {
    pub binary: PathBuf,
    pub key_path: PathBuf,
    pub additional_args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub output_signature: Option<PathBuf>,
}

impl JsonlAuditLogWriter {
    /// Creates a new writer according to the provided configuration.
    pub fn new(config: AuditLogConfig) -> Result<Self> {
        if let Some(dir) = config
            .path
            .parent()
            .and_then(|parent| (!parent.as_os_str().is_empty()).then_some(parent))
        {
            fs::create_dir_all(dir)
                .with_context(|| format!("creating audit directory {}", dir.display()))?;
        }

        let (file, len) = open_log_file(&config.path)?;
        Ok(Self {
            inner: Arc::new(AuditLogInner {
                config,
                state: Mutex::new(AuditLogState {
                    file: Some(file),
                    len,
                }),
            }),
        })
    }

    fn append_internal(&self, record: &AuditRecord) -> Result<()> {
        let mut state = self.inner.state.lock();
        let event = AuditEnvelope {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            entity: record.entity.clone(),
            action: record.action.clone(),
            payload: record.payload.clone(),
        };

        let payload = serde_json::to_vec(&event).context("serializing audit envelope")?;
        let signature = match self.inner.config.hmac_key.as_ref() {
            Some(key) => {
                if key.is_empty() {
                    return Err(anyhow!("configured audit HMAC key is empty"));
                }
                let mut mac =
                    HmacSha256::new_from_slice(key).context("initializing HMAC for audit log")?;
                mac.update(&payload);
                let digest = mac.finalize().into_bytes();
                Some(STANDARD_NO_PAD.encode(digest))
            }
            None => None,
        };

        let line = AuditLine { event, signature };
        let encoded = serde_json::to_vec(&line).context("encoding audit log line")?;
        let line_size = encoded.len() as u64 + 1; // account for newline

        if should_rotate(state.len, line_size, self.inner.config.max_bytes) {
            self.rotate_locked(&mut state)?;
        }

        let file = state
            .file
            .as_mut()
            .ok_or_else(|| anyhow!("audit log file handle missing"))?;
        file.write_all(&encoded)
            .context("writing audit log entry")?;
        file.write_all(b"\n").context("writing audit log newline")?;
        file.flush().context("flushing audit log")?;
        state.len += line_size;
        Ok(())
    }

    fn rotate_locked(&self, state: &mut AuditLogState) -> Result<()> {
        let Some(mut file) = state.file.take() else {
            return Ok(());
        };

        file.flush().context("flushing audit log before rotation")?;
        file.sync_all()
            .context("syncing audit log before rotation")?;
        drop(file);

        let rotated_path = rotated_path(&self.inner.config.path, Utc::now());
        if self.inner.config.path.exists() {
            fs::rename(&self.inner.config.path, &rotated_path)
                .with_context(|| format!("rotating audit log to {}", rotated_path.display()))?;
        }

        if let Some(cosign) = &self.inner.config.cosign {
            run_cosign(cosign, &rotated_path)
                .with_context(|| format!("cosign signing {}", rotated_path.display()))?;
        }

        let (new_file, len) = open_log_file(&self.inner.config.path)?;
        state.file = Some(new_file);
        state.len = len;
        Ok(())
    }
}

impl AuditLogWriter for JsonlAuditLogWriter {
    fn append(&self, record: &AuditRecord) -> Result<()> {
        self.append_internal(record)
    }

    fn rotate(&self) -> Result<()> {
        let mut state = self.inner.state.lock();
        self.rotate_locked(&mut state)
    }
}

fn open_log_file(path: &Path) -> Result<(File, u64)> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening audit log {}", path.display()))?;
    let len = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    Ok((file, len))
}

fn should_rotate(current: u64, additional: u64, max_bytes: u64) -> bool {
    max_bytes > 0 && current > 0 && current + additional > max_bytes
}

fn rotated_path(base: &Path, now: DateTime<Utc>) -> PathBuf {
    let parent = base.parent().unwrap_or_else(|| Path::new(""));
    let stem = base.file_stem().and_then(|v| v.to_str()).unwrap_or("audit");
    let ext = base.extension().and_then(|v| v.to_str()).unwrap_or("jsonl");
    let timestamp = now.format("%Y%m%dT%H%M%SZ");
    parent.join(format!("{stem}-{timestamp}.{ext}"))
}

fn signature_path(rotated: &Path) -> PathBuf {
    let mut os_string = rotated.as_os_str().to_os_string();
    os_string.push(".sig");
    PathBuf::from(os_string)
}

fn run_cosign(config: &CosignConfig, rotated_path: &Path) -> Result<()> {
    let signature_path = config
        .output_signature
        .clone()
        .unwrap_or_else(|| signature_path(rotated_path));

    let mut command = Command::new(&config.binary);
    command.arg("sign-blob");
    command.arg(rotated_path);
    command.arg("--key");
    command.arg(&config.key_path);
    command.arg("--output-signature");
    command.arg(&signature_path);
    for arg in &config.additional_args {
        command.arg(arg);
    }
    for (key, value) in &config.env {
        command.env(key, value);
    }

    let output = command.output().context("running cosign")?;
    if !output.status.success() {
        return Err(anyhow!(
            "cosign failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    if !signature_path.exists() {
        return Err(anyhow!(
            "cosign did not produce signature file {}",
            signature_path.display()
        ));
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditEnvelope {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub entity: String,
    pub action: String,
    pub payload: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuditLine {
    pub event: AuditEnvelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuditPipelineBlueprint {
    config: AuditLogConfig,
}

impl Default for AuditPipelineBlueprint {
    fn default() -> Self {
        Self {
            config: AuditLogConfig::default(),
        }
    }
}

impl AuditPipelineBlueprint {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_log_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.path = path.into();
        self
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.config.max_bytes = max_bytes;
        self
    }

    pub fn with_hmac_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.config.hmac_key = Some(key.into());
        self
    }

    pub fn with_cosign(mut self, cosign: CosignConfig) -> Self {
        self.config.cosign = Some(cosign);
        self
    }

    pub fn build(&self) -> Result<JsonlAuditLogWriter> {
        JsonlAuditLogWriter::new(self.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
    use jsonschema::JSONSchema;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn writes_signed_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = AuditLogConfig {
            path: path.clone(),
            max_bytes: 1024,
            hmac_key: Some(b"super-secret".to_vec()),
            cosign: None,
        };
        let writer = JsonlAuditLogWriter::new(config).unwrap();
        let record = AuditRecord {
            entity: "api_keys".into(),
            action: "create".into(),
            payload: json!({"id": 1}),
        };

        writer.append(&record).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let line = contents.lines().next().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        let signature = parsed
            .get("signature")
            .and_then(|v| v.as_str())
            .expect("signature missing");
        let event_value = parsed.get("event").cloned().unwrap();
        let envelope: AuditEnvelope = serde_json::from_value(event_value).unwrap();

        let signature_bytes = STANDARD_NO_PAD.decode(signature).unwrap();
        let mut mac = HmacSha256::new_from_slice(b"super-secret").unwrap();
        let payload = serde_json::to_vec(&envelope).unwrap();
        mac.update(&payload);
        mac.verify_slice(&signature_bytes).unwrap();
    }

    #[test]
    fn validates_json_schema() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let writer = JsonlAuditLogWriter::new(AuditLogConfig {
            path: path.clone(),
            max_bytes: 1024,
            hmac_key: None,
            cosign: None,
        })
        .unwrap();
        writer
            .append(&AuditRecord {
                entity: "sandbox".into(),
                action: "start".into(),
                payload: json!({"sandbox_id": "abc"}),
            })
            .unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let line: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).unwrap();

        let schema = json!({
            "type": "object",
            "required": ["event"],
            "properties": {
                "event": {
                    "type": "object",
                    "required": ["id", "timestamp", "entity", "action", "payload"],
                    "properties": {
                        "id": {"type": "string", "format": "uuid"},
                        "timestamp": {"type": "string", "format": "date-time"},
                        "entity": {"type": "string"},
                        "action": {"type": "string"},
                        "payload": {}
                    }
                },
                "signature": {"type": ["string", "null"]}
            }
        });

        let compiled = JSONSchema::compile(&schema).unwrap();
        if let Err(errors) = compiled.validate(&line) {
            panic!("schema validation failed: {:?}", errors.collect::<Vec<_>>());
        }
    }

    #[cfg(unix)]
    #[test]
    fn rotates_and_runs_cosign() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let cosign_path = dir.path().join("cosign.sh");
        let signature_log = dir.path().join("cosign.log");
        let script = format!(
            "#!/bin/sh\noutput=\"\"\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    --output-signature)\n      shift\n      output=\"$1\"\n      ;;\n  esac\n  echo $1 >> {log}\n  shift\ndone\nif [ -z \"$output\" ]; then\n  exit 1\nfi\necho signed > \"$output\"\n",
            log = signature_log.display()
        );
        fs::write(&cosign_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&cosign_path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&cosign_path, permissions).unwrap();
        }
        let key_path = dir.path().join("cosign.key");
        fs::write(&key_path, "dummy").unwrap();

        let writer = JsonlAuditLogWriter::new(AuditLogConfig {
            path: path.clone(),
            max_bytes: 250,
            hmac_key: None,
            cosign: Some(CosignConfig {
                binary: cosign_path.clone(),
                key_path: key_path.clone(),
                additional_args: vec![],
                env: vec![],
                output_signature: None,
            }),
        })
        .unwrap();

        writer
            .append(&AuditRecord {
                entity: "sandbox".into(),
                action: "create".into(),
                payload: json!({"index": 1}),
            })
            .unwrap();

        // Trigger rotation manually to exercise cosign path.
        writer.rotate().unwrap();

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("audit-") && name.ends_with(".jsonl"))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(entries.len(), 1, "expected exactly one rotated file");
        let rotated = &entries[0];
        let signature_path = {
            let mut os_string = rotated.as_os_str().to_os_string();
            os_string.push(".sig");
            std::path::PathBuf::from(os_string)
        };
        assert!(signature_path.exists(), "signature file missing");
        let signature_content = fs::read_to_string(signature_path).unwrap();
        assert!(signature_content.contains("signed"));

        let log_contents = fs::read_to_string(signature_log).unwrap();
        assert!(log_contents.contains("sign-blob"));
    }
}
