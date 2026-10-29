//! Invalidation methods for `CachedDatabaseAdapter`.
//!
//! This module provides view-level and entity-level cache invalidation,
//! including cascade expansion via `CascadeInvalidator`.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use fraiseql_db::{ChangedRows, ViewName};

use super::{
    CascadeInvalidator, QueryResultCache, adapter::CachedDatabaseAdapter, tviews_feed::Collection,
};
use crate::{db::DatabaseAdapter, error::Result};

impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
    /// Refuse every cache entry reading one of `views`, or a view embedding one.
    ///
    /// Call this after mutations to ensure cache consistency. Nothing is evicted, so
    /// nothing is counted: each entry is refused on its next hit (#1778).
    ///
    /// # Arguments
    ///
    /// * `views` - List of views/tables that were modified
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
    /// adapter.invalidate_views(&[ViewName::from("v_user")])?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn invalidate_views(&self, views: &[ViewName]) -> Result<()> {
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

/// Refuse every entry reading one of `views`, or a view that embeds one of them through
/// `cascade` (#1575). The one implementation for this server's writes and for another
/// server's announced evictions (#1577).
pub(super) fn invalidate_expanded(
    cache: &QueryResultCache,
    cascade: Option<&Arc<Mutex<CascadeInvalidator>>>,
    views: &[ViewName],
) -> Result<()> {
    // When caching is disabled, the CascadeInvalidator mutex is unnecessary — skip it
    // entirely to avoid serializing mutations.
    if !cache.is_enabled() {
        return Ok(());
    }
    cache.invalidate_views(&expand(cascade, views)?)
}

/// [`invalidate_expanded`] after a write that reported every row it changed (#1576): the
/// reads of one row by id are evicted by those rows, and by the types `changed` names
/// wholly, not by the sweep.
///
/// When the report also names the collections whose members changed (#1603), the levels of
/// composed reads indexed by collection are evicted by those, as the change feed evicts them
/// (#1589), not by the sweep: a comment on post A leaves the composed reads of post B's
/// comments. A type named wholly stands for every collection of it. A report that captured
/// no collections sweeps those levels too.
pub(super) fn invalidate_written(
    cache: &QueryResultCache,
    cascade: Option<&Arc<Mutex<CascadeInvalidator>>>,
    views: &[ViewName],
    changed: &ChangedRows,
) -> Result<()> {
    if !cache.is_enabled() {
        return Ok(());
    }
    let views = expand(cascade, views)?;
    // A key that is not `<child type>.<column>` names no collection: unknown, as none.
    let collections: Option<Vec<(Collection, &BTreeSet<String>)>> =
        changed.collections.as_ref().and_then(|collections| {
            collections
                .iter()
                .map(|(key, parents)| {
                    let (child_type, column) = key.split_once('.')?;
                    Some((Collection::new(child_type, column), parents))
                })
                .collect()
        });
    match collections {
        Some(_) => cache.invalidate_views_except_collections(&views)?,
        None => cache.invalidate_views_except_by_id(&views)?,
    }
    for (type_name, ids) in &changed.rows {
        for id in ids {
            cache.invalidate_by_id(type_name, id)?;
        }
    }
    for type_name in &changed.types {
        cache.invalidate_by_id_type(type_name)?;
    }
    if let Some(collections) = collections {
        for (collection, parents) in collections {
            for parent in parents {
                cache.invalidate_collection(&collection, parent)?;
            }
        }
        for type_name in &changed.types {
            cache.invalidate_collections_of(type_name)?;
        }
    }
    Ok(())
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
