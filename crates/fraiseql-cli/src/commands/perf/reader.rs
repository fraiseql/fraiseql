//! Thin PostgreSQL reader over the change-log contract view
//! `core.v_entity_change_log`.
//!
//! Deliberately minimal: it pulls a trailing window of rows into plain
//! [`ChangeLogSample`] values and hands them to the pure analysis functions. All
//! columns are cast to dependency-light scalar types in SQL (`text`, `int4`,
//! `int8`, `float8`) so the reader needs no `chrono`/`time`/`uuid` decode
//! features — `created_at` arrives as an epoch `f64` and `object_id` as text.
//!
//! PostgreSQL-only, mirroring the `--against-db` checks
//! ([`crate::schema::pg_catalog`]): the change-log contract view is a PostgreSQL
//! object and the marker lives in a `jsonb` column.

use anyhow::{Context, Result};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

/// The framework-owned read-path view shipped by the change-log contract
/// migration (`08_create_entity_change_log_contract.sql`). Trusted identifier —
/// never interpolated from user input.
pub const CONTRACT_VIEW: &str = "core.v_entity_change_log";

/// One change-log row, projected to the columns `perf` analysis needs.
///
/// `duration_ms` is `NULL` for cooperative external producers (no FraiseQL
/// request clock); `duration_calc_version` is the data-quality marker
/// (`extra_metadata->>'duration_calc_version'`) used to refuse mixing pre-fix
/// rows with the wall-clock-correct ones — see
/// [`fraiseql_db::changelog::DURATION_CALC_VERSION`].
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeLogSample {
    /// Entity type the mutation touched (`object_type`).
    pub object_type:           String,
    /// `INSERT` / `UPDATE` / `DELETE` / `CUSTOM`.
    pub modification_type:     String,
    /// Wall-clock duration in milliseconds; `None` for non-executor producers.
    pub duration_ms:           Option<i32>,
    /// `extra_metadata->>'duration_calc_version'`; `None` when unmarked (legacy).
    pub duration_calc_version: Option<i64>,
    /// `EXTRACT(EPOCH FROM created_at)` — seconds since the Unix epoch.
    pub created_at_epoch:      f64,
    /// The changed entity's id (`object_id::text`); `None` when absent.
    pub object_id:             Option<String>,
    /// W3C trace id, when populated.
    pub trace_id:              Option<String>,
}

/// A live PostgreSQL connection pool for reading the change-log contract view.
pub struct PerfReader {
    pool: Pool,
}

impl PerfReader {
    /// Connect to `db_url` (PostgreSQL only) for change-log reads.
    ///
    /// # Errors
    ///
    /// Returns an error if `db_url` is not a `postgres://` URL or the pool cannot
    /// be created. (Connection failures surface lazily on first query.)
    pub fn connect(db_url: &str) -> Result<Self> {
        if !db_url.starts_with("postgres") {
            anyhow::bail!(
                "perf reads require a PostgreSQL connection URL (postgres://…); got: {db_url}"
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
            .context("failed to create PostgreSQL connection pool for perf reads")?;
        Ok(Self { pool })
    }

    /// The database clock as a Unix epoch in seconds — the reference the analysis
    /// windows back from, so there is no app↔DB skew at the window boundary.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or the query fails.
    pub async fn db_now_epoch(&self) -> Result<f64> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let row = client
            .query_one("SELECT EXTRACT(EPOCH FROM now())::float8 AS now_epoch", &[])
            .await
            .context("failed to read database clock")?;
        Ok(row.get("now_epoch"))
    }

    /// Load every change-log sample within the trailing `window_days` window,
    /// optionally restricted to a single `object_type`.
    ///
    /// The window is applied in SQL (`created_at >= now() - make_interval(days =>
    /// $1)`) so only the rows the analysis needs cross the wire.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or the query fails.
    pub async fn load_samples(
        &self,
        window_days: i32,
        object_type: Option<&str>,
    ) -> Result<Vec<ChangeLogSample>> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;

        // All columns cast to dependency-light scalars; the marker is pulled out
        // of the view's `data` JSONB (it is not a top-level view column).
        let base = format!(
            "SELECT object_type, modification_type, duration_ms, \
             EXTRACT(EPOCH FROM created_at)::float8 AS created_at_epoch, \
             (data->'extra_metadata'->>'duration_calc_version')::bigint AS duration_calc_version, \
             object_id::text AS object_id_text, trace_id \
             FROM {CONTRACT_VIEW} \
             WHERE created_at >= now() - make_interval(days => $1)"
        );

        let rows = if let Some(object_type) = object_type {
            client
                .query(&format!("{base} AND object_type = $2"), &[&window_days, &object_type])
                .await
        } else {
            client.query(&base, &[&window_days]).await
        }
        .with_context(|| format!("failed to read {CONTRACT_VIEW}"))?;

        Ok(rows.iter().map(row_to_sample).collect())
    }
}

/// Decode one `core.v_entity_change_log` row into a [`ChangeLogSample`].
fn row_to_sample(row: &tokio_postgres::Row) -> ChangeLogSample {
    ChangeLogSample {
        object_type:           row.get("object_type"),
        modification_type:     row.get("modification_type"),
        duration_ms:           row.get("duration_ms"),
        duration_calc_version: row.get("duration_calc_version"),
        created_at_epoch:      row.get("created_at_epoch"),
        object_id:             row.get("object_id_text"),
        trace_id:              row.get("trace_id"),
    }
}
