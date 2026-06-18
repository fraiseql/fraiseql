use super::resolve_or_panic;

/// The loud-failure contract: a missing DB URL must abort, not silently default.
/// If this ever returns instead of panicking, DB-backed integration tests would
/// pass vacuously whenever CI fails to inject `DATABASE_URL`.
#[test]
#[should_panic(expected = "DATABASE_URL is not set")]
fn resolve_or_panic_is_loud_when_unset() {
    let _ = resolve_or_panic(None);
}

#[test]
fn resolve_or_panic_returns_the_set_url() {
    assert_eq!(
        resolve_or_panic(Some("postgresql://localhost/test".to_string())),
        "postgresql://localhost/test"
    );
}
