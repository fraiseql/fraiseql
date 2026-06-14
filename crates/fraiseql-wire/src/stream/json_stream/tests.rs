#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::unreadable_literal)] // Reason: byte-count constants in fixture assertions read more naturally without separators
#![allow(clippy::indexing_slicing)] // Reason: test fixtures index into known-shape collections; OOB indices correctly fail the test
use super::*;

#[test]
fn test_extract_json_bytes() {
    let data = Bytes::from_static(b"{\"key\":\"value\"}");
    let msg = BackendMessage::DataRow(vec![Some(data.clone())]);

    let extracted = extract_json_bytes(&msg).unwrap();
    assert_eq!(extracted, data);
}

#[test]
fn test_extract_null_field() {
    let msg = BackendMessage::DataRow(vec![None]);
    let result = extract_json_bytes(&msg);
    assert!(
        matches!(result, Err(WireError::Protocol(_))),
        "expected Protocol error for null field, got: {result:?}"
    );
}

#[test]
fn test_parse_json() {
    let data = Bytes::from_static(b"{\"key\":\"value\"}");
    let value = parse_json(data).unwrap();

    assert_eq!(value["key"], "value");
}

#[test]
fn test_parse_invalid_json() {
    let data = Bytes::from_static(b"not json");
    let result = parse_json(data);
    assert!(
        matches!(result, Err(WireError::JsonDecode(_))),
        "expected JsonDecode error for invalid JSON, got: {result:?}"
    );
}

#[test]
fn test_stream_stats_creation() {
    let stats = StreamStats::zero();
    assert_eq!(stats.items_buffered, 0);
    assert_eq!(stats.estimated_memory, 0);
    assert_eq!(stats.total_rows_yielded, 0);
    assert_eq!(stats.total_rows_filtered, 0);
}

#[test]
fn test_stream_stats_memory_estimation() {
    let stats = StreamStats {
        items_buffered: 100,
        estimated_memory: 100 * 2048,
        total_rows_yielded: 100,
        total_rows_filtered: 10,
    };

    // 100 items * 2KB per item = 200KB
    assert_eq!(stats.estimated_memory, 204800);
}

#[test]
fn test_stream_stats_clone() {
    let stats = StreamStats {
        items_buffered: 50,
        estimated_memory: 100000,
        total_rows_yielded: 500,
        total_rows_filtered: 50,
    };

    let cloned = stats.clone();
    assert_eq!(cloned.items_buffered, stats.items_buffered);
    assert_eq!(cloned.estimated_memory, stats.estimated_memory);
    assert_eq!(cloned.total_rows_yielded, stats.total_rows_yielded);
    assert_eq!(cloned.total_rows_filtered, stats.total_rows_filtered);
}

#[test]
fn test_stream_state_constants() {
    // Verify state constants are distinct
    assert_ne!(STATE_RUNNING, STATE_PAUSED);
    assert_ne!(STATE_RUNNING, STATE_COMPLETED);
    assert_ne!(STATE_RUNNING, STATE_FAILED);
    assert_ne!(STATE_PAUSED, STATE_COMPLETED);
    assert_ne!(STATE_PAUSED, STATE_FAILED);
    assert_ne!(STATE_COMPLETED, STATE_FAILED);
}

#[test]
fn test_stream_state_enum_equality() {
    assert_eq!(StreamState::Running, StreamState::Running);
    assert_eq!(StreamState::Paused, StreamState::Paused);
    assert_eq!(StreamState::Completed, StreamState::Completed);
    assert_eq!(StreamState::Failed, StreamState::Failed);
    assert_ne!(StreamState::Running, StreamState::Paused);
}

// ── H43: pause/resume infrastructure is eagerly wired to the reader ────────
//
// These build a `JsonStream` directly (no background reader) and exercise the
// exact handles the reader receives. They are deterministic and DB-free; the
// end-to-end socket behaviour is covered by the integration leg.

fn test_stream(channel_cap: usize) -> (mpsc::Sender<Result<Value>>, JsonStream) {
    let (tx, rx) = mpsc::channel::<Result<Value>>(channel_cap);
    // No background reader in these unit tests, so the cancel receiver is unused
    // and may drop immediately; the stream only holds the (never-fired) sender.
    let (cancel_tx, _cancel_rx) = mpsc::channel::<()>(1);
    let stream = JsonStream::new(rx, cancel_tx, "ent".to_string(), None, None, None);
    (tx, stream)
}

