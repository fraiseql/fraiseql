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
