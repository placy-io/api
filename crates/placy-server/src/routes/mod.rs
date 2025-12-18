// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! HTTP route configuration.
//!
//! This module organizes all HTTP routes into a modular structure:
//! - `health` - Health and readiness endpoints
//! - `metrics` - Prometheus metrics endpoint
//! - `v1` - API v1 endpoints

pub mod health;
pub mod metrics;
pub mod v1;

use actix_web::web;

/// Configure health and readiness routes.
///
/// These routes do not require authentication and are used by
/// load balancers and orchestrators for health checking.
///
/// Endpoints:
/// - GET `/health` - Health check
/// - GET `/ready` - Readiness check
pub fn configure_health_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(health::health))
        .route("/ready", web::get().to(health::ready));
}

/// Configure all API routes.
///
/// This function configures all authenticated API endpoints.
/// Should be called within an authenticated scope.
pub fn configure_api_routes(cfg: &mut web::ServiceConfig) {
    v1::configure(cfg);
}
