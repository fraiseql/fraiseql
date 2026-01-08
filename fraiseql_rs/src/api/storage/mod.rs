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
pub mod traits;
pub mod postgres;

pub use errors::StorageError;
pub use traits::{StorageBackend, Transaction, QueryResult, ExecuteResult};
pub use postgres::PostgresBackend;

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;
