// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! API v1 routes.
//!
//! This module contains all v1 API endpoints.

mod process;

use actix_web::web;

pub use process::process_handler;

/// Configure v1 API routes.
///
/// Registers the following endpoints:
/// - POST `/process` - Unified archive processing (auto-detects JAR/ZIP)
/// - POST `/process/jar` - Process JAR files (legacy, redirects to unified)
/// - POST `/process/zip` - Process ZIP archives (legacy, redirects to unified)
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            // Unified endpoint - auto-detects archive type
            .route("/process", web::post().to(process_handler))
            // Legacy endpoints - route to the same unified handler
            .route("/process/jar", web::post().to(process_handler))
            .route("/process/zip", web::post().to(process_handler)),
    );
}
