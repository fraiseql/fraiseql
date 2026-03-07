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
// missing_docs: all public items must be documented (workspace lint enforces this)
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// Reason: module-level allows for pedantic lints that are too noisy across this crate.
// Each allow has a justification. Prefer per-function allows for new code.
#![allow(clippy::struct_excessive_bools)] // Reason: ServerConfig uses bools for independent feature flags
#![allow(clippy::cast_possible_truncation)] // Reason: intentional u128->u64 casts for metrics counters
#![allow(clippy::cast_precision_loss)] // Reason: intentional f64 conversions for averaging metrics
#![allow(clippy::doc_markdown)] // Reason: backtick-wrapping all technical terms would reduce readability
#![allow(clippy::module_name_repetitions)] // Reason: standard Rust API style (e.g., ServerConfig in server mod)
#![allow(clippy::must_use_candidate)] // Reason: builder methods return Self but callers chain, not inspect
#![allow(clippy::missing_panics_doc)] // Reason: panics are eliminated by design; remaining are unreachable
#![allow(clippy::needless_pass_by_value)] // Reason: axum extractors require owned types in handler signatures
#![allow(clippy::unused_async)] // Reason: axum handler trait requires async fn even for sync operations
#![allow(clippy::similar_names)] // Reason: domain terms (e.g., req/res, row/col) are conventional pairs
#![allow(clippy::unused_self)] // Reason: trait implementations require &self for interface consistency
#![allow(clippy::match_same_arms)] // Reason: explicit arms document each variant's intent in state machines
#![allow(clippy::double_must_use)] // Reason: CorsLayer from tower-http already carries #[must_use]
#![allow(clippy::unnecessary_wraps)] // Reason: handler signatures must return Result for axum compatibility
#![allow(clippy::return_self_not_must_use)] // Reason: builder pattern methods are always chained, never discarded
#![allow(clippy::too_many_lines)] // Reason: route setup and middleware composition are inherently verbose
#![allow(clippy::cast_sign_loss)] // Reason: intentional signed->unsigned for timestamp/duration conversions
#![allow(clippy::missing_fields_in_debug)] // Reason: connection pools and secrets excluded from Debug for safety
#![allow(clippy::default_trait_access)] // Reason: Default::default() is clearer than type inference in structs
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for concise test setup
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use improves test readability
#![allow(clippy::no_effect_underscore_binding)] // Reason: placeholder bindings for future middleware hooks
#![allow(clippy::cast_possible_wrap)] // Reason: timestamp and duration values are positive, within i64 range
#![allow(clippy::struct_field_names)] // Reason: field prefixes match domain terminology (e.g., auth_token)
#![allow(clippy::single_char_pattern)] // Reason: string patterns like "/" are clearer than char '/' in routes
#![allow(clippy::elidable_lifetime_names)] // Reason: explicit lifetimes document borrow relationships
#![allow(clippy::manual_let_else)] // Reason: match with early return is clearer for multi-line extraction
#![allow(clippy::redundant_closure)] // Reason: explicit closures clarify argument transformation in map chains
#![allow(clippy::unchecked_time_subtraction)] // Reason: duration arithmetic on SystemTime is infallible for metrics
#![allow(clippy::uninlined_format_args)] // Reason: named variables in format strings improve readability
#![allow(clippy::unnested_or_patterns)] // Reason: flat patterns with comments are clearer for state transitions
#![allow(clippy::used_underscore_binding)] // Reason: underscore-prefixed bindings used intentionally in destructuring
#![allow(clippy::cast_lossless)] // Reason: explicit as casts make type conversion visible at call site
#![allow(clippy::format_push_string)] // Reason: format!+push_str is clearer than write! for SQL query building
#![allow(clippy::if_same_then_else)] // Reason: separate branches document distinct code paths for maintenance
#![allow(clippy::ignored_unit_patterns)] // Reason: explicit _ in pattern matches documents intentional discard
#![allow(clippy::map_unwrap_or)] // Reason: map().unwrap_or() reads left-to-right; clearer for chains
#![allow(clippy::redundant_closure_for_method_calls)] // Reason: explicit closures clarify intent in higher-order functions
#![allow(clippy::single_match_else)] // Reason: match with else is clearer than if-let for variant extraction
#![allow(clippy::unnecessary_debug_formatting)] // Reason: Debug formatting in log messages provides diagnostic detail
#![allow(clippy::useless_format)] // Reason: format!() used to satisfy String type requirements in some APIs
#![allow(clippy::float_cmp)] // Reason: test assertions compare exact metric values, not computed floats

// API key authentication
pub mod api_key;
// Token revocation
pub mod token_revocation;

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

/// Runtime configuration types loaded from `fraiseql.toml` or environment variables.
pub mod config;
/// Observability infrastructure: tracing context and logging helpers.
pub mod observability;
/// Resilience primitives: backpressure and retry policies.
pub mod resilience;
/// Utilities for distributed tracing, span propagation, and trace context formatting.
pub mod tracing_utils;

// Webhooks (extracted to fraiseql-webhooks crate) — optional, enable with `features = ["webhooks"]`
#[cfg(feature = "webhooks")]
pub use fraiseql_webhooks as webhooks;

// fraiseql-files modules (merged) — optional, enable with `features = ["files"]`
#[cfg(feature = "files")]
pub mod files;

// Authentication (extracted to fraiseql-auth crate) — optional, enable with `features = ["auth"]`
#[cfg(feature = "auth")]
pub use fraiseql_auth as auth;

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

// Secrets management
pub mod secrets;

// Secrets management and encryption (extracted to fraiseql-secrets crate) — optional, enable with `features = ["secrets"]`
#[cfg(feature = "secrets")]
pub use fraiseql_secrets::{encryption, secrets_manager};

// Backup and disaster recovery — optional, enable with `features = ["backup"]`
#[cfg(feature = "backup")]
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

// MCP (Model Context Protocol) server - optional
#[cfg(feature = "mcp")]
pub mod mcp;

// Connection pool management and auto-tuning
pub mod pool;

// Trusted documents (query allowlist)
pub mod trusted_documents;

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
