//! Unit tests for the change-log contract DDL (no database required).

use super::{ENTITY_CHANGE_LOG_CONTRACT_COLUMNS, entity_change_log_contract_sql};

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
