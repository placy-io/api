//! HTTP API handlers for JAR/ZIP processing.

use crate::auth::get_auth_context;
use crate::config::Settings;
use crate::telemetry::record_processing_metrics;
use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use placy_core::{process_archive, process_jar, Config as PlacyConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Application state shared across handlers.
pub struct AppState {
    pub settings: Arc<Settings>,
    pub prometheus_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

/// Response for successful processing.
#[derive(Debug, Serialize)]
#[allow(dead_code)] // TODO: Use this struct in responses that deal with processing results async.
pub struct ProcessResponse {
    pub success: bool,
    pub input_size: usize,
    pub output_size: usize,
    pub processing_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Reserved query parameter names that are not treated as placeholders.
const RESERVED_PARAMS: &[&str] = &["placeholders"];

/// Query parameters for processing endpoints.
/// All query parameters (except reserved ones) are treated as placeholders.
#[derive(Debug, Deserialize)]
pub struct ProcessQuery {
    /// Placeholders as JSON object string (legacy support)
    #[serde(default)]
    pub placeholders: Option<String>,
}

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Health check endpoint.
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Readiness check endpoint.
pub async fn ready() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "ready": true
    }))
}

/// Prometheus metrics endpoint.
pub async fn metrics(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    // Check metrics auth if enabled
    if state.settings.auth.metrics_auth_enabled
        && state.settings.auth.enabled
        && get_auth_context(&req).is_none()
    {
        return HttpResponse::Unauthorized().json(ErrorResponse {
            error: "unauthorized".to_string(),
            message: "Authentication required for metrics endpoint".to_string(),
            details: None,
        });
    }

    match &state.prometheus_handle {
        Some(handle) => {
            let metrics = handle.render();
            HttpResponse::Ok()
                .content_type("text/plain; version=0.0.4; charset=utf-8")
                .body(metrics)
        },
        None => HttpResponse::ServiceUnavailable().json(ErrorResponse {
            error: "metrics_disabled".to_string(),
            message: "Metrics collection is not enabled".to_string(),
            details: None,
        }),
    }
}

/// Process a JAR file.
pub async fn process_jar_handler(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ProcessQuery>,
    mut payload: Multipart,
) -> HttpResponse {
    let auth_ctx = get_auth_context(&req);
    let start = Instant::now();

    // Parse placeholders from query parameters
    let placeholders = match parse_placeholders_from_request(&req, &query.placeholders) {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "invalid_placeholders".to_string(),
                message: "Failed to parse placeholders".to_string(),
                details: Some(e),
            });
        },
    };

    // Read file from multipart
    let file_data =
        match read_multipart_file(&mut payload, state.settings.processing.max_upload_size).await {
            Ok(data) => data,
            Err(e) => {
                record_processing_metrics("jar", 0, 0, start.elapsed().as_millis() as u64, false);
                return HttpResponse::BadRequest().json(ErrorResponse {
                    error: "upload_error".to_string(),
                    message: e.to_string(),
                    details: None,
                });
            },
        };

    let input_size = file_data.len();

    tracing::info!(
        target: "placy_server::handlers",
        input_size = input_size,
        placeholders = placeholders.len(),
        auth_key = auth_ctx.as_ref().map(|a| a.key_name.as_str()),
        "Processing JAR file"
    );

    // Build placy config
    let config = build_placy_config(&state.settings, placeholders);

    // Process the JAR
    match process_jar(&file_data, &config) {
        Ok(output) => {
            let output_size = output.len();
            let duration_ms = start.elapsed().as_millis() as u64;

            record_processing_metrics("jar", input_size, output_size, duration_ms, true);

            tracing::info!(
                target: "placy_server::handlers",
                input_size,
                output_size,
                duration_ms,
                "JAR processing completed"
            );

            HttpResponse::Ok()
                .content_type("application/java-archive")
                .append_header(("X-Processing-Time-Ms", duration_ms.to_string()))
                .append_header(("X-Input-Size", input_size.to_string()))
                .append_header(("X-Output-Size", output_size.to_string()))
                .body(output)
        },
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            record_processing_metrics("jar", input_size, 0, duration_ms, false);

            tracing::error!(
                target: "placy_server::handlers",
                error = %e,
                "JAR processing failed"
            );

            HttpResponse::UnprocessableEntity().json(ErrorResponse {
                error: "processing_error".to_string(),
                message: "Failed to process JAR file".to_string(),
                details: Some(e.to_string()),
            })
        },
    }
}

