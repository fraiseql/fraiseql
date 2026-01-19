//! Query result caching for FraiseQL v2.
//!
//! # Overview
//!
//! This module provides transparent LRU-based query result caching with view-based
//! invalidation. Cache entries are automatically invalidated when mutations modify
//! the underlying data.
//!
//! # Phase 2 Scope
//!
//! - **LRU-based result caching** with TTL expiry
//! - **View-based invalidation** (not entity-level)
//! - **Security-aware cache key generation** (prevents data leakage)
//! - **Integration with `DatabaseAdapter`** via wrapper
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────┐
//! │ GraphQL Query       │
//! │ + Variables         │
//! │ + WHERE Clause      │
//! └──────────┬──────────┘
//!            │
//!            ↓ generate_cache_key()
//! ┌─────────────────────┐
//! │ SHA-256 Cache Key   │ ← Includes variables for security
//! └──────────┬──────────┘
//!            │
//!            ↓ QueryResultCache::get()
//! ┌─────────────────────┐
//! │ Cache Hit?          │
//! │ - Check TTL         │
//! │ - Check LRU         │
//! └──────────┬──────────┘
//!            │
//!      ┌─────┴─────┐
//!      │           │
//!     HIT         MISS
//!      │           │
//!      ↓           ↓ execute_query()
//! Return      Database Query
//! Cached      + Store Result
//! Result      + Track Views
//!
//! Mutation:
//! ┌─────────────────────┐
//! │ Mutation executed   │
//! │ "createUser"        │
//! └──────────┬──────────┘
//!            │
//!            ↓ InvalidationContext::for_mutation()
//! ┌─────────────────────┐
//! │ Modified Views:     │
//! │ - v_user            │
//! └──────────┬──────────┘
//!            │
//!            ↓ cache.invalidate_views()
//! ┌─────────────────────┐
//! │ Remove all caches   │
//! │ reading from v_user │
//! └─────────────────────┘
//! ```
//!
//! # Configuration
//!
//! ```rust
//! use fraiseql_core::cache::CacheConfig;
//!
//! // Production configuration
//! let config = CacheConfig {
//!     enabled: true,
//!     max_entries: 50_000,
//!     ttl_seconds: 86_400,  // 24 hours
//!     cache_list_queries: true,
//! };
//!
//! // Development (disable for deterministic tests)
//! let config = CacheConfig::disabled();
//! ```
//!
//! # Usage Example
//!
//! ```ignore
//! use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig, InvalidationContext};
//! use fraiseql_core::db::postgres::PostgresAdapter;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create database adapter
//! let db_adapter = PostgresAdapter::new("postgresql://localhost/db").await?;
//!
//! // Wrap with caching
//! let cache = QueryResultCache::new(CacheConfig::default());
//! let adapter = CachedDatabaseAdapter::new(
//!     db_adapter,
//!     cache,
//!     "1.0.0".to_string()  // schema version
//! );
//!
//! // Use as normal DatabaseAdapter - caching is transparent
//! let users = adapter
//!     .execute_where_query("v_user", None, Some(10), None)
//!     .await?;
//!
//! println!("Found {} users", users.len());
//!
//! // After mutation, invalidate
//! let invalidation = InvalidationContext::for_mutation(
//!     "createUser",
//!     vec!["v_user".to_string()]
//! );
//! adapter.invalidate_views(&invalidation.modified_views)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Performance
//!
//! - **Cache hit latency**: ~0.1ms (P99 < 1ms)
//! - **Expected hit rate**: 60-80% for typical workloads
//! - **Memory usage**: ~100 MB for default config (10,000 entries @ 10 KB avg)
//! - **Speedup**: 50-200x faster than database queries
//!
//! # Security
//!
//! Cache keys include variable values to prevent data leakage between users.
//! Different users with different query variables get different cache entries.
//!
//! **Example**:
//! ```text
//! User A: query { user(id: 1) } → Cache key: abc123...
//! User B: query { user(id: 2) } → Cache key: def456... (DIFFERENT)
//! ```
//!
//! This prevents User B from accidentally seeing User A's cached data.
//!
//! # View-Based Invalidation
//!
//! In Phase 2, invalidation operates at the **view/table level**:
//!
//! - **Mutation modifies `v_user`** → Invalidate ALL caches reading from `v_user`
//! - **Expected hit rate**: 60-70% (some over-invalidation)
//!
//! **Example**:
//! ```text
//! Cache Entry 1: query { user(id: 1) }     → reads v_user
//! Cache Entry 2: query { user(id: 2) }     → reads v_user
//! Cache Entry 3: query { post(id: 100) }   → reads v_post
//!
//! Mutation: updateUser(id: 1)
//! → Invalidates Entry 1 AND Entry 2 (even though Entry 2 not affected)
//! → Entry 3 remains cached
//! ```
//!
//! # Future Enhancements (Phase 7+)
//!
//! - **Entity-level tracking**: Track by `User:123`, not just `v_user`
//! - **Cascade integration**: Parse mutation metadata for precise invalidation
//! - **Selective invalidation**: Only invalidate affected entity IDs
//! - **Expected hit rate**: 90-95% with entity-level tracking
//!
//! # Module Organization
//!
//! - **`adapter`**: `CachedDatabaseAdapter` wrapper for transparent caching
//! - **`config`**: Cache configuration with memory-safe bounds
//! - **`key`**: Security-critical cache key generation (includes APQ integration)
//! - **`result`**: LRU cache storage with TTL and metrics
//! - **`dependency_tracker`**: Bidirectional view↔cache mapping
//! - **`invalidation`**: Public invalidation API with structured contexts

mod adapter;
mod config;
mod dependency_tracker;
mod invalidation;
mod key;
mod result;

// Phase 3+: Cascading invalidation with transitive dependencies
pub mod cascade_invalidator;

// Phase 7: Entity-level caching modules
pub mod cascade_metadata;
pub mod cascade_response_parser;
pub mod entity_key;
pub mod query_analyzer;
pub mod uuid_extractor;

// Fact table aggregation caching
pub mod fact_table_version;

// Public exports
pub use adapter::CachedDatabaseAdapter;
pub use cascade_invalidator::{CascadeInvalidator, InvalidationStats};
pub use cascade_metadata::CascadeMetadata;
pub use cascade_response_parser::CascadeResponseParser;
pub use config::CacheConfig;
// Export dependency tracker (used in doctests and advanced use cases)
pub use dependency_tracker::DependencyTracker;
pub use entity_key::EntityKey;
pub use fact_table_version::{
    FactTableCacheConfig, FactTableVersionProvider, FactTableVersionStrategy, VERSION_TABLE_SCHEMA,
};
pub use invalidation::{InvalidationContext, InvalidationReason};
pub use key::{extract_accessed_views, generate_cache_key};
pub use query_analyzer::{QueryAnalyzer, QueryCardinality, QueryEntityProfile};
pub use result::{CacheMetrics, CachedResult, QueryResultCache};
pub use uuid_extractor::UUIDExtractor;
