mod transport_mod_tests {
    use super::super::*;

    #[test]
    fn test_event_filter_default() {
        let filter = EventFilter::default();
        assert!(filter.entity_type.is_none());
        assert!(filter.operation.is_none());
        assert!(filter.tenant_id.is_none());
    }

    #[test]
    fn test_health_status_equality() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Degraded);
    }

    #[test]
    fn test_transport_type_equality() {
        assert_eq!(TransportType::PostgresNotify, TransportType::PostgresNotify);
        assert_eq!(TransportType::InMemory, TransportType::InMemory);
        assert_ne!(TransportType::PostgresNotify, TransportType::InMemory);
    }
}

#[cfg(all(feature = "postgres", feature = "nats"))]
#[allow(clippy::unwrap_used)] // Reason: test code
mod bridge_tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::super::bridge::*;
    use crate::error::ObserverError;

    #[test]
    fn test_bridge_config_default() {
        let config = BridgeConfig::default();
        assert_eq!(config.transport_name, "pg_to_nats");
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.poll_interval_secs, 1);
        assert_eq!(config.notify_channel, "fraiseql_events");
    }

    #[test]
    fn test_change_log_entry_to_event_insert() {
        use crate::event::EventKind;

        let entry = ChangeLogEntry {
            pk_entity_change_log: 1,
            id:                   Uuid::new_v4(),
            fk_customer_org:      Some(123),
            fk_contact:           Some(456),
            object_type:          "Order".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          Some(serde_json::json!({"total": 100})),
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.user_id, Some("456".to_string()));
    }

    #[test]
    fn test_change_log_entry_to_event_update() {
        use crate::event::EventKind;

        let entry = ChangeLogEntry {
            pk_entity_change_log: 2,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "UPDATE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Updated);
    }

    #[test]
    fn test_change_log_entry_to_event_delete() {
        use crate::event::EventKind;

        let entry = ChangeLogEntry {
            pk_entity_change_log: 3,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "DELETE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Deleted);
    }

    #[test]
    fn test_change_log_entry_invalid_modification_type() {
        let entry = ChangeLogEntry {
            pk_entity_change_log: 4,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Test".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INVALID".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let result = entry.to_entity_event();
        assert!(
            matches!(result, Err(ObserverError::InvalidConfig { .. })),
            "unknown modification_type must return InvalidConfig, got: {result:?}"
        );
    }

    #[test]
    fn test_postgres_checkpoint_store_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<PostgresCheckpointStore>();
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod in_memory_tests {
    use futures::StreamExt;
    use serde_json::json;
    use uuid::Uuid;

    use super::super::{EventFilter, EventTransport, HealthStatus, TransportType, in_memory::*};
    use crate::event::EventKind;

    #[tokio::test]
    async fn test_in_memory_transport_creation() {
        let transport = InMemoryTransport::new();
        assert_eq!(transport.transport_type(), TransportType::InMemory);
    }

    #[tokio::test]
    async fn test_in_memory_transport_health_check() {
        let transport = InMemoryTransport::new();
        let health = transport.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_in_memory_transport_publish_subscribe() {
        use std::sync::Arc;

        let transport = Arc::new(InMemoryTransport::new());

        // Subscribe to events
        let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

        // Publish an event
        let event = crate::event::EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        );

        let event_id = event.id;
        transport.publish(event).await.unwrap();

        // Receive the event
        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(received.id, event_id);
        assert_eq!(received.entity_type, "Order");
        assert_eq!(received.data["total"], 100);
    }

    #[tokio::test]
    async fn test_in_memory_transport_multiple_events() {
        use std::sync::Arc;

        let transport = Arc::new(InMemoryTransport::new());

        // Subscribe
        let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

        // Publish multiple events and collect their IDs
        let mut event_ids: Vec<Uuid> = Vec::new();
        for i in 0..5 {
            let event = crate::event::EntityEvent::new(
                EventKind::Created,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({"index": i}),
            );
            event_ids.push(event.id); // Store the actual event ID
            transport.publish(event).await.unwrap();
        }

        // Receive all events and verify
        for (i, expected_id) in event_ids.iter().enumerate().take(5) {
            let received = stream.next().await.unwrap().unwrap();
            assert_eq!(received.id, *expected_id);
            assert_eq!(received.data["index"], i);
        }
    }

    #[tokio::test]
    async fn test_in_memory_transport_different_event_kinds() {
        use std::sync::Arc;

        let transport = Arc::new(InMemoryTransport::new());

        let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

        let kinds = vec![EventKind::Created, EventKind::Updated, EventKind::Deleted];

        for kind in &kinds {
            let event = crate::event::EntityEvent::new(
                *kind,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({}),
            );
            transport.publish(event).await.unwrap();
        }

        for expected_kind in &kinds {
            let received = stream.next().await.unwrap().unwrap();
            assert_eq!(received.event_type, *expected_kind);
        }
    }

    #[tokio::test]
    async fn test_in_memory_transport_default() {
        let transport = InMemoryTransport::default();
        assert_eq!(transport.transport_type(), TransportType::InMemory);
    }
}

#[cfg(all(feature = "mssql", feature = "nats"))]
#[allow(clippy::unwrap_used)] // Reason: test code
mod mssql_bridge_tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::super::mssql_bridge::*;
    use crate::error::ObserverError;

    #[test]
    fn test_mssql_bridge_config_default() {
        let config = MSSQLBridgeConfig::default();
        assert_eq!(config.transport_name, "mssql_to_nats");
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.poll_interval_secs, 1);
    }

    #[test]
    fn test_mssql_change_log_entry_to_event_insert() {
        use crate::event::EventKind;

        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 1,
            id:                   Uuid::new_v4(),
            fk_customer_org:      Some(123),
            fk_contact:           Some(456),
            object_type:          "Order".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          Some(serde_json::json!({"total": 100})),
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.user_id, Some("456".to_string()));
    }

    #[test]
    fn test_mssql_change_log_entry_to_event_update() {
        use crate::event::EventKind;

        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 2,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "UPDATE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Updated);
    }

    #[test]
    fn test_mssql_change_log_entry_to_event_delete() {
        use crate::event::EventKind;

        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 3,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "DELETE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Deleted);
    }

    #[test]
    fn test_mssql_change_log_entry_invalid_modification_type() {
        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 4,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Test".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INVALID".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let result = entry.to_entity_event();
        assert!(
            matches!(result, Err(ObserverError::InvalidConfig { .. })),
            "unknown modification_type must return InvalidConfig, got: {result:?}"
        );
    }

    #[test]
    fn test_mssql_checkpoint_store_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<MSSQLCheckpointStore>();
    }

    #[test]
    fn test_mssql_change_log_entry_null_object_data() {
        // A None object_data must be converted to Value::Null in the event.
        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 10,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "UPDATE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };
        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.data, serde_json::Value::Null, "null object_data must yield Value::Null");
    }

    #[test]
    fn test_mssql_change_log_entry_no_contact_produces_no_user_id() {
        // fk_contact = None must produce user_id = None on the event.
        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 11,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "DELETE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };
        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.user_id, None, "no fk_contact must yield user_id = None");
    }

    #[test]
    fn test_mssql_change_log_entry_nats_event_id_preserved() {
        // When nats_event_id is Some, that UUID must be used as the event ID.
        let fixed_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 12,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        Some(fixed_id),
        };
        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.id, fixed_id, "provided nats_event_id must be used as event.id");
    }

    #[test]
    fn test_mssql_case_insensitive_modification_type() {
        use crate::event::EventKind;
        // to_entity_event() normalises via to_uppercase(), so mixed case must work.
        let entry = MSSQLChangeLogEntry {
            pk_entity_change_log: 13,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Inventory".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "insert".to_string(), // lowercase
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };
        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Created);
    }
}

