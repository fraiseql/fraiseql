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
// Allowing missing_docs to focus on actionable warnings
#![allow(missing_docs)]
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
#![allow(clippy::unnecessary_wraps)] // Sometimes needed for API consistency
#![allow(clippy::return_self_not_must_use)] // Builder pattern doesn't always need #[must_use]
#![allow(clippy::too_many_lines)] // Some functions are necessarily long
#![allow(clippy::cast_sign_loss)] // Intentional signed->unsigned conversions
#![allow(clippy::missing_fields_in_debug)] // Some fields are intentionally excluded from Debug
#![allow(clippy::default_trait_access)] // Style preference
#![allow(clippy::wildcard_imports)] // Used in tests
#![allow(clippy::items_after_statements)] // Sometimes clearer to define items near usage
#![allow(clippy::no_effect_underscore_binding)] // Used for future placeholders
#![allow(clippy::cast_possible_wrap)] // Intentional for timestamp conversions
#![allow(clippy::struct_field_names)] // Field naming style
#![allow(clippy::single_char_pattern)] // String patterns are clearer
#![allow(clippy::elidable_lifetime_names)] // Explicit lifetimes are clearer
#![allow(clippy::manual_let_else)] // Style preference
#![allow(clippy::redundant_closure)] // Sometimes clearer to have explicit closure
#![allow(clippy::unchecked_time_subtraction)] // Checked arithmetic overkill for durations
#![allow(clippy::uninlined_format_args)] // Style preference
#![allow(clippy::unnested_or_patterns)] // Style preference
#![allow(clippy::used_underscore_binding)] // Intentional placeholder bindings
#![allow(clippy::cast_lossless)] // Explicit casts are clearer
#![allow(clippy::format_push_string)] // Sometimes clearer than write!
#![allow(clippy::if_same_then_else)] // Sometimes intentional for clarity
#![allow(clippy::ignored_unit_patterns)] // Style preference
#![allow(clippy::map_unwrap_or)] // Style preference
#![allow(clippy::redundant_closure_for_method_calls)] // Style preference
#![allow(clippy::single_match_else)] // Sometimes clearer than if-let
#![allow(clippy::unnecessary_debug_formatting)] // Debug is useful for logging
#![allow(clippy::useless_format)] // Sometimes needed for type inference
#![allow(clippy::float_cmp)] // Test assertions with exact float comparison are intentional

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
