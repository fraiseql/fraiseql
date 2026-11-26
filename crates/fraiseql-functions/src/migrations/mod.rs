//! Database migrations for functions infrastructure tables.
//!
//! Exposes DDL that `fraiseql-cli migrate up` can execute to create:
//!
//! - `_fraiseql_cron_state` — persists cron scheduler state between server restarts
//!   ([`cron_migration_sql`]).
//! - `_fraiseql_inbound_message` — the durable inbound spine that normalized
//!   [`InboundMessage`](crate::InboundMessage)s land on before `after:ingest` dispatch, and
//!   `_fraiseql_inbound_dispatch`, the ledger that makes that dispatch at-least-once
//!   ([`inbound_migration_sql`]).
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

-- Each statement is guarded by the catalog: a boot over a current schema takes no
-- table lock, where `IF NOT EXISTS` alone still would.
DO $cron_shape$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_cron_state') AND i.relname = 'idx_cron_state_function'
    ) THEN
        CREATE INDEX idx_cron_state_function ON _fraiseql_cron_state (function_name);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_cron_state') AND i.relname = 'idx_cron_state_next_fire'
    ) THEN
        CREATE INDEX idx_cron_state_next_fire ON _fraiseql_cron_state (next_fire_at) WHERE next_fire_at IS NOT NULL;
    END IF;
END
$cron_shape$;
"
}

/// Returns the SQL DDL to create the durable inbound-message spine and its dispatch ledger.
///
/// This is the inbound mirror of the outbound `tb_entity_change_log` outbox: a
/// normalized [`InboundMessage`](crate::InboundMessage) is persisted here inside
/// the receiver's transaction, deduplicated by `(source, idempotency_key)`: a message is
/// durable and stored once. Beside it, in the same transaction, the dispatch ledger
/// (`_fraiseql_inbound_dispatch`) records one `pending` row per `after:ingest` function the
/// message triggers, leased to the receiving process; a dispatch that has not settled when
/// its lease expires is re-dispatched by the server's sweep (#1175). The DDL uses
/// `IF NOT EXISTS` for idempotency — running it multiple times is safe.
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
/// | `event_type` | `TEXT` | The verified webhook event type; `NULL` where the source has none |
/// | `received_at` | `TIMESTAMPTZ NOT NULL` | When the adapter received it |
/// | `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Row insertion time |
///
/// The `UNIQUE (source, idempotency_key)` constraint is the dedup key, and the only
/// one an inbound delivery is claimed under: an `INSERT … ON CONFLICT DO NOTHING`
/// against it discards a redelivery. `event_type` is also added by an `ALTER TABLE
/// … ADD COLUMN IF NOT EXISTS`, so the DDL converges on a table an earlier version
/// of it created.
///
/// `_fraiseql_inbound_dispatch`, keyed `(fk_inbound_message, function_name)`: `state`
/// (`pending` → `dispatched` | `dead_lettered`), `attempts`, `lease_until` (while it is in the
/// future, a dispatch is in flight somewhere), `created_at`, `settled_at`.
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
    event_type         TEXT,
    received_at        TIMESTAMPTZ NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, idempotency_key)
);

-- Every server runs this at boot while the others serve traffic. `ADD COLUMN IF NOT
-- EXISTS` and `CREATE INDEX IF NOT EXISTS` lock the table (ACCESS EXCLUSIVE, SHARE)
-- before they find there is nothing to do, so each is guarded by a catalog check: a boot
-- over a current schema takes no lock on either table.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = to_regclass('_fraiseql_inbound_message')
          AND attname = 'event_type' AND NOT attisdropped
    ) THEN
        ALTER TABLE _fraiseql_inbound_message ADD COLUMN event_type TEXT;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_inbound_message')
          AND i.relname = 'idx_inbound_message_thread'
    ) THEN
        CREATE INDEX idx_inbound_message_thread
            ON _fraiseql_inbound_message (thread_key)
            WHERE thread_key IS NOT NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_inbound_message')
          AND i.relname = 'idx_inbound_message_received'
    ) THEN
        CREATE INDEX idx_inbound_message_received
            ON _fraiseql_inbound_message (received_at);
    END IF;
END
$$;

CREATE TABLE IF NOT EXISTS _fraiseql_inbound_dispatch (
    fk_inbound_message BIGINT      NOT NULL
        REFERENCES _fraiseql_inbound_message (pk_inbound_message) ON DELETE CASCADE,
    function_name      TEXT        NOT NULL,
    state              TEXT        NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'dispatched', 'dead_lettered')),
    attempts           INTEGER     NOT NULL DEFAULT 1,
    lease_until        TIMESTAMPTZ NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    settled_at         TIMESTAMPTZ,
    PRIMARY KEY (fk_inbound_message, function_name)
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_inbound_dispatch')
          AND i.relname = 'idx_inbound_dispatch_due'
    ) THEN
        CREATE INDEX idx_inbound_dispatch_due
            ON _fraiseql_inbound_dispatch (lease_until)
            WHERE state = 'pending';
    END IF;
