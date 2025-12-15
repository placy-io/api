//! Archive processing endpoint handler.
//!
//! This module provides a unified endpoint for processing both JAR and ZIP archives.
//! The file type is auto-detected based on magic bytes and archive structure.

use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use placy_core::{process_archive, process_jar};
use std::io::Cursor;
use std::time::Instant;
use zip::ZipArchive;

use crate::auth::get_auth_context;
use crate::handlers::{
    build_placy_config, parse_placeholders_from_request, read_multipart_file, AppState,
    ErrorResponse, ProcessQuery,
};
use crate::telemetry::record_processing_metrics;

/// ZIP magic bytes: PK\x03\x04
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// Detected archive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveType {
    /// Java Archive (JAR) - a ZIP with META-INF/MANIFEST.MF
    Jar,
    /// Generic ZIP archive
    Zip,
}

impl ArchiveType {
    /// Returns the content type for HTTP responses.
    fn content_type(&self) -> &'static str {
        match self {
            ArchiveType::Jar => "application/java-archive",
            ArchiveType::Zip => "application/zip",
        }
    }

    /// Returns the metrics label for this archive type.
    fn metrics_label(&self) -> &'static str {
        match self {
            ArchiveType::Jar => "jar",
            ArchiveType::Zip => "zip",
        }
    }

    /// Returns a human-readable name for logging.
    fn display_name(&self) -> &'static str {
        match self {
            ArchiveType::Jar => "JAR",
            ArchiveType::Zip => "ZIP",
        }
    }
}

/// Detects the archive type from file data.
///
/// Uses magic bytes to verify it's a ZIP-based archive, then checks for
/// META-INF/MANIFEST.MF to determine if it's a JAR file.
fn detect_archive_type(data: &[u8]) -> Result<ArchiveType, String> {
    // Check minimum size for magic bytes
    if data.len() < 4 {
        return Err("File too small to be a valid archive".to_string());
    }

    // Check ZIP magic bytes
    if data[..4] != ZIP_MAGIC {
        return Err("Invalid archive: file does not have ZIP signature".to_string());
    }

    // Try to open as ZIP and check for JAR manifest
    let cursor = Cursor::new(data);
    let archive = ZipArchive::new(cursor).map_err(|e| format!("Failed to read archive: {e}"))?;

    // Check for META-INF/MANIFEST.MF to identify JAR
    let is_jar = archive
        .file_names()
        .any(|name: &str| name.eq_ignore_ascii_case("META-INF/MANIFEST.MF"));

    Ok(if is_jar {
        ArchiveType::Jar
    } else {
        ArchiveType::Zip
    })
}

/// Unified archive processing endpoint.
///
/// Automatically detects whether the uploaded file is a JAR or ZIP archive
/// and processes it accordingly.
///
/// # Detection Logic
/// 1. Verifies ZIP magic bytes (PK\x03\x04)
/// 2. Checks for META-INF/MANIFEST.MF to identify JAR files
/// 3. Routes to appropriate processor (JAR or ZIP)
///
/// # Request
/// - Method: POST
/// - Content-Type: multipart/form-data
/// - Form field: `file` - the archive to process
/// - Query parameters: placeholder key-value pairs (e.g., `?USER=john&ID=123`)
///
/// # Response
/// - Success: Returns processed archive with appropriate Content-Type
/// - Headers include: X-Processing-Time-Ms, X-Input-Size, X-Output-Size, X-Archive-Type
pub async fn process_handler(
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
                record_processing_metrics(
                    "unknown",
                    0,
                    0,
                    start.elapsed().as_millis() as u64,
                    false,
                );
                return HttpResponse::BadRequest().json(ErrorResponse {
                    error: "upload_error".to_string(),
                    message: e.to_string(),
                    details: None,
                });
            },
        };

    let input_size = file_data.len();

    // Detect archive type
    let archive_type = match detect_archive_type(&file_data) {
        Ok(t) => t,
        Err(e) => {
            record_processing_metrics(
                "unknown",
                input_size,
                0,
                start.elapsed().as_millis() as u64,
                false,
            );
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "invalid_archive".to_string(),
                message: "Failed to detect archive type".to_string(),
                details: Some(e),
            });
        },
    };

    tracing::info!(
        target: "placy_server::routes::v1::process",
        input_size = input_size,
        placeholders = placeholders.len(),
        archive_type = archive_type.display_name(),
        auth_key = auth_ctx.as_ref().map(|a| a.key_name.as_str()),
        "Processing {} file",
        archive_type.display_name()
    );

    // Build placy config
    let config = build_placy_config(&state.settings, placeholders);

    // Process based on detected type
    let result = match archive_type {
        ArchiveType::Jar => process_jar(&file_data, &config),
        ArchiveType::Zip => process_archive(&file_data, &config),
    };

    match result {
        Ok(output) => {
            let output_size = output.len();
            let duration_ms = start.elapsed().as_millis() as u64;

            record_processing_metrics(
                archive_type.metrics_label(),
                input_size,
                output_size,
                duration_ms,
                true,
            );

            tracing::info!(
                target: "placy_server::routes::v1::process",
                input_size,
                output_size,
                duration_ms,
                archive_type = archive_type.display_name(),
                "{} processing completed",
                archive_type.display_name()
            );

            HttpResponse::Ok()
                .content_type(archive_type.content_type())
                .append_header(("X-Processing-Time-Ms", duration_ms.to_string()))
                .append_header(("X-Input-Size", input_size.to_string()))
                .append_header(("X-Output-Size", output_size.to_string()))
                .append_header(("X-Archive-Type", archive_type.display_name()))
                .body(output)
        },
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            record_processing_metrics(
                archive_type.metrics_label(),
                input_size,
                0,
                duration_ms,
                false,
            );

            tracing::error!(
                target: "placy_server::routes::v1::process",
                error = %e,
                archive_type = archive_type.display_name(),
                "{} processing failed",
                archive_type.display_name()
            );

            HttpResponse::UnprocessableEntity().json(ErrorResponse {
                error: "processing_error".to_string(),
                message: format!("Failed to process {} file", archive_type.display_name()),
                details: Some(e.to_string()),
            })
        },
    }
}
