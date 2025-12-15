//! Health and readiness check handlers.

use actix_web::HttpResponse;
use serde::Serialize;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Health check endpoint.
///
/// Returns the current health status and version of the server.
/// This endpoint is used by load balancers and orchestrators to determine
/// if the service is alive.
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Readiness check endpoint.
///
/// Returns whether the service is ready to accept traffic.
/// This endpoint is used by orchestrators to determine if the service
/// should receive traffic.
pub async fn ready() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "ready": true
    }))
}
