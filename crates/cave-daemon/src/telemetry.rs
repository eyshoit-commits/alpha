use std::env;

use anyhow::Result;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    resource::Resource,
    runtime::Tokio,
    trace::{self, Sampler},
};
use tracing::{info, warn};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

type Registry = tracing_subscriber::registry::Registry;

#[derive(Debug)]
pub struct TelemetryGuard {
    tracer_installed: bool,
}

impl TelemetryGuard {
    fn with_tracer() -> Self {
        Self {
            tracer_installed: true,
        }
    }

    fn without_tracer() -> Self {
        Self {
            tracer_installed: false,
        }
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if self.tracer_installed {
            if let Err(error) = global::shutdown_tracer_provider() {
                warn!(%error, "failed to flush OTEL tracer on shutdown");
            }
        }
    }
}

pub fn init(service_name: &str) -> Result<TelemetryGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let (sampling_rate, sampling_warning) =
        parse_sampling_rate(env::var("CAVE_OTEL_SAMPLING_RATE").ok().as_deref());

    let otel_layer = build_otel_layer(service_name, sampling_rate);
    let mut guard = TelemetryGuard::without_tracer();
    let mut otel_error: Option<anyhow::Error> = None;

    match otel_layer {
        Ok(Some(layer)) => {
            tracing_subscriber::registry()
                .with(filter.clone())
                .with(tracing_subscriber::fmt::layer())
                .with(layer)
                .init();
            guard = TelemetryGuard::with_tracer();
        }
        Ok(None) => {
            tracing_subscriber::registry()
                .with(filter.clone())
                .with(tracing_subscriber::fmt::layer())
                .init();
        }
        Err(error) => {
            tracing_subscriber::registry()
                .with(filter.clone())
                .with(tracing_subscriber::fmt::layer())
                .init();
            otel_error = Some(error);
        }
    }

    if let Some(message) = sampling_warning {
        warn!("{message}");
    }

    if let Some(error) = otel_error {
        warn!(%error, "failed to initialize OTEL exporter; continuing with console logs only");
    }

    info!(sampling_rate, "telemetry sampling configured");

    Ok(guard)
}

fn build_otel_layer(
    service_name: &str,
    sampling_rate: f64,
) -> Result<Option<OpenTelemetryLayer<Registry, trace::Tracer>>> {
    if sampling_rate <= 0.0 {
        return Ok(None);
    }

    global::set_text_map_propagator(TraceContextPropagator::new());

    let exporter = opentelemetry_otlp::new_exporter().tonic().with_env();
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(
            trace::Config::default()
                .with_sampler(Sampler::TraceIdRatioBased(sampling_rate))
                .with_resource(Resource::new(vec![KeyValue::new(
                    "service.name",
                    service_name.to_string(),
                )])),
        )
        .with_exporter(exporter)
        .install_batch(Tokio)?;

    Ok(Some(tracing_opentelemetry::layer().with_tracer(tracer)))
}

pub fn parse_sampling_rate(raw: Option<&str>) -> (f64, Option<String>) {
    match raw {
        None => (1.0, None),
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return (
                    1.0,
                    Some("CAVE_OTEL_SAMPLING_RATE is empty; defaulting to 1.0".to_string()),
                );
            }

            match trimmed.parse::<f64>() {
                Ok(parsed) => {
                    if (0.0..=1.0).contains(&parsed) {
                        (parsed, None)
                    } else {
                        let clamped = parsed.clamp(0.0, 1.0);
                        (
                            clamped,
                            Some(format!(
                                "CAVE_OTEL_SAMPLING_RATE={} outside 0.0..=1.0; clamped to {}",
                                trimmed, clamped
                            )),
                        )
                    }
                }
                Err(_) => (
                    1.0,
                    Some(format!(
                        "CAVE_OTEL_SAMPLING_RATE='{}' is not a valid float; defaulting to 1.0",
                        trimmed
                    )),
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_sampling_rate;

    #[test]
    fn parse_valid_sampling_rate() {
        assert_eq!(parse_sampling_rate(Some("0.25")), (0.25, None));
        assert_eq!(parse_sampling_rate(Some("1")), (1.0, None));
    }

    #[test]
    fn parse_out_of_bounds_sampling_rate() {
        let (rate, warning) = parse_sampling_rate(Some("1.5"));
        assert_eq!(rate, 1.0);
        assert!(warning
            .unwrap()
            .contains("CAVE_OTEL_SAMPLING_RATE=1.5 outside 0.0..=1.0"));
    }

    #[test]
    fn parse_invalid_sampling_rate() {
        let (rate, warning) = parse_sampling_rate(Some("abc"));
        assert_eq!(rate, 1.0);
        assert!(warning
            .unwrap()
            .contains("CAVE_OTEL_SAMPLING_RATE='abc' is not a valid float"));
    }

    #[test]
    fn parse_empty_sampling_rate() {
        let (rate, warning) = parse_sampling_rate(Some("   "));
        assert_eq!(rate, 1.0);
        assert!(warning
            .unwrap()
            .contains("CAVE_OTEL_SAMPLING_RATE is empty; defaulting to 1.0"));
    }
}
