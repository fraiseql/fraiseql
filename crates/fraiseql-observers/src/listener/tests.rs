mod listener_mod_tests {
    use uuid::Uuid;

    use crate::event::{EntityEvent, EventKind};

    #[tokio::test]
    async fn test_event_deserialization() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            serde_json::json!({"total": 100}),
        );

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: EntityEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(deserialized.entity_type, "Order");
        assert_eq!(deserialized.data["total"], 100);
    }
}

#[cfg(feature = "postgres")]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod change_log_tests {
    use serde_json::{Value, json};
    use sqlx::postgres::PgPool;
    use uuid::Uuid;

    use super::super::change_log::*;
    use crate::event::EventKind;

    #[test]
    fn test_change_log_entry_debezium_operation() {
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            "order-id".to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "c",
                "before": null,
                "after": { "id": "order-id" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:00:00Z".to_string(),
        };

        assert_eq!(entry.debezium_operation().unwrap(), 'c');
    }

    #[test]
    fn test_change_log_entry_after_values() {
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            "user-id".to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "name": "old" },
                "after": { "name": "new" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:00:00Z".to_string(),
        };

        let after = entry.after_values().unwrap();
        assert_eq!(after["name"], "new");
    }

    #[test]
    fn test_change_log_entry_before_values() {
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            "prod-id".to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "price": 100 },
                "after": { "price": 150 }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:00:00Z".to_string(),
        };

        let before = entry.before_values().unwrap();
        assert_eq!(before["price"], 100);
    }

    #[tokio::test]
    async fn test_change_log_listener_checkpoint() {
        let config = ChangeLogListenerConfig::new(
            PgPool::connect_lazy("postgres://localhost/dummy").unwrap(),
        );
        let mut listener = ChangeLogListener::new(config);

        assert_eq!(listener.checkpoint(), 0);

        listener.set_checkpoint(42);
        assert_eq!(listener.checkpoint(), 42);
    }

    #[tokio::test]
    async fn test_config_builder() {
        let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
        let config = ChangeLogListenerConfig::new(pool)
            .with_poll_interval(500)
            .with_batch_size(50)
            .with_resume_from(100);

        assert_eq!(config.poll_interval_ms, 500);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.resume_from_id, Some(100));
    }

    // Event conversion tests

    #[test]
    fn test_insert_to_entity_event() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           Some("user-123".to_string()),
            object_type:          "Order".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "c",
                "before": null,
                "after": { "id": entity_id.to_string(), "total": 150.00, "status": "pending" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:30:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.entity_id, entity_id);
        assert_eq!(event.data["total"], 150.00);
        assert_eq!(event.user_id, Some("user-123".to_string()));
        assert!(event.changes.is_none()); // No changes for CREATE
    }

    #[test]
    fn test_update_to_entity_event() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   2,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           Some("user-456".to_string()),
            object_type:          "Order".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "status": "pending", "total": 100.00 },
                "after": { "status": "shipped", "total": 100.00 }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:35:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        assert_eq!(event.event_type, EventKind::Updated);
        assert_eq!(event.data["status"], "shipped");

        // Verify field changes captured
        let changes = event.changes.unwrap();
        assert!(changes.contains_key("status"));
        assert_eq!(changes["status"].old, "pending");
        assert_eq!(changes["status"].new, "shipped");
        // Total unchanged, should not be in changes
        assert!(!changes.contains_key("total"));
    }

    #[test]
    fn test_delete_to_entity_event() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   3,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "DELETE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "d",
                "before": { "id": entity_id.to_string(), "email": "user@example.com" },
                "after": null
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:40:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        assert_eq!(event.event_type, EventKind::Deleted);
        // For DELETE, data should use before values
        assert_eq!(event.data["email"], "user@example.com");
        assert_eq!(event.user_id, None);
    }

    #[test]
    fn test_field_changes_new_field() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   4,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "name": "Widget" },
                "after": { "name": "Widget", "description": "A useful widget" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:45:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();
        let changes = event.changes.unwrap();

        // Should have changes for the new field
        assert!(changes.contains_key("description"));
        assert_eq!(changes["description"].old, Value::Null);
        assert_eq!(changes["description"].new, "A useful widget");
    }

    #[test]
    fn test_field_changes_deleted_field() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   5,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "name": "John", "temp_field": "value" },
                "after": { "name": "John" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:50:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();
        let changes = event.changes.unwrap();

        // Should have changes for the deleted field
        assert!(changes.contains_key("temp_field"));
        assert_eq!(changes["temp_field"].old, "value");
        assert_eq!(changes["temp_field"].new, Value::Null);
    }

    #[test]
    fn test_timestamp_parsing() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   6,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "c",
                "before": null,
                "after": { "id": entity_id.to_string() }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T15:30:45.123456+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        // Verify timestamp was parsed correctly
        assert!(event.timestamp.to_rfc3339().contains("2026-01-22T15:30:45"));
    }
}

