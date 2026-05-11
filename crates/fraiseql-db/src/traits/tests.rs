#[allow(clippy::unwrap_used)] // Reason: test code
#[test]
fn database_adapter_is_send_sync() {
    // Static assertion: `dyn DatabaseAdapter` must be `Send + Sync`.
    // This test exists to catch accidental removal of `Send + Sync` bounds.
    // It only needs to compile — no runtime assertion required.
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn super::DatabaseAdapter>();
}
