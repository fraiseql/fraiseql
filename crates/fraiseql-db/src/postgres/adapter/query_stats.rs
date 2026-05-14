//! `query_stats` / `query_stats_by_id` / `reset_query_stats` implementations
//! for `PostgresAdapter`, backed by the `pg_stat_statements` extension (PG13+).

use fraiseql_error::{FraiseQLError, Result};
use tokio_postgres::Row;

use super::PostgresAdapter;
use crate::types::QueryStatEntry;

impl PostgresAdapter {
    /// Check whether the `pg_stat_statements` extension is available.
    ///
    /// Executes a cheap probe query against the view. If the relation doesn't
    /// exist, PostgreSQL returns SQLSTATE 42P01 (undefined table) — we treat
    /// any error as "not installed" and return gracefully.
    async fn has_pg_stat_statements(&self) -> Result<bool> {
        let client = self.acquire_connection_with_retry().await?;
        match client.query("SELECT 1 FROM pg_stat_statements LIMIT 0", &[]).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Map a `pg_stat_statements` row to a `QueryStatEntry`.
    fn map_pg_stat_row(row: &Row) -> Result<QueryStatEntry> {
        let shared_blks_hit: i64 = row.try_get("shared_blks_hit").unwrap_or(0);
        let shared_blks_read: i64 = row.try_get("shared_blks_read").unwrap_or(0);

        let cache_hit_ratio = if shared_blks_hit + shared_blks_read > 0 {
            #[allow(clippy::cast_precision_loss)]
            // Reason: block counts are always small enough that f64 is lossless
            Some(shared_blks_hit as f64 / (shared_blks_hit + shared_blks_read) as f64)
        } else {
            None
        };

        Ok(QueryStatEntry {
            query_id: row.try_get::<_, String>("query_id").unwrap_or_default(),
            query_text: row.try_get::<_, String>("query").unwrap_or_default(),
            calls: row.try_get::<_, i64>("calls").unwrap_or(0).unsigned_abs(),
            total_exec_time_ms: row.try_get("total_exec_time").unwrap_or(0.0),
            mean_exec_time_ms: row.try_get("mean_exec_time").unwrap_or(0.0),
            min_exec_time_ms: row.try_get("min_exec_time").unwrap_or(0.0),
            max_exec_time_ms: row.try_get("max_exec_time").unwrap_or(0.0),
            rows_returned: row.try_get::<_, i64>("rows").unwrap_or(0).unsigned_abs(),
            cache_hit_ratio,
            database_specific: serde_json::json!({
                "shared_blks_hit": shared_blks_hit,
                "shared_blks_read": shared_blks_read,
            }),
        })
    }

    /// Fetch query stats from `pg_stat_statements`, ordered by total execution time.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the SQL query fails (excluding
    /// extension-not-installed, which returns `Ok(vec![])`).
    pub(crate) async fn pg_query_stats(&self, limit: u32) -> Result<Vec<QueryStatEntry>> {
        if !self.has_pg_stat_statements().await? {
            return Ok(vec![]);
        }

        let client = self.acquire_connection_with_retry().await?;
        let rows = client
            .query(
                "SELECT \
                     queryid::text AS query_id, \
                     query, \
                     calls, \
                     total_exec_time, \
                     mean_exec_time, \
                     min_exec_time, \
                     max_exec_time, \
                     rows, \
                     shared_blks_hit, \
                     shared_blks_read \
                 FROM pg_stat_statements \
                 ORDER BY total_exec_time DESC \
                 LIMIT $1",
                &[&i64::from(limit)],
            )
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query pg_stat_statements: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        rows.iter().map(Self::map_pg_stat_row).collect()
    }

    /// Fetch a single query's stats by its queryid.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the SQL query fails.
    pub(crate) async fn pg_query_stats_by_id(
        &self,
        id: &str,
    ) -> Result<Option<QueryStatEntry>> {
        if !self.has_pg_stat_statements().await? {
            return Ok(None);
        }

        let client = self.acquire_connection_with_retry().await?;
        let rows = client
            .query(
                "SELECT \
                     queryid::text AS query_id, \
                     query, \
                     calls, \
                     total_exec_time, \
                     mean_exec_time, \
                     min_exec_time, \
                     max_exec_time, \
                     rows, \
                     shared_blks_hit, \
                     shared_blks_read \
                 FROM pg_stat_statements \
                 WHERE queryid::text = $1",
                &[&id],
            )
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query pg_stat_statements by id: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        rows.first().map(Self::map_pg_stat_row).transpose()
    }

    /// Reset `pg_stat_statements` statistics.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the reset call fails.
    /// Returns `Ok(())` if extension is not installed (no-op).
    pub(crate) async fn pg_reset_query_stats(&self) -> Result<()> {
        if !self.has_pg_stat_statements().await? {
            return Ok(());
        }

        let client = self.acquire_connection_with_retry().await?;
        client
            .execute("SELECT pg_stat_statements_reset()", &[])
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to reset pg_stat_statements: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;
        Ok(())
    }
}
