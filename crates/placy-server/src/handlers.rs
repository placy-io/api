// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared types and helper functions for HTTP handlers.
//!
//! This module provides:
//! - `AppState` - Application state shared across handlers
//! - Response structs for API responses
//! - Helper functions for request processing

use crate::config::Settings;
use actix_multipart::Multipart;
use actix_web::HttpRequest;
use futures::StreamExt;
use placy_core::Config as PlacyConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Application state shared across handlers.
pub struct AppState {
    /// Server settings.
    pub settings: Arc<Settings>,
    /// Prometheus metrics handle for rendering metrics.
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
pub(crate) const RESERVED_PARAMS: &[&str] = &["placeholders"];

/// Query parameters for processing endpoints.
///
/// All query parameters (except reserved ones) are treated as placeholders.
#[derive(Debug, Deserialize)]
pub struct ProcessQuery {
    /// Placeholders as JSON object string (legacy support).
    #[serde(default)]
    pub placeholders: Option<String>,
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
///
/// All query parameters (except reserved ones like 'placeholders') are treated as placeholders.
/// If the legacy 'placeholders' JSON parameter is provided, those values are merged in
/// (with explicit query params taking precedence).
pub(crate) fn parse_placeholders_from_request(
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
                if !placeholder_key.is_ascii() {
                    return Err(format!(
                        "Invalid placeholder value for key '{}': must contain only ASCII characters.",
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
///
/// # Arguments
///
/// * `payload` - The multipart payload stream
/// * `max_size` - Maximum allowed file size in bytes
///
/// # Returns
///
/// The file data as a byte vector, or an error message.
pub(crate) async fn read_multipart_file(
    payload: &mut Multipart,
    max_size: usize,
) -> Result<Vec<u8>, String> {
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

/// Build placy-core config from server settings and placeholders.
///
/// # Arguments
///
/// * `settings` - Server configuration settings
/// * `placeholders` - Map of placeholder keys to replacement values
///
/// # Returns
///
/// A configured `PlacyConfig` instance.
pub(crate) fn build_placy_config(
    settings: &Settings,
    placeholders: HashMap<String, String>,
) -> PlacyConfig {
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
