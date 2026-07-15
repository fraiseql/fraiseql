//! The thin PostgreSQL projection behind `fraiseql sources` (#573).
//!
//! Reads the durable `_fraiseql_source_cursor` watermark table — the same table the
//! runtime poller advances — so an operator can see, per source, where its cursor
//! stands and how stale it is. Deliberately dependency-light (deadpool +
//! tokio-postgres, mirroring the `perf` reader): all merge/format logic lives in
//! pure functions in the parent module, so correctness is covered without a live
//! database.

use anyhow::{Context, Result};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

/// One `_fraiseql_source_cursor` row: a source's durable watermark and its
/// staleness, computed against the database clock so there is no app↔DB skew.
pub struct CursorRow {
    /// The cursor key — the source name the runtime poller advances under.
    pub source_name: String,
    /// The opaque cursor value, or `None` if the row exists but holds SQL NULL.
    pub value:       Option<serde_json::Value>,
    /// The compare-and-swap generation counter.
    pub version:     i64,
    /// Last-advance timestamp, rendered as text (`updated_at::text`).
    pub updated_at:  String,
    /// Seconds since the last advance, from the database clock (the staleness/lag).
    pub age_seconds: f64,
}

/// A live PostgreSQL connection pool for reading source cursors.
pub struct SourceCursorReader {
    pool: Pool,
}

impl SourceCursorReader {
    /// Connect to `db_url` (PostgreSQL only) for cursor reads.
    ///
    /// # Errors
    ///
    /// Returns an error if `db_url` is not a `postgres://` URL or the pool cannot be
    /// created. (Connection failures surface lazily on the first query.)
    pub fn connect(db_url: &str) -> Result<Self> {
        if !db_url.starts_with("postgres") {
            anyhow::bail!(
                "source cursor reads require a PostgreSQL connection URL (postgres://…); got: \
                 {db_url}"
            );
        }
        let mut cfg = Config::new();
        cfg.url = Some(db_url.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        cfg.pool = Some(deadpool_postgres::PoolConfig::new(2));
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .context("failed to create PostgreSQL connection pool for source cursor reads")?;
        Ok(Self { pool })
    }

    /// Load every source cursor row.
    ///
    /// The age is computed in SQL against the database clock. `cursor_value` (JSONB)
    /// decodes directly to a [`serde_json::Value`] (the `with-serde_json-1` codec).
    /// Missing table ⇒ an empty result is *not* silently returned — the error
    /// surfaces so the operator learns the source subsystem was never initialized.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or the query fails.
    pub async fn load_cursors(&self) -> Result<Vec<CursorRow>> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let rows = client
            .query(
                "SELECT source_name, cursor_value, version, updated_at::text AS updated_at_text, \
                 EXTRACT(EPOCH FROM (now() - updated_at))::float8 AS age_seconds \
                 FROM _fraiseql_source_cursor",
                &[],
            )
            .await
            .context(
                "failed to read _fraiseql_source_cursor (is the source subsystem initialized?)",
            )?;
        Ok(rows
            .iter()
            .map(|row| CursorRow {
                source_name: row.get("source_name"),
                value:       row.get("cursor_value"),
                version:     row.get("version"),
                updated_at:  row.get("updated_at_text"),
                age_seconds: row.get("age_seconds"),
            })
            .collect())
    }
}
