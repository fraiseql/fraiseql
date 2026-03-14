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

impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
    /// Cache-aware implementation of `execute_with_projection`.
    ///
    /// Checks the cache first; on miss, delegates to the underlying adapter
    /// and stores the result.
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
        let projection_info = projection.map(|p| &p.projection_template[..]).unwrap_or("");
        let variables = json!({
            "limit": limit,
            "projection": projection_info,
        });

        let cache_key =
            generate_cache_key(&query_string, &variables, where_clause, &self.schema_version);

        // Try cache first
        if let Some(cached_result) = self.cache.get(&cache_key)? {
            return Ok((*cached_result).clone());
        }

        // Cache miss - execute via underlying adapter
        let result = self
            .adapter
            .execute_with_projection(view, projection, where_clause, limit)
            .await?;

        // Store in cache
        let ttl = self.view_ttl_overrides.get(view).copied();
        self.cache.put(cache_key, result.clone(), vec![view.to_string()], ttl, None)?;

        Ok(result)
    }

    /// Cache-aware implementation of `execute_where_query`.
    ///
    /// Checks the cache first; on miss, delegates to the underlying adapter
    /// and stores the result.
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
            // Cache hit - return cached result
            return Ok((*cached_result).clone());
        }

        // Cache miss - execute query
        let result = self.adapter.execute_where_query(view, where_clause, limit, offset).await?;

        // Store in cache
        // View-level tracking (single view name).
        // Cascade invalidation via CascadeInvalidator expands this to transitively
        // dependent views when invalidate_views() is called.
        let ttl = self.view_ttl_overrides.get(view).copied();
        self.cache.put(
            cache_key,
            result.clone(),
            vec![view.to_string()], // accessed views
            ttl,
            None, // No entity-type index for WHERE queries
        )?;

        Ok(result)
    }
}
