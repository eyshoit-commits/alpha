//! Telemetry scaffolding for bkg-db.

#![allow(dead_code)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use opentelemetry::{
    global,
    metrics::{Histogram, Meter},
    trace::Tracer as _,
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    metrics::{
        reader::{DefaultAggregationSelector, DefaultTemporalitySelector},
        SdkMeterProvider,
    },
    resource::Resource,
    runtime::Tokio,
    trace::{self as sdktrace, Sampler, TracerProvider},
};

/// Telemetry exporter contract used by the database layer.
pub trait TelemetryExporter: Send + Sync {
    fn record_metric(&self, name: &str, value: f64) -> Result<()>;
    fn record_trace(&self, name: &str) -> Result<()>;
    fn flush(&self) -> Result<()>;
}

/// Concrete OTLP/OpenTelemetry-backed implementation used in tests & daemon wiring.
#[derive(Debug)]
pub struct OtlpTelemetryExporter {
    tracer_provider: sdktrace::TracerProvider,
    tracer: sdktrace::Tracer,
    meter_provider: SdkMeterProvider,
    meter: Meter,
    histograms: Mutex<HashMap<String, Histogram<f64>>>,
    recorded_metrics: Mutex<HashMap<String, Vec<f64>>>,
    recorded_traces: Mutex<Vec<String>>,
}

impl OtlpTelemetryExporter {
    fn new(
        tracer_provider: sdktrace::TracerProvider,
        meter_provider: SdkMeterProvider,
        service_name: &str,
    ) -> Self {
        let tracer = tracer_provider.tracer(service_name.to_string());
        let meter = meter_provider.meter(service_name.to_string());
        Self {
            tracer_provider,
            tracer,
            meter_provider,
            meter,
            histograms: Mutex::new(HashMap::new()),
            recorded_metrics: Mutex::new(HashMap::new()),
            recorded_traces: Mutex::new(Vec::new()),
        }
    }

    /// Helper for tests that want to peek into metrics written during the run.
    #[cfg(test)]
    pub fn metric_values(&self, name: &str) -> Vec<f64> {
        self.recorded_metrics
            .lock()
            .expect("metrics lock poisoned")
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    /// Helper for tests to inspect emitted span names.
    #[cfg(test)]
    pub fn span_names(&self) -> Vec<String> {
        self.recorded_traces
            .lock()
            .expect("traces lock poisoned")
            .clone()
    }
}

impl TelemetryExporter for OtlpTelemetryExporter {
    fn record_metric(&self, name: &str, value: f64) -> Result<()> {
        let histogram = {
            let mut guard = self
                .histograms
                .lock()
                .map_err(|_| anyhow!("histogram map poisoned"))?;
            guard
                .entry(name.to_string())
                .or_insert_with(|| self.meter.f64_histogram(name).init())
                .clone()
        };
        histogram.record(value, &[]);

        self.recorded_metrics
            .lock()
            .map_err(|_| anyhow!("metrics buffer poisoned"))?
            .entry(name.to_string())
            .or_default()
            .push(value);

        Ok(())
    }

    fn record_trace(&self, name: &str) -> Result<()> {
        let mut span = self.tracer.start(name.to_string());
        span.end();

        self.recorded_traces
            .lock()
            .map_err(|_| anyhow!("trace buffer poisoned"))?
            .push(name.to_string());

        Ok(())
    }

    fn flush(&self) -> Result<()> {
        self.tracer_provider
            .force_flush()
            .context("flushing tracer provider")?;
        self.meter_provider
            .force_flush()
            .context("flushing meter provider")?;
        Ok(())
    }
}

/// Builder used to configure OTLP exporters for the database layer.
#[derive(Debug, Clone)]
pub struct TelemetryBlueprint {
    endpoint: Option<String>,
    service_name: String,
    sampling_rate: f64,
    disable_exporters: bool,
    metric_period: Duration,
}

impl Default for TelemetryBlueprint {
    fn default() -> Self {
        Self {
            endpoint: None,
            service_name: "bkg-db".to_string(),
            sampling_rate: 1.0,
            disable_exporters: false,
            metric_period: Duration::from_secs(10),
        }
    }
}

impl TelemetryBlueprint {
    /// Creates a new blueprint with sensible defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a custom OTLP endpoint instead of relying on environment variables.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Overrides the service name used in OTEL resources.
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = name.into();
        self
    }

