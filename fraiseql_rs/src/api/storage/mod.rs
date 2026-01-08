//! Storage Layer
//!
//! Provides pluggable database abstraction allowing multiple backends.
//!
//! # Supported Backends
//! - PostgreSQL (primary)
//! - SQLite (development)
//! - MySQL (Phase 3+)
//!
//! # Usage
//!
//! ```rust
//! // Create PostgreSQL backend
//! let storage = PostgresBackend::new("postgresql://localhost/fraiseql").await?;
//!
//! // Execute query
//! let results = storage.query("SELECT * FROM users", &[]).await?;
//! ```

pub mod errors;
pub mod postgres;
pub mod traits;

pub use errors::StorageError;
pub use postgres::PostgresBackend;
pub use traits::{ExecuteResult, QueryResult, StorageBackend, Transaction};

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;
