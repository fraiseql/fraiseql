//! Durable, atomic deduplication of inbound webhook deliveries.
//!
//! [`PostgresIdempotencyStore`] backs the [`IdempotencyStore`] seam with a real
//! table. The claim is an `INSERT … ON CONFLICT DO NOTHING RETURNING id` issued
//! on the caller's transaction, so it is atomic with the handler: a duplicate
//! delivery returns `Ok(None)` and is discarded, and a handler failure rolls the
//! claim back so the sender's retry reprocesses the event.

use sqlx::{PgPool, Postgres, Transaction};

use crate::{IdempotencyStore, Result};

/// Idempotent DDL for the inbound-delivery ledger. Exposed so a migration runner
/// can apply it explicitly; [`PostgresIdempotencyStore::init`] runs the same
/// statements.
///
/// The `(provider, event_id)` unique constraint is the dedup key — the atomic
/// claim relies on it. RLS is deny-by-default (mirrors the #411 identity store and
/// observers migration 12): `ENABLE` (not `FORCE`) so the owner that runs the
/// pipeline operates freely, while a non-owner role reads nothing (no permissive
/// policy) and `PUBLIC` holds no grant at all.
pub const SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS webhooks;

CREATE TABLE IF NOT EXISTS webhooks.tb_inbound_delivery (
    pk_inbound_delivery BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id          UUID        NOT NULL DEFAULT gen_random_uuid(),
    provider    TEXT        NOT NULL,
    event_id    TEXT        NOT NULL,
    event_type  TEXT        NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, event_id)
);

-- RLS deny-by-default (mirrors observers migration 12 / the #411 identity store).
-- ENABLE not FORCE so the owner (this store) and BYPASSRLS roles operate freely;
-- with no permissive policy a non-owner role reads nothing (fail-closed).
ALTER TABLE webhooks.tb_inbound_delivery ENABLE ROW LEVEL SECURITY;

-- Least-privilege baseline: never world-readable. RLS is defence-in-depth on top.
REVOKE ALL ON webhooks.tb_inbound_delivery FROM PUBLIC;
";

/// PostgreSQL-backed [`IdempotencyStore`] for inbound webhook deliveries.
///
/// Persists one row per successfully committed delivery; presence of a
/// `(provider, event_id)` row means "already processed". See the module
/// documentation for the schema and RLS posture.
pub struct PostgresIdempotencyStore {
    db: PgPool,
}

impl PostgresIdempotencyStore {
    /// Create a new store over an existing pool.
    ///
    /// The pool's role must own (or `BYPASSRLS`) the
    /// `webhooks.tb_inbound_delivery` table — it runs the trusted pipeline and
    /// must not be constrained by the deny-by-default RLS. Calling
    /// [`init`](Self::init) once on startup creates the table (so the connecting
    /// role owns it by construction).
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Create the `webhooks.tb_inbound_delivery` ledger (idempotent).
    ///
    /// Call once on startup. Safe to re-run and safe on a database that predates
    /// this store (the `CREATE … IF NOT EXISTS` form is the back-compat path).
    ///
    /// # Errors
    ///
    /// Returns [`WebhookError::Database`](crate::WebhookError::Database) if the DDL
    /// fails.
    pub async fn init(&self) -> Result<()> {
        sqlx::raw_sql(SCHEMA_SQL).execute(&self.db).await?;
        Ok(())
    }
}

impl IdempotencyStore for PostgresIdempotencyStore {
    async fn claim(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        provider: &str,
        event_id: &str,
        event_type: &str,
    ) -> Result<Option<uuid::Uuid>> {
        // ON CONFLICT DO NOTHING returns a row only on a fresh insert; an existing
        // (provider, event_id) yields zero rows → the delivery is a duplicate.
        // Concurrent duplicate deliveries serialise on the unique-key row lock, so
        // exactly one observes the insert and the others see the conflict.
        let claimed = sqlx::query_scalar::<_, uuid::Uuid>(
            "INSERT INTO webhooks.tb_inbound_delivery (provider, event_id, event_type) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (provider, event_id) DO NOTHING \
             RETURNING id",
        )
        .bind(provider)
        .bind(event_id)
        .bind(event_type)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(claimed)
    }
}
