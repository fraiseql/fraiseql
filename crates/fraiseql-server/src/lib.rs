//! FraiseQL HTTP Server
//!
//! HTTP server for FraiseQL v2 compiled GraphQL execution engine.
//!
//! # Architecture
//!
//! The server exposes a GraphQL HTTP endpoint that:
//! 1. Receives GraphQL queries via POST
//! 2. Executes queries using the runtime Executor
//! 3. Returns GraphQL-compliant JSON responses
//!
//! # Features
//!
//! - GraphQL endpoint (`/graphql`)
//! - Health check endpoint (`/health`)
//! - Schema introspection endpoint (`/introspection`)
//! - CORS support
//! - Compression (gzip, br, zstd)
//! - Request tracing
//! - APQ (Automatic Persisted Queries)
//! - Query caching
//! - Authentication middleware (optional)

#![forbid(unsafe_code)]

// CLI argument parsing (shared with fraiseql-cli) — requires `cli` feature
#[cfg(feature = "cli")]
pub mod cli;
// API key authentication
pub mod api_key;
// Token revocation
pub mod token_revocation;

// Original fraiseql-server modules
pub mod api;
pub mod error;
pub mod extractors;
#[cfg(feature = "federation")]
pub mod federation;
pub mod logging;
pub mod middleware;
pub mod routes;
pub mod schema;
pub mod server;
pub mod server_config;
pub mod sql_source_check;
pub mod subscriptions;
pub mod url_guard;
pub mod validation;

// Renamed to avoid conflicts with runtime modules
pub mod metrics_server;

/// Process-global `metrics`-facade recorder (Prometheus) install + render.
#[cfg(feature = "metrics")]
pub mod metrics_recorder;

// fraiseql-runtime modules (merged)

/// Runtime configuration types loaded from `fraiseql.toml` or environment variables.
pub mod config;
/// Resilience primitives: backpressure and retry policies.
pub mod resilience;
/// Utilities for distributed tracing, span propagation, and trace context formatting.
#[cfg(feature = "federation")]
pub mod tracing_utils;
#[cfg(not(feature = "federation"))]
pub mod tracing_utils {
    //! Stub tracing utilities when federation is disabled.
    use axum::http::HeaderMap;

    /// Stub trace context extraction when federation is disabled.
    #[allow(clippy::missing_const_for_fn)] // Reason: signature must match federation-enabled version which is not const
    #[must_use]
    pub fn extract_trace_context(_headers: &HeaderMap) -> Option<()> {
        None
    }

    /// Extract the W3C trace id from the inbound `traceparent` header.
    ///
    /// Feature-independent (used to stamp the change-log `trace_id`, #375), so it
    /// is a real implementation even without federation — identical to the
    /// federation-enabled [`tracing_utils::extract_trace_id`](super::tracing_utils).
    #[must_use]
    pub fn extract_trace_id(headers: &HeaderMap) -> Option<String> {
        let value = headers.get("traceparent")?.to_str().ok()?;
        let trace_id = value.split('-').nth(1)?;
        let valid = trace_id.len() == 32
            && trace_id.bytes().all(|b| b.is_ascii_hexdigit())
            && trace_id.bytes().any(|b| b != b'0');
        valid.then(|| trace_id.to_ascii_lowercase())
    }

    /// Extract the full W3C trace context as a JSON object for the change-log
    /// `trace_context` column (#375).
    ///
    /// Feature-independent — identical to the federation-enabled
    /// [`tracing_utils::extract_trace_context_json`](super::tracing_utils).
    #[must_use]
    pub fn extract_trace_context_json(headers: &HeaderMap) -> Option<serde_json::Value> {
        let traceparent = headers.get("traceparent")?.to_str().ok()?;
        let mut parts = traceparent.split('-');
        let (version, trace_id, parent_id, trace_flags) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        let is_hex =
            |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
        let valid = is_hex(version, 2)
            && is_hex(trace_id, 32)
            && trace_id.bytes().any(|b| b != b'0')
            && is_hex(parent_id, 16)
            && is_hex(trace_flags, 2);
        if !valid {
            return None;
        }
        let mut obj = serde_json::Map::with_capacity(5);
        obj.insert("version".to_owned(), version.to_ascii_lowercase().into());
        obj.insert("trace_id".to_owned(), trace_id.to_ascii_lowercase().into());
        obj.insert("parent_id".to_owned(), parent_id.to_ascii_lowercase().into());
        obj.insert("trace_flags".to_owned(), trace_flags.to_ascii_lowercase().into());
        if let Some(tracestate) = headers
            .get("tracestate")
            .and_then(|h| h.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            obj.insert("tracestate".to_owned(), tracestate.into());
        }
        Some(serde_json::Value::Object(obj))
    }
}

// Webhooks (extracted to fraiseql-webhooks crate) — optional, enable with `features = ["webhooks"]`
// Authentication (extracted to fraiseql-auth crate) — optional, enable with `features =
// ["auth"]`
#[cfg(feature = "auth")]
pub use fraiseql_auth as auth;
#[cfg(feature = "webhooks")]
pub use fraiseql_webhooks as webhooks;

