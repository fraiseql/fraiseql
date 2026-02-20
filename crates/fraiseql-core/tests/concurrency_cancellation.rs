//! Task cancellation safety tests.
//!
//! Verifies that cancelling in-flight queries (via `tokio::time::timeout` or
//! `JoinHandle::abort`) does not corrupt adapter state or deadlock subsequent
//! operations.

use std::{sync::Arc, time::Duration};

use fraiseql_core::db::DatabaseAdapter;
use fraiseql_test_utils::failing_adapter::{FailError, FailingAdapter};

#[tokio::test]
async fn test_query_cancellation_via_tokio_timeout() {
    let adapter =
        Arc::new(FailingAdapter::new().fail_with_error(FailError::Timeout { timeout_ms: 5000 }));

    let adapter_clone = Arc::clone(&adapter);
    let result = tokio::time::timeout(Duration::from_millis(50), async move {
        adapter_clone.execute_where_query("v_user", None, None, None).await
    })
    .await;

    // The adapter returns an error immediately (no actual sleep), so the
    // query completes before the timeout. Either outcome is valid:
    // - Ok(Err(_)) means the adapter error arrived first
    // - Err(_) means the timeout fired first
    match result {
        Ok(Err(_)) => {
            // Adapter error propagated — expected since FailingAdapter is synchronous
        },
        Err(_) => {
            // Timeout fired — also acceptable
        },
        Ok(Ok(_)) => panic!("query should not succeed with global timeout error"),
    }
}

#[tokio::test]
async fn test_aborted_task_doesnt_corrupt_adapter_state() {
    let adapter = Arc::new(FailingAdapter::new());

    // Spawn and immediately abort a task
    let adapter_clone = Arc::clone(&adapter);
    let handle =
        tokio::spawn(
            async move { adapter_clone.execute_where_query("v_user", None, None, None).await },
        );
    handle.abort();
    let _ = handle.await; // Ignore result (JoinError if aborted)

    // Adapter should still be functional
    let result = adapter.execute_where_query("v_user", None, None, None).await;
    assert!(result.is_ok(), "adapter must remain usable after task abort");
}

#[tokio::test]
async fn test_concurrent_queries_with_one_cancelled() {
    let adapter = Arc::new(FailingAdapter::new());

    let mut handles = Vec::with_capacity(10);
    for _ in 0..10 {
        let adapter = Arc::clone(&adapter);
        handles.push(tokio::spawn(async move {
            adapter.execute_where_query("v_user", None, None, None).await
        }));
    }

    // Abort the first task
    handles[0].abort();

    let mut successes = 0u64;
    let mut aborted = 0u64;
    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => successes += 1,
            Err(e) if e.is_cancelled() => aborted += 1,
            other => panic!("unexpected result: {other:?}"),
        }
    }

    // The aborted task may or may not have completed before abort
    assert!(successes >= 9, "at least 9 non-aborted tasks should succeed, got {successes}");
    assert!(successes + aborted == 10, "all tasks should be accounted for");
}

#[tokio::test]
async fn test_rapid_spawn_and_cancel_cycle() {
    let adapter = Arc::new(FailingAdapter::new());

    for _ in 0..100 {
        let adapter = Arc::clone(&adapter);
        let handle =
            tokio::spawn(
                async move { adapter.execute_where_query("v_user", None, None, None).await },
            );
        handle.abort();
        let _ = handle.await;
    }

    // Adapter state should be consistent: query_count reflects only
    // queries that actually ran (0 to 100 depending on scheduling)
    let count = adapter.query_count();
    assert!(count <= 100, "query count should not exceed spawn count, got {count}");

    // Adapter must still work after rapid spawn/cancel cycles
    let result = adapter.execute_where_query("v_user", None, None, None).await;
    assert!(result.is_ok(), "adapter must remain usable after rapid cancel cycles");
}
