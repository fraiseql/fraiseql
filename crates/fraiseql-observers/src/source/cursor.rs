//! The durable, opaque cursor store for scheduled ingress sources.
//!
//! A [`Source`](https://github.com/fraiseql/fraiseql/issues/573) advances an
//! opaque watermark between runs; this store persists it so a re-run resumes from
//! the last committed position (at-least-once, cursor-gated). The stored value is
//! **owned end to end by the source** — the framework treats it as opaque JSONB
//! and never interprets or assembles it into SQL text.
//!
//! ## Compare-and-swap, not last-writer-wins
//!
//! Advances are guarded by a monotonic `version` generation counter, mirroring
//! [`CheckpointStore::compare_and_swap`](crate::checkpoint::CheckpointStore). An
//! advance that read version `N` applies only while the row is still at `N`, so a
//! stale writer — one that acquired the single-firing lease, then lost it across a
//! failover while another replica moved the cursor on — can never regress the
//! watermark. Under normal single-firing there is no contention; the CAS is the
//! belt-and-suspenders guard for the lease-boundary edge case.

use serde_json::Value;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::{ObserverError, Result};

/// A point-in-time read of a source's cursor: the opaque value plus the
/// generation `version` it was read at.
///
/// The `version` is the compare-and-swap token: pass the snapshot you loaded back
/// into [`advance`](SourceCursorStore::advance) so a stale advance is rejected
/// rather than silently regressing the watermark. A never-advanced source reads as
/// [`CursorSnapshot::default`] (`value = None`, `version = 0`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CursorSnapshot {
    /// The opaque cursor value, or `None` if the source has never advanced.
    pub value:   Option<Value>,
    /// Generation counter the value was read at; `0` before the first advance.
    pub version: i64,
}

impl CursorSnapshot {
    /// The empty snapshot for a source that has never advanced.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            value:   None,
            version: 0,
        }
    }

    /// Whether the source has never advanced (no cursor row yet).
    #[must_use]
    pub const fn is_unset(&self) -> bool {
        self.version == 0
    }
}

/// Durable storage for the opaque per-source cursor.
///
/// One implementation ([`PostgresSourceCursorStore`]) backs the shipped runtime;
/// the trait is the seam a source's coordination is written against. Native
/// `async fn` in trait (no `async_trait`); used through concrete types, so the
/// missing `Send` bound on the returned future is not a constraint here.
#[allow(async_fn_in_trait)] // Reason: concrete-type use only; no dyn dispatch, no cross-await Send bound needed.
pub trait SourceCursorStore {
    /// Load the current cursor snapshot for `source`, or the empty snapshot if it
    /// has never advanced.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the query fails or a stored
    /// value cannot be parsed.
    async fn load(&self, source: &str) -> Result<CursorSnapshot>;

    /// Advance `source` to `value`, guarded by the snapshot's `version`
    /// (compare-and-swap). Returns `true` when the advance applied, `false` when a
    /// concurrent writer had already moved the cursor on (the advance is a no-op).
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the value cannot be serialized
    /// or the write fails.
    async fn advance(&self, source: &str, from: &CursorSnapshot, value: Value) -> Result<bool>;
}

/// PostgreSQL-backed cursor store over a connection pool.
///
/// Stamps `tenant_id` on every write (default: `None` = a global/system source),
/// so the deny-by-default RLS on `_fraiseql_source_cursor` applies uniformly.
#[derive(Clone)]
pub struct PostgresSourceCursorStore {
    pool:      PgPool,
    tenant_id: Option<Uuid>,
}

