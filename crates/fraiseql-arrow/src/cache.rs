//! Query result caching for Arrow Flight service.
//!
//! Provides an in-memory LRU cache with TTL support for caching query results.
//! Improves throughput by 10-20% for repeated queries.
//!
//! # Entries are scoped to a principal (#716)
//!
//! Every entry lives under a *scope*: a hash of the requesting principal, from
//! the same `hash_security_context` the executor's response cache uses. The
//! Flight read paths execute their SQL against the raw database adapter with no
//! per-user row filtering, so the cache is the last place a principal boundary
//! is still observable — and a cache keyed on the SQL text alone erases it, by
//! construction, for any two principals who issue the same query.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;

/// Query cache entry with expiration time.
#[derive(Clone, Debug)]
struct CacheEntry {
    /// Cached result as JSON rows
    result:     Arc<Vec<std::collections::HashMap<String, serde_json::Value>>>,
    /// Unix timestamp when entry expires
    expires_at: u64,
}

/// Hash of the principal an entry belongs to.
///
/// Derived by [`principal_scope`]; entries under different scopes never alias.
pub type CacheScope = u64;

/// The cache scope for a request's principal.
///
/// Reuses `fraiseql-core`'s response-cache principal hash so "which principal is
/// this" has one definition across every cache in the workspace: `user_id`,
/// roles, `tenant_id`, scopes and attributes — the fields that can change which
/// rows a request is entitled to.
#[must_use]
pub fn principal_scope(context: &fraiseql_core::security::SecurityContext) -> CacheScope {
    fraiseql_core::cache::response_cache::hash_security_context(Some(context))
}

/// In-memory query result cache with TTL support.
///
/// Caches query results keyed by `(principal scope, SQL query string)`. Entries
/// expire after a configurable TTL (default 60 seconds).
///
/// Uses `DashMap` for concurrent lock-free access without blocking the
/// Flight service during cache operations.
///
/// # Example
///
/// ```no_run
/// use fraiseql_arrow::cache::QueryCache;
/// use std::collections::HashMap;
/// use std::sync::Arc;
///
/// let cache = QueryCache::new(60); // 60-second TTL
/// let scope = 0; // fraiseql_arrow::cache::principal_scope(&security_context)
///
/// // Check cache
/// if let Some(result) = cache.get(scope, "SELECT * FROM users") {
///     println!("Cache hit: {:?}", result);
/// }
///
/// // Store result
/// let result = vec![HashMap::new()];
/// cache.put(scope, "SELECT * FROM users", Arc::new(result));
/// ```
pub struct QueryCache {
    /// Principal scope → SQL query → cached result.
    ///
    /// Two levels rather than a composite key so a lookup needs no allocation:
    /// the outer key is a `u64` and the inner map is looked up by `&str`.
    entries:  DashMap<CacheScope, DashMap<String, CacheEntry>>,
    /// Time-to-live in seconds for cache entries
    ttl_secs: u64,
}

