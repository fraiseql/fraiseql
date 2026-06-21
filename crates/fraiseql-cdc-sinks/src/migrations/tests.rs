#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn migration_sql_has_table_columns_unique_and_indexes() {
    let sql = outbox_sink_state_migration_sql();
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS core.tb_cdc_sink_state"));
    for col in [
        "sink_name",
        "pk_entity_change_log",
        "seq",
        "status",
        "attempt_count",
        "max_attempts",
        "next_attempt_at",
        "last_error",
        "published_at",
    ] {
        assert!(sql.contains(col), "migration missing column {col}");
    }
    assert!(sql.contains("UNIQUE (sink_name, pk_entity_change_log)"));
    assert!(sql.contains("idx_cdc_sink_state_due"));
    assert!(sql.contains("idx_cdc_sink_state_seq"));
    assert!(sql.contains("WHERE status = 'dead'"));
    assert!(sql.contains("IF NOT EXISTS"));
}
