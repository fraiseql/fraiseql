//! The durable inbound-message spine.
//!
//! [`PostgresInboundSpine`] persists normalized [`InboundMessage`]s into the
//! `_fraiseql_inbound_message` table, deduplicated by `(source, idempotency_key)`.
//! This is the inbound mirror of the outbound `tb_entity_change_log` outbox: a
//! message is written durably *before* `after:ingest` dispatch, so dispatch is
//! **at-least-once** — if the process dies after the commit but before dispatch,
//! the message survives for replay, and a redelivery of an already-committed
//! message is discarded by the unique-key claim rather than re-emitted.
//!
//! The claim ([`emit_in_tx`]) is an `INSERT … ON CONFLICT DO NOTHING RETURNING id`
//! and is designed to run *inside the receiver's transaction* (the
//! `fraiseql-webhooks` pipeline hands its handler a `&mut Transaction`), so the
//! spine write and the receiver's idempotency claim commit or roll back together.

use fraiseql_functions::InboundMessage;
use sqlx::{PgPool, Postgres, Transaction};

/// Outcome of persisting a message onto the spine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emitted {
    /// The message was newly recorded; the caller should dispatch `after:ingest`.
    /// Carries the durable message id.
    New(uuid::Uuid),
    /// An earlier committed emit already owns this `(source, idempotency_key)`; the
    /// redelivery was discarded and no dispatch should fire.
    Duplicate,
}

impl Emitted {
    /// Whether this emit newly recorded the message (i.e. dispatch should fire).
    #[must_use]
    pub const fn is_new(&self) -> bool {
        matches!(self, Emitted::New(_))
    }
}

/// Map a `sqlx` error onto the canonical database error.
fn db_err(context: &str, error: &sqlx::Error) -> fraiseql_error::FraiseQLError {
    fraiseql_error::FraiseQLError::database(format!("inbound spine: {context}: {error}"))
}

/// Persist a normalized message onto the spine within the caller's transaction,
/// deduplicated by `(source, idempotency_key)`.
///
/// Returns [`Emitted::New`] with the durable id when the message was newly
/// recorded, or [`Emitted::Duplicate`] when an earlier committed emit already
/// owns the key. Running inside the caller's transaction keeps the spine write
/// atomic with the receiver's own idempotency claim.
///
/// # Errors
///
/// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database) if
/// the message cannot be serialized or the insert fails.
pub async fn emit_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    message: &InboundMessage,
) -> fraiseql_error::Result<Emitted> {
    // The whole normalized message is the durable payload; `InboundMessage` is a
    // plain serde struct of standard types, so this serialization is infallible in
    // practice, but we surface any failure loudly rather than storing a partial row.
    let payload = serde_json::to_string(message).map_err(|error| {
        fraiseql_error::FraiseQLError::database(format!(
            "inbound spine: serialize message: {error}"
        ))
    })?;

    // ON CONFLICT DO NOTHING returns a row only on a fresh insert; a redelivery of an
    // already-committed (source, idempotency_key) yields zero rows. Concurrent
    // duplicate deliveries serialise on the unique-key row lock, so exactly one wins.
    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO _fraiseql_inbound_message \
             (source, idempotency_key, thread_key, payload, received_at) \
         VALUES ($1, $2, $3, $4::jsonb, $5) \
         ON CONFLICT (source, idempotency_key) DO NOTHING \
         RETURNING id",
    )
    .bind(message.source.as_key())
    .bind(&message.idempotency_key)
    .bind(message.thread_key.as_deref())
    .bind(payload)
    .bind(message.received_at)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| db_err("claim", &error))?;

    Ok(id.map_or(Emitted::Duplicate, Emitted::New))
}

/// PostgreSQL-backed durable inbound spine over a connection pool.
///
/// See the module documentation for the at-least-once semantics and the schema.
pub struct PostgresInboundSpine {
    pool: PgPool,
}

impl PostgresInboundSpine {
    /// Create a spine over an existing pool.
    ///
    /// The pool's role must own (or `BYPASSRLS`) the `_fraiseql_inbound_message`
    /// table; calling [`init`](Self::init) once on startup creates it, so the
    /// connecting role owns it by construction.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create the `_fraiseql_inbound_message` spine table (idempotent).
    ///
    /// Call once on startup. Safe to re-run — the DDL uses `CREATE … IF NOT
    /// EXISTS` (see
    /// [`inbound_migration_sql`](fraiseql_functions::migrations::inbound_migration_sql)).
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database)
    /// if the DDL fails.
    pub async fn init(&self) -> fraiseql_error::Result<()> {
        sqlx::raw_sql(fraiseql_functions::migrations::inbound_migration_sql())
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("init", &error))?;
        Ok(())
    }

    /// Persist a message onto the spine in its own transaction.
    ///
    /// A convenience for callers that are not already inside a receiver
    /// transaction (e.g. a pull adapter). Push adapters that run inside the
    /// receiver's transaction should call [`emit_in_tx`] instead so the write is
    /// atomic with the receiver's idempotency claim.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database)
    /// if the transaction or insert fails.
    pub async fn emit(&self, message: &InboundMessage) -> fraiseql_error::Result<Emitted> {
        let mut tx = self.pool.begin().await.map_err(|error| db_err("begin", &error))?;
        let emitted = emit_in_tx(&mut tx, message).await?;
        tx.commit().await.map_err(|error| db_err("commit", &error))?;
        Ok(emitted)
    }
}

#[cfg(test)]
mod tests;
