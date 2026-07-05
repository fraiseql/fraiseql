//! Database migrations for functions infrastructure tables.
//!
//! Exposes DDL that `fraiseql-cli migrate up` can execute to create:
//!
//! - `_fraiseql_cron_state` — persists cron scheduler state between server restarts
//!   ([`cron_migration_sql`]).
//! - `_fraiseql_inbound_message` — the durable inbound spine that normalized
//!   [`InboundMessage`](crate::InboundMessage)s land on before `after:ingest` dispatch
//!   ([`inbound_migration_sql`]).
//! - `_fraiseql_inbound_email_cursor` — the per-mailbox UID watermark the poll-IMAP email adapter
//!   advances between polls ([`inbound_email_cursor_migration_sql`]).
//! - `_fraiseql_send_status` + `_fraiseql_suppression` — the delivery-feedback stores that
//!   correlate an inbound bounce/challenge/reply back to a tracked send and hold the do-not-contact
//!   list checked before every send ([`send_tracking_migration_sql`]).

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

/// Returns the SQL DDL to create the poll-IMAP email cursor table.
///
/// The poll-IMAP adapter is *stateless with a cursor*: the only thing it persists
/// between polls is, per mailbox, the IMAP `UIDVALIDITY` it last saw and the
/// highest message `UID` it has already ingested. On the next poll it fetches
/// everything above that watermark; a changed `UIDVALIDITY` means the UID space
/// was reset, so the watermark is discarded and the mailbox is re-scanned
/// (deduplicated on the spine by `Message-ID`). The DDL uses `IF NOT EXISTS` for
/// idempotency.
///
/// # Table Schema
///
/// | Column | Type | Notes |
/// |--------|------|-------|
/// | `pk_inbound_email_cursor` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Trinity-style PK |
/// | `mailbox_key` | `TEXT NOT NULL` | Configured mailbox name (unique) |
/// | `uid_validity` | `BIGINT NOT NULL` | IMAP `UIDVALIDITY` the watermark was taken under |
/// | `last_uid` | `BIGINT NOT NULL` | Highest ingested `UID` |
/// | `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Last advance |
///
/// `uid_validity` / `last_uid` are `BIGINT` because IMAP UIDs are unsigned 32-bit
/// and would overflow a signed `INTEGER`.
///
/// # Example
///
/// ```
/// let sql = fraiseql_functions::migrations::inbound_email_cursor_migration_sql();
/// assert!(sql.contains("_fraiseql_inbound_email_cursor"));
/// ```
#[must_use]
pub const fn inbound_email_cursor_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_inbound_email_cursor (
    pk_inbound_email_cursor BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    mailbox_key             TEXT        NOT NULL,
    uid_validity            BIGINT      NOT NULL,
    last_uid                BIGINT      NOT NULL,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (mailbox_key)
);
"
}

/// Returns the SQL DDL to create the delivery-feedback stores.
///
/// Two fraiseql-managed tables underpin the delivery-feedback loop (the outbound
/// mirror of the inbound spine):
///
/// - `_fraiseql_send_status` — one row per tracked send, keyed by the per-dispatch VERP `send_id`.
///   It records the recipient, the sending address, the send-status lifecycle (`Sent` → `Bounced` /
///   `ChallengePending` / `Replied` / …), the challenge count, and the relay message id. The
///   exactly-once unique key on `(COALESCE(tenant_id, ''), send_id)` is what makes a durable retry
///   skip an already-sent dispatch instead of double-sending.
/// - `_fraiseql_suppression` — the do-not-contact list checked before every send, keyed on a
///   **keyed hash** of the address (never the raw address, so the match survives a GDPR erasure of
///   the recipient's PII elsewhere) plus a granular reason (`hard_bounce` / `challenge_unanswered`
///   / `unsubscribe`) and optional TTL.
///
/// Both tables carry an explicit `tenant_id` (stamped from the security context at
/// write time — a `TEXT` column because the runtime tenant id is an opaque string,
/// not necessarily a UUID) and are protected by a tenant-scoped RLS policy for
/// app-facing reads, mirroring #443's `tb_entity_change_log`. The platform writes
/// through the table-owning role (which bypasses RLS) and stamps `tenant_id`
/// explicitly; app reads through a non-owner role are filtered to the session's
/// `fraiseql.tenant_id`. `COALESCE(tenant_id, '')` in the unique indexes keeps the
/// exactly-once and suppression keys correct for single-tenant (NULL) rows, which a
/// bare `UNIQUE (tenant_id, …)` would treat as always-distinct.
///
/// The DDL is idempotent: `CREATE … IF NOT EXISTS` for tables/indexes, `ENABLE ROW
/// LEVEL SECURITY` is a no-op when already enabled, and each policy is dropped-if-
/// exists before creation (`CREATE POLICY` has no `IF NOT EXISTS` form).
///
/// # Example
///
/// ```
/// let sql = fraiseql_functions::migrations::send_tracking_migration_sql();
/// assert!(sql.contains("_fraiseql_send_status"));
/// assert!(sql.contains("_fraiseql_suppression"));
/// ```
#[must_use]
pub const fn send_tracking_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_send_status (
    pk_send_status  BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    send_id         TEXT        NOT NULL,
    tenant_id       TEXT,
    recipient       TEXT        NOT NULL,
    sending_address TEXT        NOT NULL,
    status          TEXT        NOT NULL,
    challenge_count INT         NOT NULL DEFAULT 0,
    last_signal     TEXT,
    message_id      TEXT,
    sent_at         TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_send_status_tenant_send
    ON _fraiseql_send_status (COALESCE(tenant_id, ''), send_id);

-- The correlation path looks a send up by send_id alone (from the inbound
-- Return-Path plus-tag), without a session tenant, so it needs its own index.
CREATE INDEX IF NOT EXISTS idx_send_status_send_id
    ON _fraiseql_send_status (send_id);

-- Fallback correlation: matching our sent message-id quoted in the inbound
-- References / In-Reply-To when the VERP plus-tag was stripped.
CREATE INDEX IF NOT EXISTS idx_send_status_message_id
    ON _fraiseql_send_status (message_id)
    WHERE message_id IS NOT NULL;

-- The challenge policy counts a recipient's pending challenges across campaigns.
CREATE INDEX IF NOT EXISTS idx_send_status_recipient
    ON _fraiseql_send_status (recipient);

ALTER TABLE _fraiseql_send_status ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS p_send_status_tenant ON _fraiseql_send_status;
CREATE POLICY p_send_status_tenant ON _fraiseql_send_status
    USING (tenant_id IS NOT DISTINCT FROM current_setting('fraiseql.tenant_id', true));

CREATE TABLE IF NOT EXISTS _fraiseql_suppression (
    pk_suppression BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id      TEXT,
    address_hash   TEXT        NOT NULL,
    reason         TEXT        NOT NULL,
    since          TIMESTAMPTZ NOT NULL DEFAULT now(),
    ttl            TIMESTAMPTZ,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_suppression_tenant_addr
    ON _fraiseql_suppression (COALESCE(tenant_id, ''), address_hash);

ALTER TABLE _fraiseql_suppression ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS p_suppression_tenant ON _fraiseql_suppression;
CREATE POLICY p_suppression_tenant ON _fraiseql_suppression
    USING (tenant_id IS NOT DISTINCT FROM current_setting('fraiseql.tenant_id', true));
"
}
