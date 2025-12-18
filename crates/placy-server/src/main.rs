// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Placy Server - HTTP API for JAR/ZIP placeholder replacement.
//!
//! This server provides a REST API for processing JAR and ZIP archives,
//! replacing placeholders with specified values.

mod auth;
mod config;
mod handlers;
mod routes;
mod telemetry;

use actix_web::{middleware, web, App, HttpServer};
use anyhow::Result;
use std::sync::Arc;
use tracing_actix_web::TracingLogger;

use crate::auth::ApiKeyAuth;
use crate::config::Settings;
use crate::handlers::AppState;

#[actix_web::main]
async fn main() -> Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Load configuration
    let settings = Settings::load().expect("Failed to load configuration");
    let settings = Arc::new(settings);

    // Initialize telemetry
    let (telemetry_guard, prometheus_handle) = telemetry::init_telemetry(&settings.observability)?;

    let bind_addr = settings.bind_address();

    tracing::info!(
        target: "placy_server",
        host = %settings.server.host,
        port = settings.server.port,
        auth_enabled = settings.auth.enabled,
        "Starting Placy Server"
    );

    // Create shared state
    let app_state = web::Data::new(AppState {
        settings: settings.clone(),
        prometheus_handle,
    });

    let auth_settings = Arc::new(settings.auth.clone());
    let metrics_path = settings.observability.metrics.path.clone();

    // Build and run server
    let server = HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            // Request logging
            .wrap(TracingLogger::default())
            // Default middleware
            .wrap(middleware::Compress::default())
            .wrap(middleware::NormalizePath::trim())
            // Health endpoints (no auth)
            .configure(routes::configure_health_routes)
            // Metrics endpoint (optional auth based on config)
            .route(&metrics_path, web::get().to(routes::metrics::metrics))
            // API routes with authentication
            .service(
                web::scope("")
                    .wrap(ApiKeyAuth::new(auth_settings.clone()))
                    .configure(routes::configure_api_routes),
            )
    })
    .bind(&bind_addr)?
    .workers(if settings.server.workers > 0 {
        settings.server.workers
    } else {
        num_cpus::get()
    })
    .shutdown_timeout(30);

    tracing::info!(
        target: "placy_server",
        address = %bind_addr,
        workers = if settings.server.workers > 0 { settings.server.workers } else { num_cpus::get() },
        "Server listening"
    );

    server.run().await?;

    // Cleanup
    drop(telemetry_guard);

    Ok(())
}
