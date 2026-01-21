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
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// Allow common pedantic lints that are too noisy for this codebase
#![allow(clippy::struct_excessive_bools)] // ServerConfig uses bools for flags
#![allow(clippy::cast_possible_truncation)] // Intentional u128->u64 casts for metrics
#![allow(clippy::cast_precision_loss)] // Intentional f64 conversions for averages
#![allow(clippy::doc_markdown)] // Would require extensive backtick additions
#![allow(clippy::module_name_repetitions)] // Common in Rust APIs
#![allow(clippy::must_use_candidate)] // Too noisy for builder methods
#![allow(clippy::missing_errors_doc)] // Would require extensive doc additions
#![allow(clippy::missing_panics_doc)] // Would require extensive doc additions
#![allow(clippy::needless_pass_by_value)] // Sometimes clearer API
#![allow(clippy::unused_async)] // Placeholder for future async work
#![allow(clippy::similar_names)] // Variable naming style
#![allow(clippy::unused_self)] // Often needed for trait consistency
#![allow(clippy::match_same_arms)] // Sometimes clearer to be explicit
#![allow(clippy::double_must_use)] // CorsLayer already has must_use

// Original fraiseql-server modules
pub mod server_config;
pub mod error;
pub mod logging;
pub mod middleware;
pub mod performance;
pub mod routes;
pub mod schema;
pub mod server;
pub mod validation;

// Renamed to avoid conflicts with runtime modules
pub mod metrics_server;
pub mod tracing_server;

// fraiseql-runtime modules (merged)
pub mod config;
pub mod lifecycle;
pub mod resilience;
pub mod observability;
pub mod runtime_middleware;
pub mod runtime_server;
pub mod runtime_state;

// fraiseql-webhooks modules (merged)
pub mod webhooks;

// fraiseql-files modules (merged)
pub mod files;

// Authentication modules (Phase 5)
pub mod auth;

// Testing utilities
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use server_config::ServerConfig;
pub use logging::{
    ErrorDetails, LogLevel, LogMetrics, RequestContext, RequestId, RequestLogger, SourceLocation,
    StructuredLogEntry,
};
pub use metrics_server::{MetricsCollector, PrometheusMetrics};
pub use performance::{
    OperationProfile, PerformanceMonitor, PerformanceStats, PerformanceTimer, QueryPerformance,
};
pub use schema::CompiledSchemaLoader;
pub use server::Server;
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
}

/// Server result type.
pub type Result<T> = std::result::Result<T, ServerError>;
