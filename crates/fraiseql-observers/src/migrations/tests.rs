//! Unit tests for the change-log contract DDL (no database required).

use super::{
    ENTITY_CHANGE_LOG_CONTRACT, ENTITY_CHANGE_LOG_CONTRACT_COLUMNS, entity_change_log_contract_sql,
};

#[test]
fn contract_columns_match_typed_contract() {
    // The name-only projection must stay in lockstep with the typed contract —
    // same names, same order — so the doctor drift check (typed) and the
    // DDL-coverage test (names) can never disagree about the column set.
    let typed_names: Vec<&str> = ENTITY_CHANGE_LOG_CONTRACT.iter().map(|c| c.name).collect();
    assert_eq!(
        typed_names.as_slice(),
        ENTITY_CHANGE_LOG_CONTRACT_COLUMNS,
        "ENTITY_CHANGE_LOG_CONTRACT_COLUMNS must mirror ENTITY_CHANGE_LOG_CONTRACT names in order"
    );
}

#[test]
fn typed_contract_uses_known_udt_names() {
    // Guard against a typo in an expected `udt` (e.g. "uuids" or "bigint"):
    // every value must be a real information_schema.columns.udt_name token.
    const KNOWN: &[&str] = &[
        "uuid",
        "int8",
        "int4",
        "text",
        "_text",
        "jsonb",
        "timestamptz",
    ];
    for col in ENTITY_CHANGE_LOG_CONTRACT {
        assert!(
            KNOWN.contains(&col.udt),
            "column `{}` has unknown expected udt `{}`",
            col.name,
            col.udt
        );
    }
}

#[test]
fn migration_targets_the_owned_table_and_view() {
    let sql = entity_change_log_contract_sql();
    assert!(sql.contains("core.tb_entity_change_log"), "creates the contract table");
    assert!(sql.contains("core.v_entity_change_log"), "creates the read-path view");
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS"), "fresh-install path is idempotent");
    assert!(
        sql.contains("ADD COLUMN IF NOT EXISTS"),
        "reconcile path uses additive, idempotent ALTERs"
    );
}

#[test]
fn migration_sql_covers_every_contract_column() {
    let sql = entity_change_log_contract_sql();
    for column in ENTITY_CHANGE_LOG_CONTRACT_COLUMNS {
        assert!(
            sql.contains(column),
            "contract column `{column}` is declared in ENTITY_CHANGE_LOG_CONTRACT_COLUMNS but missing from the migration DDL"
        );
    }
}

#[test]
fn migration_declares_both_tenant_id_and_fk_customer_org() {
    let sql = entity_change_log_contract_sql();
    // tenant_id (RLS stamp) and fk_customer_org (join FK) are complementary —
    // both must be present. (That neither is renamed into the other is proven
    // behaviourally by the `migration_reconciles_existing_app_table` integration
    // test: the seeded fk_customer_org value survives the migration.)
    assert!(sql.contains("ADD COLUMN IF NOT EXISTS tenant_id"), "tenant_id stamp added");
    assert!(
        sql.contains("ADD COLUMN IF NOT EXISTS fk_customer_org"),
        "fk_customer_org join FK kept (additive, not renamed)"
    );
}

#[test]
fn migration_installs_the_global_seq_sequence() {
    let sql = entity_change_log_contract_sql();
    // `seq` is fed by a plain global SEQUENCE default, so ANY INSERTer (the
    // executor AND cooperative external producers) gets a monotonic value.
    assert!(
        sql.contains("CREATE SEQUENCE IF NOT EXISTS core.seq_entity_change_log"),
        "global seq sequence is created"
    );
    assert!(
        sql.contains("ALTER COLUMN seq SET DEFAULT nextval('core.seq_entity_change_log')"),
        "seq defaults to nextval of the sequence"
    );
}

#[test]
fn migration_creates_the_five_contract_indexes() {
    let sql = entity_change_log_contract_sql();
    for index in [
        "idx_entity_log_duration",
        "idx_entity_log_type",
        "idx_entity_log_created",
        "idx_entity_log_tenant_seq",
        "idx_entity_log_type_seq",
    ] {
        assert!(sql.contains(index), "index `{index}` is created");
    }
}
