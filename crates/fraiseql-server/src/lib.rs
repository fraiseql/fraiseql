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

pub mod config;
pub mod middleware;
pub mod routes;
pub mod server;

pub use config::ServerConfig;
pub use server::Server;

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
