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

// CLI argument parsing (shared with fraiseql-cli)
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
pub mod subscriptions;
pub mod validation;

// Renamed to avoid conflicts with runtime modules
pub mod metrics_server;

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
    pub fn extract_trace_context(_headers: &HeaderMap) -> Option<()> {
        None
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

// Object storage backends (local, S3, GCS, Azure Blob)
pub mod storage;

// Trusted documents (query allowlist)
pub mod trusted_documents;

// Testing utilities
#[cfg(any(test, feature = "testing"))]
pub mod testing;

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

/// Convenience re-exports for common server types.
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

    /// Runtime error.
    #[error("Runtime error: {0}")]
    RuntimeError(#[from] fraiseql_core::error::FraiseQLError),

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
