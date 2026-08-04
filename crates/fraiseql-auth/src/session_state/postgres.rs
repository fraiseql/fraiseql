//! PostgreSQL session-state backend (`_system.session_state`).

use chrono::{DateTime, Utc};
use sqlx::{Row, postgres::PgPool};
use uuid::Uuid;

use crate::{
    error::{AuthError, Result},
    session_state::store::SessionStateEntry,
};

/// PostgreSQL-backed [`SessionStateEntry`] store.
///
/// Owns `_system.session_state`, created by [`init`](Self::init) at server
/// startup exactly like `_system.sessions` (auth-owned tables live in this
/// crate, not in server migrations). TTL is enforced on every read
/// (`expires_at > now()`) and by the periodic
/// [`evict_expired`](Self::evict_expired) sweep.
pub struct PostgresSessionStateStore {
    db: PgPool,
}

fn db_err(what: &str, e: &sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: format!("session-state: failed to {what}: {e}"),
    }
}

impl PostgresSessionStateStore {
    /// Create a store over an existing pool. Call [`init`](Self::init) before use.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Create the `_system.session_state` table and its expiry index.
    ///
    /// Idempotent; called once at server startup. A failure here must abort the
    /// boot — a configured `postgres` backend that cannot reach its table would
    /// otherwise be a silent in-memory downgrade waiting to happen.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] when the DDL cannot be applied.
    pub async fn init(&self) -> Result<()> {
        sqlx::raw_sql(
            r"
            CREATE SCHEMA IF NOT EXISTS _system;

            CREATE TABLE IF NOT EXISTS _system.session_state (
                session_id  UUID        NOT NULL,
                thread_id   TEXT        NOT NULL,
                key         TEXT        NOT NULL,
                value       JSONB       NOT NULL,
                updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at  TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (session_id, thread_id, key)
            );

            CREATE INDEX IF NOT EXISTS idx_session_state_expiry
                ON _system.session_state (expires_at);
            ",
        )
        .execute(&self.db)
        .await
        .map_err(|e| db_err("initialize _system.session_state", &e))?;
        Ok(())
    }

    /// Fetch one live (non-expired) entry.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn get(
        &self,
        session_id: Uuid,
        thread_id: &str,
        key: &str,
    ) -> Result<Option<SessionStateEntry>> {
        let row = sqlx::query(
            "SELECT session_id, thread_id, key, value, updated_at, expires_at
             FROM _system.session_state
             WHERE session_id = $1 AND thread_id = $2 AND key = $3 AND expires_at > now()",
        )
        .bind(session_id)
        .bind(thread_id)
        .bind(key)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_err("get entry", &e))?;
        Ok(row.map(|r| entry_from_row(&r)))
    }

    /// Insert or overwrite an entry (upsert on the `(session, thread, key)` PK).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn set(&self, entry: &SessionStateEntry) -> Result<()> {
        sqlx::query(
            "INSERT INTO _system.session_state
               (session_id, thread_id, key, value, updated_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (session_id, thread_id, key) DO UPDATE
               SET value = EXCLUDED.value,
                   updated_at = EXCLUDED.updated_at,
                   expires_at = EXCLUDED.expires_at",
        )
        .bind(entry.session_id)
        .bind(&entry.thread_id)
        .bind(&entry.key)
        .bind(&entry.value)
        .bind(entry.updated_at)
        .bind(entry.expires_at)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("set entry", &e))?;
        Ok(())
    }

    /// Remove one entry (idempotent).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn delete(&self, session_id: Uuid, thread_id: &str, key: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM _system.session_state
             WHERE session_id = $1 AND thread_id = $2 AND key = $3",
        )
        .bind(session_id)
        .bind(thread_id)
        .bind(key)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("delete entry", &e))?;
        Ok(())
    }

    /// Every live entry of a thread, oldest write first (ties by key).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn list_thread(
        &self,
        session_id: Uuid,
        thread_id: &str,
    ) -> Result<Vec<SessionStateEntry>> {
        let rows = sqlx::query(
            "SELECT session_id, thread_id, key, value, updated_at, expires_at
             FROM _system.session_state
             WHERE session_id = $1 AND thread_id = $2 AND expires_at > now()
             ORDER BY updated_at, key",
        )
        .bind(session_id)
        .bind(thread_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_err("list thread", &e))?;
        Ok(rows.iter().map(entry_from_row).collect())
    }

    /// Remove every entry of a thread.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn expire_thread(&self, session_id: Uuid, thread_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM _system.session_state WHERE session_id = $1 AND thread_id = $2")
            .bind(session_id)
            .bind(thread_id)
            .execute(&self.db)
            .await
            .map_err(|e| db_err("expire thread", &e))?;
        Ok(())
    }

    /// Count a thread's live entries, excluding `exclude_key`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn thread_count(
        &self,
        session_id: Uuid,
        thread_id: &str,
        exclude_key: &str,
    ) -> Result<usize> {
        let row = sqlx::query(
            "SELECT count(*) AS n FROM _system.session_state
             WHERE session_id = $1 AND thread_id = $2 AND key <> $3 AND expires_at > now()",
        )
        .bind(session_id)
        .bind(thread_id)
        .bind(exclude_key)
        .fetch_one(&self.db)
        .await
        .map_err(|e| db_err("count thread", &e))?;
        let n: i64 = row.get("n");
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Atomically replace a whole thread with the single `summary` entry.
    ///
    /// Runs in one transaction so a reader never observes the thread half-gone.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn replace_thread(&self, summary: &SessionStateEntry) -> Result<()> {
        let mut tx = self.db.begin().await.map_err(|e| db_err("begin replace", &e))?;
        sqlx::query("DELETE FROM _system.session_state WHERE session_id = $1 AND thread_id = $2")
            .bind(summary.session_id)
            .bind(&summary.thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| db_err("clear thread for summary", &e))?;
        sqlx::query(
            "INSERT INTO _system.session_state
               (session_id, thread_id, key, value, updated_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(summary.session_id)
        .bind(&summary.thread_id)
        .bind(&summary.key)
        .bind(&summary.value)
        .bind(summary.updated_at)
        .bind(summary.expires_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_err("write summary", &e))?;
        tx.commit().await.map_err(|e| db_err("commit replace", &e))?;
        Ok(())
    }

    /// Remove every expired entry; returns how many were evicted.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] on a query failure.
    pub async fn evict_expired(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM _system.session_state WHERE expires_at <= now()")
            .execute(&self.db)
            .await
            .map_err(|e| db_err("evict expired", &e))?;
        Ok(result.rows_affected())
    }
}

fn entry_from_row(row: &sqlx::postgres::PgRow) -> SessionStateEntry {
    SessionStateEntry {
        session_id: row.get("session_id"),
        thread_id:  row.get("thread_id"),
        key:        row.get("key"),
        value:      row.get("value"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at"),
        expires_at: row.get::<DateTime<Utc>, _>("expires_at"),
    }
}
