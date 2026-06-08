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

/// The canonical column set of the `core.tb_entity_change_log` contract.
///
/// This is the authoritative list the migration installs and the
/// `fraiseql doctor` `changelog-contract` check compares live
/// `information_schema.columns` against. Keep it in lockstep with
/// [`entity_change_log_contract_sql`] — the `migration_sql_covers_every_contract_column`
/// unit test fails if a column here is missing from the DDL.
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
