//! Cache Layer
//!
//! Provides pluggable caching abstraction for query result caching.
//!
//! # Supported Backends
//! - Redis (distributed, high-performance)
//! - In-memory (local, no external dependencies)
//! - Future: disk-based, distributed
//!
//! # Usage
//!
//! ```rust
//! // Create in-memory cache
//! let cache = MemoryCache::new();
//!
//! // Set cache entry with 1-hour TTL
//! cache.set("key", serde_json::json!({}), 3600).await?;
//!
//! // Get from cache
//! if let Some(value) = cache.get("key").await? {
//!     // Use cached value
//! }
//!
//! // Delete from cache
//! cache.delete("key").await?;
//! ```

pub mod errors;
pub mod traits;
pub mod memory;

pub use errors::CacheError;
pub use traits::{CacheBackend, CacheEntry};
pub use memory::MemoryCache;

/// Result type for cache operations
pub type CacheResult<T> = Result<T, CacheError>;
