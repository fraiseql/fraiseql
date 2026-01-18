//! Fact table versioning for aggregation query caching.
//!
//! This module provides multiple strategies for caching aggregation queries on fact tables.
//! Users can choose the strategy that best fits their data pipeline and freshness requirements.
//!
//! # Strategies
//!
//! | Strategy | Best For | Trade-off |
//! |----------|----------|-----------|
//! | `Disabled` | Real-time accuracy | No caching benefit |
//! | `VersionTable` | ETL/batch loads | Requires version bump discipline |
//! | `TimeBased` | Dashboards with acceptable lag | May serve stale data |
//! | `SchemaVersion` | Append-only/immutable facts | Only invalidates on deploy |
//!
//! # Version Table Schema
//!
//! When using `VersionTable` strategy, create the following table:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS tf_versions (
//!     table_name TEXT PRIMARY KEY,
//!     version BIGINT NOT NULL DEFAULT 1,
//!     updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
//! );
//!
//! -- Helper function to bump version
//! CREATE OR REPLACE FUNCTION bump_tf_version(p_table_name TEXT)
//! RETURNS BIGINT AS $$
//! DECLARE
//!     new_version BIGINT;
//! BEGIN
//!     INSERT INTO tf_versions (table_name, version, updated_at)
//!     VALUES (p_table_name, 1, NOW())
//!     ON CONFLICT (table_name) DO UPDATE
//!     SET version = tf_versions.version + 1,
//!         updated_at = NOW()
//!     RETURNING version INTO new_version;
//!     RETURN new_version;
//! END;
//! $$ LANGUAGE plpgsql;
//!
//! -- Optional: Auto-bump trigger (adds write overhead)
//! CREATE OR REPLACE FUNCTION tf_auto_version_bump()
//! RETURNS TRIGGER AS $$
//! BEGIN
//!     PERFORM bump_tf_version(TG_TABLE_NAME);
//!     RETURN NULL;
//! END;
//! $$ LANGUAGE plpgsql;
//!
//! -- Apply to specific fact tables:
//! CREATE TRIGGER tf_sales_version_bump
//! AFTER INSERT OR UPDATE OR DELETE ON tf_sales
//! FOR EACH STATEMENT EXECUTE FUNCTION tf_auto_version_bump();
//! ```
//!
//! # Example Configuration
//!
//! ```rust
//! use fraiseql_core::cache::fact_table_version::{
//!     FactTableVersionStrategy, FactTableCacheConfig,
//! };
//! use std::collections::HashMap;
//!
//! let mut config = FactTableCacheConfig::default();
//!
//! // ETL-loaded sales data - use version table
//! config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);
//!
//! // Real-time page views - cache for 5 minutes
//! config.set_strategy("tf_page_views", FactTableVersionStrategy::TimeBased {
//!     ttl_seconds: 300,
//! });
//!
//! // Historical exchange rates - never changes
//! config.set_strategy("tf_historical_rates", FactTableVersionStrategy::SchemaVersion);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Versioning strategy for fact table aggregation caching.
///
/// Different strategies offer different trade-offs between cache hit rate,
/// data freshness, and operational complexity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FactTableVersionStrategy {
    /// No caching for aggregations (always query database).
    ///
    /// Use when: Real-time accuracy is required.
    /// Trade-off: No caching benefit, every query hits database.
    Disabled,

    /// Read version from `tf_versions` table.
    ///
    /// Cache key includes the version number, so when version bumps,
    /// old cache entries are automatically ignored.
    ///
    /// Use when: Data is loaded via ETL/batch processes that can bump versions.
    /// Trade-off: Requires discipline to bump version after data changes.
    ///
    /// # Version Bumping
    ///
    /// After loading data, call:
    /// ```sql
    /// SELECT bump_tf_version('tf_sales');
    /// ```
    ///
    /// Or use triggers for automatic bumping (adds write overhead).
    VersionTable,

    /// Time-based TTL caching.
    ///
    /// Cache entries expire after the specified duration regardless of
    /// whether the underlying data has changed.
    ///
    /// Use when: Some staleness is acceptable (e.g., dashboards).
    /// Trade-off: May serve stale data within TTL window.
    TimeBased {
        /// Cache TTL in seconds.
        ttl_seconds: u64,
    },

    /// Use schema version only (invalidate on deployment).
    ///
    /// Cache is only invalidated when the schema version changes,
    /// which typically happens during deployments.
    ///
    /// Use when: Fact table data is immutable or append-only and
    /// queries always filter to recent data.
    /// Trade-off: Stale data until next deployment.
    SchemaVersion,
}

