use super::*;

#[test]
fn test_isolation_level_sql() {
    assert_eq!(WebhookIsolation::ReadCommitted.as_sql(), "READ COMMITTED");
}

#[test]
fn test_default_isolation_level() {
    let default = WebhookIsolation::default();
    assert_eq!(default.as_sql(), "READ COMMITTED");
}

#[test]
fn only_read_committed_is_offered() {
    // #1894: pg_tviews refuses a REPEATABLE READ or SERIALIZABLE writer (0A000). The match is
    // exhaustive, so a level added back fails to compile here until it is weighed against that.
    match WebhookIsolation::default() {
        WebhookIsolation::ReadCommitted => {},
    }
}
