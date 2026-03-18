//! Cache-aware query implementations for [`CachedDatabaseAdapter`].
//!
//! Contains the inherent helper methods with the actual cache logic.
//! The `DatabaseAdapter` trait impl in `mod.rs` delegates to these.

use serde_json::json;

use super::CachedDatabaseAdapter;
use crate::{
    cache::key::generate_cache_key,
    db::{DatabaseAdapter, WhereClause, types::JsonbValue},
    error::Result,
    schema::SqlProjectionHint,
};

/// Derives the GraphQL entity type name from a database view name.
///
/// Strips the view prefix (everything up to and including the first `_`),
/// then converts the remainder from snake_case to PascalCase.
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
    /// Checks the cache first; on miss, delegates to the underlying adapter
    /// and stores the result.
    #[tracing::instrument(skip_all, fields(cache.view = view))]
    pub(super) async fn execute_with_projection_impl(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Short-circuit when cache is disabled: skip SHA-256 key generation and result clone.
        if !self.cache.is_enabled() {
            return self
                .adapter
                .execute_with_projection(view, projection, where_clause, limit)
                .await;
        }

        // Generate cache key including projection info
        let query_string = format!("query {{ {view} }}");
        let projection_info = projection.map_or("", |p| &p.projection_template[..]);
        let variables = json!({
            "limit": limit,
            "projection": projection_info,
        });

        let cache_key =
            generate_cache_key(&query_string, &variables, where_clause, &self.schema_version);

        // Try cache first
        if let Some(cached_result) = self.cache.get(&cache_key)? {
            return Ok(std::sync::Arc::unwrap_or_clone(cached_result));
        }

        // Cache miss - execute via underlying adapter
        let result = self
            .adapter
            .execute_with_projection(view, projection, where_clause, limit)
            .await?;

        // Store in cache; derive entity type from view name so that
        // selective entity-level invalidation can target precise entries.
        let ttl = self.view_ttl_overrides.get(view).copied();
        let entity_type = view_name_to_entity_type(view);
        self.cache.put(
            cache_key,
            result.clone(),
            vec![view.to_string()],
            ttl,
            entity_type.as_deref(),
        )?;

        Ok(result)
    }

    /// Cache-aware implementation of `execute_where_query`.
    ///
    /// Checks the cache first; on miss, delegates to the underlying adapter
    /// and stores the result.
    #[tracing::instrument(skip_all, fields(cache.view = view))]
    pub(super) async fn execute_where_query_impl(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Short-circuit when cache is disabled: skip SHA-256 key generation and result clone.
        if !self.cache.is_enabled() {
            return self.adapter.execute_where_query(view, where_clause, limit, offset).await;
        }

        // Generate cache key
        let query_string = format!("query {{ {view} }}");
        let variables = json!({
            "limit": limit,
            "offset": offset,
        });

        let cache_key =
            generate_cache_key(&query_string, &variables, where_clause, &self.schema_version);

        // Try cache first
        if let Some(cached_result) = self.cache.get(&cache_key)? {
            return Ok(std::sync::Arc::unwrap_or_clone(cached_result));
        }

        // Cache miss - execute query
        let result = self.adapter.execute_where_query(view, where_clause, limit, offset).await?;

        // Store in cache with entity-type index so that mutation-side
        // invalidate_by_entity() can evict only the entries that actually
        // fetched a specific entity, rather than all entries for the view.
        // Cascade invalidation via CascadeInvalidator still expands the view
        // list to transitively dependent views when invalidate_views() is called.
        let ttl = self.view_ttl_overrides.get(view).copied();
        let entity_type = view_name_to_entity_type(view);
        self.cache.put(
            cache_key,
            result.clone(),
            vec![view.to_string()],
            ttl,
            entity_type.as_deref(),
        )?;

        Ok(result)
    }
}
