//! Invalidation methods for `CachedDatabaseAdapter`.
//!
//! This module provides view-level and entity-level cache invalidation,
//! including cascade expansion via `CascadeInvalidator`.

use fraiseql_db::ViewName;

use super::adapter::CachedDatabaseAdapter;
use crate::{db::DatabaseAdapter, error::Result};

impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
    /// Invalidate cache entries that read from specified views.
    ///
    /// Call this after mutations to ensure cache consistency. All cache entries
    /// that accessed any of the modified views will be removed.
    ///
    /// # Arguments
    ///
    /// * `views` - List of views/tables that were modified
    ///
    /// # Returns
    ///
    /// Number of cache entries invalidated
    ///
    /// # Errors
    ///
    /// Returns error if cache mutex is poisoned (very rare).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::CachedDatabaseAdapter;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # use fraiseql_db::ViewName;
    /// # async fn example(adapter: CachedDatabaseAdapter<PostgresAdapter>) -> Result<(), Box<dyn std::error::Error>> {
    /// // After creating a user
    /// let count = adapter.invalidate_views(&[ViewName::from("v_user")])?;
    /// println!("Invalidated {} cache entries", count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn invalidate_views(&self, views: &[ViewName]) -> Result<u64> {
        // When caching is disabled, both the shard scan and the CascadeInvalidator
        // mutex are unnecessary — skip them entirely to avoid serializing mutations.
        if !self.cache.is_enabled() {
            return Ok(0);
        }

        // Expand the view list with transitive dependents when a cascade
        // invalidator is configured. `CascadeInvalidator` works in terms of raw
        // `String` view names today; promote each `ViewName` to `String` for the
        // cascade walk and convert the expanded result back at the end.
        if let Some(cascader) = &self.cascade_invalidator {
            let mut expanded: std::collections::HashSet<String> =
                views.iter().map(|v| v.as_str().to_owned()).collect();
            let mut guard = cascader.lock().map_err(|e| crate::error::FraiseQLError::Internal {
                message: format!("Cascade invalidator lock poisoned: {e}"),
                source:  None,
            })?;
            for view in views {
                let transitive = guard.cascade_invalidate(view.as_str())?;
                expanded.extend(transitive);
            }
            let expanded_views: Vec<ViewName> = expanded.into_iter().map(ViewName::from).collect();
            return self.cache.invalidate_views(&expanded_views);
        }
        self.cache.invalidate_views(views)
    }

    /// Evict only list (multi-row) cache entries for the given views.
    ///
    /// Unlike `invalidate_views()`, leaves single-entity point-lookup entries
    /// intact.  Used for CREATE mutations: creating a new entity does not affect
    /// queries that fetch a *different* existing entity by UUID.
    ///
    /// Expands the view list with transitive dependents when a
    /// `CascadeInvalidator` is configured (same logic as `invalidate_views()`).
    ///
    /// # Returns
    ///
    /// Number of cache entries evicted.
    ///
    /// # Errors
    ///
    /// Returns error if the cascade invalidator lock is poisoned.
    pub fn invalidate_list_queries(&self, views: &[ViewName]) -> Result<u64> {
        if !self.cache.is_enabled() {
            return Ok(0);
        }

        if let Some(cascader) = &self.cascade_invalidator {
            let mut expanded: std::collections::HashSet<String> =
                views.iter().map(|v| v.as_str().to_owned()).collect();
            let mut guard = cascader.lock().map_err(|e| crate::error::FraiseQLError::Internal {
                message: format!("Cascade invalidator lock poisoned: {e}"),
                source:  None,
            })?;
            for view in views {
                let transitive = guard.cascade_invalidate(view.as_str())?;
                expanded.extend(transitive);
            }
            let expanded_views: Vec<ViewName> = expanded.into_iter().map(ViewName::from).collect();
            return self.cache.invalidate_list_queries(&expanded_views);
        }
        self.cache.invalidate_list_queries(views)
    }

    /// Evict cache entries that contain the given entity UUID.
    ///
    /// Delegates to `QueryResultCache::invalidate_by_entity`. Only entries
    /// whose entity-ID index (built at `put()` time) contains the given UUID
    /// are removed; all other entries remain warm.
    ///
    /// # Returns
    ///
    /// Number of cache entries evicted.
    ///
    /// # Errors
    ///
    /// Returns error if the cache mutex is poisoned.
    pub fn invalidate_by_entity(&self, entity_type: &str, entity_id: &str) -> Result<u64> {
        self.cache.invalidate_by_entity(entity_type, entity_id)
    }
}
