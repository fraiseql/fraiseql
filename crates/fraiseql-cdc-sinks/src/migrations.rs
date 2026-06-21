//! The #382-owned per-sink delivery-state DDL.
//!
//! Distinct from `core.tb_entity_change_log` (the executor-owned outbox this
//! crate only *reads*): `core.tb_cdc_sink_state` records, per configured sink,
//! which outbox rows have been published and their retry/dead-letter state.
//! Const SQL (`IF NOT EXISTS`, idempotent), mirroring
//! `fraiseql-functions/src/migrations/mod.rs`; applied by `fraiseql-cli migrate
//! up` (or directly in tests).

/// Returns the idempotent DDL for the per-sink delivery-state table + indexes.
///
/// One row per `(sink_name, outbox row)`: a single outbox row matching `N`
/// configured sinks fans out to `N` tracking rows, so per-sink retry/dead-letter
/// is independent. `seq` carries the source outbox row's ordering/dedup key;
/// `pk_entity_change_log` references the outbox row for payload re-read.
///
/// # Example
///
/// ```
/// let sql = fraiseql_cdc_sinks::outbox_sink_state_migration_sql();
/// assert!(sql.contains("core.tb_cdc_sink_state"));
/// ```
#[must_use]
pub const fn outbox_sink_state_migration_sql() -> &'static str {
    "\
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_cdc_sink_state (
    pk_cdc_sink_state    BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sink_name            TEXT        NOT NULL,
    pk_entity_change_log BIGINT      NOT NULL,
    seq                  BIGINT      NOT NULL,
    tenant_id            UUID,
    table_name           TEXT        NOT NULL,
    op                   TEXT        NOT NULL,
    status               TEXT        NOT NULL DEFAULT 'pending',
    attempt_count        INT         NOT NULL DEFAULT 0,
    max_attempts         INT         NOT NULL DEFAULT 8,
    next_attempt_at      TIMESTAMPTZ,
    last_error           TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at         TIMESTAMPTZ,
    -- Idempotent enqueue / per-sink dedup: an outbox row enqueues at most once
    -- per sink.
    UNIQUE (sink_name, pk_entity_change_log)
);

-- Drain query: due rows for a sink (pending/retrying past next_attempt_at).
CREATE INDEX IF NOT EXISTS idx_cdc_sink_state_due
    ON core.tb_cdc_sink_state (sink_name, status, next_attempt_at);

-- Ordered per-sink draining + the enqueue cursor MAX(seq).
CREATE INDEX IF NOT EXISTS idx_cdc_sink_state_seq
    ON core.tb_cdc_sink_state (sink_name, seq);

-- Dead-letter monitoring view.
CREATE INDEX IF NOT EXISTS idx_cdc_sink_state_dead
    ON core.tb_cdc_sink_state (sink_name)
    WHERE status = 'dead';
"
}

#[cfg(test)]
mod tests;
