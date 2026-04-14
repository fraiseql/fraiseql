//! Cache-aware query implementations for [`CachedDatabaseAdapter`].
//!
//! Contains the inherent helper methods with the actual cache logic.
//! The `DatabaseAdapter` trait impl in `mod.rs` delegates to these.

use std::sync::Arc;

use super::CachedDatabaseAdapter;
use crate::{
    cache::key::{generate_projection_query_key, generate_view_query_key},
    db::{DatabaseAdapter, WhereClause, types::{JsonbValue, sql_hints::OrderByClause}},
    error::Result,
    schema::SqlProjectionHint,
};

/// Derives the GraphQL entity type name from a database view name.
///
/// Strips the view prefix (everything up to and including the first `_`),
/// then converts the remainder from `snake_case` to `PascalCase`.
///
/// # Examples
///
/// ```
/// use fraiseql_core::cache::view_name_to_entity_type;
/// assert_eq!(view_name_to_entity_type("v_user"),        Some("User".to_string()));
/// assert_eq!(view_name_to_entity_type("v_order_item"),  Some("OrderItem".to_string()));
/// assert_eq!(view_name_to_entity_type("tv_user_event"), Some("UserEvent".to_string()));
/// assert_eq!(view_name_to_entity_type("users"),         None);
/// assert_eq!(view_name_to_entity_type("v_"),            None);
/// ```
pub fn view_name_to_entity_type(view: &str) -> Option<String> {
    // Strip prefix: everything up to and including the first '_'.
    // Returns None if there is no '_' (not a typed view) or if the
    // remainder after the prefix is empty.
    let after_prefix = view.split_once('_')?.1;
    if after_prefix.is_empty() {
        return None;
    }
    // snake_case → PascalCase: capitalise the first letter of each segment.
    let pascal = after_prefix
        .split('_')
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<String>();
    Some(pascal)
}

impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
    /// Cache-aware implementation of `execute_with_projection`.
    ///
    /// Returns the result as `Arc<Vec<JsonbValue>>` so that the caller can borrow
    /// the data without a full `Vec` clone.  On a hit the cached `Arc` is returned
    /// directly (one atomic increment).  On a miss the result is wrapped in a fresh
    /// `Arc`, an `Arc::clone` is stored in the cache, and the original `Arc` is
    /// returned — again without cloning the `Vec` contents.
    #[tracing::instrument(skip_all, fields(cache.view = view))]
    pub(super) async fn execute_with_projection_impl(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        // Short-circuit when cache is disabled, or when opt-in mode is active and
        // the view has no explicit `cache_ttl_seconds` annotation.  This eliminates
        // key-generation allocations entirely for un-annotated views.
        if !self.cache.is_enabled()
            || (self.opt_in_mode && !self.cacheable_views.contains(view))
        {
            return self
                .adapter
                .execute_with_projection(view, projection, where_clause, limit, offset, order_by)
                .await
                .map(Arc::new);
        }

        // Generate cache key — zero heap allocations on the hot path.
        let cache_key =
            generate_projection_query_key(view, projection, where_clause, limit, offset, order_by, &self.schema_version);

        // Hit: return cached Arc directly — zero-copy, just one atomic increment.
        if let Some(cached_arc) = self.cache.get(cache_key)? {
            return Ok(cached_arc);
        }

        // Miss: wrap result in Arc, give a clone to the cache, return the Arc.
        // The Vec contents are never copied — the cache and the caller share the
        // same allocation via Arc reference counting.
        let arc = Arc::new(
            self.adapter
                .execute_with_projection(view, projection, where_clause, limit, offset, order_by)
                .await?,
        );

        // Store in cache; derive entity type from view name so that
        // selective entity-level invalidation can target precise entries.
        let ttl = self.view_ttl_overrides.get(view).copied();
        let entity_type = view_name_to_entity_type(view);
        self.cache.put_arc(
            cache_key,
            Arc::clone(&arc),
            vec![view.to_string()],
            ttl,
            entity_type.as_deref(),
        )?;

        Ok(arc)
    }

    /// Cache-aware implementation of `execute_where_query`.
    ///
    /// Returns the result as `Arc<Vec<JsonbValue>>`.  See `execute_with_projection_impl`
    /// for the zero-copy rationale.
    #[tracing::instrument(skip_all, fields(cache.view = view))]
    pub(super) async fn execute_where_query_impl(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        // Short-circuit when cache is disabled, or when opt-in mode is active and
        // the view has no explicit `cache_ttl_seconds` annotation.  This eliminates
        // key-generation allocations entirely for un-annotated views.
        if !self.cache.is_enabled() || (self.opt_in_mode && !self.cacheable_views.contains(view)) {
            return self
                .adapter
                .execute_where_query(view, where_clause, limit, offset, order_by)
                .await
                .map(Arc::new);
        }

        // Generate cache key — zero heap allocations on the hot path.
        let cache_key =
            generate_view_query_key(view, where_clause, limit, offset, order_by, &self.schema_version);

        // Hit: return cached Arc directly — zero-copy.
        if let Some(cached_arc) = self.cache.get(cache_key)? {
            return Ok(cached_arc);
        }

        // Miss: wrap result in Arc, give a clone to the cache, return the Arc.
        let arc = Arc::new(
            self.adapter
                .execute_where_query(view, where_clause, limit, offset, order_by)
                .await?,
        );

        // Store in cache with entity-type index so that mutation-side
        // invalidate_by_entity() can evict only the entries that actually
        // fetched a specific entity, rather than all entries for the view.
        // Cascade invalidation via CascadeInvalidator still expands the view
        // list to transitively dependent views when invalidate_views() is called.
        let ttl = self.view_ttl_overrides.get(view).copied();
        let entity_type = view_name_to_entity_type(view);
        self.cache.put_arc(
            cache_key,
            Arc::clone(&arc),
            vec![view.to_string()],
            ttl,
            entity_type.as_deref(),
        )?;

        Ok(arc)
    }
}
