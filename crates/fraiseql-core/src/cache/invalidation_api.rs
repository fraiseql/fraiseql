//! Invalidation methods for `CachedDatabaseAdapter`.
//!
//! This module provides view-level and entity-level cache invalidation,
//! including cascade expansion via `CascadeInvalidator`.

use std::sync::{Arc, Mutex};

use fraiseql_db::{ChangedRows, ViewName};

use super::{CascadeInvalidator, QueryResultCache, adapter::CachedDatabaseAdapter};
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
        invalidate_expanded(&self.cache, self.cascade_invalidator.as_ref(), views)
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

/// Evict every entry reading one of `views`, or a view that embeds one of them through
/// `cascade` (#1575). The one implementation for this server's writes and for another
/// server's announced evictions (#1577).
pub(super) fn invalidate_expanded(
    cache: &QueryResultCache,
    cascade: Option<&Arc<Mutex<CascadeInvalidator>>>,
    views: &[ViewName],
) -> Result<u64> {
    // When caching is disabled, both the shard scan and the CascadeInvalidator
    // mutex are unnecessary — skip them entirely to avoid serializing mutations.
    if !cache.is_enabled() {
        return Ok(0);
    }
    cache.invalidate_views(&expand(cascade, views)?)
}

/// [`invalidate_expanded`] after a write that reported every row it changed (#1576): the
/// reads of one row by id are evicted by those rows, and by the types `changed` names
/// wholly, not by the sweep.
pub(super) fn invalidate_written(
    cache: &QueryResultCache,
    cascade: Option<&Arc<Mutex<CascadeInvalidator>>>,
    views: &[ViewName],
    changed: &ChangedRows,
) -> Result<u64> {
    if !cache.is_enabled() {
        return Ok(0);
    }
    let mut evicted = cache.invalidate_views_except_by_id(&expand(cascade, views)?)?;
    for (type_name, ids) in &changed.rows {
        for id in ids {
            evicted += cache.invalidate_by_id(type_name, id)?;
        }
    }
    for type_name in &changed.types {
        evicted += cache.invalidate_by_id_type(type_name)?;
    }
    Ok(evicted)
}

/// `views` and every view that embeds one of them through `cascade` (#1575).
fn expand(
    cascade: Option<&Arc<Mutex<CascadeInvalidator>>>,
    views: &[ViewName],
) -> Result<Vec<ViewName>> {
    // `CascadeInvalidator` works in terms of raw `String` view names today; promote each
    // `ViewName` to `String` for the cascade walk and convert the expanded result back.
    let Some(cascader) = cascade else {
        return Ok(views.to_vec());
    };
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
    Ok(expanded.into_iter().map(ViewName::from).collect())
}
