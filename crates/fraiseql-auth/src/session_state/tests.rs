//! Unit tests for the session-state policy layer over the in-memory backend.
//!
//! The `PostgresSessionStateStore` runs the same policy layer against a real
//! database in `tests/session_state_integration.rs`.
#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use uuid::Uuid;

use super::*;

fn state() -> SessionState {
    SessionState::new(SessionStateBackend::InMemory(InMemorySessionStateStore::new()), 3600)
}

fn sid() -> Uuid {
    Uuid::new_v4()
}

/// A counting summarizer that rolls threads up into `{"summary_of": N}`.
struct CountingSummarizer {
    calls: AtomicUsize,
}

impl CountingSummarizer {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
        })
    }
}

impl Summarizer for CountingSummarizer {
    fn summarize(&self, entries: Vec<SessionStateEntry>) -> SummarizeFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(serde_json::json!({ "summary_of": entries.len() })) })
    }
}

/// A summarizer that always fails — the collapse must leave the thread intact.
struct FailingSummarizer;

impl Summarizer for FailingSummarizer {
    fn summarize(&self, _entries: Vec<SessionStateEntry>) -> SummarizeFuture<'_> {
        Box::pin(async move {
            Err(crate::error::AuthError::Internal {
                message: "summarizer unavailable".to_string(),
            })
        })
    }
}

#[tokio::test]
async fn set_get_roundtrip() {
    let s = state();
    let session = sid();
    s.set(session, "t1", "context", serde_json::json!({"step": 1})).await.unwrap();

    let entry = s.get(session, "t1", "context").await.unwrap().expect("entry present");
    assert_eq!(entry.value, serde_json::json!({"step": 1}));
    assert!(entry.expires_at > entry.updated_at, "TTL stamped in the future");
}

#[tokio::test]
async fn get_respects_expiry() {
    // TTL clamps to 1s minimum; write an already-expired entry directly through
    // the backend to simulate the passage of time.
    let backend = InMemorySessionStateStore::new();
    let session = sid();
    backend
        .set(SessionStateEntry {
            session_id: session,
            thread_id:  "t1".to_string(),
            key:        "old".to_string(),
            value:      serde_json::json!(1),
            updated_at: chrono::Utc::now() - chrono::Duration::hours(2),
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
        })
        .unwrap();
    let s = SessionState::new(SessionStateBackend::InMemory(backend), 3600);

    assert!(
        s.get(session, "t1", "old").await.unwrap().is_none(),
        "an expired entry must be invisible to reads even before eviction runs"
    );
}

#[tokio::test]
async fn delete_and_expire_thread() {
    let s = state();
    let session = sid();
    s.set(session, "t1", "a", serde_json::json!(1)).await.unwrap();
    s.set(session, "t1", "b", serde_json::json!(2)).await.unwrap();
    s.set(session, "t2", "c", serde_json::json!(3)).await.unwrap();

    s.delete(session, "t1", "a").await.unwrap();
    assert!(s.get(session, "t1", "a").await.unwrap().is_none());
    assert!(s.get(session, "t1", "b").await.unwrap().is_some());

    s.expire_thread(session, "t1").await.unwrap();
    assert!(s.list_thread(session, "t1").await.unwrap().is_empty());
    assert_eq!(s.list_thread(session, "t2").await.unwrap().len(), 1, "other threads untouched");
}

#[tokio::test]
async fn sessions_are_isolated() {
    let s = state();
    let (alice, mallory) = (sid(), sid());
    s.set(alice, "t1", "secret", serde_json::json!("alice-context")).await.unwrap();

    assert!(
        s.get(mallory, "t1", "secret").await.unwrap().is_none(),
        "an entry is only visible to its owning session_id"
    );
    assert!(s.list_thread(mallory, "t1").await.unwrap().is_empty());
}

#[tokio::test]
async fn oversized_value_is_refused_loudly() {
    let s = state();
    let big = serde_json::json!("x".repeat(MAX_SESSION_VALUE_BYTES + 1));
    let err = s.set(sid(), "t1", "blob", big).await.expect_err("must refuse");
    assert!(err.to_string().contains("cap is"), "error names the cap: {err}");
}

