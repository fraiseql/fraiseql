#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

//! Tests for the connect-timeout helper (audit L-wire-timeout).

use super::with_connect_timeout;
use crate::{Result, WireError};
use std::time::Duration;

#[tokio::test]
async fn connect_timeout_elapses_to_connection_error() {
    // A connect future that never resolves must be cut off by the timeout rather
    // than hanging — the bug was that `connect_timeout` was never applied.
    let never = async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok::<(), WireError>(())
    };

    let result: Result<()> = with_connect_timeout(Some(Duration::from_millis(20)), never).await;

    match result {
        Err(WireError::Connection(msg)) => {
            assert!(msg.contains("timed out"), "unexpected message: {msg}");
        }
        other => panic!("expected a timeout Connection error, got {other:?}"),
    }
}

#[tokio::test]
async fn no_timeout_passes_the_result_through() {
    let ready = async { Ok::<i32, WireError>(7) };
    let value = with_connect_timeout(None, ready).await.unwrap();
    assert_eq!(value, 7);
}
