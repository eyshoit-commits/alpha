//! Realtime/CDC scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;
use serde_json::Value;

// TODO(bkg-db/realtime): Entwickle WAL-basiertes Pub/Sub System + Client SDK Hooks.

#[derive(Debug, Clone)]
pub struct ChangeEvent {
    pub channel: String,
    pub payload: Value,
}

pub trait RealtimeHub {
    fn publish(&self, event: ChangeEvent) -> Result<()>;
    fn subscribe(&self, channel: &str) -> Result<RealtimeSubscription>;
}

#[derive(Debug, Clone)]
pub struct RealtimeSubscription {
    pub channel: String,
}

impl RealtimeSubscription {
    pub fn cancel(self) -> Result<()> {
        Ok(())
    }
}