impl Default for FactTableVersionStrategy {
    /// Default strategy is `Disabled` for safety (no stale data).
    fn default() -> Self {
        Self::Disabled
    }
}

impl FactTableVersionStrategy {
    /// Create a time-based strategy with the given TTL.
    #[must_use]
    pub const fn time_based(ttl_seconds: u64) -> Self {
        Self::TimeBased { ttl_seconds }
    }

    /// Check if caching is enabled for this strategy.
    #[must_use]
    pub const fn is_caching_enabled(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Get TTL for time-based strategy, if applicable.
    #[must_use]
    pub const fn ttl_seconds(&self) -> Option<u64> {
        match self {
            Self::TimeBased { ttl_seconds } => Some(*ttl_seconds),
            _ => None,
        }
    }
}

/// Configuration for fact table aggregation caching.
///
/// Maps fact table names to their versioning strategies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactTableCacheConfig {
    /// Default strategy for tables not explicitly configured.
    #[serde(default)]
    pub default_strategy: FactTableVersionStrategy,

    /// Per-table strategy overrides.
    #[serde(default)]
    pub table_strategies: HashMap<String, FactTableVersionStrategy>,
}

impl FactTableCacheConfig {
    /// Create a new config with the given default strategy.
    #[must_use]
    pub fn with_default(strategy: FactTableVersionStrategy) -> Self {
        Self {
            default_strategy: strategy,
            table_strategies: HashMap::new(),
        }
    }

    /// Set strategy for a specific table.
    pub fn set_strategy(&mut self, table_name: &str, strategy: FactTableVersionStrategy) {
        self.table_strategies
            .insert(table_name.to_string(), strategy);
    }

    /// Get strategy for a table (falls back to default).
    #[must_use]
    pub fn get_strategy(&self, table_name: &str) -> &FactTableVersionStrategy {
        self.table_strategies
            .get(table_name)
            .unwrap_or(&self.default_strategy)
    }

    /// Check if caching is enabled for a table.
    #[must_use]
    pub fn is_caching_enabled(&self, table_name: &str) -> bool {
        self.get_strategy(table_name).is_caching_enabled()
    }
}

/// Cached version information for a fact table.
#[derive(Debug, Clone)]
pub struct CachedVersion {
    /// The version number.
    pub version: i64,
    /// When the version was fetched.
    pub fetched_at: Instant,
}

impl CachedVersion {
    /// Create a new cached version.
    #[must_use]
    pub fn new(version: i64) -> Self {
        Self {
            version,
            fetched_at: Instant::now(),
        }
    }

    /// Check if the cached version is still fresh.
    ///
    /// Versions are cached for a short time (default 1 second) to avoid
    /// hammering the tf_versions table on every query.
    #[must_use]
    pub fn is_fresh(&self, max_age: Duration) -> bool {
        self.fetched_at.elapsed() < max_age
    }
}

/// Version provider for fact tables.
///
/// Fetches and caches version numbers from the `tf_versions` table.
#[derive(Debug)]
pub struct FactTableVersionProvider {
    /// Cached versions (table_name -> version).
    versions: std::sync::RwLock<HashMap<String, CachedVersion>>,
    /// How long to cache version lookups.
    version_cache_ttl: Duration,
}

impl Default for FactTableVersionProvider {
    fn default() -> Self {
        Self::new(Duration::from_secs(1))
    }
}

impl FactTableVersionProvider {
    /// Create a new version provider.
    ///
    /// # Arguments
    ///
    /// * `version_cache_ttl` - How long to cache version lookups (default 1 second)
    #[must_use]
    pub fn new(version_cache_ttl: Duration) -> Self {
        Self {
            versions: std::sync::RwLock::new(HashMap::new()),
            version_cache_ttl,
        }
    }

    /// Get cached version if still fresh, otherwise return None.
    #[must_use]
    pub fn get_cached_version(&self, table_name: &str) -> Option<i64> {
        let versions = self.versions.read().ok()?;
        let cached = versions.get(table_name)?;
        if cached.is_fresh(self.version_cache_ttl) {
            Some(cached.version)
        } else {
            None
        }
    }

