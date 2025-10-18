use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};
use uuid::Uuid;

use crate::ResourceLimits;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_path: PathBuf,
    pub hmac_key: Option<Vec<u8>>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            log_path: PathBuf::from("./logs/audit.jsonl"),
            hmac_key: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuditLogWriter {
    state: Arc<AuditLogState>,
}

#[derive(Debug)]
struct AuditLogState {
    path: PathBuf,
    hmac_key: Option<Vec<u8>>,
    lock: Mutex<()>,
}

impl AuditLogWriter {
    pub fn try_new(config: &AuditConfig) -> Result<Self> {
        if let Some(parent) = config.log_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating audit log directory {}", parent.display()))?;
        }

        Ok(Self {
            state: Arc::new(AuditLogState {
                path: config.log_path.clone(),
                hmac_key: config.hmac_key.clone(),
                lock: Mutex::new(()),
            }),
        })
    }

    pub async fn append(&self, event: &AuditEvent) -> Result<()> {
        let _guard = self.state.lock.lock().await;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.state.path)
            .await
            .with_context(|| format!("opening audit log {}", self.state.path.display()))?;

        let payload = serde_json::to_vec(event).context("serializing audit payload")?;
        let signature = match &self.state.hmac_key {
            Some(key) => {
                let mut mac = HmacSha256::new_from_slice(key)
                    .context("initializing HMAC for audit log entry")?;
                mac.update(&payload);
                let digest = mac.finalize().into_bytes();
                Some(STANDARD_NO_PAD.encode(digest))
            }
            None => None,
        };

        let line = AuditLogLine {
            event: event.clone(),
            signature,
        };

        let encoded = serde_json::to_vec(&line).context("serializing audit line")?;
        file.write_all(&encoded).await?;
        file.write_all(b"\n").await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub sandbox_id: Uuid,
    pub namespace: String,
    #[serde(flatten)]
    pub kind: AuditEventKind,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum AuditEventKind {
    #[serde(rename = "sandbox_created")]
    Created {
        name: String,
        runtime: String,
        limits: ResourceLimits,
    },
    #[serde(rename = "sandbox_started")]
    Started,
    #[serde(rename = "sandbox_exec")]
    Exec {
        command: String,
        args: Vec<String>,
        exit_code: Option<i32>,
        duration_ms: u64,
        timed_out: bool,
    },
    #[serde(rename = "sandbox_stopped")]
    Stopped,
    #[serde(rename = "sandbox_deleted")]
    Deleted,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AuditLogLine {
    #[serde(flatten)]
    event: AuditEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
}

pub fn verify_signed_line(line: &str, key: &[u8]) -> Result<AuditEvent> {
    if key.is_empty() {
        return Err(anyhow::anyhow!("audit verification key is empty"));
    }

    let parsed: AuditLogLine =
        serde_json::from_str(line).context("parsing audit log line for verification")?;
    let signature = parsed
        .signature
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("audit log line missing signature"))?;
    let signature_bytes = STANDARD_NO_PAD
        .decode(signature)
        .context("decoding audit log signature")?;

    let mut mac = HmacSha256::new_from_slice(key).context("initializing HMAC for verification")?;
    let payload =
        serde_json::to_vec(&parsed.event).context("serializing audit event for verification")?;
    mac.update(&payload);
    mac.verify_slice(&signature_bytes)
        .context("audit log signature mismatch")?;

    Ok(parsed.event)
}

impl AuditEvent {
    pub fn sandbox_created(
        sandbox_id: Uuid,
        namespace: impl Into<String>,
        name: impl Into<String>,
        runtime: impl Into<String>,
        limits: ResourceLimits,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            sandbox_id,
            namespace: namespace.into(),
            kind: AuditEventKind::Created {
                name: name.into(),
                runtime: runtime.into(),
                limits,
            },
        }
    }

    pub fn sandbox_started(sandbox_id: Uuid, namespace: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            sandbox_id,
            namespace: namespace.into(),
            kind: AuditEventKind::Started,
        }
    }

    pub fn sandbox_exec(
        sandbox_id: Uuid,
        namespace: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        exit_code: Option<i32>,
        duration_ms: u64,
        timed_out: bool,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            sandbox_id,
            namespace: namespace.into(),
            kind: AuditEventKind::Exec {
                command: command.into(),
                args,
                exit_code,
                duration_ms,
                timed_out,
            },
        }
    }

    pub fn sandbox_stopped(sandbox_id: Uuid, namespace: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            sandbox_id,
            namespace: namespace.into(),
            kind: AuditEventKind::Stopped,
        }
    }

    pub fn sandbox_deleted(sandbox_id: Uuid, namespace: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            sandbox_id,
            namespace: namespace.into(),
            kind: AuditEventKind::Deleted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD_NO_PAD;

    #[tokio::test]
    async fn writes_signed_entries() {
        let path = std::env::temp_dir().join(format!("audit-{}.jsonl", Uuid::new_v4()));
        let config = AuditConfig {
            enabled: true,
            log_path: path.clone(),
            hmac_key: Some(b"super-secret".to_vec()),
        };

        let writer = AuditLogWriter::try_new(&config).unwrap();
        let event = AuditEvent::sandbox_started(Uuid::new_v4(), "ns");
        writer.append(&event).await.unwrap();

        let contents = tokio::fs::read_to_string(&path).await.unwrap();
        tokio::fs::remove_file(&path).await.unwrap();
        let line = contents.lines().next().unwrap();
        let parsed = verify_signed_line(line, b"super-secret").unwrap();
        assert_eq!(parsed.kind, AuditEventKind::Started);
        assert_eq!(parsed.sandbox_id, event.sandbox_id);
    }

    #[tokio::test]
    async fn omits_signature_when_key_absent() {
        let path = std::env::temp_dir().join(format!("audit-nokey-{}.jsonl", Uuid::new_v4()));
        let config = AuditConfig {
            enabled: true,
            log_path: path.clone(),
            hmac_key: None,
        };

        let writer = AuditLogWriter::try_new(&config).unwrap();
        let event = AuditEvent::sandbox_deleted(Uuid::new_v4(), "ns");
        writer.append(&event).await.unwrap();

        let contents = tokio::fs::read_to_string(&path).await.unwrap();
        tokio::fs::remove_file(&path).await.unwrap();
        let line = contents.lines().next().unwrap();
        let value: AuditLogLine = serde_json::from_str(line).unwrap();
        assert!(value.signature.is_none());
    }
}
