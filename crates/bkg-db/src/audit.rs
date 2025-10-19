//! Audit pipeline for persisting tamper-evident records alongside the SQL store.

use std::{
    ffi::OsStr,
    fmt,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
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

/// Utility for parsing and verifying persisted audit log entries.
#[derive(Debug, Clone, Default)]
pub struct AuditLogVerifier {
    key: Option<Vec<u8>>,
}

impl AuditLogVerifier {
    /// Creates a verifier that optionally checks HMAC signatures.
    pub fn new(key: Option<Vec<u8>>) -> Self {
        Self { key }
    }

    /// Returns a verifier that enforces HMAC signatures with the provided key.
    pub fn with_key(key: Vec<u8>) -> Self {
        Self { key: Some(key) }
    }

    /// Parses and verifies a single JSONL entry.
    pub fn verify_line(&self, line: &str) -> Result<VerifiedAuditLine> {
        let parsed: AuditLogLine =
            serde_json::from_str(line).context("parsing audit log line from JSON")?;
        self.verify_parsed_line(parsed)
    }

    /// Parses and verifies each line from the provided reader, returning the
    /// validated envelopes.
    pub fn verify_reader<R: BufRead>(&self, reader: R) -> Result<Vec<VerifiedAuditLine>> {
        reader
            .lines()
            .enumerate()
            .map(|(index, line)| {
                let raw = line.with_context(|| format!("reading audit log line {}", index + 1))?;
                self.verify_line(&raw)
                    .with_context(|| format!("validating audit log line {}", index + 1))
            })
            .collect()
    }

    /// Opens a file from disk, streaming and validating each entry.
    pub fn verify_file<P: AsRef<Path>>(&self, path: P) -> Result<Vec<VerifiedAuditLine>> {
        let file = File::open(path.as_ref())
            .with_context(|| format!("opening audit log {}", path.as_ref().display()))?;
        let reader = BufReader::new(file);
        self.verify_reader(reader)
    }

    fn verify_parsed_line(&self, line: AuditLogLine) -> Result<VerifiedAuditLine> {
        match (self.key.as_deref(), line.hmac.as_ref()) {
            (Some(key), _) => line.verify_hmac(key).context("verifying audit log HMAC")?,
            (None, Some(_)) => {
                return Err(anyhow!(
                    "audit log line carries an HMAC signature but the verifier has no key"
                ))
            }
            (None, None) => {}
        }

        Ok(VerifiedAuditLine::from(line))
    }
}

/// Materialised audit log entry returned by [`AuditLogVerifier`].
#[derive(Debug, Clone)]
pub struct VerifiedAuditLine {
    pub version: String,
    pub event_id: String,
    pub timestamp: DateTime<Utc>,
    pub entity: String,
    pub action: String,
    pub payload: Value,
    pub hmac: Option<String>,
}

impl From<AuditLogLine> for VerifiedAuditLine {
    fn from(line: AuditLogLine) -> Self {
        Self {
            version: line.envelope.version.to_string(),
            event_id: line.envelope.event_id,
            timestamp: line.envelope.timestamp,
            entity: line.envelope.entity,
            action: line.envelope.action,
            payload: line.envelope.payload,
            hmac: line.hmac,
        }
    }
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
            .write(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening audit log {}", path.display()))
    }
}

#[derive(Debug, Clone)]
struct RotationOutcome {
    rotated_path: PathBuf,
    cosign: Option<CosignConfig>,
}

impl RotationOutcome {
    fn into_cosign_job(self) -> Option<(CosignConfig, PathBuf)> {
        self.cosign.map(|cosign| (cosign, self.rotated_path))
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

    fn rotate_locked(state: &mut AuditLogState) -> Result<Option<RotationOutcome>> {
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
        state.current_size = 0;
        state.file = Some(new_file);

        Ok(Some(RotationOutcome {
            rotated_path,
            cosign: state.cosign.clone(),
        }))
    }
}

impl AuditLogWriter for FileAuditLogWriter {
    fn append(&self, record: &AuditRecord) -> Result<()> {
        let envelope = AuditEnvelope::from_record(record);
        let canonical = serde_json::to_vec(&envelope).context("serializing audit envelope")?;
        let hmac_key = {
            let guard = self.state.lock();
            guard.hmac_key.clone()
        };
        let signature = match hmac_key.as_deref() {
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
        let cosign_job = {
            let mut guard = self.state.lock();

            let rotation = if guard.current_size + entry_size > guard.max_bytes {
                FileAuditLogWriter::rotate_locked(&mut guard)?
            } else {
                None
            };

            let file = guard
                .file
                .as_mut()
                .ok_or_else(|| anyhow!("audit log file handle missing"))?;
            file.write_all(&encoded).context("writing audit log line")?;
            file.write_all(b"\n").context("writing audit log newline")?;
            file.flush().context("flushing audit log")?;
            file.sync_data().context("syncing audit log to disk")?;

            guard.current_size += entry_size;

            Ok(rotation.and_then(|outcome| outcome.into_cosign_job()))
        }?;

        if let Some((cosign, rotated_path)) = cosign_job {
            cosign.sign(&rotated_path)?;
        }

        Ok(())
    }

    fn rotate(&self) -> Result<()> {
        let cosign_job = {
            let mut guard = self.state.lock();
            FileAuditLogWriter::rotate_locked(&mut guard)?
                .and_then(|outcome| outcome.into_cosign_job())
        };

        if let Some((cosign, rotated_path)) = cosign_job {
            cosign.sign(&rotated_path)?;
        }

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

impl AuditLogLine {
    fn canonical_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(&self.envelope).context("serializing audit envelope")
    }

    fn verify_hmac(&self, key: &[u8]) -> Result<()> {
        let signature = self
            .hmac
            .as_ref()
            .ok_or_else(|| anyhow!("audit log entry is not signed"))?;
        let signature_bytes = STANDARD_NO_PAD
            .decode(signature.as_bytes())
            .context("decoding audit log HMAC")?;

        let mut mac =
            HmacSha256::new_from_slice(key).context("initializing audit log HMAC verifier")?;
        mac.update(&self.canonical_bytes()?);
        mac.verify_slice(&signature_bytes)
            .map_err(|_| anyhow!("audit log HMAC verification failed"))
    }
}

fn rotated_log_path(path: &Path) -> PathBuf {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let parent = path.parent().map(Path::to_path_buf);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audit");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("jsonl");

    let rotated_name = format!("{}.{}.{}", stem, timestamp, extension);

    parent
        .unwrap_or_else(|| PathBuf::from("."))
        .join(rotated_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use jsonschema::JSONSchema;
    use serde_json::json;
    use std::{fs, path::PathBuf};
    use tempfile::tempdir;

    #[test]
    fn emitted_lines_validate_against_schema() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 4096,
            hmac_key: Some(b"schema-secret".to_vec()),
            cosign: None,
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record = AuditRecord {
            entity: "api_keys".into(),
            action: "insert".into(),
            payload: json!({"id": 1}),
        };

        writer.append(&record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let raw_line = contents.lines().next().unwrap();
        let value: Value = serde_json::from_str(raw_line).unwrap();

        let schema = json!({
            "type": "object",
            "required": [
                "version",
                "event_id",
                "timestamp",
                "entity",
                "action",
                "payload"
            ],
            "properties": {
                "version": {"type": "string"},
                "event_id": {
                    "type": "string",
                    "pattern": "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"
                },
                "timestamp": {"type": "string", "format": "date-time"},
                "entity": {"type": "string"},
                "action": {"type": "string"},
                "payload": {
                    "oneOf": [
                        {"type": "object"},
                        {"type": "array"},
                        {"type": "string"},
                        {"type": "number"},
                        {"type": "boolean"},
                        {"type": "null"}
                    ]
                },
                "hmac": {
                    "type": "string",
                    "pattern": "^[A-Za-z0-9+/]+$"
                }
            },
            "additionalProperties": false
        });

        let compiled = JSONSchema::compile(&schema).unwrap();
        if let Err(errors) = compiled.validate(&value) {
            let messages: Vec<String> = errors.map(|error| format!("{}", error)).collect();
            panic!("schema validation failed: {:?}", messages);
        }
    }

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
            payload: json!({"id": 1}),
        };

        writer.append(&record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let raw = contents.lines().next().unwrap();
        let verifier = AuditLogVerifier::with_key(b"super-secret".to_vec());
        let verified = verifier.verify_line(raw).unwrap();

        assert_eq!(verified.version, AUDIT_LOG_VERSION);
        assert_eq!(verified.entity, "api_keys");
        assert_eq!(verified.action, "insert");
        assert!(Uuid::parse_str(&verified.event_id).is_ok());
        DateTime::parse_from_rfc3339(&verified.timestamp.to_rfc3339()).unwrap();
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
            payload: json!({"id": 42}),
        };
        writer.append(&record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let line: AuditLogLine = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!(line.hmac.is_none());
    }

    #[test]
    fn verifier_requires_key_for_signed_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 4096,
            hmac_key: Some(b"secret".to_vec()),
            cosign: None,
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record = AuditRecord {
            entity: "api_keys".into(),
            action: "insert".into(),
            payload: json!({"id": 1}),
        };

        writer.append(&record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let raw = contents.lines().next().unwrap();
        let verifier = AuditLogVerifier::new(None);
        let error = verifier.verify_line(raw).unwrap_err();
        assert!(error.to_string().contains("verifier has no key"));
    }

    #[test]
    fn verifier_streams_file_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 4096,
            hmac_key: None,
            cosign: None,
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record_a = AuditRecord {
            entity: "sandboxes".into(),
            action: "create".into(),
            payload: json!({"namespace": "alpha"}),
        };
        let record_b = AuditRecord {
            entity: "sandboxes".into(),
            action: "start".into(),
            payload: json!({"namespace": "alpha"}),
        };

        writer.append(&record_a).unwrap();
        writer.append(&record_b).unwrap();

        let verifier = AuditLogVerifier::new(None);
        let entries = verifier.verify_file(&path).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entity, "sandboxes");
        assert_eq!(entries[0].action, "create");
        assert_eq!(entries[1].action, "start");
        assert!(entries.iter().all(|entry| entry.hmac.is_none()));
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
            payload: json!({"namespace": "alpha"}),
        };
        let record_b = AuditRecord {
            entity: "sandboxes".into(),
            action: "start".into(),
            payload: json!({"namespace": "alpha"}),
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
            .find(|name| name.starts_with("audit.") && name.ends_with(".jsonl"))
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
            payload: json!({"namespace": "alpha"}),
        };
        let record_b = AuditRecord {
            entity: "sandboxes".into(),
            action: "start".into(),
            payload: json!({"namespace": "alpha"}),
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
                    .map(|name| name.starts_with("audit.") && name.ends_with(".jsonl"))
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

    #[test]
    fn tampering_is_detected_by_verifier() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let config = FileAuditLogConfig {
            path: path.clone(),
            max_bytes: 4096,
            hmac_key: Some(b"tamper-secret".to_vec()),
            cosign: None,
        };
        let writer = FileAuditLogWriter::new(config).unwrap();

        let record = AuditRecord {
            entity: "api_keys".into(),
            action: "insert".into(),
            payload: json!({"id": 1}),
        };

        writer.append(&record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let raw = contents.lines().next().unwrap();
        let verifier = AuditLogVerifier::with_key(b"tamper-secret".to_vec());
        verifier.verify_line(raw).unwrap();

        let mut tampered_value: Value = serde_json::from_str(raw).unwrap();
        tampered_value["payload"] = json!({"id": 2});
        let tampered_raw = serde_json::to_string(&tampered_value).unwrap();

        let error = verifier.verify_line(&tampered_raw).unwrap_err();
        assert!(error.to_string().contains("verifying audit log HMAC"));
    }
}