END
$$;
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
///   exactly-once unique key on `(send_id, tenant_id) NULLS NOT DISTINCT` is what makes a durable
///   retry skip an already-sent dispatch instead of double-sending.
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
/// `fraiseql.tenant_id`. The unique keys are `NULLS NOT DISTINCT`, so single-tenant
/// (NULL) rows are one space, which a bare `UNIQUE (…, tenant_id)` would treat as
/// always-distinct. 2.15 expressed this as a `COALESCE(tenant_id, '')` expression index;
/// the DDL creates the new key before dropping that one, so uniqueness never lapses.
///
/// The DDL is idempotent, and a boot over a current schema takes no table lock: each index,
/// the row-level security and each policy is guarded by the catalog, a policy by its whole
/// definition, so one whose definition differs is replaced and one that matches is left
/// alone.
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

CREATE TABLE IF NOT EXISTS _fraiseql_suppression (
    pk_suppression BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id      TEXT,
    address_hash   TEXT        NOT NULL,
    reason         TEXT        NOT NULL,
    since          TIMESTAMPTZ NOT NULL DEFAULT now(),
    ttl            TIMESTAMPTZ,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Each statement is guarded by the catalog: a boot over a current schema takes no
-- table lock, where `IF NOT EXISTS` alone still would.
DO $send_tracking_shape$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_send_status') AND i.relname = 'uq_send_status_per_space'
    ) THEN
        CREATE UNIQUE INDEX uq_send_status_per_space
            ON _fraiseql_send_status (send_id, tenant_id) NULLS NOT DISTINCT;
    END IF;
    -- The correlation path looks a send up by send_id alone (from the inbound
    -- Return-Path plus-tag), without a session tenant, so it needs its own index.
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_send_status') AND i.relname = 'idx_send_status_send_id'
    ) THEN
        CREATE INDEX idx_send_status_send_id ON _fraiseql_send_status (send_id);
    END IF;
    -- Fallback correlation: matching our sent message-id quoted in the inbound
    -- References / In-Reply-To when the VERP plus-tag was stripped.
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_send_status') AND i.relname = 'idx_send_status_message_id'
    ) THEN
        CREATE INDEX idx_send_status_message_id
            ON _fraiseql_send_status (message_id) WHERE message_id IS NOT NULL;
    END IF;
    -- The challenge policy counts a recipient's pending challenges across campaigns.
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_send_status') AND i.relname = 'idx_send_status_recipient'
    ) THEN
        CREATE INDEX idx_send_status_recipient ON _fraiseql_send_status (recipient);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_suppression') AND i.relname = 'uq_suppression_per_space'
    ) THEN
        CREATE UNIQUE INDEX uq_suppression_per_space
            ON _fraiseql_suppression (address_hash, tenant_id) NULLS NOT DISTINCT;
    END IF;
END
$send_tracking_shape$;
-- The 2.15 keys, dropped once the keys above exist.
DROP INDEX IF EXISTS uq_send_status_tenant_send;
DROP INDEX IF EXISTS uq_suppression_tenant_addr;

-- Row-level security and the tenant policy, per table. `ENABLE ROW LEVEL SECURITY`,
-- `DROP POLICY` and `CREATE POLICY` each take an ACCESS EXCLUSIVE lock even when they change
-- nothing, so each is guarded by the catalog: RLS by `relrowsecurity`, the policy by its
-- whole definition (permissive, roles, command and expression), and a policy whose
-- definition differs is replaced. `tenant` is the policy's expression as PostgreSQL 18 (the
-- supported floor) stores it; should it differ from what the CREATE below stores, every boot
-- replaces the policy under its lock and the boot-lock test fails.
DO $send_status_rls$
DECLARE
    tenant CONSTANT text :=
        $q$(NOT (tenant_id IS DISTINCT FROM current_setting('fraiseql.tenant_id'::text, true)))$q$;
BEGIN
    IF NOT (SELECT relrowsecurity FROM pg_class WHERE oid = to_regclass('_fraiseql_send_status')) THEN
        ALTER TABLE _fraiseql_send_status ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_policy
        WHERE polrelid = to_regclass('_fraiseql_send_status') AND polname = 'p_send_status_tenant'
          AND polpermissive AND polroles = '{0}'::oid[] AND polcmd = '*'
          AND pg_get_expr(polqual, polrelid) = tenant AND polwithcheck IS NULL
    ) THEN
        DROP POLICY IF EXISTS p_send_status_tenant ON _fraiseql_send_status;
        CREATE POLICY p_send_status_tenant ON _fraiseql_send_status
            USING (tenant_id IS NOT DISTINCT FROM current_setting('fraiseql.tenant_id', true));
    END IF;
