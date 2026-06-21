#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn enqueue_sql_shape() {
    let sql = enqueue_sql();
    assert!(sql.contains("INSERT INTO core.tb_cdc_sink_state"));
    assert!(sql.contains("FROM core.tb_entity_change_log"));
    assert!(sql.contains("MAX(seq)"));
    assert!(sql.contains("ORDER BY e.seq"));
    assert!(sql.contains("ON CONFLICT (sink_name, pk_entity_change_log) DO NOTHING"));
}

#[test]
fn publish_select_sql_locks_and_orders_by_seq() {
    let sql = publish_select_sql();
    assert!(sql.contains("FOR UPDATE OF s SKIP LOCKED"));
    assert!(sql.contains("ORDER BY s.seq"));
    assert!(sql.contains("status IN ('pending', 'retrying')"));
    assert!(sql.contains("JOIN core.tb_entity_change_log e"));
}

#[test]
fn drain_stats_default_is_zero() {
    assert_eq!(
        DrainStats::default(),
        DrainStats {
            enqueued:  0,
            published: 0,
            retried:   0,
            dead:      0,
        }
    );
}