    /// Update cached version for a table.
    pub fn set_cached_version(&self, table_name: &str, version: i64) {
        if let Ok(mut versions) = self.versions.write() {
            versions.insert(table_name.to_string(), CachedVersion::new(version));
        }
    }

    /// Clear cached version for a table.
    pub fn clear_cached_version(&self, table_name: &str) {
        if let Ok(mut versions) = self.versions.write() {
            versions.remove(table_name);
        }
    }

    /// Clear all cached versions.
    pub fn clear_all(&self) {
        if let Ok(mut versions) = self.versions.write() {
            versions.clear();
        }
    }
}

/// Generate cache key component for a fact table based on its versioning strategy.
///
/// This function returns the version component to include in cache keys.
///
/// # Returns
///
/// - `Some(version_string)` - Version to include in cache key
/// - `None` - Caching disabled, should not cache
#[must_use]
pub fn generate_version_key_component(
    _table_name: &str,
    strategy: &FactTableVersionStrategy,
    table_version: Option<i64>,
    schema_version: &str,
) -> Option<String> {
    match strategy {
        FactTableVersionStrategy::Disabled => None,

        FactTableVersionStrategy::VersionTable => {
            // Require a version from tf_versions table
            table_version.map(|v| format!("tv:{v}"))
        }

        FactTableVersionStrategy::TimeBased { ttl_seconds } => {
            // Use time bucket as version (floor to TTL boundary)
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let bucket = now / ttl_seconds;
            Some(format!("tb:{bucket}"))
        }

        FactTableVersionStrategy::SchemaVersion => {
            // Use schema version only
            Some(format!("sv:{schema_version}"))
        }
    }
}

/// SQL to query version from tf_versions table.
pub const VERSION_TABLE_QUERY: &str = r"
    SELECT version FROM tf_versions WHERE table_name = $1
";

