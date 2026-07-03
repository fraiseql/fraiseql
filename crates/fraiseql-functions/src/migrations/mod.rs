//! Database migrations for functions infrastructure tables.
//!
//! Exposes DDL that `fraiseql-cli migrate up` can execute to create:
//!
//! - `_fraiseql_cron_state` — persists cron scheduler state between server restarts
//!   ([`cron_migration_sql`]).
//! - `_fraiseql_inbound_message` — the durable inbound spine that normalized
//!   [`InboundMessage`](crate::InboundMessage)s land on before `after:ingest` dispatch
//!   ([`inbound_migration_sql`]).

#[cfg(test)]
mod tests;

/// Returns the SQL DDL to create the cron state table and indexes.
///
/// The DDL uses `IF NOT EXISTS` for idempotency — running it multiple times
/// is safe and produces no errors.
///
/// # Table Schema
///
/// | Column | Type | Notes |
/// |--------|------|-------|
/// | `pk_cron_state` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Trinity-style PK |
/// | `function_name` | `TEXT NOT NULL` | Function with the cron trigger |
/// | `cron_expr` | `TEXT NOT NULL` | Cron expression that fired |
/// | `last_fired_at` | `TIMESTAMPTZ NOT NULL` | When the cron last fired |
/// | `next_fire_at` | `TIMESTAMPTZ` | Computed next fire time (optional) |
/// | `fire_count` | `BIGINT NOT NULL DEFAULT 0` | Total number of fires |
/// | `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Last row update |
///
/// # Example
///
/// ```
/// let sql = fraiseql_functions::migrations::cron_migration_sql();
/// assert!(sql.contains("_fraiseql_cron_state"));
/// ```
#[must_use]
pub const fn cron_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_cron_state (
    pk_cron_state   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    function_name   TEXT        NOT NULL,
    cron_expr       TEXT        NOT NULL,
    last_fired_at   TIMESTAMPTZ NOT NULL,
    next_fire_at    TIMESTAMPTZ,
    fire_count      BIGINT      NOT NULL DEFAULT 0,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (function_name, cron_expr)
);

CREATE INDEX IF NOT EXISTS idx_cron_state_function
    ON _fraiseql_cron_state (function_name);

CREATE INDEX IF NOT EXISTS idx_cron_state_next_fire
    ON _fraiseql_cron_state (next_fire_at)
    WHERE next_fire_at IS NOT NULL;
"
}

/// Returns the SQL DDL to create the durable inbound-message spine table.
///
/// This is the inbound mirror of the outbound `tb_entity_change_log` outbox: a
/// normalized [`InboundMessage`](crate::InboundMessage) is persisted here inside
/// the receiver's transaction, deduplicated by `(source, idempotency_key)`, so
/// `after:ingest` dispatch is at-least-once. The DDL uses `IF NOT EXISTS` for
/// idempotency — running it multiple times is safe.
///
/// # Table Schema
///
/// | Column | Type | Notes |
/// |--------|------|-------|
/// | `pk_inbound_message` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Trinity-style PK |
/// | `id` | `UUID` | Stable message id, `gen_random_uuid()` default |
/// | `source` | `TEXT NOT NULL` | `webhook:<provider>` / `email` routing key |
/// | `idempotency_key` | `TEXT NOT NULL` | Provider event id or `Message-ID` |
/// | `thread_key` | `TEXT` | Conversation key (reply-awareness) |
/// | `payload` | `JSONB NOT NULL` | The full normalized `InboundMessage` |
/// | `received_at` | `TIMESTAMPTZ NOT NULL` | When the adapter received it |
/// | `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Row insertion time |
///
/// The `UNIQUE (source, idempotency_key)` constraint is the dedup key: an
/// `INSERT … ON CONFLICT DO NOTHING` against it discards a redelivery.
///
/// # Example
///
/// ```
/// let sql = fraiseql_functions::migrations::inbound_migration_sql();
/// assert!(sql.contains("_fraiseql_inbound_message"));
/// ```
#[must_use]
pub const fn inbound_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_inbound_message (
    pk_inbound_message BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id                 UUID        NOT NULL DEFAULT gen_random_uuid(),
    source             TEXT        NOT NULL,
    idempotency_key    TEXT        NOT NULL,
    thread_key         TEXT,
    payload            JSONB       NOT NULL,
    received_at        TIMESTAMPTZ NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_inbound_message_thread
    ON _fraiseql_inbound_message (thread_key)
    WHERE thread_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_inbound_message_received
    ON _fraiseql_inbound_message (received_at);
"
}
