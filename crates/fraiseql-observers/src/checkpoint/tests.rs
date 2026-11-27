#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod checkpoint_tests {
    use std::sync::Arc;

    use crate::checkpoint::*;

    #[test]
    fn test_checkpoint_state_default() {
        let state = CheckpointState::default();
        assert_eq!(state.last_processed_id, 0);
        assert_eq!(state.batch_size, DEFAULT_BATCH_SIZE);
        assert_eq!(state.event_count, 0);
        assert!(state.listener_id.is_empty(), "{:?}", state.listener_id);
    }

    #[test]
    fn test_checkpoint_state_serialization() {
        use fraiseql_time::Timestamp;

        let state = CheckpointState {
            listener_id:       "test-listener".to_string(),
            last_processed_id: 1000,
            last_processed_at: Timestamp::now(),
            batch_size:        50,
            event_count:       50,
        };

        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: CheckpointState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.listener_id, state.listener_id);
        assert_eq!(deserialized.last_processed_id, state.last_processed_id);
        assert_eq!(deserialized.batch_size, state.batch_size);
        assert_eq!(deserialized.event_count, state.event_count);
    }

    // ── InMemoryCheckpointStore ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_load_save_round_trip() {
        use fraiseql_time::Timestamp;

        let store = InMemoryCheckpointStore::new_silent();
        assert!(store.load("l1").await.unwrap().is_none());

        let state = CheckpointState {
            listener_id:       "l1".to_string(),
            last_processed_id: 42,
            last_processed_at: Timestamp::now(),
            batch_size:        10,
            event_count:       10,
        };
        store.save("l1", &state).await.unwrap();

        let loaded = store.load("l1").await.unwrap().unwrap();
        assert_eq!(loaded.last_processed_id, 42);
    }

    #[tokio::test]
    async fn test_in_memory_delete() {
        use fraiseql_time::Timestamp;

        let store = InMemoryCheckpointStore::new_silent();
        let state = CheckpointState {
            listener_id:       "l1".to_string(),
            last_processed_id: 1,
            last_processed_at: Timestamp::now(),
            batch_size:        0,
            event_count:       0,
        };
        store.save("l1", &state).await.unwrap();
        store.delete("l1").await.unwrap();
        assert!(store.load("l1").await.unwrap().is_none());
    }

    /// CAS edge-case: first-ever save with `expected_id` == 0 and no entry → succeeds.
    #[tokio::test]
    async fn test_in_memory_cas_first_checkpoint() {
        let store = InMemoryCheckpointStore::new_silent();
        let ok = store.compare_and_swap("l1", 0, 100).await.unwrap();
        assert!(ok, "first CAS from 0 must succeed when no entry exists");

        let loaded = store.load("l1").await.unwrap().unwrap();
        assert_eq!(loaded.last_processed_id, 100);
    }

    /// CAS with wrong `expected_id` when no entry exists → fails.
    #[tokio::test]
    async fn test_in_memory_cas_wrong_expected_no_entry() {
        let store = InMemoryCheckpointStore::new_silent();
        let ok = store.compare_and_swap("l1", 50, 100).await.unwrap();
        assert!(!ok, "CAS with non-zero expected_id when entry absent must fail");
    }

    /// Normal CAS progression.
    #[tokio::test]
    async fn test_in_memory_cas_progression() {
        let store = InMemoryCheckpointStore::new_silent();
        assert!(store.compare_and_swap("l1", 0, 10).await.unwrap());
        assert!(store.compare_and_swap("l1", 10, 20).await.unwrap());
        // Stale expected → fails.
        assert!(!store.compare_and_swap("l1", 10, 30).await.unwrap());
        // Correct expected → succeeds.
        assert!(store.compare_and_swap("l1", 20, 30).await.unwrap());

        let loaded = store.load("l1").await.unwrap().unwrap();
        assert_eq!(loaded.last_processed_id, 30);
    }

    /// Concurrent CAS: exactly one winner among N tasks.
    #[tokio::test]
    async fn test_in_memory_cas_concurrent_one_winner() {
        let store = Arc::new(InMemoryCheckpointStore::new_silent());

        // Seed the initial checkpoint.
        store.compare_and_swap("l1", 0, 0).await.unwrap();

        let tasks: Vec<_> = (1..=16_i64)
            .map(|new_id| {
                let s = store.clone();
                tokio::spawn(async move { s.compare_and_swap("l1", 0, new_id).await.unwrap() })
            })
            .collect();

        let results: Vec<bool> =
            futures::future::join_all(tasks).await.into_iter().map(|r| r.unwrap()).collect();

        assert_eq!(
            results.iter().filter(|&&v| v).count(),
            1,
            "exactly one concurrent CAS must win"
        );
    }
}

#[cfg(feature = "checkpoint")]
mod postgres_tests {
    use crate::checkpoint::postgres::*;

    #[test]
    fn test_checkpoint_store_clone() {
        // Ensure CheckpointStore trait is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<PostgresCheckpointStore>();
    }
}

/// Timestamps are written as stored documents hold them today: the fraction padded to
/// `.120` / `.500`, none at a whole second, and a `Z` suffix. A document read and written
/// again keeps those bytes; webhooks, idempotency keys and JSONB comparisons depend on them.
mod stored_time_format {
    use crate::checkpoint::CheckpointState;

    #[test]
    fn a_checkpoint_keeps_its_stored_time() {
        let doc = serde_json::json!({"listener_id":"l1","last_processed_id":1,"last_processed_at":"2025-11-20T12:00:00.120Z","batch_size":10,"event_count":2});
        let read: CheckpointState =
            serde_json::from_value(doc.clone()).expect("the stored document reads");
        assert_eq!(serde_json::to_value(&read).expect("it writes"), doc);
    }
}
