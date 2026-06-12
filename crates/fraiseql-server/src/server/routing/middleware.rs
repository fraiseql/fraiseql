//! Global middleware layers: metrics, tracing, CORS, body limits, header limits,
//! timeout, and rate limiting.

use axum::{Router, extract::DefaultBodyLimit, middleware};
use fraiseql_core::db::traits::DatabaseAdapter;
use tracing::info;

use super::super::{Server, cors_layer_restricted, metrics_middleware, trace_layer};
use crate::{middleware::security_headers_middleware, routes::graphql::AppState};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Apply global middleware layers to the router.
    pub(super) fn apply_middleware(&self, mut app: Router, state: &AppState<A>) -> Router {
        let metrics = state.metrics.clone();

        // Add HTTP metrics middleware (tracks requests and response status codes)
        // This runs on ALL routes, even when metrics endpoints are disabled
        app = app.layer(middleware::from_fn_with_state(metrics, metrics_middleware));

        // Add security response headers (nosniff/XFO/HSTS/Referrer/CSP/XSS) to every
        // response (M-sec-headers). Set-if-absent, so a handler that needs a different
        // policy (e.g. the playground's relaxed CSP) is preserved.
        app = app.layer(middleware::from_fn(security_headers_middleware));

        // Add middleware
        if self.config.tracing_enabled {
            app = app.layer(trace_layer());
        }

        if self.config.cors_enabled {
            let origins = if self.config.cors_origins.is_empty() {
                tracing::warn!(
                    "CORS enabled but no origins configured. Using localhost:3000 as default. \
                     Set cors_origins in config for production."
                );
                vec!["http://localhost:3000".to_string()]
            } else {
                self.config.cors_origins.clone()
            };
            app = app.layer(cors_layer_restricted(&origins));
        }

        // Add request body size limit (default 1 MB -- prevents memory exhaustion)
        if self.config.max_request_body_bytes > 0 {
            info!(
                max_bytes = self.config.max_request_body_bytes,
                "Request body size limit enabled"
            );
            app = app.layer(DefaultBodyLimit::max(self.config.max_request_body_bytes));
        }

        // Add HTTP header count and size limits (prevents header-flooding DoS)
        {
            let max_header_count = self.config.max_header_count;
            let max_header_bytes = self.config.max_header_bytes;
            info!(max_header_count, max_header_bytes, "HTTP header limits enabled");
            app = app.layer(axum::middleware::from_fn(move |req, next| {
                crate::middleware::header_limits_middleware(
                    req,
                    next,
                    max_header_count,
                    max_header_bytes,
                )
            }));
        }

        // Add per-request timeout (optional -- defence against runaway DB queries).
        if let Some(timeout_secs) = self.config.request_timeout_secs {
            use std::time::Duration;

            use tower_http::timeout::TimeoutLayer;

            info!(timeout_secs, "Request timeout enabled");
            app = app.layer(TimeoutLayer::with_status_code(
                axum::http::StatusCode::REQUEST_TIMEOUT,
                Duration::from_secs(timeout_secs),
            ));
        }

        // Add rate limiting middleware if configured.
        if let Some(ref limiter) = self.rate_limiter {
            use axum::Extension;

            use crate::middleware::rate_limit::rate_limit_middleware;

            info!("Enabling rate limiting middleware");
            app = app
                .layer(middleware::from_fn(rate_limit_middleware))
                .layer(Extension(limiter.clone()));
        }

        app
    }
}