#[cfg(all(feature = "mysql", feature = "nats"))]
#[allow(clippy::unwrap_used)] // Reason: test code
mod mysql_bridge_tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::super::mysql_bridge::*;
    use crate::error::ObserverError;

    #[test]
    fn test_mysql_bridge_config_default() {
        let config = MySQLBridgeConfig::default();
        assert_eq!(config.transport_name, "mysql_to_nats");
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.poll_interval_secs, 1);
    }

    #[test]
    fn test_mysql_change_log_entry_to_event_insert() {
        use crate::event::EventKind;

        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 1,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      Some(123),
            fk_contact:           Some(456),
            object_type:          "Order".to_string(),
            object_id:            Uuid::new_v4().to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          Some(serde_json::json!({"total": 100})),
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.user_id, Some("456".to_string()));
    }

    #[test]
    fn test_mysql_change_log_entry_to_event_update() {
        use crate::event::EventKind;

        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 2,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            Uuid::new_v4().to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Updated);
    }

    #[test]
    fn test_mysql_change_log_entry_to_event_delete() {
        use crate::event::EventKind;

        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 3,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            Uuid::new_v4().to_string(),
            modification_type:    "DELETE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Deleted);
    }

    #[test]
    fn test_mysql_change_log_entry_invalid_modification_type() {
        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 4,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Test".to_string(),
            object_id:            Uuid::new_v4().to_string(),
            modification_type:    "INVALID".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let result = entry.to_entity_event();
        assert!(
            matches!(result, Err(ObserverError::InvalidConfig { .. })),
            "unknown modification_type must return InvalidConfig, got: {result:?}"
        );
    }

    #[test]
    fn test_mysql_checkpoint_store_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<MySQLCheckpointStore>();
    }

    #[test]
    fn test_mysql_change_log_entry_invalid_object_id() {
        // An object_id that is not a valid UUID must return an error.
        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 10,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            "not-a-uuid".to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };
        let result = entry.to_entity_event();
        assert!(result.is_err(), "invalid object_id UUID must return an error");
    }

    #[test]
    fn test_mysql_change_log_entry_null_object_data() {
        // A None object_data must be converted to Value::Null in the event.
        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 11,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            Uuid::new_v4().to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };
        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.data, serde_json::Value::Null, "null object_data must yield Value::Null");
    }

    #[test]
    fn test_mysql_change_log_entry_no_contact_produces_no_user_id() {
        // fk_contact = None must produce user_id = None on the event.
        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 12,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            Uuid::new_v4().to_string(),
            modification_type:    "DELETE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };
        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.user_id, None, "no fk_contact must yield user_id = None");
    }

    #[test]
    fn test_mysql_change_log_entry_nats_event_id_preserved() {
        // When nats_event_id is set to a valid UUID, that UUID is used as the event ID.
        let fixed_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let entry = MySQLChangeLogEntry {
            pk_entity_change_log: 13,
            id:                   Uuid::new_v4().to_string(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            Uuid::new_v4().to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        Some(fixed_id.to_string()),
        };
        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.id, fixed_id, "provided nats_event_id must be used as event.id");
    }
}

