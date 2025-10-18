use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::Serialize;
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

        #[derive(Serialize)]
        struct AuditLine<'a> {
            #[serde(flatten)]
            event: &'a AuditEvent,
            #[serde(skip_serializing_if = "Option::is_none")]
            signature: Option<String>,
        }

        let line = AuditLine { event, signature };
        let encoded = serde_json::to_vec(&line).context("serializing audit line")?;
        file.write_all(&encoded).await?;
        file.write_all(b"\n").await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub sandbox_id: Uuid,
    pub namespace: String,
    #[serde(flatten)]
    pub kind: AuditEventKind,
}

#[derive(Debug, Serialize, Clone)]
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
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value.get("type").unwrap(), "sandbox_started");

        let signature = value.get("signature").and_then(|v| v.as_str()).unwrap();
        let mut mac = HmacSha256::new_from_slice(b"super-secret").unwrap();
        let payload = serde_json::to_vec(&event).unwrap();
        mac.update(&payload);
        let expected = STANDARD_NO_PAD.encode(mac.finalize().into_bytes());
        assert_eq!(signature, expected);
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
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(value.get("signature").is_none());
    }
}
