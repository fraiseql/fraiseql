//! Query result caching for FraiseQL v2.
//!
//! # Overview
//!
//! This module provides transparent W-TinyLFU query result caching with view-based
//! and entity-based invalidation. Cache entries are automatically invalidated when
//! mutations modify the underlying data.
//!
//! # Scope
//!
//! - **W-TinyLFU result caching** with per-entry TTL (via moka)
//! - **Lock-free reads** — cache hits do not acquire any shared lock
//! - **View-based invalidation** and **entity-based invalidation** via O(k) reverse indexes
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
//! │ ahash Cache Key     │ ← Includes variables for security
//! └──────────┬──────────┘
//!            │
//!            ↓ QueryResultCache::get()
//! ┌─────────────────────┐
//! │ Cache Hit?          │
//! │ - Check TTL (moka)  │
//! │ - W-TinyLFU policy  │
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
//!     ..Default::default()
//! };
//!
//! // Development (disable for deterministic tests)
//! let config = CacheConfig::disabled();
//! ```
//!
//! # Usage Example
//!
//! ```no_run
//! use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig, InvalidationContext};
//! use fraiseql_core::db::{postgres::PostgresAdapter, DatabaseAdapter};
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
//!     .execute_where_query("v_user", None, Some(10), None, None)
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
//! - **Cache hit latency**: ~0.05ms (P99 < 0.5ms) — lock-free read path
//! - **Expected hit rate**: 60-80% for typical workloads (higher than LRU under skewed access)
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
//! Invalidation operates at the **view/table level**:
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
//! # Cache Security Requirements
//!
//! The cache is safe in single-tenant deployments with no additional configuration.
//! In **multi-tenant deployments**, two requirements must be met to prevent data
//! leakage between tenants:
//!
//! 1. **Row-Level Security (RLS) must be active.** The cache key includes the per-request WHERE
//!    clause injected by FraiseQL's RLS policy engine. Different users with different RLS
//!    predicates receive different cache entries. If RLS is disabled or returns an empty clause,
//!    all users share the same key for identical queries and variables — Tenant A's data appears in
//!    Tenant B's responses.
//!
//! 2. **Schema content hash must be used as the schema version.** Use
//!    `CompiledSchema::content_hash()` (not `env!("CARGO_PKG_VERSION")`) when constructing
//!    `CachedDatabaseAdapter`. This ensures that any schema change automatically invalidates all
//!    cached entries, preventing stale-schema hits after deployment.
//!
//! The server emits a startup `warn!` when caching is enabled but no RLS policies
//! are declared in the compiled schema. This warning is informational in
//! single-tenant deployments and a critical security indicator in multi-tenant ones.
//!
//! # Future Enhancements
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
//! - **`result`**: W-TinyLFU cache storage (moka) with per-entry TTL, reverse indexes, and metrics
//! - **`dependency_tracker`**: Bidirectional view↔cache mapping
//! - **`invalidation`**: Public invalidation API with structured contexts

mod adapter;
mod config;
mod dependency_tracker;
mod fact_table_cache;
mod invalidation;
mod invalidation_api;
mod key;
mod relay_cache;
mod result;

// Cascading invalidation with transitive dependencies
pub mod cascade_invalidator;

// Entity-level caching modules
pub mod cascade_metadata;
pub mod cascade_response_parser;
pub mod entity_key;
pub mod query_analyzer;
pub mod uuid_extractor;

// Fact table aggregation caching
pub mod fact_table_version;

// Public exports
pub use adapter::{CachedDatabaseAdapter, view_name_to_entity_type};
pub use cascade_invalidator::{CascadeInvalidator, InvalidationStats};
pub use cascade_metadata::CascadeMetadata;
pub use cascade_response_parser::CascadeResponseParser;
pub use config::{CacheConfig, RlsEnforcement};
// Export dependency tracker (used in doctests and advanced use cases)
pub use dependency_tracker::DependencyTracker;
pub use entity_key::EntityKey;
pub use fact_table_version::{
    FactTableCacheConfig, FactTableVersionProvider, FactTableVersionStrategy, VERSION_TABLE_SCHEMA,
};
pub use invalidation::{InvalidationContext, InvalidationReason};
pub use key::{
    extract_accessed_views, generate_cache_key, generate_projection_query_key,
    generate_view_query_key,
};
pub use query_analyzer::{QueryAnalyzer, QueryCardinality, QueryEntityProfile};
pub use result::{CacheMetrics, CachedResult, QueryResultCache};
pub use uuid_extractor::UUIDExtractor;