impl PostgresSourceCursorStore {
    /// Create a store for global (untenanted) sources over an existing pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self {
            pool,
            tenant_id: None,
        }
    }

    /// Create a store that stamps every cursor row with `tenant_id` (the RLS
    /// partition stamp — Trinity: a tenant stamp, never a business FK).
    #[must_use]
    pub const fn with_tenant(pool: PgPool, tenant_id: Uuid) -> Self {
        Self {
            pool,
            tenant_id: Some(tenant_id),
        }
    }

    /// Create the `_fraiseql_source_cursor` table and its RLS policies
    /// (idempotent). Call once on startup.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the DDL fails.
    pub async fn init(&self) -> Result<()> {
        sqlx::raw_sql(crate::migrations::source_cursor_sql())
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("init", &error))?;
        Ok(())
    }

    /// Advance the cursor **inside the caller's transaction** (Model A: the native
    /// `PullSource` path). The watermark then commits or rolls back atomically with
    /// the ingest writes in the same transaction — no reprocess window.
    ///
    /// Same compare-and-swap semantics as [`advance`](SourceCursorStore::advance).
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the value cannot be serialized
    /// or the write fails.
    pub async fn advance_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        source: &str,
        from: &CursorSnapshot,
        value: Value,
    ) -> Result<bool> {
        cas_advance(tx, source, from.version, &value, self.tenant_id).await
    }
}

impl SourceCursorStore for PostgresSourceCursorStore {
    async fn load(&self, source: &str) -> Result<CursorSnapshot> {
        // Read the value as text (`::text`) and parse it, so this needs only sqlx's
        // text codec — not the optional `json` feature — mirroring the spine.
        let row: Option<(Option<String>, i64)> =
            sqlx::query_as("SELECT cursor_value::text, version FROM _fraiseql_source_cursor WHERE source_name = $1")
                .bind(source)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| db_err("load", &error))?;

        match row {
            None => Ok(CursorSnapshot::default()),
            Some((None, version)) => Ok(CursorSnapshot {
                value: None,
                version,
            }),
            Some((Some(text), version)) => {
                let value =
                    serde_json::from_str(&text).map_err(|error| ObserverError::DatabaseError {
                        reason: format!("source cursor: parse stored value: {error}"),
                    })?;
                Ok(CursorSnapshot {
                    value: Some(value),
                    version,
                })
            },
        }
    }

    async fn advance(&self, source: &str, from: &CursorSnapshot, value: Value) -> Result<bool> {
        let mut conn = self.pool.acquire().await.map_err(|error| db_err("acquire", &error))?;
        cas_advance(&mut conn, source, from.version, &value, self.tenant_id).await
    }
}

/// Serialize the opaque value and apply the compare-and-swap on a single
/// connection (shared by the pooled and in-transaction advance paths).
///
/// `from_version == 0` means "no row expected": a first-write `INSERT … ON
/// CONFLICT DO NOTHING` that a stale writer (a row already exists) loses. Otherwise
/// an `UPDATE … WHERE version = from_version` that bumps the generation; it affects
/// zero rows — and so returns `false` — if another writer moved the cursor on.
async fn cas_advance(
    conn: &mut PgConnection,
    source: &str,
    from_version: i64,
    value: &Value,
    tenant_id: Option<Uuid>,
) -> Result<bool> {
    // Serialize to text and cast `$2::jsonb`, so binding needs only the text codec.
    let json = serde_json::to_string(value).map_err(|error| ObserverError::DatabaseError {
        reason: format!("source cursor: serialize value: {error}"),
    })?;

    let affected = if from_version == 0 {
        sqlx::query(
            "INSERT INTO _fraiseql_source_cursor (source_name, cursor_value, version, tenant_id, updated_at) \
             VALUES ($1, $2::jsonb, 1, $3, NOW()) \
             ON CONFLICT (source_name) DO NOTHING",
        )
        .bind(source)
        .bind(&json)
        .bind(tenant_id)
        .execute(&mut *conn)
        .await
        .map_err(|error| db_err("insert", &error))?
        .rows_affected()
    } else {
        sqlx::query(
            "UPDATE _fraiseql_source_cursor \
             SET cursor_value = $2::jsonb, version = version + 1, updated_at = NOW() \
             WHERE source_name = $1 AND version = $3",
        )
        .bind(source)
        .bind(&json)
        .bind(from_version)
        .execute(&mut *conn)
        .await
        .map_err(|error| db_err("update", &error))?
        .rows_affected()
    };

    Ok(affected > 0)
}

/// Map a `sqlx` error onto the canonical observer database error.
fn db_err(context: &str, error: &sqlx::Error) -> ObserverError {
    ObserverError::DatabaseError {
        reason: format!("source cursor: {context}: {error}"),
    }
}
