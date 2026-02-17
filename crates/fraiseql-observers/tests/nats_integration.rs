//! NATS `JetStream` transport integration tests
//!
//! These tests verify the NATS transport implementation against a real NATS server.
//!
//! ## Running Tests
//!
//! 1. Start NATS with `JetStream`:
//!
//!    ```bash
//!    docker run -d --name nats -p 4222:4222 nats:latest -js
//!    ```
//!
//! 2. Run tests:
//!
//!    ```bash
//!    cargo test --test nats_integration --features nats -- --ignored
//!    ```

#![allow(unused_imports)]
#![cfg(feature = "nats")]

use std::time::Duration;

use uuid::Uuid;

#[cfg(test)]
mod nats_tests {
    use fraiseql_observers::{
        event::{EntityEvent, EventKind},
        transport::{
            EventFilter, EventTransport, HealthStatus, NatsConfig, NatsTransport, TransportType,
        },
    };
    use futures::StreamExt;
    use serde_json::json;

    use super::*;

    /// Test `NatsConfig` default values
    #[test]
    fn test_nats_config_defaults() {
        let config = NatsConfig::default();

        assert_eq!(config.url, "nats://localhost:4222");
        assert_eq!(config.stream_name, "fraiseql.entity_changes");
        assert_eq!(config.consumer_name, "observer-default");
        assert_eq!(config.subject_prefix, "entity.change");
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.reconnect_delay_ms, 1000);
        assert_eq!(config.ack_wait_secs, 30);
        assert_eq!(config.retention_max_messages, 1_000_000);
        assert_eq!(config.retention_max_bytes, 1_073_741_824);
    }

    /// Test `NatsConfig` custom values
    #[test]
    fn test_nats_config_custom() {
        let config = NatsConfig {
            url:                    "nats://custom:4222".to_string(),
            stream_name:            "custom.stream".to_string(),
            consumer_name:          "custom-consumer".to_string(),
            subject_prefix:         "custom.prefix".to_string(),
            max_reconnect_attempts: 10,
            reconnect_delay_ms:     2000,
            ack_wait_secs:          60,
            retention_max_messages: 500_000,
            retention_max_bytes:    512_000_000,
        };

        assert_eq!(config.url, "nats://custom:4222");
        assert_eq!(config.stream_name, "custom.stream");
        assert_eq!(config.consumer_name, "custom-consumer");
        assert_eq!(config.subject_prefix, "custom.prefix");
        assert_eq!(config.max_reconnect_attempts, 10);
        assert_eq!(config.reconnect_delay_ms, 2000);
        assert_eq!(config.ack_wait_secs, 60);
        assert_eq!(config.retention_max_messages, 500_000);
        assert_eq!(config.retention_max_bytes, 512_000_000);
    }

    /// Test NATS connection to local server
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_connection() {
        let config = NatsConfig {
            consumer_name: format!("test-connection-{}", Uuid::new_v4()),
            ..Default::default()
        };

        let transport = NatsTransport::new(config).await.expect("Should connect to NATS server");

        assert_eq!(transport.transport_type(), TransportType::Nats);
    }

    /// Test NATS health check
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_health_check() {
        let config = NatsConfig {
            consumer_name: format!("test-health-{}", Uuid::new_v4()),
            ..Default::default()
        };

        let transport = NatsTransport::new(config).await.expect("Should connect to NATS server");

        let health = transport.health_check().await.expect("Health check should succeed");
        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(health.message.is_none());
    }

    /// Test NATS publish and subscribe
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_publish_subscribe() {
        let test_id = Uuid::new_v4();
        let config = NatsConfig {
            stream_name: format!("test-stream-{test_id}"),
            consumer_name: format!("test-consumer-{test_id}"),
            subject_prefix: format!("test.{test_id}"),
            ..Default::default()
        };

        let transport = NatsTransport::new(config).await.expect("Should connect to NATS server");

        // Create test event using the builder pattern
        let event = EntityEvent::new(
            EventKind::Created,
            "User".to_string(),
            Uuid::new_v4(),
            json!({
                "name": "Test User",
                "email": "test@example.com"
            }),
        );

        // Subscribe first
        let filter = EventFilter::default();
        let mut stream = transport.subscribe(filter).await.expect("Subscribe should succeed");

        // Publish event
        transport.publish(event.clone()).await.expect("Publish should succeed");

        // Receive event with timeout
        let received = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("Should receive event within timeout")
            .expect("Stream should not end")
            .expect("Event should be valid");

        assert_eq!(received.entity_type, "User");
        assert_eq!(received.entity_id, event.entity_id);
        assert!(matches!(received.event_type, EventKind::Created));
    }

    /// Test NATS filtering by entity type
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_entity_type_filter() {
        let test_id = Uuid::new_v4();
        let config = NatsConfig {
            stream_name: format!("test-filter-stream-{test_id}"),
            consumer_name: format!("test-filter-consumer-{test_id}"),
            subject_prefix: format!("test.filter.{test_id}"),
            ..Default::default()
        };

        let transport = NatsTransport::new(config).await.expect("Should connect to NATS server");

        // Subscribe with filter for "Product" only
        let filter = EventFilter {
            entity_type: Some("Product".to_string()),
            operation:   None,
            tenant_id:   None,
        };
        let mut stream = transport.subscribe(filter).await.expect("Subscribe should succeed");

        // Publish User event (should be filtered out by subject)
        let user_event = EntityEvent::new(
            EventKind::Created,
            "User".to_string(),
            Uuid::new_v4(),
            json!({"name": "Test User"}),
        );
        transport.publish(user_event).await.expect("Publish should succeed");

        // Publish Product event (should be received)
        let product_event = EntityEvent::new(
            EventKind::Created,
            "Product".to_string(),
            Uuid::new_v4(),
            json!({"name": "Test Product"}),
        );
        transport.publish(product_event.clone()).await.expect("Publish should succeed");

        // Receive Product event
        let received = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("Should receive event within timeout")
            .expect("Stream should not end")
            .expect("Event should be valid");

        assert_eq!(received.entity_type, "Product");
        assert_eq!(received.entity_id, product_event.entity_id);
    }

    /// Test NATS filtering by operation
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_operation_filter() {
        let test_id = Uuid::new_v4();
        let config = NatsConfig {
            stream_name: format!("test-op-filter-stream-{test_id}"),
            consumer_name: format!("test-op-filter-consumer-{test_id}"),
            subject_prefix: format!("test.op.{test_id}"),
            ..Default::default()
        };

        let transport = NatsTransport::new(config).await.expect("Should connect to NATS server");

        // Subscribe with filter for UPDATE operations only
        let filter = EventFilter {
            entity_type: None,
            operation:   Some("UPDATE".to_string()),
            tenant_id:   None,
        };
        let mut stream = transport.subscribe(filter).await.expect("Subscribe should succeed");

        // Publish CREATE event (should be filtered out)
        let create_event = EntityEvent::new(
            EventKind::Created,
            "User".to_string(),
            Uuid::new_v4(),
            json!({"name": "Created User"}),
        );
        transport.publish(create_event).await.expect("Publish should succeed");

        // Publish UPDATE event (should be received)
        let update_event = EntityEvent::new(
            EventKind::Updated,
            "User".to_string(),
            Uuid::new_v4(),
            json!({"name": "New Name"}),
        );
        transport.publish(update_event.clone()).await.expect("Publish should succeed");

        // Receive UPDATE event
        let received = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("Should receive event within timeout")
            .expect("Stream should not end")
            .expect("Event should be valid");

        assert!(matches!(received.event_type, EventKind::Updated));
        assert_eq!(received.entity_id, update_event.entity_id);
    }

    /// Test NATS connection error handling
    #[tokio::test]
    async fn test_nats_connection_error() {
        let config = NatsConfig {
            url: "nats://nonexistent:9999".to_string(),
            ..Default::default()
        };

        let result = NatsTransport::new(config).await;
        assert!(result.is_err(), "Connection to nonexistent server should fail");

        // Check error message contains expected text
        let error_msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("Expected error"),
        };
        assert!(
            error_msg.contains("Failed to connect") || error_msg.contains("connection"),
            "Error message should mention connection failure: {error_msg}"
        );
    }

    /// Test multiple events in sequence
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_multiple_events() {
        let test_id = Uuid::new_v4();
        let config = NatsConfig {
            stream_name: format!("test-multi-stream-{test_id}"),
            consumer_name: format!("test-multi-consumer-{test_id}"),
            subject_prefix: format!("test.multi.{test_id}"),
            ..Default::default()
        };

        let transport = NatsTransport::new(config).await.expect("Should connect to NATS server");

        let filter = EventFilter::default();
        let mut stream = transport.subscribe(filter).await.expect("Subscribe should succeed");

        // Publish multiple events
        let event_count = 10;
        for i in 0..event_count {
            let event = EntityEvent::new(
                EventKind::Created,
                "BatchItem".to_string(),
                Uuid::new_v4(),
                json!({"index": i}),
            );
            transport.publish(event).await.expect("Publish should succeed");
        }

        // Receive all events
        let mut received_count = 0;
        let timeout = Duration::from_secs(10);
        let start = std::time::Instant::now();

        while received_count < event_count && start.elapsed() < timeout {
            if let Ok(Some(result)) =
                tokio::time::timeout(Duration::from_secs(2), stream.next()).await
            {
                if result.is_ok() {
                    received_count += 1;
                }
            }
        }

        assert_eq!(received_count, event_count, "Should receive all {event_count} events");
    }

    /// Test durable consumer recovery
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_durable_consumer() {
        let test_id = Uuid::new_v4();
        let stream_name = format!("test-durable-stream-{test_id}");
        let consumer_name = format!("test-durable-consumer-{test_id}");
        let subject_prefix = format!("test.durable.{test_id}");

        // First connection - publish event
        {
            let config = NatsConfig {
                stream_name: stream_name.clone(),
                consumer_name: consumer_name.clone(),
                subject_prefix: subject_prefix.clone(),
                ..Default::default()
            };

            let transport =
                NatsTransport::new(config).await.expect("Should connect to NATS server");

            let event = EntityEvent::new(
                EventKind::Created,
                "DurableTest".to_string(),
                Uuid::new_v4(),
                json!({"test": "durable"}),
            );

            transport.publish(event).await.expect("Publish should succeed");
        }
        // Transport dropped here, simulating disconnect

        // Small delay to ensure message is persisted
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Second connection - should receive the event from durable consumer
        {
            let config = NatsConfig {
                stream_name,
                consumer_name,
                subject_prefix,
                ..Default::default()
            };

            let transport =
                NatsTransport::new(config).await.expect("Should reconnect to NATS server");

            let filter = EventFilter::default();
            let mut stream = transport.subscribe(filter).await.expect("Subscribe should succeed");

            let received = tokio::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .expect("Should receive event within timeout")
                .expect("Stream should not end")
                .expect("Event should be valid");

            assert_eq!(received.entity_type, "DurableTest");
        }
    }

    /// Test event with user context
    ///
    /// Requires: NATS server running on localhost:4222
    #[tokio::test]
    #[ignore = "requires NATS server - run with: cargo test --test nats_integration --features nats -- --ignored"]
    async fn test_nats_event_with_user_id() {
        let test_id = Uuid::new_v4();
        let config = NatsConfig {
            stream_name: format!("test-user-stream-{test_id}"),
            consumer_name: format!("test-user-consumer-{test_id}"),
            subject_prefix: format!("test.user.{test_id}"),
            ..Default::default()
        };

        let transport = NatsTransport::new(config).await.expect("Should connect to NATS server");

        let filter = EventFilter::default();
        let mut stream = transport.subscribe(filter).await.expect("Subscribe should succeed");

        // Create event with user_id
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        )
        .with_user_id("user-123".to_string());

        transport.publish(event.clone()).await.expect("Publish should succeed");

        let received = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("Should receive event within timeout")
            .expect("Stream should not end")
            .expect("Event should be valid");

        assert_eq!(received.user_id, Some("user-123".to_string()));
        assert_eq!(received.entity_type, "Order");
    }
}