#[cfg(feature = "nats")]
mod nats_tests {
    use super::super::nats::*;
    use crate::ssrf::validate_nats_url;

    #[test]
    fn test_nats_config_default() {
        let config = NatsConfig::default();
        assert_eq!(config.url, "nats://localhost:4222");
        assert_eq!(config.stream_name, "fraiseql-entity-changes");
        assert_eq!(config.consumer_name, "observer-default");
        assert_eq!(config.subject_prefix, "entity.change");
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.reconnect_delay_ms, 1000);
        assert_eq!(config.ack_wait_secs, 30);
        assert_eq!(config.retention_max_messages, 1_000_000);
        assert_eq!(config.retention_max_bytes, 1_073_741_824);
    }

    // Note: Integration tests with an embedded NATS server live in the tests/ directory.
    // Unit tests for NatsTransport require a running NATS server and are therefore
    // deferred to integration tests.

    #[test]
    fn validate_nats_url_rejects_loopback() {
        let result = validate_nats_url("nats://127.0.0.1:4222");
        assert!(result.is_err(), "loopback NATS URL must be rejected");
    }

    #[test]
    fn validate_nats_url_rejects_private_ip() {
        let result = validate_nats_url("nats://10.0.0.1:4222");
        assert!(result.is_err(), "private-IP NATS URL must be rejected");
    }

    #[test]
    fn validate_nats_url_rejects_wrong_scheme() {
        let result = validate_nats_url("http://nats.example.com:4222");
        assert!(result.is_err(), "non-nats:// scheme must be rejected");
    }
}

#[cfg(feature = "postgres")]
mod postgres_notify_tests {
    use std::{env, time::Duration};

    use sqlx::postgres::PgPool;

    use super::super::{
        EventFilter, EventTransport, HealthStatus, TransportType, postgres_notify::*,
    };
    use crate::listener::ChangeLogListenerConfig;

    /// Returns `None` if `TEST_DATABASE_URL` is not set, allowing tests to skip gracefully.
    async fn try_test_pool() -> Option<PgPool> {
        let database_url = env::var("TEST_DATABASE_URL").ok()?;
        Some(
            PgPool::connect(&database_url)
                .await
                .expect("Failed to connect to TEST_DATABASE_URL"),
        )
    }

    #[tokio::test]
    async fn test_postgres_transport_creation() {
        let Some(pool) = try_test_pool().await else {
            eprintln!("Skipping: TEST_DATABASE_URL not set");
            return;
        };

        let config = ChangeLogListenerConfig::new(pool);
        let transport = PostgresNotifyTransport::from_config(config);

        assert_eq!(transport.transport_type(), TransportType::PostgresNotify);
    }

    #[tokio::test]
    async fn test_postgres_transport_health_check() {
        let Some(pool) = try_test_pool().await else {
            eprintln!("Skipping: TEST_DATABASE_URL not set");
            return;
        };

        let config = ChangeLogListenerConfig::new(pool);
        let transport = PostgresNotifyTransport::from_config(config);

        let health = transport.health_check().await.expect("health_check should succeed");
        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_postgres_transport_subscribe() {
        let Some(pool) = try_test_pool().await else {
            eprintln!("Skipping: TEST_DATABASE_URL not set");
            return;
        };

        let config = ChangeLogListenerConfig::new(pool).with_poll_interval(50);
        let transport = PostgresNotifyTransport::from_config(config);

        // Verify the stream can be created (won't produce events without data)
        let stream = transport
            .subscribe(EventFilter::default())
            .await
            .expect("subscribe should succeed");
        drop(stream);
    }

    #[tokio::test]
    async fn test_postgres_transport_poll_interval() {
        let Some(pool) = try_test_pool().await else {
            eprintln!("Skipping: TEST_DATABASE_URL not set");
            return;
        };

        let config = ChangeLogListenerConfig::new(pool);
        let transport = PostgresNotifyTransport::from_config(config)
            .with_poll_interval(Duration::from_millis(200));

        assert_eq!(transport.poll_interval, Duration::from_millis(200));
    }
}
