//! Telemetry scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;

// TODO(bkg-db/telemetry): Binde OpenTelemetry Exporter & Metrics Pipeline ein.

pub trait TelemetryExporter {
    fn record_metric(&self, name: &str, value: f64) -> Result<()>;
    fn record_trace(&self, name: &str) -> Result<()>;
    fn flush(&self) -> Result<()>;
}

#[derive(Debug, Default, Clone)]
pub struct TelemetryBlueprint;

impl TelemetryBlueprint {
    pub fn new() -> Self {
        Self
    }
}