#[tokio::test]
async fn paused_occupancy_reflects_buffered_rows() {
    let (tx, mut stream) = test_stream(16);
    for i in 0..5 {
        tx.send(Ok(serde_json::json!({ "i": i }))).await.unwrap();
    }

    stream.pause().await.unwrap();

    // Before the fix `paused_occupancy` was never recorded and always returned 0.
    assert_eq!(
        stream.paused_occupancy(),
        5,
        "paused_occupancy must reflect the rows buffered at pause time"
    );
}

#[tokio::test]
async fn reader_state_handle_tracks_pause_and_resume() {
    let (_tx, mut stream) = test_stream(4);

    // `clone_state` returns the exact handle the background reader holds. Before
    // the eager-allocation fix the reader captured `None`, so pause()/resume()
    // never reached it.
    let reader_state = stream.clone_state();
    assert_eq!(*reader_state.lock().await, StreamState::Running);

    stream.pause().await.unwrap();
    assert_eq!(
        *reader_state.lock().await,
        StreamState::Paused,
        "the reader's state handle must observe pause()"
    );

    stream.resume().await.unwrap();
    assert_eq!(
        *reader_state.lock().await,
        StreamState::Running,
        "the reader's state handle must observe resume()"
    );
}

#[tokio::test]
async fn resume_signal_wakes_a_parked_reader() {
    let (_tx, mut stream) = test_stream(4);
    stream.pause().await.unwrap();

    let resume = stream.clone_resume_signal();
    let waiter = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(2), resume.notified()).await
    });

    tokio::task::yield_now().await;
    stream.resume().await.unwrap();

    let woke = waiter.await.unwrap();
    assert!(
        woke.is_ok(),
        "resume() must wake a reader parked on the shared resume signal"
    );
}

#[tokio::test]
async fn stats_track_yielded_rows() {
    use futures::StreamExt as _;

    let (tx, mut stream) = test_stream(16);
    for i in 0..3 {
        tx.send(Ok(serde_json::json!({ "i": i }))).await.unwrap();
    }
    drop(tx); // close the channel so the stream terminates

    let mut yielded = 0;
    while let Some(item) = stream.next().await {
        item.unwrap();
        yielded += 1;
    }

    assert_eq!(yielded, 3);
    assert_eq!(
        stream.stats().total_rows_yielded,
        3,
        "stats().total_rows_yielded must count rows handed to the consumer"
    );
}

#[tokio::test]
async fn query_stream_stats_track_filtered_rows() {
    use crate::stream::QueryStream;
    use futures::StreamExt as _;

    let (tx, inner) = test_stream(16);
    for i in 0..4 {
        tx.send(Ok(serde_json::json!({ "keep": i % 2 == 0 })))
            .await
            .unwrap();
    }
    drop(tx);

    // Keep only objects with keep == true (2 of the 4).
    let predicate: Box<dyn Fn(&Value) -> bool + Send> =
        Box::new(|v: &Value| v.get("keep").and_then(serde_json::Value::as_bool) == Some(true));
    let mut query_stream: QueryStream<Value> = QueryStream::new(inner, Some(predicate));

    let mut kept = 0;
    while let Some(item) = query_stream.next().await {
        item.unwrap();
        kept += 1;
    }

    assert_eq!(kept, 2);
    assert_eq!(
        query_stream.stats().total_rows_filtered,
        2,
        "stats().total_rows_filtered must count rows rejected by the predicate"
    );
}

#[test]
fn set_pause_timeout_updates_the_shared_handle() {
    let (_tx, mut stream) = test_stream(1);

    // The reader reads this handle live; before the fix the timeout was captured
    // by value at spawn time, so `set_pause_timeout` was a dead no-op.
    let reader_timeout = stream.clone_pause_timeout();
    assert_eq!(
        reader_timeout.load(Ordering::Relaxed),
        0,
        "no auto-resume timeout by default"
    );

    stream.set_pause_timeout(Duration::from_millis(250));
    assert_eq!(
        reader_timeout.load(Ordering::Relaxed),
        250,
        "set_pause_timeout must update the handle the reader reads"
    );

    stream.clear_pause_timeout();
    assert_eq!(
        reader_timeout.load(Ordering::Relaxed),
        0,
        "clear_pause_timeout must reset the shared handle"
    );
}
