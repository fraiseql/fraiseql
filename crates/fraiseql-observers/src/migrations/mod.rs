//! Database migrations for the change-log contract.
//!
//! Exposes the DDL and the canonical column set for `core.tb_entity_change_log`
//! — the framework-owned change-log table (the Change Spine Tier 0 outbox; see
//! `docs/architecture/change-log-contract.md`). The DDL is a single source of
//! truth shared by the migration runner and the `fraiseql doctor` drift check.

#[cfg(test)]
mod tests;

/// SQL DDL that installs (or reconciles to) the `core.tb_entity_change_log`
/// contract: the superset table, its indexes, and the `core.v_entity_change_log`
/// read-path view.
///
/// The DDL is **purely additive and idempotent**: it uses `CREATE TABLE IF NOT
/// EXISTS` then `ALTER TABLE ... ADD COLUMN IF NOT EXISTS`, so running it
/// multiple times, or against a pre-existing app-created table, is safe and
/// produces no errors. It never drops or renames a column — `tenant_id` is
/// added alongside `fk_customer_org`, not in place of it.
///
/// # Example
///
/// ```
/// let sql = fraiseql_observers::migrations::entity_change_log_contract_sql();
/// assert!(sql.contains("core.tb_entity_change_log"));
/// assert!(sql.contains("duration_ms"));
/// ```
#[must_use]
pub const fn entity_change_log_contract_sql() -> &'static str {
    include_str!("../../migrations/08_create_entity_change_log_contract.sql")
}

/// SQL DDL that installs the suppressible external-write capture trigger function
/// `core.fn_entity_change_log_capture()` (#366).
///
/// The shipped fallback that brings *uncooperative external writes* (raw
/// `INSERT INTO tb_post` from psql / a migration / a third-party tool) onto the
/// Change Spine without double-emitting for writes already handled by the
/// mutation executor: it suppresses its row when the executor's transaction-local
/// marker `fraiseql.cdc_mediated = 'on'` is set, and otherwise writes a
/// contract-conforming `core.tb_entity_change_log` row with a Debezium-style
/// `{op, before, after}` envelope. Statement-level + transition tables, so a bulk
/// statement captures all its rows in one set-based INSERT.
///
/// This installs only the *function*; per-table triggers are generated from a
/// compiled schema's `@subscribable` declarations by
/// `fraiseql_core::schema::generate_capture_trigger_ddl`. PostgreSQL only;
/// idempotent (`CREATE OR REPLACE`). Requires the contract table from
/// [`entity_change_log_contract_sql`] to exist first.
///
/// # Example
///
/// ```
/// let sql = fraiseql_observers::migrations::entity_change_log_capture_trigger_sql();
/// assert!(sql.contains("core.fn_entity_change_log_capture"));
/// assert!(sql.contains("fraiseql.cdc_mediated"));
/// ```
#[must_use]
pub const fn entity_change_log_capture_trigger_sql() -> &'static str {
    include_str!("../../migrations/11_create_change_log_capture_trigger.sql")
}

/// SQL DDL that enables Row-Level Security on `core.tb_entity_change_log` and
/// installs its tenant-isolation policies (audit #437 finding F6 / #443).
///
/// Turns the change-log table fail-closed: a role that is neither the table owner
/// nor `BYPASSRLS`, and that has not set the `fraiseql.tenant_id` session GUC,
/// reads zero change-log rows (deny-by-default). The trusted internal consumers
/// (poller, NATS bridges, server handlers, executor outbox) must run as the table
/// owner or a `BYPASSRLS` role — a **BREAKING** operator requirement, since a
/// role without it silently sees an empty change-log. Uses `ENABLE` (not `FORCE`)
/// so the owner and the `SECURITY DEFINER` capture function are exempt.
///
/// PostgreSQL only; idempotent (`ENABLE` no-ops when already on; `DROP POLICY IF
/// EXISTS` + `CREATE POLICY` replaces cleanly). Requires the contract table from
/// [`entity_change_log_contract_sql`] to exist first.
///
/// # Example
///
/// ```
/// let sql = fraiseql_observers::migrations::entity_change_log_rls_sql();
/// assert!(sql.contains("ENABLE ROW LEVEL SECURITY"));
/// assert!(sql.contains("p_change_log_tenant_read"));
/// ```
#[must_use]
pub const fn entity_change_log_rls_sql() -> &'static str {
    include_str!("../../migrations/12_enable_change_log_rls.sql")
}

/// SQL DDL that installs the `_fraiseql_source_cursor` table and its
/// deny-by-default Row-Level Security (#573 scheduled ingress `Source`s).
///
/// One row per source holds the opaque JSONB watermark the source advances between
/// runs, plus a monotonic `version` generation counter used as the compare-and-swap
/// guard so a stale writer cannot regress the cursor. RLS + `REVOKE ALL FROM PUBLIC`
/// make it fail-closed exactly like the change-log (migration 12): a non-owner,
/// non-`BYPASSRLS` role without the `fraiseql.tenant_id` GUC reads zero rows.
///
/// PostgreSQL only; idempotent (`CREATE TABLE IF NOT EXISTS`; `ENABLE` no-ops when
/// already on; `DROP POLICY IF EXISTS` + `CREATE POLICY` replaces cleanly).
///
/// # Example
///
/// ```
/// let sql = fraiseql_observers::migrations::source_cursor_sql();
/// assert!(sql.contains("_fraiseql_source_cursor"));
/// assert!(sql.contains("ENABLE ROW LEVEL SECURITY"));
/// ```
#[must_use]
pub const fn source_cursor_sql() -> &'static str {
    include_str!("../../migrations/13_create_source_cursor.sql")
}

