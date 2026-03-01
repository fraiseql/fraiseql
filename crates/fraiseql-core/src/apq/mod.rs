//! Automatic Persisted Queries (APQ) infrastructure.
//!
//! APQ is a GraphQL optimization technique that allows clients to:
//! 1. Hash queries and send only the hash on subsequent requests
//! 2. Server responds with original query if not cached
//! 3. Reduces bandwidth for frequently-used queries
//!
//! # Security Considerations
//!
//! Cache keys MUST include variables to prevent data leakage between requests
//! with different variable values.
//!
//! # Module Contents
//!
//! - **hasher**: Query hashing with SHA-256 (pure Rust implementation)
//! - **storage**: APQ result storage and retrieval
//! - **metrics**: APQ performance metrics and monitoring

pub mod hasher; // Pure Rust query hasher
pub mod memory_storage;
pub mod metrics;
#[cfg(feature = "redis-apq")]
pub mod redis_storage;
pub mod storage;

// Re-export key types for convenience
pub use hasher::{hash_query, hash_query_with_variables, verify_hash, verify_hash_with_variables};
pub use memory_storage::InMemoryApqStorage;
pub use metrics::ApqMetrics;
#[cfg(feature = "redis-apq")]
pub use redis_storage::RedisApqStorage;
pub use storage::{ApqError, ApqStats, ApqStorage};
