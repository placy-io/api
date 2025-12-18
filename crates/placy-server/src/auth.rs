// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! API key authentication middleware.

use crate::config::AuthSettings;
use crate::telemetry::record_auth_event;
use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpResponse,
};
use futures::future::{ok, Either, Ready};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Authenticated request context.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Name of the authenticated API key
    pub key_name: String,
    /// Rate limit for this key (if any)
    #[allow(dead_code)] // Reserved for future rate limiting implementation
    pub rate_limit: Option<u32>,
}

/// API key authentication middleware factory.
#[derive(Clone)]
pub struct ApiKeyAuth {
    settings: Arc<AuthSettings>,
    /// Whether this middleware should require auth (false for health endpoints)
    required: bool,
}

impl ApiKeyAuth {
    /// Create a new API key auth middleware that requires authentication.
    pub fn new(settings: Arc<AuthSettings>) -> Self {
        Self {
            settings,
            required: true,
        }
    }

    /// Create a middleware that makes auth optional (for health endpoints).
    #[allow(dead_code)] // Reserved for future route-specific auth configuration
    pub fn optional(settings: Arc<AuthSettings>) -> Self {
        Self {
            settings,
            required: false,
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for ApiKeyAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = ApiKeyAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(ApiKeyAuthMiddleware {
            service,
            settings: self.settings.clone(),
            required: self.required,
        })
    }
}

/// The actual middleware service.
pub struct ApiKeyAuthMiddleware<S> {
    service: S,
    settings: Arc<AuthSettings>,
    required: bool,
}

impl<S, B> Service<ServiceRequest> for ApiKeyAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = Either<
        Ready<Result<Self::Response, Self::Error>>,
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>,
    >;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // If auth is disabled globally, pass through
        if !self.settings.enabled {
            let fut = self.service.call(req);
            return Either::Right(Box::pin(async move {
                fut.await.map(|res| res.map_into_left_body())
            }));
        }

        // If auth is not required for this route, pass through
        if !self.required {
            let fut = self.service.call(req);
            return Either::Right(Box::pin(async move {
                fut.await.map(|res| res.map_into_left_body())
            }));
        }

        // Extract API key from header
        let api_key = req
            .headers()
            .get(&self.settings.api_key_header)
            .and_then(|v| v.to_str().ok());

        match api_key {
            Some(key) => {
                // Validate the key
                if let Some(api_key_config) = self.settings.validate_key(key) {
                    // Record successful auth
                    record_auth_event(true, Some(&api_key_config.name));

                    // Attach auth context to request
                    req.extensions_mut().insert(AuthContext {
                        key_name: api_key_config.name.clone(),
                        rate_limit: api_key_config.rate_limit,
                    });

                    let fut = self.service.call(req);
                    Either::Right(Box::pin(async move {
                        fut.await.map(|res| res.map_into_left_body())
                    }))
                } else {
                    // Invalid key
                    record_auth_event(false, None);
                    tracing::warn!(
                        target: "placy_server::auth",
                        "Invalid API key provided"
                    );

                    let response = HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "unauthorized",
                        "message": "Invalid API key"
                    }));

                    Either::Left(ok(req.into_response(response).map_into_right_body()))
                }
            },
            None => {
                // Missing key
                record_auth_event(false, None);
                tracing::warn!(
                    target: "placy_server::auth",
                    header = %self.settings.api_key_header,
                    "Missing API key header"
                );

                let response = HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": "unauthorized",
                    "message": format!("Missing {} header", self.settings.api_key_header)
                }));

                Either::Left(ok(req.into_response(response).map_into_right_body()))
            },
        }
    }
}

/// Extract auth context from request extensions.
pub fn get_auth_context(req: &actix_web::HttpRequest) -> Option<AuthContext> {
    req.extensions().get::<AuthContext>().cloned()
}
