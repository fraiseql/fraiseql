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

// Original fraiseql-server modules
pub mod api;
pub mod error;
pub mod extractors;
pub mod federation;
pub mod logging;
pub mod middleware;
pub mod performance;
pub mod routes;
pub mod schema;
pub mod server;
pub mod server_config;
pub mod subscriptions;
pub mod validation;

// Renamed to avoid conflicts with runtime modules
pub mod metrics_server;
pub mod tracing_server;

// fraiseql-runtime modules (merged)
pub mod config;
pub mod lifecycle;
pub mod observability;
pub mod operational;
pub mod resilience;
pub mod runtime_state;
pub mod tracing_utils;

// fraiseql-webhooks modules (merged)
pub mod webhooks;

// fraiseql-files modules (merged)
pub mod files;

// Authentication modules
pub mod auth;

// Secrets management
pub mod secrets;

// Phase 12: Secrets Manager Interface
pub mod secrets_manager;

// Field-level encryption for sensitive database fields
pub mod encryption;

// Backup and disaster recovery
pub mod backup;

// TLS/SSL and encryption
pub mod tls;
pub mod tls_listener;

// Observer management - optional
#[cfg(feature = "observers")]
pub mod observers;

// Arrow Flight integration - optional
#[cfg(feature = "arrow")]
pub mod arrow;

// Testing utilities
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use logging::{
    ErrorDetails, LogLevel, LogMetrics, RequestContext, RequestId, RequestLogger, SourceLocation,
    StructuredLogEntry,
};
pub use metrics_server::{MetricsCollector, PrometheusMetrics};
pub use performance::{
    OperationProfile, PerformanceMonitor, PerformanceStats, PerformanceTimer, QueryPerformance,
};
pub use schema::CompiledSchemaLoader;
pub use secrets::SecretManager;
pub use server::Server;
pub use server_config::ServerConfig;
pub use tls::TlsSetup;
pub use tracing_server::{SpanStatus, TraceContext, TraceEvent, TraceParseError, TraceSpan};
pub use validation::{RequestValidator, ValidationError};

/// Server error type.
#[derive(Debug, thiserror::Error)]
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
