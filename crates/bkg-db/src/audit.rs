//! Audit pipeline scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;
use serde_json::Value;

// TODO(bkg-db/audit): Implementiere signierte JSONL-Logs + cosign Integration.

#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub entity: String,
    pub action: String,
    pub payload: Value,
}

pub trait AuditLogWriter {
    fn append(&self, record: &AuditRecord) -> Result<()>;
    fn rotate(&self) -> Result<()>;
}

#[derive(Debug, Default, Clone)]
pub struct AuditPipelineBlueprint;

impl AuditPipelineBlueprint {
    pub fn new() -> Self {
        Self
    }
}