    /// Adjusts the trace sampling rate (0.0 – 1.0).
    pub fn with_sampling_rate(mut self, rate: f64) -> Self {
        self.sampling_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Disables OTLP exporters (useful for unit tests).
    pub fn without_exporter(mut self) -> Self {
        self.disable_exporters = true;
        self
    }

    /// Sets the periodic export interval for metrics.
    pub fn with_metric_period(mut self, period: Duration) -> Self {
        self.metric_period = period;
        self
    }

    /// Builds the exporter and returns a sharable handle.
    pub fn build(self) -> Result<Arc<OtlpTelemetryExporter>> {
        let resource = Resource::new(vec![KeyValue::new(
            "service.name",
            self.service_name.clone(),
        )]);

        let trace_config = sdktrace::Config::default()
            .with_sampler(Sampler::TraceIdRatioBased(self.sampling_rate))
            .with_resource(resource.clone());

        let tracer_provider = if self.disable_exporters {
            sdktrace::TracerProvider::builder()
                .with_config(trace_config)
                .build()
        } else {
            let exporter_builder = match &self.endpoint {
                Some(endpoint) => opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint),
                None => opentelemetry_otlp::new_exporter().tonic().with_env(),
            };
            let exporter = exporter_builder
                .build_span_exporter()
                .context("building OTLP trace exporter")?;

            sdktrace::TracerProvider::builder()
                .with_config(trace_config)
                .with_batch_exporter(exporter, Tokio)
                .build()
        };

        global::set_tracer_provider(tracer_provider.clone());

        let meter_provider = if self.disable_exporters {
            SdkMeterProvider::builder()
                .with_resource(resource.clone())
                .build()
        } else {
            let exporter_builder = match &self.endpoint {
                Some(endpoint) => opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint),
                None => opentelemetry_otlp::new_exporter().tonic().with_env(),
            };
            let metric_exporter = exporter_builder
                .build_metrics_exporter(
                    Box::new(DefaultTemporalitySelector::new()),
                    Box::new(DefaultAggregationSelector::new()),
                )
                .context("building OTLP metrics exporter")?;

            opentelemetry_otlp::new_pipeline()
                .metrics(Tokio, tokio::spawn)
                .with_period(self.metric_period)
                .with_exporter(metric_exporter)
                .with_resource(resource.clone())
                .build()
                .context("building OTLP metrics pipeline")?
        };

        Ok(Arc::new(OtlpTelemetryExporter::new(
            tracer_provider,
            meter_provider,
            &self.service_name,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_metrics_and_traces() {
        let exporter = TelemetryBlueprint::new()
            .with_service_name("bkg-db-test")
            .with_sampling_rate(1.0)
            .without_exporter()
            .build()
            .expect("exporter");

        let exporter_clone = exporter.clone();
        let span_task = tokio::spawn(async move {
            for idx in 0..10 {
                exporter_clone
                    .record_trace(&format!("span-{idx}"))
                    .expect("record span");
            }
        });

        let exporter_clone = exporter.clone();
        let metric_task = tokio::spawn(async move {
            for idx in 0..25 {
                let value = (idx % 5) as f64;
                exporter_clone
                    .record_metric("db.query.latency", value)
                    .expect("record metric");
            }
        });

        span_task.await.expect("span task");
        metric_task.await.expect("metric task");

        exporter.flush().expect("flush exporter");

        let spans = exporter.span_names();
        assert_eq!(spans.len(), 10);
        assert!(spans.iter().any(|name| name == "span-0"));

        let metrics = exporter.metric_values("db.query.latency");
        assert_eq!(metrics.len(), 25);
        assert!(metrics
            .iter()
            .any(|value| (*value - 3.0).abs() < f64::EPSILON));
    }
}