/// SQL to create the tf_versions table and helper functions.
pub const VERSION_TABLE_SCHEMA: &str = r"
-- Fact table version tracking for aggregation cache
CREATE TABLE IF NOT EXISTS tf_versions (
    table_name TEXT PRIMARY KEY,
    version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for fast lookups
CREATE INDEX IF NOT EXISTS idx_tf_versions_updated_at ON tf_versions (updated_at);

-- Helper function to bump version (call after data loads)
CREATE OR REPLACE FUNCTION bump_tf_version(p_table_name TEXT)
RETURNS BIGINT AS $$
DECLARE
    new_version BIGINT;
BEGIN
    INSERT INTO tf_versions (table_name, version, updated_at)
    VALUES (p_table_name, 1, NOW())
    ON CONFLICT (table_name) DO UPDATE
    SET version = tf_versions.version + 1,
        updated_at = NOW()
    RETURNING version INTO new_version;
    RETURN new_version;
END;
$$ LANGUAGE plpgsql;

-- Optional: Trigger function for automatic version bumping
-- Note: This adds overhead to every INSERT/UPDATE/DELETE
CREATE OR REPLACE FUNCTION tf_auto_version_bump()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM bump_tf_version(TG_TABLE_NAME);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Example: Apply auto-bump trigger to a fact table
-- CREATE TRIGGER tf_sales_version_bump
-- AFTER INSERT OR UPDATE OR DELETE ON tf_sales
-- FOR EACH STATEMENT EXECUTE FUNCTION tf_auto_version_bump();
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_default_is_disabled() {
        let strategy = FactTableVersionStrategy::default();
        assert_eq!(strategy, FactTableVersionStrategy::Disabled);
        assert!(!strategy.is_caching_enabled());
    }

    #[test]
    fn test_strategy_time_based() {
        let strategy = FactTableVersionStrategy::time_based(300);
        assert!(strategy.is_caching_enabled());
        assert_eq!(strategy.ttl_seconds(), Some(300));
    }

    #[test]
    fn test_strategy_version_table() {
        let strategy = FactTableVersionStrategy::VersionTable;
        assert!(strategy.is_caching_enabled());
        assert_eq!(strategy.ttl_seconds(), None);
    }

    #[test]
    fn test_strategy_schema_version() {
        let strategy = FactTableVersionStrategy::SchemaVersion;
        assert!(strategy.is_caching_enabled());
        assert_eq!(strategy.ttl_seconds(), None);
    }

    #[test]
    fn test_config_default_strategy() {
        let config = FactTableCacheConfig::default();
        assert_eq!(
            config.get_strategy("tf_sales"),
            &FactTableVersionStrategy::Disabled
        );
    }

    #[test]
    fn test_config_per_table_strategy() {
        let mut config = FactTableCacheConfig::default();
        config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);
        config.set_strategy(
            "tf_page_views",
            FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
        );

        assert_eq!(
            config.get_strategy("tf_sales"),
            &FactTableVersionStrategy::VersionTable
        );
        assert_eq!(
            config.get_strategy("tf_page_views"),
            &FactTableVersionStrategy::TimeBased { ttl_seconds: 300 }
        );
        // Unconfigured table uses default
        assert_eq!(
            config.get_strategy("tf_other"),
            &FactTableVersionStrategy::Disabled
        );
    }

    #[test]
    fn test_config_with_default() {
        let config =
            FactTableCacheConfig::with_default(FactTableVersionStrategy::SchemaVersion);
        assert_eq!(
            config.get_strategy("tf_any"),
            &FactTableVersionStrategy::SchemaVersion
        );
    }

    #[test]
    fn test_generate_version_key_disabled() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::Disabled,
            Some(42),
            "1.0.0",
        );
        assert!(key.is_none());
    }

    #[test]
    fn test_generate_version_key_version_table() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::VersionTable,
            Some(42),
            "1.0.0",
        );
        assert_eq!(key, Some("tv:42".to_string()));

        // No version available - should return None
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::VersionTable,
            None,
            "1.0.0",
        );
        assert!(key.is_none());
    }

    #[test]
    fn test_generate_version_key_time_based() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
            None,
            "1.0.0",
        );
        assert!(key.is_some());
        assert!(key.unwrap().starts_with("tb:"));
    }

    #[test]
    fn test_generate_version_key_schema_version() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::SchemaVersion,
            None,
            "1.0.0",
        );
        assert_eq!(key, Some("sv:1.0.0".to_string()));
    }

    #[test]
    fn test_version_provider_caching() {
        let provider = FactTableVersionProvider::new(Duration::from_secs(10));

        // Initially no cached version
        assert!(provider.get_cached_version("tf_sales").is_none());

        // Set version
        provider.set_cached_version("tf_sales", 42);
        assert_eq!(provider.get_cached_version("tf_sales"), Some(42));

        // Clear version
        provider.clear_cached_version("tf_sales");
        assert!(provider.get_cached_version("tf_sales").is_none());
    }

    #[test]
    fn test_version_provider_clear_all() {
        let provider = FactTableVersionProvider::new(Duration::from_secs(10));

        provider.set_cached_version("tf_sales", 1);
        provider.set_cached_version("tf_orders", 2);

        provider.clear_all();

        assert!(provider.get_cached_version("tf_sales").is_none());
        assert!(provider.get_cached_version("tf_orders").is_none());
    }

    #[test]
    fn test_cached_version_freshness() {
        let cached = CachedVersion::new(42);

        // Should be fresh immediately
        assert!(cached.is_fresh(Duration::from_secs(1)));

        // Should be fresh for a longer duration
        assert!(cached.is_fresh(Duration::from_secs(60)));
    }

    #[test]
    fn test_strategy_serialization() {
        let strategies = vec![
            FactTableVersionStrategy::Disabled,
            FactTableVersionStrategy::VersionTable,
            FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
            FactTableVersionStrategy::SchemaVersion,
        ];

        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let deserialized: FactTableVersionStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(strategy, deserialized);
        }
    }

    #[test]
    fn test_config_serialization() {
        let mut config = FactTableCacheConfig::with_default(FactTableVersionStrategy::SchemaVersion);
        config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);
        config.set_strategy(
            "tf_events",
            FactTableVersionStrategy::TimeBased { ttl_seconds: 60 },
        );

        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: FactTableCacheConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.default_strategy,
            FactTableVersionStrategy::SchemaVersion
        );
        assert_eq!(
            deserialized.get_strategy("tf_sales"),
            &FactTableVersionStrategy::VersionTable
        );
    }
}
