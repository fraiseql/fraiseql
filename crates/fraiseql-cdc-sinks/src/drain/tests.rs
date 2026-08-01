#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn enqueue_is_an_anti_join_not_a_seq_watermark() {
    // #797: a MAX(seq) cursor permanently drops rows that commit out of
    // sequence order; the enqueue must be an anti-join against the tracking
    // table, bounded by the commit-lag window.
    let sql = enqueue_sql();
    assert!(sql.contains("INSERT INTO core.tb_cdc_sink_state"));
    assert!(sql.contains("LEFT JOIN core.tb_cdc_sink_state s"));
    assert!(sql.contains("s.pk_cdc_sink_state IS NULL"));
    assert!(sql.contains("e.created_at > now() - make_interval(secs => $5)"));
    assert!(!sql.contains("MAX(seq)"), "the seq watermark drops late-committing rows (#797)");
    assert!(sql.contains("ON CONFLICT (sink_name, pk_entity_change_log) DO NOTHING"));
}

#[test]
fn sweep_covers_the_window_complement() {
    let sql = sweep_sql();
    assert!(sql.contains("e.created_at <= now() - make_interval(secs => $5)"));
    assert!(sql.contains("s.pk_cdc_sink_state IS NULL"));
    assert!(!sql.contains("MAX(seq)"));
}

#[test]
fn claim_blocks_on_the_first_undue_row_and_leases() {
    // #815/#814: the claim takes a contiguous-from-head prefix of due rows
    // (head-of-line blocking) and marks them in_flight under a lease, so
    // publishing happens outside any database transaction.
    let sql = claim_sql();
    assert!(sql.contains("bool_and(is_due) OVER"));
    assert!(sql.contains("ORDER BY seq"));
    assert!(sql.contains("status = 'in_flight'"));
    assert!(sql.contains("lease_expires_at = now() + make_interval(secs => $3)"));
    assert!(
        !sql.contains("FOR UPDATE"),
        "row locks would need a transaction held across publishes"
    );
}

#[test]
fn claimed_payload_is_ordered_and_lock_free() {
    let sql = claimed_payload_sql();
    assert!(sql.contains("ORDER BY s.seq"));
    assert!(sql.contains("JOIN core.tb_entity_change_log e"));
    assert!(!sql.contains("FOR UPDATE"));
}

#[test]
fn drain_stats_default_is_zero() {
    assert_eq!(
        DrainStats::default(),
        DrainStats {
            enqueued:       0,
            published:      0,
            retried:        0,
            dead:           0,
            late_recovered: 0,
        }
    );
}
