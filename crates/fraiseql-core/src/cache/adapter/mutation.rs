//! Fact-table version bump implementation for [`CachedDatabaseAdapter`].
//!
//! Contains the `bump_fact_table_versions_impl` inherent helper method.
//! The `DatabaseAdapter` trait impl in `mod.rs` delegates to it.

use super::CachedDatabaseAdapter;
use crate::{
    cache::fact_table_version::FactTableVersionStrategy, db::DatabaseAdapter, error::Result,
};

impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
    /// Bump version counters for fact tables, enabling cache invalidation by version key.
    ///
    /// Only acts on tables using the [`FactTableVersionStrategy::VersionTable`] strategy.
    /// `TimeBased` and `SchemaVersion` strategies are invalidated by their own mechanisms.
    ///
    /// # Errors
    ///
    /// Propagates errors from calling the `bump_tf_version` database function via
    /// the underlying [`DatabaseAdapter::execute_function_call`].
    pub(super) async fn bump_fact_table_versions_impl(&self, tables: &[String]) -> Result<()> {
        for table in tables {
            // Only act when this table uses the version-table strategy.
            // TimeBased and SchemaVersion strategies are invalidated by their own
            // mechanisms (clock / schema hash); no runtime bump is needed.
            if !matches!(
                self.fact_table_config.get_strategy(table),
                FactTableVersionStrategy::VersionTable
            ) {
                continue;
            }

            // Call the PostgreSQL function that increments the counter and returns
            // the new version.  The table name originates from
            // `MutationDefinition.invalidates_fact_tables`, which the CLI compiler
            // validates as a safe SQL identifier — no string interpolation needed.
            let rows = self
                .adapter
                .execute_function_call("bump_tf_version", &[serde_json::json!(table)])
                .await?;

            // Extract the new version number from the function result.
            // The function must return a single-column row with the incremented
            // integer.  Accept whatever column name the function uses.
            if let Some(new_version) =
                rows.first().and_then(|row| row.values().find_map(serde_json::Value::as_i64))
            {
                self.version_provider.set_cached_version(table, new_version);
            }
        }
        Ok(())
    }
}
