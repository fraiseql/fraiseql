//! Database migrations for functions infrastructure tables.
//!
//! Exposes DDL that `fraiseql-cli migrate up` can execute to create the
//! `_fraiseql_cron_state` table, which persists cron scheduler state between
//! server restarts.

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
pub fn cron_migration_sql() -> &'static str {
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
