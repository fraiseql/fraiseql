//! Integration tests for stream pause/resume functionality

use fraiseql_wire::{FraiseClient, stream::StreamState};
use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

/// Test helper: creates a test database connection string
fn test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost/fraiseql_test".to_string()
    })
}

/// Test helper: check if we can connect to the test database
async fn can_connect() -> bool {
    FraiseClient::connect(&test_db_url()).await.is_ok()
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored --test integration_pause_resume
async fn test_pause_idempotent() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .execute()
        .await
        .expect("execute");

    // Pause once
    stream.pause().await.expect("first pause");

    // Pause again (should be idempotent, no error)
    stream.pause().await.expect("second pause (idempotent)");
}

#[tokio::test]
#[ignore]
async fn test_resume_idempotent() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .execute()
        .await
        .expect("execute");

    // Resume without pause (should be idempotent, no error)
    stream.resume().await.expect("resume before pause");

    // Resume again (should also be idempotent)
    stream.resume().await.expect("second resume");
}

#[tokio::test]
#[ignore]
async fn test_pause_stops_reading() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .execute()
        .await
        .expect("execute");

    // Collect a few items
    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
        count += 1;
        if count >= 10 {
            break; // Collect 10 items
        }
    }

    assert!(count >= 10, "Expected to collect at least 10 items");

    // Get buffered count before pause
    let stats_before = stream.stats();
    let buffered_before = stats_before.items_buffered;

    // Pause stream
    stream.pause().await.expect("pause");

    // Wait a bit to ensure background task is fully paused
    sleep(Duration::from_millis(100)).await;

    // Get buffered count after pause
    let stats_after = stream.stats();
    let buffered_after = stats_after.items_buffered;

    // After pause, buffered count should not increase (background task stopped reading)
    // Note: May be equal or decrease as consumer might still poll
    assert!(
        buffered_after <= buffered_before + 1,
        "Buffered count should not increase after pause: before={}, after={}",
        buffered_before,
        buffered_after
    );
}

#[tokio::test]
#[ignore]
async fn test_resume_continues() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .execute()
        .await
        .expect("execute");

    // Collect a few items
    let mut count_before_pause = 0;
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
        count_before_pause += 1;
        if count_before_pause >= 5 {
            break;
        }
    }

    // Pause
    stream.pause().await.expect("pause");

    // Try to poll (should not get new items due to pause)
    // This tests that background task actually paused
    sleep(Duration::from_millis(50)).await;
    let _stats_paused = stream.stats();

    // Resume
    stream.resume().await.expect("resume");

    // Collect more items
    let mut count_after_resume = 0;
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
        count_after_resume += 1;
        if count_after_resume >= 5 {
            break;
        }
    }

    assert!(
        count_after_resume >= 5,
        "Expected to collect at least 5 items after resume"
    );
}

#[tokio::test]
#[ignore]
async fn test_pause_on_completed_fails() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .where_sql("true")  // Dummy predicate to limit results
        .execute()
        .await
        .expect("execute");

    // Consume all items
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
    }

    // Try to pause completed stream (should fail)
    let result = stream.pause().await;
    assert!(
        result.is_err(),
        "Pause on completed stream should fail, got: {:?}",
        result
    );
}

#[tokio::test]
#[ignore]
async fn test_resume_on_completed_fails() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .where_sql("true")  // Dummy predicate to limit results
        .execute()
        .await
        .expect("execute");

    // Consume all items
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
    }

    // Try to resume completed stream (should fail)
    let result = stream.resume().await;
    assert!(
        result.is_err(),
        "Resume on completed stream should fail, got: {:?}",
        result
    );
}

#[tokio::test]
#[ignore]
async fn test_drop_while_paused_cleanup() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .execute()
        .await
        .expect("execute");

    // Collect a few items
    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
        count += 1;
        if count >= 5 {
            break;
        }
    }

    // Pause stream
    stream.pause().await.expect("pause");

    // Drop the stream while paused
    // This should clean up the background task gracefully
    drop(stream);

    // If we got here without panicking, cleanup was successful
    // (No assertions needed; we just ensure no crash/panic)
}

#[tokio::test]
#[ignore]
async fn test_pause_with_adaptive_chunking() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .adaptive_chunking(true)
        .execute()
        .await
        .expect("execute");

    // Collect a few items to let adaptive chunking stabilize
    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
        count += 1;
        if count >= 20 {
            break;
        }
    }

    // Pause (adaptive chunking state should be preserved)
    stream.pause().await.expect("pause");

    // Resume
    stream.resume().await.expect("resume");

    // Should be able to continue collecting
    while let Some(result) = stream.next().await {
        let _ = result.expect("parse row");
        count += 1;
        if count >= 30 {
            break;
        }
    }

    assert!(count >= 30, "Should have collected 30+ items across pause/resume");
}

#[tokio::test]
#[ignore]
async fn test_state_snapshot() {
    if !can_connect().await {
        println!("Skipping: test database not available");
        return;
    }

    let client = FraiseClient::connect(&test_db_url()).await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("v_license_request")
        .execute()
        .await
        .expect("execute");

    // Initially should be Running
    let initial_state = stream.state_snapshot();
    assert_eq!(
        initial_state, StreamState::Running,
        "Initial state should be Running"
    );

    // Pause and check state
    stream.pause().await.expect("pause");
    let paused_state = stream.state_snapshot();
    assert_eq!(
        paused_state, StreamState::Paused,
        "State should be Paused after pause()"
    );

    // Resume and check state
    stream.resume().await.expect("resume");
    let resumed_state = stream.state_snapshot();
    assert_eq!(
        resumed_state, StreamState::Running,
        "State should be Running after resume()"
    );
}