impl QueryCache {
    /// Create a new query cache with specified TTL.
    ///
    /// # Arguments
    ///
    /// * `ttl_secs` - Time-to-live in seconds for cached entries
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_arrow::cache::QueryCache;
    ///
    /// let cache = QueryCache::new(60); // Cache entries for 60 seconds
    /// ```
    #[must_use]
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: DashMap::new(),
            ttl_secs,
        }
    }

    /// Get a cached query result if it exists and hasn't expired.
    ///
    /// # Arguments
    ///
    /// * `scope` - Requesting principal, from [`principal_scope`]
    /// * `query` - SQL query string to look up
    ///
    /// # Returns
    ///
    /// `Some(result)` if cache hit and not expired, `None` if miss or expired
    #[must_use]
    pub fn get(
        &self,
        scope: CacheScope,
        query: &str,
    ) -> Option<Arc<Vec<std::collections::HashMap<String, serde_json::Value>>>> {
        let by_query = self.entries.get(&scope)?;
        let entry = by_query.get(query)?;
        let now = current_unix_timestamp();
        if now < entry.expires_at {
            return Some(Arc::clone(&entry.result));
        }
        None
    }

    /// Store a query result in the cache.
    ///
    /// # Arguments
    ///
    /// * `scope` - Requesting principal, from [`principal_scope`]
    /// * `query` - SQL query string as key
    /// * `result` - Query result rows to cache
    pub fn put(
        &self,
        scope: CacheScope,
        query: impl Into<String>,
        result: Arc<Vec<std::collections::HashMap<String, serde_json::Value>>>,
    ) {
        let expires_at = current_unix_timestamp() + self.ttl_secs;
        self.entries
            .entry(scope)
            .or_default()
            .insert(query.into(), CacheEntry { result, expires_at });
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        self.entries.clear();
    }

    /// Get current cache size (entries count, summed over every principal).
    ///
    /// Note: This includes expired entries that haven't been accessed yet.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.iter().map(|scope| scope.value().len()).sum()
    }

    /// Check if cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Invalidate cache entries for specific views by name.
    ///
    /// Removes all entries whose queries mention any of the given view names.
    /// Used for entity-based cache invalidation (e.g., "`v_user`", "`v_order`").
    ///
    /// # Arguments
    ///
    /// * `view_names` - View names to invalidate (e.g., `"v_user"`)
    ///
    /// # Returns
    ///
    /// Count of entries removed
    #[must_use]
    pub fn invalidate_views(&self, view_names: &[&str]) -> usize {
        self.remove_matching(|query| view_names.iter().any(|view| query.contains(view)))
    }

    /// Invalidate cache entries matching a glob pattern.
    ///
    /// Removes all entries whose queries match the given glob pattern.
    /// Used for pattern-based cache invalidation (e.g., "*_user", "SELECT * FROM v_*").
    ///
    /// # Arguments
    ///
    /// * `pattern` - Glob pattern to match against queries
    ///
    /// # Returns
    ///
    /// Count of entries removed
    #[must_use]
    pub fn invalidate_pattern(&self, pattern: &str) -> usize {
        // Simple wildcard matching: * matches any sequence of characters
        self.remove_matching(|query| self.matches_pattern(query, pattern))
    }

    /// Drop every entry whose query satisfies `predicate`, across all principals.
    ///
    /// Invalidation is a property of the data, not of who read it, so it always
    /// sweeps every scope.
    fn remove_matching(&self, predicate: impl Fn(&str) -> bool) -> usize {
        let mut removed = 0;
        for scope in &self.entries {
            let by_query = scope.value();
            let to_remove: Vec<String> = by_query
                .iter()
                .filter(|e| predicate(e.key()))
                .map(|e| e.key().clone())
                .collect();
            for query in to_remove {
                if by_query.remove(&query).is_some() {
                    removed += 1;
                }
            }
        }
        removed
    }

    /// Check if a query matches a pattern with * wildcards.
    fn matches_pattern(&self, query: &str, pattern: &str) -> bool {
        // Simple wildcard matching implementation
        let pattern_parts: Vec<&str> = pattern.split('*').collect();

        if pattern_parts.len() == 1 {
            // No wildcards, exact match
            return query == pattern;
        }

        let mut pos = 0;
        for (i, part) in pattern_parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if i == 0 {
                // First part must match at the beginning
                if !query.starts_with(part) {
                    return false;
                }
                pos = part.len();
            } else if i == pattern_parts.len() - 1 {
                // Last part must match at the end
                if !query.ends_with(part) {
                    return false;
                }
            } else {
                // Middle parts must be found after current position
                match query[pos..].find(part) {
                    Some(idx) => pos += idx + part.len(),
                    None => return false,
                }
            }
        }

        true
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(60) // Default 60-second TTL
    }
}

/// Get current Unix timestamp in seconds.
fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time should be after Unix epoch")
        .as_secs()
}

#[cfg(test)]
mod tests;
