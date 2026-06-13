//! Tests for the MSSQL→NATS bridge.

use super::FETCH_BATCH_SQL;

// M-mssql-batch: the row cap must be a bound parameter, never the old
// hardcoded `TOP (100)`, so the configured `batch_size` is actually honored.
#[test]
fn fetch_batch_sql_binds_the_row_cap_not_a_literal() {
    assert!(FETCH_BATCH_SQL.contains("TOP (@P1)"), "batch size must be bound via TOP (@P1)");
    assert!(!FETCH_BATCH_SQL.contains("TOP (100)"), "the hardcoded TOP (100) must be gone");
    assert!(
        FETCH_BATCH_SQL.contains("pk_entity_change_log > @P2"),
        "cursor must move to @P2 now that @P1 is the cap"
    );
}
