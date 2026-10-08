//! Cache-aware query implementations for [`CachedDatabaseAdapter`].
//!
//! Contains the inherent helper methods with the actual cache logic.
//! The `DatabaseAdapter` trait impl in `mod.rs` delegates to these.

use std::sync::Arc;

use fraiseql_db::types::ReadRouting;

use super::CachedDatabaseAdapter;
use crate::{
    backend::{
        DatabaseAdapter, ProjectionRequest, WhereClause,
        types::{JsonbValue, sql_hints::OrderByClause},
    },
    cache::key::{generate_projection_query_key, generate_view_query_key},
    error::Result,
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
#[must_use]
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
    #[tracing::instrument(skip_all, fields(cache.view = request.view))]
    pub(super) async fn execute_with_projection_impl(
        &self,
        request: &ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
    ) -> Result<Arc<Vec<JsonbValue>>> {
        let view = request.view;
        // Short-circuit when cache is disabled, or when opt-in mode is active and
        // the view has no explicit `cache_ttl_seconds` annotation.  This eliminates
        // key-generation allocations entirely for un-annotated views.
        if !self.cache.is_enabled() || (self.opt_in_mode && !self.cacheable_views.contains(view)) {
            return self
                .adapter
                .execute_with_projection_arc_with_session(request, session_vars, ReadRouting::Any)
                .await;
        }

        // Generate cache key — zero heap allocations on the hot path, bar the sort of
        // the session variables (#1373).
        let cache_key = generate_projection_query_key(
            request,
            session_vars,
            crate::runtime::scoped_request_locale().as_deref(),
            &self.schema_version,
        );

        // Hit: return cached Arc directly — zero-copy, just one atomic increment.
        if let Some(cached_arc) = self.cache.get(cache_key)? {
            return Ok(cached_arc);
        }

        // Snapshot the invalidation generation BEFORE the round trip (#1079). A mutation
        // that commits and invalidates while we await the database would otherwise evict
        // nothing — this key is not in the reverse index yet — and the put below would
        // store rows fetched before that mutation, undoing it in the cache.
        let fence = self.cache.invalidation_generation();

        // Miss: wrap result in Arc, give a clone to the cache, return the Arc.
        // The Vec contents are never copied — the cache and the caller share the
        // same allocation via Arc reference counting.
        // The read runs under its session variables (with connection affinity, #329):
        // the entry stored is the result for exactly the settings it is keyed on.
        let arc = self
            .adapter
            .execute_with_projection_arc_with_session(request, session_vars, ReadRouting::Any)
            .await?;

        // Store in cache; derive entity type from view name so that
        // selective entity-level invalidation can target precise entries.
        // The entry is registered under the view AND every secondary view a
        // query over it declares, so a mutation on a joined view evicts it (#761).
        let ttl = self.view_ttl_overrides.get(view).copied();
        let entity_type = view_name_to_entity_type(view);
        self.cache.put_arc(
            cache_key,
            Arc::clone(&arc),
            self.accessed_views_for(view),
            ttl,
            entity_type.as_deref(),
            Some(fence),
        )?;

        Ok(arc)
    }

    /// Cache-aware implementation of `execute_where_query`.
    ///
    /// Returns the result as `Arc<Vec<JsonbValue>>`.  See `execute_with_projection_impl`
    /// for the zero-copy rationale.
    ///
    /// # Cache isolation in RLS deployments
    ///
    /// Isolation is structural: the key covers everything that can change the rows — the
    /// `WHERE` clause (where tenant-scoped `inject_params` land) **and** the session
    /// variables the read runs under (where `current_setting()`-backed RLS and request
    /// context land, #1373). Two requests share an entry only when both agree.
    ///
    /// The `has_rls` flag is retained for observability and future extension (e.g., metrics
    /// on RLS-aware cache behaviour).
    #[tracing::instrument(skip_all, fields(cache.view = view))]
    pub(super) async fn execute_where_query_impl(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
        session_vars: &[(&str, &str)],
    ) -> Result<Arc<Vec<JsonbValue>>> {
        // Short-circuit when cache is disabled, or when opt-in mode is active and
        // the view has no explicit `cache_ttl_seconds` annotation.  This eliminates
        // key-generation allocations entirely for un-annotated views.
        if !self.cache.is_enabled() || (self.opt_in_mode && !self.cacheable_views.contains(view)) {
            return self
                .adapter
                .execute_where_query_arc_with_session(
                    view,
                    where_clause,
                    limit,
                    offset,
                    order_by,
                    session_vars,
                    ReadRouting::Any,
                )
                .await;
        }

        // Generate cache key (#1373: the session variables are part of it).
        let cache_key = generate_view_query_key(
            view,
            where_clause,
            limit,
            offset,
            order_by,
            session_vars,
            crate::runtime::scoped_request_locale().as_deref(),
            &self.schema_version,
        );

        // Hit: return cached Arc directly — zero-copy.
        if let Some(cached_arc) = self.cache.get(cache_key)? {
            return Ok(cached_arc);
        }

        // Snapshot before the round trip — see the sibling path above (#1079).
        let fence = self.cache.invalidation_generation();

        // Miss: wrap result in Arc, give a clone to the cache, return the Arc.
        // Run under the session variables it is keyed on (#329 affinity, #1373 key).
        let arc = self
            .adapter
            .execute_where_query_arc_with_session(
                view,
                where_clause,
                limit,
                offset,
                order_by,
                session_vars,
                ReadRouting::Any,
            )
            .await?;

        // Store in cache with entity-type index so that mutation-side
        // invalidate_by_entity() can evict only the entries that actually
        // fetched a specific entity, rather than all entries for the view.
        // Cascade invalidation via CascadeInvalidator still expands the view
        // list to transitively dependent views when invalidate_views() is called.
        // `accessed_views_for` adds the query's declared secondary views (#761).
        let ttl = self.view_ttl_overrides.get(view).copied();
        let entity_type = view_name_to_entity_type(view);
        self.cache.put_arc(
            cache_key,
            Arc::clone(&arc),
            self.accessed_views_for(view),
            ttl,
            entity_type.as_deref(),
            Some(fence),
        )?;

        Ok(arc)
    }
}
