//! Database migrations for functions infrastructure tables.
//!
//! Exposes DDL that `fraiseql-cli migrate up` can execute to create:
//! - `_fraiseql_functions`: versioned function artifact store
//! - `_fraiseql_cron_state`: cron scheduler state persistence

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
/// Returns the SQL DDL to create the function artifact store table and indexes.
///
/// The DDL uses `IF NOT EXISTS` for idempotency.
///
/// # Table Schema
///
/// | Column | Type | Notes |
/// |--------|------|-------|
/// | `pk_function` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Trinity-style PK |
/// | `name` | `TEXT NOT NULL` | Function name |
/// | `runtime` | `TEXT NOT NULL` | `"wasm"` or `"deno"` |
/// | `bytecode` | `BYTEA NOT NULL` | Compiled binary or source text |
/// | `version` | `INT NOT NULL` | Monotonically increasing deploy version |
/// | `deployed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Deploy timestamp |
/// | `status` | `TEXT NOT NULL DEFAULT 'active'` | `"active"` or `"inactive"` |
///
/// # Example
///
/// ```
/// let sql = fraiseql_functions::migrations::functions_migration_sql();
/// assert!(sql.contains("_fraiseql_functions"));
/// ```
pub const fn functions_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_functions (
    pk_function  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name         TEXT        NOT NULL,
    runtime      TEXT        NOT NULL,
    bytecode     BYTEA       NOT NULL,
    version      INT         NOT NULL DEFAULT 1,
    deployed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    status       TEXT        NOT NULL DEFAULT 'active'
);

CREATE INDEX IF NOT EXISTS idx_fraiseql_functions_name_status
    ON _fraiseql_functions (name, status);
"
}

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