/// Stub auth types compiled when the `auth` feature is disabled.
///
/// These zero-sized types allow internal code that references `crate::auth::*` to compile
/// in no-auth builds without requiring every call-site to be cfg-gated.  All stub methods
/// are pure stubs that the compiler will dead-code-eliminate.
#[cfg(not(feature = "auth"))]
pub mod auth {
    use std::sync::Arc;

    /// Stub for `fraiseql_auth::state_encryption::StateEncryptionService`.
    pub mod state_encryption {
        /// Zero-sized stub; never instantiated when `auth` feature is off.
        pub struct StateEncryptionService;
        impl StateEncryptionService {
            /// Stub: returns `None`.
            ///
            /// # Errors
            ///
            /// Currently infallible — always returns `Ok(None)`.
            /// Errors may be returned when the `auth` feature is enabled.
            pub fn from_compiled_schema(
                _s: &serde_json::Value,
            ) -> crate::Result<Option<std::sync::Arc<Self>>> {
                Ok(None)
            }
        }
    }

    /// Stub for `fraiseql_auth::PkceStateStore`.
    pub struct PkceStateStore;
    impl PkceStateStore {
        /// Stub: always returns `true` (in-memory).
        pub fn is_in_memory(&self) -> bool {
            true
        }

        /// Stub: no-op.
        pub async fn cleanup_expired(&self) {}
    }

    /// Stub for `fraiseql_auth::OidcServerClient`.
    pub struct OidcServerClient;
    impl OidcServerClient {
        /// Stub: always returns `None`.
        pub fn from_compiled_schema(_schema_json: &serde_json::Value) -> Option<Arc<Self>> {
            None
        }
    }
}

// Secrets management and encryption (extracted to fraiseql-secrets crate) — optional, enable with
// `features = ["secrets"]`
#[cfg(feature = "secrets")]
pub use fraiseql_secrets::{encryption, secrets_manager};

// TLS/SSL and encryption
pub mod tls;

// Observer management - optional
#[cfg(feature = "observers")]
pub mod observers;

// Arrow Flight integration - optional
#[cfg(feature = "arrow")]
pub mod arrow;

// MCP (Model Context Protocol) server - optional
#[cfg(feature = "mcp")]
pub mod mcp;

// Connection pool management and auto-tuning
pub mod pool;

// Realtime WebSocket server — entity change streams (complementary to subscriptions/)
pub mod realtime;

// Object storage backends (local, S3, GCS, Azure Blob)
pub mod storage;

// Server subsystem assembly and lifecycle management
pub mod subsystems;

// Trusted documents (query allowlist)
pub mod trusted_documents;

// Multi-tenancy: pool factory, executor construction, health monitoring
pub mod tenancy;

// Usage aggregation: in-memory mutation counters fed by tracing events
pub mod usage;

// Testing utilities
#[cfg(any(test, feature = "testing"))]
pub mod testing;

#[cfg(test)]
mod tests;

#[cfg(feature = "cli")]
pub use cli::{Cli, ServerArgs};
pub use logging::{
    ErrorDetails, LogLevel, LogMetrics, RequestContext, RequestId, RequestLogger, SourceLocation,
    StructuredLogEntry,
};
pub use metrics_server::{MetricsCollector, PrometheusMetrics};
pub use schema::CompiledSchemaLoader;
pub use server::Server;
pub use server_config::ServerConfig;
pub use tls::TlsSetup;
pub use validation::{ComplexityValidationError, RequestValidator};

/// Convenience re-exports for building a FraiseQL HTTP server.
///
/// Provides [`Server`], [`ServerConfig`], `CompiledSchema`, and [`RequestValidator`].
///
/// ```rust
/// use fraiseql_server::prelude::*;
/// ```
pub mod prelude {
    pub use fraiseql_core::schema::CompiledSchema;

    pub use crate::{
        ComplexityValidationError, RequestValidator, Server, ServerConfig, ServerError, TlsSetup,
    };
}

/// Server error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerError {
    /// Server binding error.
    #[error("Failed to bind server: {0}")]
    BindError(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Error from the FraiseQL execution engine (parse, validate, execute, …).
    ///
    /// Wraps the canonical [`fraiseql_core::error::FraiseQLError`] so engine
    /// failures bubble up through `ServerError` without losing the structured
    /// payload. The original variant name (`RuntimeError`) collided with the
    /// retired `fraiseql_error::RuntimeError` HTTP-shaped enum; `Engine`
    /// reflects what the variant actually wraps.
    #[error("Engine error: {0}")]
    Engine(#[from] fraiseql_core::error::FraiseQLError),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Database error.
    #[error("Database error: {0}")]
    Database(String),

    /// Validation error.
    #[error("Validation error: {0}")]
    Validation(String),

    /// Resource conflict error.
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Resource not found error.
    #[error("Not found: {0}")]
    NotFound(String),
}

/// Server result type.
pub type Result<T> = std::result::Result<T, ServerError>;
