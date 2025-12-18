// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Prometheus metrics endpoint handler.

use actix_web::{web, HttpRequest, HttpResponse};

use crate::auth::get_auth_context;
use crate::handlers::{AppState, ErrorResponse};

/// Prometheus metrics endpoint.
///
/// Returns metrics in Prometheus text format for scraping.
/// Authentication may be required based on server configuration.
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
