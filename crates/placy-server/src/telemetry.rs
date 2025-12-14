//! Observability setup: logging, metrics, and tracing.

use crate::config::{LogFormat, LogRotation, ObservabilitySettings};
use anyhow::Result;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::runtime::Tokio;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

/// Telemetry guard that ensures proper shutdown.
pub struct TelemetryGuard {
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        // Shutdown OpenTelemetry on drop
        opentelemetry::global::shutdown_tracer_provider();
    }
}

/// Initialize the observability stack.
///
/// This sets up:
/// - Console logging
/// - File logging (if enabled)
/// - OpenTelemetry tracing (if enabled)
/// - Prometheus metrics
pub fn init_telemetry(
    settings: &ObservabilitySettings,
) -> Result<(TelemetryGuard, Option<PrometheusHandle>)> {
    // Build env filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&settings.logging.level));

    // Console layer
    let console_layer = if settings.logging.console_enabled {
        let layer = match settings.logging.format {
            LogFormat::Json => fmt::layer()
                .json()
                .with_span_events(FmtSpan::CLOSE)
                .with_current_span(true)
                .boxed(),
            LogFormat::Pretty => fmt::layer()
                .pretty()
                .with_span_events(FmtSpan::CLOSE)
                .boxed(),
            LogFormat::Compact => fmt::layer()
                .compact()
                .with_span_events(FmtSpan::CLOSE)
                .boxed(),
        };
        Some(layer)
    } else {
        None
    };

    // File layer
    let (file_layer, file_guard) = if settings.logging.file_enabled {
        let rotation = match settings.logging.file_rotation {
            LogRotation::Daily => Rotation::DAILY,
            LogRotation::Hourly => Rotation::HOURLY,
            LogRotation::Minutely => Rotation::MINUTELY,
            LogRotation::Never => Rotation::NEVER,
        };

        let file_appender = RollingFileAppender::new(
            rotation,
            &settings.logging.file_dir,
            &settings.logging.file_prefix,
        );

        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let layer = match settings.logging.format {
            LogFormat::Json => fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_span_events(FmtSpan::CLOSE)
                .boxed(),
            _ => fmt::layer()
                .with_writer(non_blocking)
                .with_span_events(FmtSpan::CLOSE)
                .with_ansi(false)
                .boxed(),
        };

        (Some(layer), Some(guard))
    } else {
        (None, None)
    };

    // OpenTelemetry layer
    let otel_layer = if settings.otel.enabled && settings.otel.traces_enabled {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&settings.otel.endpoint)
            .build()?;

        let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, Tokio)
            .build();

        let tracer = tracer_provider.tracer(settings.otel.service_name.clone());

        // Set the global tracer provider
        opentelemetry::global::set_tracer_provider(tracer_provider);

        Some(tracing_opentelemetry::layer().with_tracer(tracer))
    } else {
        None
    };

    // Prometheus metrics
    let prometheus_handle = if settings.metrics.enabled {
        let builder = PrometheusBuilder::new();
        let handle = builder.install_recorder()?;
        Some(handle)
    } else {
        None
    };

    // Combine all layers
    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .with(otel_layer)
        .init();

    // Log startup info
    tracing::info!(
        target: "placy_server::telemetry",
        console = settings.logging.console_enabled,
        file = settings.logging.file_enabled,
        otel = settings.otel.enabled,
        metrics = settings.metrics.enabled,
        "Telemetry initialized"
    );

    Ok((
        TelemetryGuard {
            _file_guard: file_guard,
        },
        prometheus_handle,
    ))
}

/// Record a processing metric.
pub fn record_processing_metrics(
    file_type: &str,
    input_size: usize,
    output_size: usize,
    duration_ms: u64,
    success: bool,
) {
    let labels = [
        ("file_type", file_type.to_string()),
        ("success", success.to_string()),
    ];

    metrics::counter!("placy_processing_total", &labels).increment(1);
    metrics::histogram!("placy_processing_duration_ms", &labels).record(duration_ms as f64);
    metrics::histogram!("placy_processing_input_bytes", &labels).record(input_size as f64);
    metrics::histogram!("placy_processing_output_bytes", &labels).record(output_size as f64);

    if success {
        metrics::counter!("placy_processing_success_total", &labels).increment(1);
    } else {
        metrics::counter!("placy_processing_error_total", &labels).increment(1);
    }
}

/// Record an authentication event.
pub fn record_auth_event(success: bool, key_name: Option<&str>) {
    let labels = [
        ("success", success.to_string()),
        ("key_name", key_name.unwrap_or("unknown").to_string()),
    ];

    metrics::counter!("placy_auth_attempts_total", &labels).increment(1);
}

/// Record request metrics.
#[allow(dead_code)] // Reserved for future request-level metrics middleware
pub fn record_request(method: &str, path: &str, status: u16, duration_ms: u64) {
    let labels = [
        ("method", method.to_string()),
        ("path", path.to_string()),
        ("status", status.to_string()),
    ];

    metrics::counter!("placy_http_requests_total", &labels).increment(1);
    metrics::histogram!("placy_http_request_duration_ms", &labels).record(duration_ms as f64);
}
