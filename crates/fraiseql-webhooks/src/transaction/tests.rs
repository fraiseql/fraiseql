use super::*;

#[test]
fn test_isolation_level_sql() {
    assert_eq!(WebhookIsolation::ReadCommitted.as_sql(), "READ COMMITTED");
    assert_eq!(WebhookIsolation::RepeatableRead.as_sql(), "REPEATABLE READ");
    assert_eq!(WebhookIsolation::Serializable.as_sql(), "SERIALIZABLE");
}

#[test]
fn test_default_isolation_level() {
    let default = WebhookIsolation::default();
    assert_eq!(default.as_sql(), "READ COMMITTED");
}
