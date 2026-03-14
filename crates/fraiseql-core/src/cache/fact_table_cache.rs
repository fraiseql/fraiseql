//! Fact table aggregation caching methods for `CachedDatabaseAdapter`.
//!
//! Provides transparent caching for aggregation queries on fact tables
//! (`tf_*` prefix), using version-table, time-based, or schema-version
//! strategies to determine cache validity.

use sha2::{Digest, Sha256};

use super::{
    adapter::CachedDatabaseAdapter,
    fact_table_version::{FactTableVersionStrategy, generate_version_key_component},
};
use crate::{
    db::{DatabaseAdapter, types::JsonbValue},
    error::Result,
};

impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
    /// Extract fact table name from SQL query.
    ///
    /// Looks for `FROM tf_<name>` pattern in the SQL.
    pub(super) fn extract_fact_table_from_sql(sql: &str) -> Option<String> {
        // Look for FROM tf_xxx pattern (case insensitive)
        let sql_lower = sql.to_lowercase();
        let from_idx = sql_lower.find("from ")?;
        let after_from = &sql_lower[from_idx + 5..];

        // Skip whitespace
        let trimmed = after_from.trim_start();

        // Check if it starts with tf_
        if !trimmed.starts_with("tf_") {
            return None;
        }

        // Extract table name (until whitespace, comma, or end)
        let end_idx = trimmed
            .find(|c: char| c.is_whitespace() || c == ',' || c == ')')
            .unwrap_or(trimmed.len());

        Some(trimmed[..end_idx].to_string())
    }

    /// Generate cache key for aggregation query.
    ///
    /// Includes SQL, schema version, and version component based on strategy.
    pub(super) fn generate_aggregation_cache_key(
        sql: &str,
        schema_version: &str,
        version_component: Option<&str>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(sql.as_bytes());
        hasher.update(schema_version.as_bytes());
        if let Some(vc) = version_component {
            hasher.update(vc.as_bytes());
        }
        let result = hasher.finalize();
        format!("agg:{:x}", result)
    }

    /// Fetch version from tf_versions table.
    ///
    /// Returns cached version if fresh, otherwise queries database.
    pub(super) async fn fetch_table_version(&self, table_name: &str) -> Option<i64> {
        // Check cached version first
        if let Some(version) = self.version_provider.get_cached_version(table_name) {
            return Some(version);
        }

        // Query tf_versions table
        let sql = format!(
            "SELECT version FROM tf_versions WHERE table_name = '{}'",
            table_name.replace('\'', "''") // Escape single quotes
        );

        match self.adapter.execute_raw_query(&sql).await {
            Ok(rows) if !rows.is_empty() => {
                if let Some(serde_json::Value::Number(n)) = rows[0].get("version") {
                    if let Some(v) = n.as_i64() {
                        self.version_provider.set_cached_version(table_name, v);
                        return Some(v);
                    }
                }
                None
            },
            _ => None,
        }
    }

    /// Execute aggregation query with caching based on fact table versioning strategy.
    ///
    /// This method provides transparent caching for aggregation queries on fact tables.
    /// The caching behavior depends on the configured strategy for the fact table.
    ///
    /// # Arguments
    ///
    /// * `sql` - The aggregation SQL query
    ///
    /// # Returns
    ///
    /// Query results (from cache or database)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig};
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = PostgresAdapter::new("postgresql://localhost/db").await?;
    /// # let cache = QueryResultCache::new(CacheConfig::default());
    /// # let adapter = CachedDatabaseAdapter::new(db, cache, "1.0.0".to_string());
    /// // This query will be cached according to tf_sales strategy
    /// let results = adapter.execute_aggregation_query(
    ///     "SELECT SUM(revenue) FROM tf_sales WHERE year = 2024"
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_aggregation_query(
        &self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Extract fact table from SQL
        let Some(table_name) = Self::extract_fact_table_from_sql(sql) else {
            // Not a fact table query - execute without caching
            return self.adapter.execute_raw_query(sql).await;
        };

        // Get strategy for this table
        let strategy = self.fact_table_config.get_strategy(&table_name);

        // Check if caching is enabled
        if !strategy.is_caching_enabled() {
            return self.adapter.execute_raw_query(sql).await;
        }

        // Get version component based on strategy
        let table_version = if matches!(strategy, FactTableVersionStrategy::VersionTable) {
            self.fetch_table_version(&table_name).await
        } else {
            None
        };

        let version_component = generate_version_key_component(
            &table_name,
            strategy,
            table_version,
            &self.schema_version,
        );

        // If version table strategy but no version found, skip caching
        let Some(version_component) = version_component else {
            // VersionTable strategy but no version in tf_versions - skip cache
            return self.adapter.execute_raw_query(sql).await;
        };

        // Generate cache key
        let cache_key = Self::generate_aggregation_cache_key(
            sql,
            &self.schema_version,
            Some(&version_component),
        );

        // Try cache first
        if let Some(cached_result) = self.cache.get(&cache_key)? {
            // Cache hit - convert JsonbValue back to HashMap
            let results: Vec<std::collections::HashMap<String, serde_json::Value>> = cached_result
                .iter()
                .filter_map(|jv| serde_json::from_value(jv.as_value().clone()).ok())
                .collect();
            return Ok(results);
        }

        // Cache miss - execute query
        let result = self.adapter.execute_raw_query(sql).await?;

        // Store in cache (convert HashMap to JsonbValue)
        let cached_values: Vec<JsonbValue> = result
            .iter()
            .filter_map(|row| serde_json::to_value(row).ok().map(JsonbValue::new))
            .collect();

        self.cache.put(
            cache_key,
            cached_values,
            vec![table_name], // Track which fact table this query reads
            None,             // Fact-table queries use the global TTL
            None,             // No entity-type index for raw queries
        )?;

        Ok(result)
    }
}