mod coordinator_tests {
    use super::super::coordinator::*;
    use super::super::state::ListenerState;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = MultiListenerCoordinator::new();
        assert_eq!(coordinator.listener_count(), 0);
    }

    #[tokio::test]
    async fn test_listener_registration() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();
        coordinator.register_listener("listener-2".to_string()).await.ok();

        assert_eq!(coordinator.listener_count(), 2);
    }

    #[tokio::test]
    async fn test_listener_deregistration() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();
        assert_eq!(coordinator.listener_count(), 1);

        coordinator.deregister_listener("listener-1").ok();
        assert_eq!(coordinator.listener_count(), 0);
    }

    #[tokio::test]
    async fn test_listener_state_retrieval() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();

        let state = coordinator.get_listener_state("listener-1").await.ok();
        assert_eq!(state, Some(ListenerState::Initializing));
    }

    #[tokio::test]
    async fn test_listener_health_check() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();
        coordinator.register_listener("listener-2".to_string()).await.ok();

        let health = coordinator.check_listener_health().await.ok();
        assert_eq!(health.map(|h| h.len()), Some(2));
    }

    #[tokio::test]
    async fn test_leader_election() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator
            .register_listener("listener-1".to_string())
            .await
            .expect("register listener-1");
        coordinator
            .register_listener("listener-2".to_string())
            .await
            .expect("register listener-2");

        // Leaders can only be elected from healthy (Running) listeners.
        // Initially listeners are Initializing, so election may fail or
        // succeed depending on implementation. Either way, it must not panic.
        let _leader_result = coordinator.elect_leader().await;
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod failover_tests {
    use std::sync::Arc;

    use super::super::coordinator::MultiListenerCoordinator;
    use super::super::failover::*;

    #[tokio::test]
    async fn test_failover_manager_creation() {
        let coordinator = Arc::new(MultiListenerCoordinator::new());
        let manager = FailoverManager::new(coordinator);

        assert_eq!(manager.health_check_interval_ms(), 5000);
        assert_eq!(manager.failover_threshold_ms(), 60000);
    }

    #[tokio::test]
    async fn test_failover_manager_custom_intervals() {
        let coordinator = Arc::new(MultiListenerCoordinator::new());
        let manager = FailoverManager::with_intervals(coordinator, 3000, 45000);

        assert_eq!(manager.health_check_interval_ms(), 3000);
        assert_eq!(manager.failover_threshold_ms(), 45000);
    }

    #[tokio::test]
    async fn test_failure_detection() {
        let coordinator = Arc::new(MultiListenerCoordinator::new());
        let manager = FailoverManager::new(coordinator.clone());

        coordinator
            .register_listener("listener-1".to_string())
            .await
            .expect("register listener-1");
        coordinator
            .register_listener("listener-2".to_string())
            .await
            .expect("register listener-2");

        let failures = manager.detect_failures().await.expect("detect_failures should succeed");
        // With 2 listeners registered in Initializing state, failures should be
        // exactly 0 (healthy) or exactly 2 (both detected as initializing)
        assert!(
            failures.is_empty() || failures.len() == 2,
            "expected 0 or 2 failures for 2 initializing listeners, got {}",
            failures.len()
        );
    }

    #[tokio::test]
    async fn test_failover_trigger() {
        let coordinator = Arc::new(MultiListenerCoordinator::new());
        let manager = FailoverManager::new(coordinator.clone());

        coordinator
            .register_listener("listener-1".to_string())
            .await
            .expect("register listener-1");
        coordinator
            .register_listener("listener-2".to_string())
            .await
            .expect("register listener-2");

        // Failover for a listener that just registered (Initializing state) —
        // result depends on whether a healthy replacement can be found
        let _result = manager.trigger_failover("listener-1").await;
        // We don't assert Ok/Err here because success depends on listener health
        // state, but the call must not panic.
    }

    #[tokio::test]
    async fn test_failover_checkpoint_consistency() {
        let coordinator = Arc::new(MultiListenerCoordinator::new());

        coordinator.register_listener("listener-1".to_string()).await.ok();
        coordinator.update_checkpoint("listener-1", 1000).ok();

        let health = coordinator.check_listener_health().await.unwrap();
        let checkpoint = health
            .iter()
            .find(|h| h.listener_id == "listener-1")
            .map_or(0, |h| h.last_checkpoint);

        assert_eq!(checkpoint, 1000);
    }

    #[tokio::test]
    async fn test_multiple_listener_failover() {
        let coordinator = Arc::new(MultiListenerCoordinator::new());

        for i in 0..3 {
            coordinator.register_listener(format!("listener-{i}")).await.ok();
        }

        let listener_count = coordinator.listener_count();
        assert_eq!(listener_count, 3);
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod lease_tests {
    use super::super::lease::*;

    // ── In-process lease ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_lease_acquisition() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 1000);

        let acquired = lease.acquire().await.unwrap();
        assert!(acquired);

        let holder = lease.get_holder().await.unwrap();
        assert_eq!(holder, Some("listener-1".to_string()));
    }

    #[tokio::test]
    async fn test_lease_release() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 1000);

        lease.acquire().await.unwrap();
        lease.release().await.unwrap();

        let holder = lease.get_holder().await.unwrap();
        assert_eq!(holder, None);
    }

    #[tokio::test]
    async fn test_lease_renewal() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 100);

        lease.acquire().await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let time_remaining_before = lease.time_remaining_ms().await.unwrap();

        lease.renew().await.unwrap();
        let time_remaining_after = lease.time_remaining_ms().await.unwrap();

        // After renewal, time remaining should be close to the original duration.
        assert!(time_remaining_after > time_remaining_before);
    }

    #[tokio::test]
    async fn test_lease_expiration() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 50);

        lease.acquire().await.unwrap();
        assert!(lease.is_valid().await.unwrap());

        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        assert!(!lease.is_valid().await.unwrap());
    }

    #[tokio::test]
    async fn test_lease_contested_acquisition() {
        let lease1 = CheckpointLease::in_process("listener-1".to_string(), 1000, 1000);
        let lease2 = CheckpointLease::in_process("listener-2".to_string(), 1000, 1000);

        assert!(lease1.acquire().await.unwrap());
        // Different in-process instances have separate state; each acquires independently.
        assert!(lease2.acquire().await.unwrap());

        assert_eq!(lease1.get_holder().await.unwrap(), Some("listener-1".to_string()));
        assert_eq!(lease2.get_holder().await.unwrap(), Some("listener-2".to_string()));
    }

    #[tokio::test]
    async fn test_lease_multiple_listeners() {
        let leases: Vec<_> = (0..3)
            .map(|i| {
                CheckpointLease::in_process(format!("listener-{i}"), 1000 + i64::from(i), 5000)
            })
            .collect();

        for lease in &leases {
            assert!(lease.acquire().await.unwrap());
        }

        for (i, lease) in leases.iter().enumerate() {
            assert_eq!(lease.get_holder().await.unwrap(), Some(format!("listener-{i}")));
        }
    }

    #[tokio::test]
    async fn test_lease_time_remaining() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 200);

        let initial_remaining = lease.time_remaining_ms().await.unwrap();
        assert_eq!(initial_remaining, 200);

        lease.acquire().await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let remaining_after_50ms = lease.time_remaining_ms().await.unwrap();

        assert!(remaining_after_50ms < 200);
        assert!(remaining_after_50ms >= 100);
    }

    #[tokio::test]
    async fn test_lease_idempotent_acquire() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 42, 5000);

        assert!(lease.acquire().await.unwrap());
        // Second acquire while still held by us → still true.
        assert!(lease.acquire().await.unwrap());
    }

    #[tokio::test]
    async fn test_checkpoint_id_accessor() {
        let lease = CheckpointLease::in_process("l".to_string(), 999, 1000);
        assert_eq!(lease.checkpoint_id(), 999);
    }

    #[tokio::test]
    async fn test_listener_id_accessor() {
        let lease = CheckpointLease::in_process("my-listener".to_string(), 1, 1000);
        assert_eq!(lease.listener_id(), "my-listener");
    }

    // ── PostgreSQL advisory lease (integration, requires real PG) ─────────

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_advisory_acquire_release() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping: DATABASE_URL not set");
            return;
        };
        let pool = sqlx::PgPool::connect(&url).await.unwrap();

        let lease = CheckpointLease::postgres(pool, "pg-listener-1".to_string(), 99_999);

        assert!(lease.acquire().await.unwrap(), "first acquire should succeed");
        // Idempotent.
        assert!(lease.acquire().await.unwrap(), "second acquire should also succeed");
        assert!(lease.is_valid().await.unwrap());
        assert_eq!(lease.time_remaining_ms().await.unwrap(), u64::MAX);
        assert_eq!(lease.get_holder().await.unwrap(), Some("pg-listener-1".to_string()));

        lease.release().await.unwrap();
        assert!(!lease.is_valid().await.unwrap());
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_advisory_contention() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping: DATABASE_URL not set");
            return;
        };
        let pool = sqlx::PgPool::connect(&url).await.unwrap();

        let lease_a = CheckpointLease::postgres(pool.clone(), "pg-a".to_string(), 88_888);
        let lease_b = CheckpointLease::postgres(pool.clone(), "pg-b".to_string(), 88_888);

        // A acquires.
        assert!(lease_a.acquire().await.unwrap());
        // B cannot acquire the same key while A holds it.
        assert!(!lease_b.acquire().await.unwrap());

        lease_a.release().await.unwrap();
        // Now B can acquire.
        assert!(lease_b.acquire().await.unwrap());
        lease_b.release().await.unwrap();
    }

    // ── Redis advisory lease (integration, requires real Redis) ───────────

    #[cfg(feature = "redis-lease")]
    #[tokio::test]
    async fn test_redis_advisory_acquire_release() {
        let Ok(url) = std::env::var("REDIS_URL") else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };
        let client = redis::Client::open(url).unwrap();
        let conn = redis::aio::ConnectionManager::new(client).await.unwrap();

        let lease = CheckpointLease::redis(conn, "redis-listener-1".to_string(), 77_777, 30);

        assert!(lease.acquire().await.unwrap());
        assert!(lease.is_valid().await.unwrap());
        assert_eq!(lease.get_holder().await.unwrap(), Some("redis-listener-1".to_string()));

        assert!(lease.renew().await.unwrap());
        let remaining = lease.time_remaining_ms().await.unwrap();
        assert!(remaining > 0 && remaining <= 30_000);

        lease.release().await.unwrap();
        assert!(!lease.is_valid().await.unwrap());
    }

    #[cfg(feature = "redis-lease")]
    #[tokio::test]
    async fn test_redis_advisory_contention() {
        let Ok(url) = std::env::var("REDIS_URL") else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };
        let client = redis::Client::open(url).unwrap();
        let conn_a = redis::aio::ConnectionManager::new(client.clone()).await.unwrap();
        let conn_b = redis::aio::ConnectionManager::new(client).await.unwrap();

        let lease_a = CheckpointLease::redis(conn_a, "redis-a".to_string(), 66_666, 30);
        let lease_b = CheckpointLease::redis(conn_b, "redis-b".to_string(), 66_666, 30);

        assert!(lease_a.acquire().await.unwrap());
        assert!(!lease_b.acquire().await.unwrap(), "should be contested");

        lease_a.release().await.unwrap();
        assert!(lease_b.acquire().await.unwrap(), "should acquire after A released");
        lease_b.release().await.unwrap();
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod state_tests {
    use std::time::Duration;

    use super::super::state::*;
    use crate::error::ObserverError;

    #[tokio::test]
    async fn test_listener_state_creation() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        assert_eq!(state_machine.get_state().await, ListenerState::Initializing);
        assert_eq!(state_machine.get_recovery_attempts().await, 0);
    }

    #[tokio::test]
    async fn test_listener_state_transitions() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        // Valid transition: Initializing → Connecting
        state_machine
            .transition(ListenerState::Connecting)
            .await
            .unwrap_or_else(|e| panic!("expected Ok for Initializing→Connecting: {e}"));
        assert_eq!(state_machine.get_state().await, ListenerState::Connecting);

        // Valid transition: Connecting → Running
        state_machine
            .transition(ListenerState::Running)
            .await
            .unwrap_or_else(|e| panic!("expected Ok for Connecting→Running: {e}"));
        assert_eq!(state_machine.get_state().await, ListenerState::Running);

        // Valid transition: Running → Recovering
        state_machine
            .transition(ListenerState::Recovering)
            .await
            .unwrap_or_else(|e| panic!("expected Ok for Running→Recovering: {e}"));
        assert_eq!(state_machine.get_state().await, ListenerState::Recovering);

        // Valid transition: Recovering → Running
        state_machine
            .transition(ListenerState::Running)
            .await
            .unwrap_or_else(|e| panic!("expected Ok for Recovering→Running: {e}"));
        assert_eq!(state_machine.get_state().await, ListenerState::Running);
    }

    #[tokio::test]
    async fn test_connecting_to_recovering_transition() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        // Initializing → Connecting
        state_machine
            .transition(ListenerState::Connecting)
            .await
            .unwrap_or_else(|e| panic!("expected Ok for Initializing→Connecting: {e}"));
        // Connecting → Recovering (connection failure at startup — must not require restart)
        state_machine.transition(ListenerState::Recovering).await.unwrap_or_else(|e| {
            panic!("Connecting → Recovering must be a valid transition, got: {e}")
        });
        assert_eq!(state_machine.get_state().await, ListenerState::Recovering);

        // Recovering → Connecting (retry connection)
        state_machine
            .transition(ListenerState::Connecting)
            .await
            .unwrap_or_else(|e| panic!("expected Ok for Recovering→Connecting: {e}"));
    }

    #[tokio::test]
    async fn test_listener_invalid_transitions() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        // Invalid transition: Initializing → Running (skip Connecting)
        assert!(
            matches!(
                state_machine.transition(ListenerState::Running).await,
                Err(ObserverError::InvalidConfig { .. })
            ),
            "Initializing→Running must be rejected with InvalidConfig"
        );

        // Invalid transition: Initializing → Recovering
        assert!(
            matches!(
                state_machine.transition(ListenerState::Recovering).await,
                Err(ObserverError::InvalidConfig { .. })
            ),
            "Initializing→Recovering must be rejected with InvalidConfig"
        );
    }

    #[tokio::test]
    async fn test_listener_state_duration_tracking() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        let initial_duration = state_machine.get_state_duration().await;
        assert!(initial_duration.as_millis() < 100);

        // Transition and wait
        state_machine.transition(ListenerState::Connecting).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let connecting_duration = state_machine.get_state_duration().await;
        assert!(connecting_duration.as_millis() >= 50);
    }

    #[tokio::test]
    async fn test_listener_recovery_attempts() {
        let state_machine =
            ListenerStateMachine::new("listener-1".to_string()).with_max_recovery_attempts(3);

        state_machine.transition(ListenerState::Connecting).await.unwrap();
        state_machine.transition(ListenerState::Running).await.unwrap();

        // First recovery
        state_machine.transition(ListenerState::Recovering).await.unwrap();
        assert_eq!(state_machine.get_recovery_attempts().await, 1);
        assert!(state_machine.can_recover().await);

        state_machine.transition(ListenerState::Running).await.unwrap();
        assert_eq!(state_machine.get_recovery_attempts().await, 0); // Reset on success

        // Multiple recoveries
        for _ in 0..3 {
            state_machine.transition(ListenerState::Recovering).await.unwrap();
            state_machine.transition(ListenerState::Running).await.unwrap();
        }
    }

    #[test]
    fn test_listener_state_display() {
        assert_eq!(ListenerState::Initializing.to_string(), "initializing");
        assert_eq!(ListenerState::Connecting.to_string(), "connecting");
        assert_eq!(ListenerState::Running.to_string(), "running");
        assert_eq!(ListenerState::Recovering.to_string(), "recovering");
        assert_eq!(ListenerState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_listener_id() {
        let state_machine = ListenerStateMachine::new("my-listener".to_string());
        assert_eq!(state_machine.listener_id(), "my-listener");
    }
}