#[tokio::test]
async fn reserved_summary_key_is_refused() {
    let s = state();
    let err = s
        .set(sid(), "t1", SUMMARY_KEY, serde_json::json!("spoof"))
        .await
        .expect_err("must refuse the reserved key");
    assert!(err.to_string().contains("reserved"), "error explains why: {err}");
}

#[tokio::test]
async fn threshold_collapse_replaces_thread_with_summary() {
    let summarizer = CountingSummarizer::new();
    let s =
        SessionState::new(SessionStateBackend::InMemory(InMemorySessionStateStore::new()), 3600)
            .with_summarizer(summarizer.clone(), 3);
    let session = sid();

    for i in 0..3 {
        s.set(session, "t1", &format!("k{i}"), serde_json::json!(i)).await.unwrap();
    }
    assert_eq!(summarizer.calls.load(Ordering::SeqCst), 0, "at threshold: no collapse yet");
    assert_eq!(s.list_thread(session, "t1").await.unwrap().len(), 3);

    // The 4th write crosses the threshold: the whole thread collapses to _summary.
    s.set(session, "t1", "k3", serde_json::json!(3)).await.unwrap();
    assert_eq!(summarizer.calls.load(Ordering::SeqCst), 1);
    let entries = s.list_thread(session, "t1").await.unwrap();
    assert_eq!(entries.len(), 1, "only the summary remains: {entries:?}");
    assert_eq!(entries[0].key, SUMMARY_KEY);
    assert_eq!(entries[0].value, serde_json::json!({"summary_of": 4}));

    // The summary does not count toward the next threshold: three more writes
    // accumulate alongside it before the next collapse.
    for i in 4..7 {
        s.set(session, "t1", &format!("k{i}"), serde_json::json!(i)).await.unwrap();
    }
    assert_eq!(summarizer.calls.load(Ordering::SeqCst), 1, "3 ordinary entries ≤ threshold");
    assert_eq!(s.list_thread(session, "t1").await.unwrap().len(), 4, "summary + 3 new");
}

#[tokio::test]
async fn without_summarizer_threads_grow_uncollapsed() {
    let s = state();
    let session = sid();
    for i in 0..100 {
        s.set(session, "t1", &format!("k{i}"), serde_json::json!(i)).await.unwrap();
    }
    assert_eq!(s.list_thread(session, "t1").await.unwrap().len(), 100);
}

#[tokio::test]
async fn summarizer_failure_leaves_thread_intact() {
    let s =
        SessionState::new(SessionStateBackend::InMemory(InMemorySessionStateStore::new()), 3600)
            .with_summarizer(Arc::new(FailingSummarizer), 2);
    let session = sid();

    s.set(session, "t1", "k0", serde_json::json!(0)).await.unwrap();
    s.set(session, "t1", "k1", serde_json::json!(1)).await.unwrap();
    let err = s.set(session, "t1", "k2", serde_json::json!(2)).await.expect_err("propagated");
    assert!(err.to_string().contains("summarizer unavailable"));

    // State loss is worse than an oversized thread: all three entries survive.
    assert_eq!(s.list_thread(session, "t1").await.unwrap().len(), 3);
}

#[tokio::test]
async fn evict_expired_removes_only_expired() {
    let backend = InMemorySessionStateStore::new();
    let session = sid();
    let now = chrono::Utc::now();
    backend
        .set(SessionStateEntry {
            session_id: session,
            thread_id:  "t1".to_string(),
            key:        "dead".to_string(),
            value:      serde_json::json!(1),
            updated_at: now - chrono::Duration::hours(2),
            expires_at: now - chrono::Duration::hours(1),
        })
        .unwrap();
    backend
        .set(SessionStateEntry {
            session_id: session,
            thread_id:  "t1".to_string(),
            key:        "live".to_string(),
            value:      serde_json::json!(2),
            updated_at: now,
            expires_at: now + chrono::Duration::hours(1),
        })
        .unwrap();
    let s = SessionState::new(SessionStateBackend::InMemory(backend), 3600);

    assert_eq!(s.evict_expired().await.unwrap(), 1, "exactly the expired entry evicted");
    assert!(s.get(session, "t1", "live").await.unwrap().is_some());
}