/// One column of the `core.tb_entity_change_log` contract: its name and the
/// canonical PostgreSQL base type the migration installs it as.
///
/// `udt` is the `information_schema.columns.udt_name` the column carries once the
/// contract migration has run — the lower-case PG base type, e.g. `"uuid"`,
/// `"int8"` (BIGINT), `"int4"` (INTEGER), `"text"`, `"jsonb"`, `"timestamptz"`,
/// or `"_text"` (the array element form of `TEXT[]`).
///
/// The `fraiseql doctor` `changelog-contract` drift check compares a live
/// table's `udt_name` against this so it can flag a **pre-existing** column the
/// additive migration cannot reconcile: `ADD COLUMN IF NOT EXISTS` no-ops on a
/// column that already exists, so a legacy `object_id text` survives even though
/// the contract wants `object_id uuid` (this bit the #149 change-log e2e). A
/// missing column is harmless — the migration adds it with the right type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractColumn {
    /// Column name.
    pub name: &'static str,
    /// Expected `information_schema.columns.udt_name` after the migration runs.
    pub udt:  &'static str,
}

/// The canonical, **typed** column set of the `core.tb_entity_change_log`
/// contract — the authoritative source.
///
/// The migration DDL is checked against the names here by the
/// `migration_sql_covers_every_contract_column` unit test, and the
/// `fraiseql doctor` `changelog-contract` check compares live
/// `information_schema.columns` (name + `udt_name`) against it.
/// [`ENTITY_CHANGE_LOG_CONTRACT_COLUMNS`] is the name-only projection, kept in
/// lockstep by `contract_columns_match_typed_contract`.
pub const ENTITY_CHANGE_LOG_CONTRACT: &[ContractColumn] = &[
    ContractColumn {
        name: "pk_entity_change_log",
        udt:  "int8",
    },
    ContractColumn {
        name: "id",
        udt:  "uuid",
    },
    ContractColumn {
        name: "tenant_id",
        udt:  "uuid",
    },
    ContractColumn {
        name: "fk_customer_org",
        udt:  "int8",
    },
    ContractColumn {
        name: "fk_contact",
        udt:  "int8",
    },
    ContractColumn {
        name: "object_type",
        udt:  "text",
    },
    ContractColumn {
        name: "modification_type",
        udt:  "text",
    },
    ContractColumn {
        name: "object_id",
        udt:  "uuid",
    },
    ContractColumn {
        name: "object_data",
        udt:  "jsonb",
    },
    ContractColumn {
        name: "object_data_before",
        udt:  "jsonb",
    },
    ContractColumn {
        name: "updated_fields",
        udt:  "_text",
    },
    ContractColumn {
        name: "cascade",
        udt:  "jsonb",
    },
    ContractColumn {
        name: "duration_ms",
        udt:  "int4",
    },
    ContractColumn {
        name: "started_at",
        udt:  "timestamptz",
    },
    ContractColumn {
        name: "created_at",
        udt:  "timestamptz",
    },
    ContractColumn {
        name: "commit_time",
        udt:  "timestamptz",
    },
    ContractColumn {
        name: "seq",
        udt:  "int8",
    },
    ContractColumn {
        name: "actor_type",
        udt:  "text",
    },
    ContractColumn {
        name: "acting_for",
        udt:  "uuid",
    },
    ContractColumn {
        name: "schema_version",
        udt:  "text",
    },
    ContractColumn {
        name: "trace_id",
        udt:  "text",
    },
    ContractColumn {
        name: "trace_context",
        udt:  "jsonb",
    },
    ContractColumn {
        name: "change_status",
        udt:  "text",
    },
    ContractColumn {
        name: "extra_metadata",
        udt:  "jsonb",
    },
    ContractColumn {
        name: "nats_published_at",
        udt:  "timestamptz",
    },
    ContractColumn {
        name: "nats_event_id",
        udt:  "uuid",
    },
];

/// The canonical column **names** of the `core.tb_entity_change_log` contract.
///
/// The name-only projection of [`ENTITY_CHANGE_LOG_CONTRACT`]; kept in lockstep
/// with it by the `contract_columns_match_typed_contract` unit test and with the
/// migration DDL by `migration_sql_covers_every_contract_column`.
pub const ENTITY_CHANGE_LOG_CONTRACT_COLUMNS: &[&str] = &[
    "pk_entity_change_log",
    "id",
    "tenant_id",
    "fk_customer_org",
    "fk_contact",
    "object_type",
    "modification_type",
    "object_id",
    "object_data",
    "object_data_before",
    "updated_fields",
    "cascade",
    "duration_ms",
    "started_at",
    "created_at",
    "commit_time",
    "seq",
    "actor_type",
    "acting_for",
    "schema_version",
    "trace_id",
    "trace_context",
    "change_status",
    "extra_metadata",
    "nats_published_at",
    "nats_event_id",
];
