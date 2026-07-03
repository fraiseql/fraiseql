//! The durable per-mailbox UID cursor store.
//!
//! Persists the [`Cursor`] the poll-IMAP adapter advances between polls into the
//! `_fraiseql_inbound_email_cursor` table (one row per configured mailbox). The
//! store is the only mutable state the pull transport keeps; losing it degrades
//! to a full re-scan (harmless — the spine dedups by `Message-ID`), never to
//! dropped messages.

use sqlx::PgPool;

use super::cursor::Cursor;

/// Map a `sqlx` error onto the canonical database error.
fn db_err(context: &str, error: &sqlx::Error) -> fraiseql_error::FraiseQLError {
    fraiseql_error::FraiseQLError::database(format!("inbound email cursor: {context}: {error}"))
}

/// PostgreSQL-backed UID cursor store over a connection pool.
pub struct PostgresEmailCursorStore {
    pool: PgPool,
}

impl PostgresEmailCursorStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create the cursor table (idempotent). Call once on startup.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database)
    /// if the DDL fails.
    pub async fn init(&self) -> fraiseql_error::Result<()> {
        sqlx::raw_sql(fraiseql_functions::migrations::inbound_email_cursor_migration_sql())
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("init", &error))?;
        Ok(())
    }

    /// Load the stored cursor for a mailbox, or `None` if it has never polled.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database)
    /// if the query fails or a stored value is out of the `u32` UID range.
    pub async fn load(&self, mailbox_key: &str) -> fraiseql_error::Result<Option<Cursor>> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT uid_validity, last_uid FROM _fraiseql_inbound_email_cursor \
             WHERE mailbox_key = $1",
        )
        .bind(mailbox_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_err("load", &error))?;

        row.map(|(uid_validity, last_uid)| {
            Ok(Cursor::new(uid_range(uid_validity)?, uid_range(last_uid)?))
        })
        .transpose()
    }

    /// Upsert the cursor for a mailbox.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database)
    /// if the write fails.
    pub async fn save(&self, mailbox_key: &str, cursor: Cursor) -> fraiseql_error::Result<()> {
        sqlx::query(
            "INSERT INTO _fraiseql_inbound_email_cursor (mailbox_key, uid_validity, last_uid) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (mailbox_key) DO UPDATE \
                 SET uid_validity = EXCLUDED.uid_validity, \
                     last_uid = EXCLUDED.last_uid, \
                     updated_at = now()",
        )
        .bind(mailbox_key)
        .bind(i64::from(cursor.uid_validity))
        .bind(i64::from(cursor.last_uid))
        .execute(&self.pool)
        .await
        .map_err(|error| db_err("save", &error))?;
        Ok(())
    }
}

/// Narrow a stored `BIGINT` UID back to `u32`, failing loud on a corrupt value.
fn uid_range(value: i64) -> fraiseql_error::Result<u32> {
    u32::try_from(value).map_err(|_| {
        fraiseql_error::FraiseQLError::database(format!(
            "inbound email cursor: stored UID {value} is out of the u32 range"
        ))
    })
}