/// Process a ZIP archive.
pub async fn process_archive_handler(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ProcessQuery>,
    mut payload: Multipart,
) -> HttpResponse {
    let auth_ctx = get_auth_context(&req);
    let start = Instant::now();

    // Parse placeholders from query parameters
    let placeholders = match parse_placeholders_from_request(&req, &query.placeholders) {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "invalid_placeholders".to_string(),
                message: "Failed to parse placeholders".to_string(),
                details: Some(e),
            });
        },
    };

    // Read file from multipart
    let file_data =
        match read_multipart_file(&mut payload, state.settings.processing.max_upload_size).await {
            Ok(data) => data,
            Err(e) => {
                record_processing_metrics("zip", 0, 0, start.elapsed().as_millis() as u64, false);
                return HttpResponse::BadRequest().json(ErrorResponse {
                    error: "upload_error".to_string(),
                    message: e.to_string(),
                    details: None,
                });
            },
        };

    let input_size = file_data.len();

    tracing::info!(
        target: "placy_server::handlers",
        input_size = input_size,
        placeholders = placeholders.len(),
        auth_key = auth_ctx.as_ref().map(|a| a.key_name.as_str()),
        "Processing ZIP archive"
    );

    // Build placy config
    let config = build_placy_config(&state.settings, placeholders);

    // Process the archive
    match process_archive(&file_data, &config) {
        Ok(output) => {
            let output_size = output.len();
            let duration_ms = start.elapsed().as_millis() as u64;

            record_processing_metrics("zip", input_size, output_size, duration_ms, true);

            tracing::info!(
                target: "placy_server::handlers",
                input_size,
                output_size,
                duration_ms,
                "ZIP processing completed"
            );

            HttpResponse::Ok()
                .content_type("application/zip")
                .append_header(("X-Processing-Time-Ms", duration_ms.to_string()))
                .append_header(("X-Input-Size", input_size.to_string()))
                .append_header(("X-Output-Size", output_size.to_string()))
                .body(output)
        },
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            record_processing_metrics("zip", input_size, 0, duration_ms, false);

            tracing::error!(
                target: "placy_server::handlers",
                error = %e,
                "ZIP processing failed"
            );

            HttpResponse::UnprocessableEntity().json(ErrorResponse {
                error: "processing_error".to_string(),
                message: "Failed to process ZIP archive".to_string(),
                details: Some(e.to_string()),
            })
        },
    }
}

/// Parse placeholders from JSON string (legacy support).
fn parse_placeholders_json(json_str: &Option<String>) -> Result<HashMap<String, String>, String> {
    match json_str {
        Some(s) if !s.is_empty() => {
            serde_json::from_str(s).map_err(|e| format!("Invalid JSON: {e}"))
        },
        _ => Ok(HashMap::new()),
    }
}

/// Parse placeholders from request query parameters.
/// All query parameters (except reserved ones like 'placeholders') are treated as placeholders.
/// If the legacy 'placeholders' JSON parameter is provided, those values are merged in
/// (with explicit query params taking precedence).
fn parse_placeholders_from_request(
    req: &HttpRequest,
    legacy_placeholders: &Option<String>,
) -> Result<HashMap<String, String>, String> {
    // Start with legacy JSON placeholders if provided
    let mut placeholders = parse_placeholders_json(legacy_placeholders)?;

    // Parse all query parameters and add non-reserved ones as placeholders
    let query_string = req.query_string();
    if !query_string.is_empty() {
        for (key, value) in form_urlencoded::parse(query_string.as_bytes()) {
            let key_str = key.as_ref();
            // Skip reserved parameters
            if !RESERVED_PARAMS.contains(&key_str) {
                // Process the value: trim, replace spaces with underscores, validate alphanumeric
                let placeholder_key = key.trim().replace(' ', "_");

                // Verify that it contains only alphabets and digits (and underscores from replacement)
                if !placeholder_key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_')
                {
                    return Err(format!(
                        "Invalid placeholder value for key '{}': must contain only letters, digits, and spaces",
                        key_str
                    ));
                }

                // Query params override legacy JSON placeholders
                placeholders.insert(placeholder_key, value.to_string());
            }
        }
    }

    Ok(placeholders)
}

/// Read file data from multipart upload.
async fn read_multipart_file(payload: &mut Multipart, max_size: usize) -> Result<Vec<u8>, String> {
    let mut file_data = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = item.map_err(|e| format!("Multipart error: {e}"))?;

        // Check content disposition for file field
        let field_name = field
            .content_disposition()
            .and_then(|cd| cd.get_name().map(|s| s.to_string()))
            .unwrap_or_default();

        if field_name == "file" {
            while let Some(chunk) = field.next().await {
                let chunk = chunk.map_err(|e| format!("Read error: {e}"))?;

                if file_data.len() + chunk.len() > max_size {
                    return Err(format!(
                        "File size exceeds maximum allowed size of {max_size} bytes"
                    ));
                }

                file_data.extend_from_slice(&chunk);
            }
            break;
        }
    }

    if file_data.is_empty() {
        return Err("No file uploaded. Use 'file' as the form field name.".to_string());
    }

    Ok(file_data)
}

/// Build placy-core config from server settings.
fn build_placy_config(settings: &Settings, placeholders: HashMap<String, String>) -> PlacyConfig {
    PlacyConfig::builder()
        .with_placeholders(placeholders)
        .with_max_file_size(settings.processing.max_file_size)
        .with_max_file_count(settings.processing.max_file_count)
        .with_max_zip_size(settings.processing.max_zip_size)
        .with_max_compression_ratio(settings.processing.max_compression_ratio)
        .with_delete_process_file(settings.processing.delete_process_file)
        .with_delete_ignore_file(settings.processing.delete_ignore_file)
        .build()
}

/// Configure API routes.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .route("/process/jar", web::post().to(process_jar_handler))
            .route("/process/zip", web::post().to(process_archive_handler)),
    );
}

/// Configure health and metrics routes (no auth required for health).
pub fn configure_health_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(health))
        .route("/ready", web::get().to(ready));
}