END
$send_status_rls$;
DO $suppression_rls$
DECLARE
    tenant CONSTANT text :=
        $q$(NOT (tenant_id IS DISTINCT FROM current_setting('fraiseql.tenant_id'::text, true)))$q$;
BEGIN
    IF NOT (SELECT relrowsecurity FROM pg_class WHERE oid = to_regclass('_fraiseql_suppression')) THEN
        ALTER TABLE _fraiseql_suppression ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_policy
        WHERE polrelid = to_regclass('_fraiseql_suppression') AND polname = 'p_suppression_tenant'
          AND polpermissive AND polroles = '{0}'::oid[] AND polcmd = '*'
          AND pg_get_expr(polqual, polrelid) = tenant AND polwithcheck IS NULL
    ) THEN
        DROP POLICY IF EXISTS p_suppression_tenant ON _fraiseql_suppression;
        CREATE POLICY p_suppression_tenant ON _fraiseql_suppression
            USING (tenant_id IS NOT DISTINCT FROM current_setting('fraiseql.tenant_id', true));
    END IF;
END
$suppression_rls$;
"
}

/// Returns the SQL DDL to create the durable function-dispatch dead-letter table (#598).
///
/// The Postgres-backed counterpart to the server's in-memory function DLQ: a
/// function-trigger dispatch (after:mutation / after:ingest / after:capture) that
/// exhausts its retries lands here rather than in a store that vanishes on restart,
/// so an operator can inspect and replay it. The DDL uses `IF NOT EXISTS` for
/// idempotency — running it multiple times is safe.
///
/// # Table Schema
///
/// | Column | Type | Notes |
/// |--------|------|-------|
/// | `pk_function_dlq` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Trinity-style PK |
/// | `id` | `UUID NOT NULL` | Stable dead-letter id (`FunctionDispatchRecord::id`) |
/// | `source` | `TEXT NOT NULL` | `DispatchSource` label (`after:mutation`, …) |
/// | `function_name` | `TEXT NOT NULL` | Function whose dispatch failed |
/// | `trigger_type` | `TEXT NOT NULL` | e.g. `after:mutation:onUserCreated` |
/// | `idempotency_token` | `TEXT NOT NULL` | The per-dispatch token every attempt saw |
/// | `payload` | `JSONB NOT NULL` | The event payload, for inspection/replay |
/// | `error_message` | `TEXT NOT NULL` | Final error from the exhausted dispatch |
/// | `attempts` | `BIGINT NOT NULL` | Attempts made before dead-lettering |
/// | `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | When it was dead-lettered |
///
/// **Sensitivity.** A dead-lettered `payload` can contain row data (the after-image
/// that a money/mail function was dispatched with). The table is a server-owned
/// operational table in the reserved `_fraiseql_*` namespace — accessed only through
/// the connecting role, like `_fraiseql_cron_state` — and carries no RLS policy
/// (a dispatch record has no single tenant boundary). Treat it as sensitive: it is
/// never logged at default level, and its retention is bounded by the same
/// `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE` cap the in-memory store honors.
///
/// # Example
///
/// ```
/// let sql = fraiseql_functions::migrations::dlq_migration_sql();
/// assert!(sql.contains("_fraiseql_function_dlq"));
/// ```
#[must_use]
pub const fn dlq_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_function_dlq (
    pk_function_dlq   BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id                UUID        NOT NULL,
    source            TEXT        NOT NULL,
    function_name     TEXT        NOT NULL,
    trigger_type      TEXT        NOT NULL,
    idempotency_token TEXT        NOT NULL,
    payload           JSONB       NOT NULL,
    error_message     TEXT        NOT NULL,
    attempts          BIGINT      NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Each statement is guarded by the catalog: a boot over a current schema takes no
-- table lock, where `IF NOT EXISTS` alone still would.
DO $dlq_shape$
BEGIN
    -- The dead-letter id is the stable handle an operator inspects / replays by.
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_function_dlq') AND i.relname = 'uq_function_dlq_id'
    ) THEN
        CREATE UNIQUE INDEX uq_function_dlq_id ON _fraiseql_function_dlq (id);
    END IF;

    -- Oldest-first draining for the inspection/replay list.
    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_function_dlq') AND i.relname = 'idx_function_dlq_created'
    ) THEN
        CREATE INDEX idx_function_dlq_created ON _fraiseql_function_dlq (created_at);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_class i JOIN pg_class t ON t.relnamespace = i.relnamespace
        WHERE t.oid = to_regclass('_fraiseql_function_dlq') AND i.relname = 'idx_function_dlq_function'
    ) THEN
        CREATE INDEX idx_function_dlq_function ON _fraiseql_function_dlq (function_name);
    END IF;
END
$dlq_shape$;
"
}
