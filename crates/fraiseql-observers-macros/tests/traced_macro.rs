// Integration tests for fraiseql-observers-macros proc-macro attributes.
// Proc-macro crates cannot have inline unit tests; integration tests in tests/ are the
// canonical approach — they import and use the macro as an external consumer would.

use fraiseql_observers_macros::{instrument, traced};

// -- helpers -----------------------------------------------------------------

fn init_tracing() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
}

// -- #[traced] on sync functions -------------------------------------------

#[traced]
fn sync_ok() -> Result<i32, String> {
    Ok(42)
}

#[traced]
fn sync_err() -> Result<i32, String> {
    Err("boom".to_string())
}

#[test]
fn test_traced_sync_ok_returns_value() {
    init_tracing();
    assert_eq!(sync_ok().unwrap(), 42);
}

#[test]
fn test_traced_sync_err_propagates() {
    init_tracing();
    assert_eq!(sync_err().unwrap_err(), "boom");
}

// -- #[traced] with explicit name ------------------------------------------

#[traced(name = "custom_span_name")]
fn sync_named() -> Result<(), String> {
    Ok(())
}

#[test]
fn test_traced_named_span_ok() {
    init_tracing();
    assert!(sync_named().is_ok());
}

// -- #[traced] on async functions ------------------------------------------

#[traced]
async fn async_ok() -> Result<u64, String> {
    Ok(100)
}

#[traced]
async fn async_err() -> Result<u64, String> {
    Err("async error".to_string())
}

#[tokio::test]
async fn test_traced_async_ok() {
    init_tracing();
    assert_eq!(async_ok().await.unwrap(), 100);
}

#[tokio::test]
async fn test_traced_async_err_propagates() {
    init_tracing();
    assert_eq!(async_err().await.unwrap_err(), "async error");
}

// Verify that #[traced] on async functions does NOT hold a sync guard across
// .await points. The function must complete without deadlock even when run on
// a multi-threaded runtime where tasks can migrate between threads.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_traced_async_does_not_hold_sync_guard_across_await() {
    init_tracing();

    #[traced]
    async fn async_with_yield() -> Result<i32, String> {
        tokio::task::yield_now().await;
        Ok(7)
    }

    // Should complete without deadlock on a multi-threaded runtime.
    assert_eq!(async_with_yield().await.unwrap(), 7);
}

// -- #[instrument] macro ---------------------------------------------------

#[instrument]
fn instrumented_fn(x: i32, y: i32) -> i32 {
    x + y
}

#[test]
fn test_instrument_sync_returns_correct_value() {
    init_tracing();
    assert_eq!(instrumented_fn(3, 4), 7);
}

#[instrument]
async fn instrumented_async(label: &str) -> String {
    label.to_uppercase()
}

#[tokio::test]
async fn test_instrument_async_returns_correct_value() {
    init_tracing();
    assert_eq!(instrumented_async("hello").await, "HELLO");
}
