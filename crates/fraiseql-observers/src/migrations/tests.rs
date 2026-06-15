//! Unit tests for the change-log contract DDL (no database required).

use super::{
    ENTITY_CHANGE_LOG_CONTRACT, ENTITY_CHANGE_LOG_CONTRACT_COLUMNS,
    entity_change_log_capture_trigger_sql, entity_change_log_contract_sql,
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
fn migration_declares_the_pre_image_column_and_debezium_view() {
    let sql = entity_change_log_contract_sql();
    // changelog_pre_image: the additive before-image column + the Debezium
    // projection view (the envelope is a view, never a stored shape).
    assert!(
        sql.contains("ADD COLUMN IF NOT EXISTS object_data_before JSONB"),
        "pre-image column is added additively"
    );
    assert!(
        sql.contains("CREATE OR REPLACE VIEW core.v_entity_change_log_debezium"),
        "Debezium projection view is created"
    );
    assert!(
        sql.contains("object_data_before AS before") && sql.contains("object_data        AS after"),
        "the view projects before/after from the two columns"
    );
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

// ── #366 external-write capture trigger ──────────────────────────────────────

#[test]
fn capture_trigger_function_is_idempotent_and_named() {
    let sql = entity_change_log_capture_trigger_sql();
    assert!(
        sql.contains("CREATE OR REPLACE FUNCTION core.fn_entity_change_log_capture()"),
        "installs the capture function idempotently"
    );
    assert!(sql.contains("RETURNS trigger"), "is a trigger function");
}

#[test]
fn capture_trigger_suppresses_app_mediated_writes() {
    let sql = entity_change_log_capture_trigger_sql();
    // The suppression contract: an exact match against the executor's marker GUC
    // short-circuits before any INSERT, so an app-path write is never captured
    // twice. The GUC name/value mirror fraiseql_db::CDC_MEDIATED_VAR /
    // CDC_MEDIATED_ON (observers does not depend on fraiseql-db, so the literal
    // here is the lockstep — a change on either side trips this test).
    assert!(
        sql.contains("current_setting('fraiseql.cdc_mediated', true) = 'on'"),
        "checks the cdc_mediated marker for app-path suppression"
    );
}

#[test]
fn capture_trigger_writes_the_after_image_into_object_data_not_an_envelope() {
    let sql = entity_change_log_capture_trigger_sql();
    // Unified contract shape (changelog_pre_image): object_data is the after-image
    // (NEW) from every producer, NOT a {op,before,after} envelope. The op is the
    // modification_type column; the reader derives the Debezium code from it.
    assert!(
        sql.contains("object_data, object_data_before, tenant_id"),
        "INSERT column list carries object_data + object_data_before: {sql}"
    );
    assert!(
        !sql.contains("jsonb_build_object('op'"),
        "no {{op,before,after}} envelope is built into object_data anymore: {sql}"
    );
    assert!(!sql.contains("'op', 'c'"), "no inline op code: {sql}");
}

#[test]
fn capture_trigger_records_pre_image_only_when_opted_in() {
    let sql = entity_change_log_capture_trigger_sql();
    // The per-table opt-in is TG_ARGV[3]; OLD reaches object_data_before only when
    // it is on, so a non-opted-in table captures the after-image only.
    assert!(
        sql.contains("v_pre_image"),
        "reads the pre-image opt-in flag from TG_ARGV[3]: {sql}"
    );
    assert!(
        sql.contains("CASE WHEN v_pre_image THEN to_jsonb(o) ELSE NULL END"),
        "object_data_before = OLD only when the table opts in: {sql}"
    );
}

#[test]
fn capture_trigger_is_statement_level_with_transition_tables() {
    let sql = entity_change_log_capture_trigger_sql();
    // Bulk efficiency: one set-based INSERT...SELECT over the transition tables,
    // not a per-row invocation.
    assert!(sql.contains("FROM new_table"), "reads the NEW transition table");
    assert!(sql.contains("FROM old_table"), "reads the OLD transition table");
    assert!(
        sql.contains("JOIN old_table o ON"),
        "UPDATE pairs OLD/NEW on the PK (transition tables are unordered)"
    );
}

#[test]
fn capture_trigger_guards_object_id_to_a_uuid() {
    let sql = entity_change_log_capture_trigger_sql();
    // object_id is decoded as a non-null uuid over the whole batch, so a NULL/
    // non-UUID PK would stall the poller — the capture SELECT must filter to
    // UUID-shaped PKs.
    assert!(
        sql.contains("[0-9a-fA-F]{8}-[0-9a-fA-F]{4}"),
        "filters captured rows to a strict UUID PK shape"
    );
    assert!(sql.contains("v_pk_col"), "uses the configurable PK column");
}

#[test]
fn capture_trigger_stamps_tenant_and_marks_its_source() {
    let sql = entity_change_log_capture_trigger_sql();
    // Per-tenant subscription filtering needs tenant_id: column else session GUC.
    assert!(sql.contains("v_tenant_col"), "reads the configurable tenant column");
    assert!(
        sql.contains("current_setting('fraiseql.tenant_id'"),
        "falls back to the cooperative tenant session GUC"
    );
    // Captured rows are distinguishable from executor-written ones.
    assert!(
        sql.contains("'cdc_source', 'fallback_trigger'"),
        "marks captured rows with extra_metadata.cdc_source"
    );
}
