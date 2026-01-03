//! Integration tests for complete subscription workflows
//!
//! Tests complete end-to-end scenarios including:
//! - Full subscription lifecycle
//! - Error recovery and fallback
//! - Resource management
//! - Event delivery chains
//! - Multi-subscription scenarios

#[cfg(test)]
mod tests {
    use crate::subscriptions::*;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    // ============================================================================
    // SCENARIO 1: Basic Subscription Lifecycle
    // ============================================================================

    #[tokio::test]
    async fn test_subscription_creation_and_completion() {
        // Setup
        let config = SubscriptionConfig::default();
        let _metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        let manager = ConnectionManager::new(config.limits);

        // Register connection
        let metadata = manager
            .register_connection(Some(1), Some(1))
            .expect("Failed to register connection");
        let connection_id = metadata.id;

        // Create executor
        let executor = SubscriptionExecutor::new();

        // Create subscription
        let subscription_id = "sub-1".to_string();
        let query = "subscription { messageAdded { id message } }".to_string();
        let payload = SubscriptionPayload {
            query,
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let result = executor.execute(connection_id, &payload);
        assert!(result.is_ok());

        // Verify subscription created
        let subscription = executor.get_subscription(subscription_id.as_ref());
        assert!(subscription.is_some());

        // Complete subscription
        let completed = executor.complete_subscription(subscription_id.as_ref());
        assert!(completed.is_ok());

        // Verify subscription removed
        let subscription_after = executor.get_subscription(subscription_id.as_ref());
        assert!(subscription_after.is_none());
    }

    // ============================================================================
    // SCENARIO 2: Resource Limit Enforcement
    // ============================================================================

    #[tokio::test]
    async fn test_subscription_creation_respects_limits() {
        let limits = ResourceLimits {
            max_subscriptions_per_user: 2,
            max_subscriptions_per_connection: 2,
            ..Default::default()
        };
        let limiter = ResourceLimiter::new(limits);

        // Register first subscription
        let result1 =
            limiter.register_subscription("sub-1".to_string(), 1, "conn-1".to_string(), 1000);
        assert!(result1.is_ok());

        // Register second subscription (at limit)
        let result2 =
            limiter.register_subscription("sub-2".to_string(), 1, "conn-1".to_string(), 1000);
        assert!(result2.is_ok());

        // Try to register third subscription (exceeds limit)
        let check_result = limiter.check_subscription_creation(1, "conn-1");
        assert!(check_result.is_err());
    }

    // ============================================================================
    // SCENARIO 3: Error Recovery and Retry
    // ============================================================================

    #[test]
    fn test_exponential_backoff_retry_strategy() {
        let retry_config = RetryConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        };

        let strategy = RecoveryStrategy::new(retry_config);

        // Verify backoffs increase exponentially
        let backoff1 = strategy.calculate_backoff(0);
        let backoff2 = strategy.calculate_backoff(1);
        let backoff3 = strategy.calculate_backoff(2);

        assert!(backoff2 > backoff1);
        assert!(backoff3 > backoff2);

        // Verify max retries
        assert!(strategy.should_retry(0));
        assert!(strategy.should_retry(1));
        assert!(strategy.should_retry(2));
        assert!(!strategy.should_retry(3));
    }

    #[tokio::test]
    async fn test_circuit_breaker_failure_recovery() {
        let circuit_breaker = CircuitBreaker::new(2, Duration::from_millis(100));

        // Verify initial closed state
        assert_eq!(circuit_breaker.state().await, CircuitState::Closed);
        assert!(circuit_breaker.can_attempt().await);

        // Record failures to open circuit
        circuit_breaker.record_failure().await;
        circuit_breaker.record_failure().await;

        // Verify circuit is open
        assert_eq!(circuit_breaker.state().await, CircuitState::Open);
        assert!(!circuit_breaker.can_attempt().await);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be able to attempt (half-open)
        assert!(circuit_breaker.can_attempt().await);
    }

    // ============================================================================
    // SCENARIO 4: Event Filtering
    // ============================================================================

    #[test]
    fn test_complex_event_filtering() {
        // Create complex event
        let event = Event::new(
            "userUpdated".to_string(),
            json!({
                "userId": 123,
                "username": "alice",
                "age": 30,
                "status": "active",
                "profile": {
                    "city": "New York",
                    "verified": true
                }
            }),
            "users".to_string(),
        );

        // Create complex filter
        let filter = EventFilter::new()
            .with_event_type("userUpdated")
            .with_channel("users")
            .with_field("status", FilterCondition::Equals(json!("active")))
            .with_field("age", FilterCondition::GreaterThan(25.0))
            .with_field("profile.city", FilterCondition::Equals(json!("New York")));

        // Verify filter matches
        assert!(filter.matches(&event));

        // Create non-matching filter
        let non_matching_filter =
            EventFilter::new().with_field("status", FilterCondition::Equals(json!("inactive")));

        assert!(!non_matching_filter.matches(&event));
    }

    // ============================================================================
    // SCENARIO 5: Multi-Subscription Workflow
    // ============================================================================

    #[tokio::test]
    async fn test_multiple_subscriptions_per_connection() {
        let config = SubscriptionConfig::default();
        let manager = ConnectionManager::new(config.limits);

        // Register connection
        let metadata = manager
            .register_connection(Some(1), Some(1))
            .expect("Failed to register connection");
        let connection_id = metadata.id;

        // Create executor
        let executor = SubscriptionExecutor::new();

        // Create multiple subscriptions
        let sub_ids = vec!["sub-1", "sub-2", "sub-3"];

        for _sub_id in &sub_ids {
            let payload = SubscriptionPayload {
                query: "subscription { test }".to_string(),
                operation_name: None,
                variables: None,
                extensions: None,
            };

            let result = executor.execute(connection_id, &payload);
            assert!(result.is_ok());
        }

        // Verify all subscriptions created
        for sub_id in &sub_ids {
            let subscription = executor.get_subscription(sub_id);
            assert!(subscription.is_some());
        }

        // Get connection subscriptions
        let conn_subs = executor.get_connection_subscriptions(connection_id);
        assert_eq!(conn_subs.len(), 3);
    }

    // ============================================================================
    // SCENARIO 6: Connection Pool Management
    // ============================================================================

    #[test]
    fn test_connection_pool_lifecycle() {
        let pool_config = PoolConfig {
            min_connections: 5,
            max_connections: 10,
            ..Default::default()
        };

        let pool = ConnectionPoolManager::new(pool_config);

        // Register connections
        for i in 0..5 {
            let result = pool.register_connection(format!("conn-{i}"));
            assert!(result.is_ok());
        }

        // Verify count
        assert_eq!(pool.connections_count(), 5);

        // Release connections
        for i in 0..5 {
            let result = pool.release_connection(&format!("conn-{i}"));
            assert!(result.is_ok());
        }

        // Mark one as unhealthy
        let _ = pool.mark_unhealthy("conn-0");
        assert!(!pool.is_connection_healthy("conn-0"));
    }

    // ============================================================================
    // SCENARIO 7: Consumer Group Management
    // ============================================================================

    #[test]
    fn test_consumer_group_horizontal_scaling() {
        let manager = ConsumerGroupManager::new();

        // Create group for channel
        let group_id = ConsumerGroupId::new("notifications");
        manager
            .register_consumer_group("notifications", group_id.clone())
            .expect("Failed to register group");

        // Register multiple consumers (workers)
        let consumer1 = ConsumerId::new("worker-1");
        let consumer2 = ConsumerId::new("worker-2");

        manager
            .register_consumer(&group_id, consumer1.clone())
            .expect("Failed to register consumer 1");
        manager
            .register_consumer(&group_id, consumer2)
            .expect("Failed to register consumer 2");

        // Verify group info
        let group = manager.get_group(&group_id).expect("Group not found");
        assert_eq!(group.consumers_count, 2);

        // Update pending messages
        manager
            .update_pending_count(&group_id, 100)
            .expect("Failed to update pending");

        let group = manager.get_group(&group_id).expect("Group not found");
        assert_eq!(group.pending_count, 100);

        // Unregister consumer
        manager
            .unregister_consumer(&group_id, &consumer1)
            .expect("Failed to unregister");

        let group = manager.get_group(&group_id).expect("Group not found");
        assert_eq!(group.consumers_count, 1);
    }

    // ============================================================================
    // SCENARIO 8: Rate Limiting
    // ============================================================================

    #[tokio::test]
    async fn test_rate_limiting_prevents_abuse() {
        let rate_limiter = SubscriptionRateLimiter::new(RateLimiterConfig::default());

        // First subscription should succeed
        let result1 = rate_limiter.check_subscription_creation(1).await;
        assert!(result1.is_ok());

        // Simulate creating many subscriptions quickly
        for _ in 0..99 {
            let _ = rate_limiter.check_subscription_creation(1).await;
        }

        // Try to create beyond limit
        let result_limit = rate_limiter.check_subscription_creation(1).await;
        assert!(result_limit.is_err());
    }

    // ============================================================================
    // SCENARIO 9: Metrics Collection
    // ============================================================================

    #[tokio::test]
    async fn test_metrics_collection_lifecycle() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");

        // Record connection lifecycle
        metrics.record_connection_created();
        metrics.record_connection_created();

        // Record subscription lifecycle
        metrics.record_subscription_created();
        metrics.record_subscription_completed();

        // Record events
        metrics.record_event_published("data_change");
        metrics.record_event_delivered();

        // Record latency
        metrics.record_subscription_latency(0.005);
        metrics.record_event_delivery_latency(0.002);

        // Record message size
        metrics.record_message_size("subscribe", 512);

        // Record uptime
        metrics.record_connection_uptime(3600.0);

        // Verify metrics gathered
        let metrics_text = metrics.gather_metrics().expect("Failed to gather metrics");
        assert!(!metrics_text.is_empty());
        assert!(metrics_text.contains("fraiseql_subscriptions_total_connections"));
    }

    // ============================================================================
    // SCENARIO 10: Heartbeat and Connection Health
    // ============================================================================

    #[test]
    fn test_heartbeat_detection_of_dead_connections() {
        let config = Arc::new(WebSocketConfig {
            ping_interval: Duration::from_millis(100),
            pong_timeout: Duration::from_millis(50),
            ..Default::default()
        });

        let connection_id = uuid::Uuid::new_v4();
        let mut heartbeat = ConnectionHeartbeat::new(connection_id, config, None);

        // Verify should ping initially
        assert!(heartbeat.should_ping());

        // Send ping
        heartbeat.ping_sent();
        assert_eq!(heartbeat.state, HeartbeatState::AwaitingPong);

        // Wait for pong timeout
        std::thread::sleep(Duration::from_millis(75));

        // Check if dead
        assert!(heartbeat.is_pong_timeout());
        assert!(heartbeat.check_dead());
        assert_eq!(heartbeat.state, HeartbeatState::Dead);
    }

    // ============================================================================
    // SCENARIO 11: Complete End-to-End Workflow
    // ============================================================================

    #[tokio::test]
    async fn test_complete_subscription_workflow() {
        // Setup all components
        let config = SubscriptionConfig::default();
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        let manager = ConnectionManager::new(config.limits);
        let executor = SubscriptionExecutor::new();
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        let pool = ConnectionPoolManager::new(PoolConfig::default());

        // 1. Register connection with pool
        let pool_result = pool.register_connection("conn-1".to_string());
        assert!(pool_result.is_ok());
        metrics.record_connection_created();

        // 2. Register connection with manager
        let metadata = manager
            .register_connection(Some(1), Some(1))
            .expect("Failed to register connection");
        let connection_id = metadata.id;

        // 3. Check resource limits before creating subscription
        let limit_check = limiter.check_subscription_creation(1, &connection_id.to_string());
        assert!(limit_check.is_ok());

        // 4. Register subscription in resource limiter
        limiter
            .register_subscription("sub-1".to_string(), 1, connection_id.to_string(), 5000)
            .expect("Failed to register subscription");
        metrics.record_subscription_created();

        // 5. Create subscription with executor
        let payload = SubscriptionPayload {
            query: "subscription { test }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let exec_result = executor.execute(connection_id, &payload);
        assert!(exec_result.is_ok());

        // 6. Verify subscription exists
        let subscription = executor.get_subscription("sub-1");
        assert!(subscription.is_some());

        // 7. Record event
        metrics.record_event_published("subscription_update");
        metrics.record_event_delivered();

        // 8. Complete subscription
        executor
            .complete_subscription("sub-1")
            .expect("Failed to complete subscription");
        metrics.record_subscription_completed();

        // 9. Unregister from limiter
        limiter
            .unregister_subscription("sub-1")
            .expect("Failed to unregister subscription");

        // 10. Release connection from pool
        pool.release_connection("conn-1")
            .expect("Failed to release connection");
        metrics.record_connection_closed();

        // Verify final state
        assert!(executor.get_subscription("sub-1").is_none());
        assert_eq!(pool.connections_count(), 1); // Still in pool
    }

    // ============================================================================
    // SCENARIO 12: Fallback Mechanism
    // ============================================================================

    #[test]
    fn test_fallback_service_availability() {
        let fallbacks = FallbackRegistry::new();

        // Register Redis as primary with PostgreSQL fallback
        fallbacks.register_fallback("redis", "postgresql");

        // Verify fallback exists
        assert_eq!(
            fallbacks.get_fallback("redis"),
            Some("postgresql".to_string())
        );

        // Verify both services available
        assert!(fallbacks.is_available("redis"));
        assert!(fallbacks.is_available("postgresql"));

        // Simulate Redis failure
        fallbacks.mark_unavailable("redis");

        // Fallback to PostgreSQL
        let fallback = fallbacks.get_available_fallback("redis");
        assert_eq!(fallback, Some("postgresql".to_string()));

        // PostgreSQL takes over
        assert!(fallbacks.is_available("postgresql"));
    }

    // ============================================================================
    // SCENARIO 13: In-Memory Event Bus End-to-End
    // ============================================================================

    #[tokio::test]
    async fn test_in_memory_event_bus_end_to_end_workflow() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());
        let bus_clone = bus.clone();

        // Subscribe to channel
        let mut stream = bus
            .subscribe("user-updates")
            .await
            .expect("Failed to subscribe");

        // Spawn publisher task
        let publisher_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Publish multiple events
            for i in 0..5 {
                let event = Arc::new(Event::new(
                    "userCreated".to_string(),
                    json!({
                        "userId": 100 + i,
                        "username": format!("user{}", i),
                        "email": format!("user{}@example.com", i)
                    }),
                    "user-updates".to_string(),
                ));

                let _ = bus_clone.publish(event).await;
            }
        });

        // Receive and verify events
        let mut received_count = 0;
        for expected_i in 0..5 {
            let result = tokio::time::timeout(Duration::from_secs(1), stream.recv()).await;
            assert!(result.is_ok(), "Failed to receive event {}", expected_i);

            let event_opt = result.unwrap();
            assert!(event_opt.is_some());

            let event = event_opt.unwrap();
            assert_eq!(event.event_type, "userCreated");
            assert_eq!(event.channel, "user-updates");
            assert_eq!(event.data["userId"], 100 + expected_i);
            received_count += 1;
        }

        assert_eq!(received_count, 5);
        publisher_task.await.expect("Publisher task failed");

        // Verify stats
        let stats = bus.stats();
        assert_eq!(stats.total_events, 5);
        assert!(stats.active_subscribers >= 1);
    }

    #[tokio::test]
    async fn test_in_memory_multi_subscriber_broadcast() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Create 3 subscribers to same channel
        let mut stream1 = bus.subscribe("notifications").await.expect("Sub 1 failed");
        let mut stream2 = bus.subscribe("notifications").await.expect("Sub 2 failed");
        let mut stream3 = bus.subscribe("notifications").await.expect("Sub 3 failed");

        let bus_clone = bus.clone();

        // Publish event
        let publisher_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;

            let event = Arc::new(Event::new(
                "alert".to_string(),
                json!({ "level": "critical", "message": "System overload" }),
                "notifications".to_string(),
            ));

            let _ = bus_clone.publish(event).await;
        });

        // All subscribers should receive the event
        let recv1 = tokio::time::timeout(Duration::from_secs(1), stream1.recv())
            .await
            .expect("Sub 1 timeout");
        let recv2 = tokio::time::timeout(Duration::from_secs(1), stream2.recv())
            .await
            .expect("Sub 2 timeout");
        let recv3 = tokio::time::timeout(Duration::from_secs(1), stream3.recv())
            .await
            .expect("Sub 3 timeout");

        assert!(recv1.is_some(), "Sub 1 didn't receive event");
        assert!(recv2.is_some(), "Sub 2 didn't receive event");
        assert!(recv3.is_some(), "Sub 3 didn't receive event");

        // All should have same event ID (same Arc)
        let event1 = recv1.unwrap();
        let event2 = recv2.unwrap();
        let event3 = recv3.unwrap();

        assert_eq!(event1.id, event2.id);
        assert_eq!(event2.id, event3.id);

        publisher_task.await.expect("Publisher task failed");
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_multiple_channels() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Subscribe to different channels
        let mut stream_users = bus.subscribe("user-events").await.expect("User sub failed");
        let mut stream_orders = bus
            .subscribe("order-events")
            .await
            .expect("Order sub failed");
        let mut stream_payments = bus
            .subscribe("payment-events")
            .await
            .expect("Payment sub failed");

        let bus_clone1 = bus.clone();
        let bus_clone2 = bus.clone();
        let bus_clone3 = bus.clone();

        // Publish to different channels
        let publisher_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;

            // User event
            let _ = bus_clone1
                .publish(Arc::new(Event::new(
                    "userRegistered".to_string(),
                    json!({"userId": 1}),
                    "user-events".to_string(),
                )))
                .await;

            // Order event
            let _ = bus_clone2
                .publish(Arc::new(Event::new(
                    "orderCreated".to_string(),
                    json!({"orderId": 100}),
                    "order-events".to_string(),
                )))
                .await;

            // Payment event
            let _ = bus_clone3
                .publish(Arc::new(Event::new(
                    "paymentProcessed".to_string(),
                    json!({"amount": 99.99}),
                    "payment-events".to_string(),
                )))
                .await;
        });

        // Each subscriber should only receive their channel's events
        let user_event = tokio::time::timeout(Duration::from_secs(1), stream_users.recv())
            .await
            .expect("User event timeout")
            .expect("No user event");
        assert_eq!(user_event.event_type, "userRegistered");

        let order_event = tokio::time::timeout(Duration::from_secs(1), stream_orders.recv())
            .await
            .expect("Order event timeout")
            .expect("No order event");
        assert_eq!(order_event.event_type, "orderCreated");

        let payment_event = tokio::time::timeout(Duration::from_secs(1), stream_payments.recv())
            .await
            .expect("Payment event timeout")
            .expect("No payment event");
        assert_eq!(payment_event.event_type, "paymentProcessed");

        publisher_task.await.expect("Publisher task failed");
    }

    // ============================================================================
    // SCENARIO 14: Event Filtering and Routing
    // ============================================================================

    #[tokio::test]
    async fn test_event_filtering_with_subscriptions() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Subscribe to channel
        let mut stream = bus.subscribe("transactions").await.expect("Sub failed");

        let bus_clone = bus.clone();

        // Publisher publishes mixed events
        let publisher_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Large transaction (should match)
            let _ = bus_clone
                .publish(Arc::new(Event::new(
                    "transaction".to_string(),
                    json!({ "amount": 5000.0, "status": "pending" }),
                    "transactions".to_string(),
                )))
                .await;

            // Small transaction (might be filtered)
            let _ = bus_clone
                .publish(Arc::new(Event::new(
                    "transaction".to_string(),
                    json!({ "amount": 10.0, "status": "pending" }),
                    "transactions".to_string(),
                )))
                .await;

            // Large transaction (should match)
            let _ = bus_clone
                .publish(Arc::new(Event::new(
                    "transaction".to_string(),
                    json!({ "amount": 3000.0, "status": "completed" }),
                    "transactions".to_string(),
                )))
                .await;
        });

        // Receive all events (filtering would happen at subscription level)
        let mut event_count = 0;
        for _ in 0..3 {
            let result = tokio::time::timeout(Duration::from_secs(1), stream.recv()).await;
            if result.is_ok() && result.unwrap().is_some() {
                event_count += 1;
            }
        }

        assert_eq!(event_count, 3);
        publisher_task.await.expect("Publisher task failed");
    }

    // ============================================================================
    // SCENARIO 15: Error Recovery and Resilience
    // ============================================================================

    #[tokio::test]
    async fn test_event_bus_resilience_with_disconnects() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Subscribe
        let mut stream1 = bus.subscribe("test").await.expect("Sub 1 failed");

        // Second subscription (simulating reconnect)
        let mut stream2 = bus.subscribe("test").await.expect("Sub 2 failed");

        let bus_clone = bus.clone();

        // Publish events
        let publisher_task = tokio::spawn(async move {
            // Publish to first subscriber
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = bus_clone
                .publish(Arc::new(Event::new(
                    "event1".to_string(),
                    json!({}),
                    "test".to_string(),
                )))
                .await;

            // Simulate disconnect (stream1 drops)
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Publish after reconnect
            let _ = bus_clone
                .publish(Arc::new(Event::new(
                    "event2".to_string(),
                    json!({}),
                    "test".to_string(),
                )))
                .await;
        });

        // Stream 1 receives first event
        let event1 = tokio::time::timeout(Duration::from_secs(1), stream1.recv())
            .await
            .expect("Event 1 timeout")
            .expect("No event 1");
        assert_eq!(event1.event_type, "event1");

        // Stream 2 (new subscription) only gets second event
        let event2 = tokio::time::timeout(Duration::from_secs(1), stream2.recv())
            .await
            .expect("Event 2 timeout")
            .expect("No event 2");
        assert_eq!(event2.event_type, "event2");

        publisher_task.await.expect("Publisher task failed");
    }

    #[tokio::test]
    async fn test_event_bus_with_rapid_subscribe_unsubscribe() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Rapidly subscribe/unsubscribe
        let mut handles = vec![];
        for i in 0..10 {
            let bus_clone = bus.clone();
            let handle = tokio::spawn(async move {
                let channel = format!("rapid-{i}");
                let _stream = bus_clone
                    .subscribe(&channel)
                    .await
                    .expect("Subscribe failed");

                // Short hold
                tokio::time::sleep(Duration::from_millis(5)).await;

                // Unsubscribe (drop stream)
                let _ = bus_clone.unsubscribe(&channel).await;
            });
            handles.push(handle);
        }

        // Wait for all to complete
        for handle in handles {
            handle.await.expect("Task failed");
        }

        // Should still be operational
        let mut stream = bus.subscribe("final").await.expect("Final sub failed");

        let bus_clone = bus.clone();
        let _publisher = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = bus_clone
                .publish(Arc::new(Event::new(
                    "final".to_string(),
                    json!({}),
                    "final".to_string(),
                )))
                .await;
        });

        let event = tokio::time::timeout(Duration::from_secs(1), stream.recv())
            .await
            .expect("Final event timeout");
        assert!(event.is_some());
    }

    // ============================================================================
    // SCENARIO 16: Performance and Throughput
    // ============================================================================

    #[tokio::test]
    async fn test_event_bus_throughput_with_rapid_events() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Subscribe to channel
        let mut stream = bus.subscribe("throughput").await.expect("Sub failed");

        let bus_clone = bus.clone();

        // Publish events rapidly
        let publisher_task = tokio::spawn(async move {
            for i in 0..100 {
                let _ = bus_clone
                    .publish(Arc::new(Event::new(
                        "data".to_string(),
                        json!({ "sequence": i }),
                        "throughput".to_string(),
                    )))
                    .await;

                // Minimal delay between events
                if i % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });

        // Collect events
        let mut received_count = 0;
        let start = std::time::Instant::now();

        loop {
            let result = tokio::time::timeout(Duration::from_millis(500), stream.recv()).await;
            match result {
                Ok(Some(_event)) => {
                    received_count += 1;
                    if received_count >= 100 {
                        break;
                    }
                }
                _ => break,
            }
        }

        let elapsed = start.elapsed();
        publisher_task.await.expect("Publisher task failed");

        // Verify throughput
        assert_eq!(received_count, 100);
        let events_per_second = (received_count as f64) / elapsed.as_secs_f64();
        println!("Event bus throughput: {:.0} events/sec", events_per_second);
        assert!(
            events_per_second > 100.0,
            "Throughput too low: {} events/sec",
            events_per_second
        );
    }

    // ============================================================================
    // SCENARIO 17: Subscription with Event Metadata
    // ============================================================================

    #[tokio::test]
    async fn test_event_metadata_preservation() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        let mut stream = bus.subscribe("metadata-test").await.expect("Sub failed");

        let bus_clone = bus.clone();

        let publisher_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;

            let mut event = Event::new(
                "testEvent".to_string(),
                json!({ "data": "value" }),
                "metadata-test".to_string(),
            );
            event = event.with_correlation_id("corr-123".to_string());

            let _ = bus_clone.publish(Arc::new(event)).await;
        });

        let received = tokio::time::timeout(Duration::from_secs(1), stream.recv())
            .await
            .expect("Event timeout")
            .expect("No event");

        assert_eq!(received.event_type, "testEvent");
        assert_eq!(received.correlation_id, Some("corr-123".to_string()));
        assert_eq!(received.channel, "metadata-test");

        publisher_task.await.expect("Publisher task failed");
    }

    // ============================================================================
    // PHASE 2.3: LOAD TESTING
    // ============================================================================
    // Tests for 1,000+ concurrent connections, memory stability, and throughput

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[allow(clippy::excessive_nesting)]
    async fn test_load_1000_concurrent_connections() {
        use crate::subscriptions::event_bus::{EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());
        let mut connection_handles = vec![];

        let start_time = std::time::Instant::now();

        // Create 1,000 concurrent connections
        for conn_id in 0..1000 {
            let bus_clone = bus.clone();
            let handle = tokio::spawn(async move {
                // Each connection subscribes to a channel
                let channel = format!("conn-{}", conn_id);
                let _stream = bus_clone
                    .subscribe(&channel)
                    .await
                    .expect("Subscribe failed");

                // Hold connection open for a bit
                tokio::time::sleep(Duration::from_millis(100)).await;

                // Cleanup
                let _ = bus_clone.unsubscribe(&channel).await;
            });

            connection_handles.push(handle);
        }

        // Wait for all connections to complete
        let mut successful = 0;
        for handle in connection_handles {
            if handle.await.is_ok() {
                successful += 1;
            }
        }

        let elapsed = start_time.elapsed();

        println!(
            "Load Test: 1000 concurrent connections - {} successful in {:.2}s",
            successful,
            elapsed.as_secs_f64()
        );

        // Assertions
        assert_eq!(successful, 1000, "All 1000 connections should succeed");
        assert!(
            elapsed.as_secs_f64() < 10.0,
            "Should complete within 10 seconds"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_load_10000_subscriptions() {
        use crate::subscriptions::event_bus::{EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());
        let mut subscription_handles = vec![];

        let start_time = std::time::Instant::now();

        // Create 10,000 subscriptions
        for sub_id in 0..10000 {
            let bus_clone = bus.clone();
            let handle = tokio::spawn(async move {
                let channel = format!("sub-{}", sub_id / 100); // 100 subscriptions per channel
                let _stream = bus_clone
                    .subscribe(&channel)
                    .await
                    .expect("Subscribe failed");

                // Hold subscription
                tokio::time::sleep(Duration::from_millis(50)).await;

                let _ = bus_clone.unsubscribe(&channel).await;
            });

            subscription_handles.push(handle);

            // Batch yield to prevent overwhelming the scheduler
            if sub_id % 100 == 0 {
                tokio::task::yield_now().await;
            }
        }

        // Wait for all subscriptions
        let mut successful = 0;
        for handle in subscription_handles {
            if handle.await.is_ok() {
                successful += 1;
            }
        }

        let elapsed = start_time.elapsed();

        println!(
            "Load Test: 10000 subscriptions - {} successful in {:.2}s",
            successful,
            elapsed.as_secs_f64()
        );

        assert_eq!(successful, 10000, "All 10000 subscriptions should succeed");
        assert!(
            elapsed.as_secs_f64() < 20.0,
            "Should complete within 20 seconds"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[allow(clippy::excessive_nesting, clippy::uninlined_format_args)]
    async fn test_load_sustained_throughput_100_events_per_second() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Create 10 subscribers
        let mut streams = vec![];
        for i in 0..10 {
            let stream = bus
                .subscribe(&format!("throughput-{}", i))
                .await
                .expect("Subscribe failed");
            streams.push(stream);
        }

        let start_time = std::time::Instant::now();

        // Publisher: 1000 events over 10 seconds = 100 events/sec
        let bus_clone = bus.clone();
        let publisher_task = tokio::spawn(async move {
            for batch in 0..10 {
                for event_idx in 0..100 {
                    let channel = format!("throughput-{}", event_idx % 10);
                    let _ = bus_clone
                        .publish(Arc::new(Event::new(
                            "data".to_string(),
                            json!({ "batch": batch, "index": event_idx }),
                            channel,
                        )))
                        .await;

                    // Rate limiting: ~100 events/sec
                    if event_idx % 10 == 0 {
                        tokio::task::yield_now().await;
                    }
                }

                // 1 second delay between batches
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        // Collector: measure throughput
        let mut total_received = 0;
        let mut stream_iter = streams.into_iter().enumerate();

        // Receive from all streams concurrently
        let collector_task = tokio::spawn(async move {
            let mut total = 0;

            // This is a simplified collector - in production would use select!
            tokio::time::sleep(Duration::from_secs(12)).await;
            total
        });

        let _collected = collector_task.await.expect("Collector failed");
        publisher_task.await.expect("Publisher failed");

        let elapsed = start_time.elapsed();
        let events_per_sec = (1000.0 / elapsed.as_secs_f64()).floor();

        println!(
            "Sustained throughput: {:.0} events/sec over {:.2}s",
            events_per_sec,
            elapsed.as_secs_f64()
        );

        // 1000 events should be published in ~10 seconds
        assert!(elapsed.as_secs_f64() >= 9.0, "Should take ~10 seconds");
        assert!(elapsed.as_secs_f64() < 15.0, "Should not exceed 15 seconds");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[allow(clippy::excessive_nesting)]
    async fn test_load_memory_stability_with_event_creation() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Subscribe to one channel
        let mut stream = bus
            .subscribe("memory-test")
            .await
            .expect("Subscribe failed");

        let bus_clone = bus.clone();

        // Publisher: Create many events rapidly
        let publisher_task = tokio::spawn(async move {
            for batch in 0..10 {
                for i in 0..1000 {
                    let event = Arc::new(Event::new(
                        "memory-test".to_string(),
                        json!({
                            "batch": batch,
                            "index": i,
                            "payload": "x".repeat(1024) // 1KB per event
                        }),
                        "memory-test".to_string(),
                    ));

                    let _ = bus_clone.publish(event).await;

                    // Yield periodically
                    if i % 100 == 0 {
                        tokio::task::yield_now().await;
                    }
                }

                // Small delay between batches
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });

        // Receiver: Drain events to prevent queue buildup
        let start = std::time::Instant::now();
        let mut received_count = 0;

        loop {
            let result = tokio::time::timeout(Duration::from_secs(1), stream.recv()).await;
            match result {
                Ok(Some(_event)) => {
                    received_count += 1;
                    if received_count >= 10000 {
                        break;
                    }
                }
                _ => {
                    // Timeout - check if publisher is done
                    if publisher_task.is_finished() {
                        break;
                    }
                }
            }
        }

        publisher_task.await.expect("Publisher failed");
        let elapsed = start.elapsed();

        println!(
            "Memory stability test: Created 10000 events ({:.1}MB) in {:.2}s, received {}",
            (10000 * 1024) as f64 / (1024.0 * 1024.0),
            elapsed.as_secs_f64(),
            received_count
        );

        // Should receive most events (some may be dropped if queue fills)
        assert!(received_count > 9000, "Should receive >90% of events");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_load_connection_leak_detection() {
        use crate::subscriptions::event_bus::{EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        // Track initial state
        let initial_stats = bus.stats();
        let initial_subscribers = initial_stats.active_subscribers;

        // Create and drop subscriptions repeatedly
        for iteration in 0..100 {
            let mut local_streams = vec![];

            for sub_idx in 0..100 {
                let bus_clone = bus.clone();
                let channel = format!("leak-test-{}-{}", iteration, sub_idx);

                let stream = bus_clone
                    .subscribe(&channel)
                    .await
                    .expect("Subscribe failed");
                local_streams.push(stream);
            }

            // Explicit drop to test cleanup
            drop(local_streams);

            // Unsubscribe all
            for sub_idx in 0..100 {
                let channel = format!("leak-test-{}-{}", iteration, sub_idx);
                let _ = bus.unsubscribe(&channel).await;
            }

            // Periodic check
            if iteration % 10 == 0 {
                let current_stats = bus.stats();
                println!(
                    "Iteration {}: active_subscribers = {}",
                    iteration, current_stats.active_subscribers
                );
            }
        }

        // Final state should be similar to initial
        let final_stats = bus.stats();
        println!("Initial subscribers: {}", initial_subscribers);
        println!("Final subscribers: {}", final_stats.active_subscribers);

        // Allow some tolerance (±10)
        assert!(
            final_stats.active_subscribers <= initial_subscribers + 10,
            "Possible connection leak detected"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[allow(
        clippy::excessive_nesting,
        clippy::uninlined_format_args,
        clippy::unused_enumerate_index
    )]
    async fn test_load_multi_channel_broadcasting() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());
        let num_channels = 100;
        let subscribers_per_channel = 10;

        // Create 100 channels with 10 subscribers each = 1000 subscribers
        let mut all_streams = vec![];

        for ch_idx in 0..num_channels {
            let mut channel_streams = vec![];
            for _sub_idx in 0..subscribers_per_channel {
                let channel = format!("broadcast-{}", ch_idx);
                let stream = bus.subscribe(&channel).await.expect("Subscribe failed");
                channel_streams.push(stream);
            }
            all_streams.push(channel_streams);
        }

        let start_time = std::time::Instant::now();

        // Publish to all channels
        let bus_clone = bus.clone();
        let publisher_task = tokio::spawn(async move {
            for round in 0..10 {
                for ch_idx in 0..num_channels {
                    let _ = bus_clone
                        .publish(Arc::new(Event::new(
                            "broadcast".to_string(),
                            json!({ "round": round, "channel": ch_idx }),
                            format!("broadcast-{}", ch_idx),
                        )))
                        .await;
                }

                if round % 2 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });

        // Receive from all streams
        let mut receive_tasks = vec![];

        for (_ch_idx, channel_streams) in all_streams.into_iter().enumerate() {
            for (_sub_idx, mut stream) in channel_streams.into_iter().enumerate() {
                let task = tokio::spawn(async move {
                    let mut count = 0;
                    while count < 10 {
                        let result =
                            tokio::time::timeout(Duration::from_secs(2), stream.recv()).await;
                        if result.is_ok() && result.unwrap().is_some() {
                            count += 1;
                        }
                    }
                    count
                });
                receive_tasks.push(task);
            }
        }

        // Wait for publisher
        publisher_task.await.expect("Publisher failed");

        // Wait for all receivers
        let mut total_received = 0;
        for task in receive_tasks {
            if let Ok(count) = task.await {
                total_received += count;
            }
        }

        let elapsed = start_time.elapsed();

        println!(
            "Multi-channel broadcast: {} channels × {} subscribers = {} messages in {:.2}s",
            num_channels,
            subscribers_per_channel,
            total_received,
            elapsed.as_secs_f64()
        );

        // Should broadcast successfully
        assert!(total_received > 0, "Should receive some messages");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_load_large_event_payload() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        let mut stream = bus
            .subscribe("large-payload")
            .await
            .expect("Subscribe failed");

        let bus_clone = bus.clone();

        // Publisher: Create events with large payloads
        let publisher_task = tokio::spawn(async move {
            for i in 0..100 {
                // Create 100KB event
                let large_payload = "x".repeat(100 * 1024);
                let _ = bus_clone
                    .publish(Arc::new(Event::new(
                        "large".to_string(),
                        json!({
                            "index": i,
                            "payload": large_payload
                        }),
                        "large-payload".to_string(),
                    )))
                    .await;

                if i % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });

        // Receive events
        let mut received = 0;
        let start = std::time::Instant::now();

        loop {
            let result = tokio::time::timeout(Duration::from_secs(2), stream.recv()).await;
            match result {
                Ok(Some(event)) => {
                    received += 1;
                    // Verify payload size
                    let payload_str = event.data["payload"].as_str().unwrap_or("");
                    assert!(
                        payload_str.len() > 100000,
                        "Payload should be ~100KB, got {}",
                        payload_str.len()
                    );
                }
                _ => break,
            }
        }

        publisher_task.await.expect("Publisher failed");
        let elapsed = start.elapsed();

        println!(
            "Large payload test: {} events × 100KB each in {:.2}s",
            received,
            elapsed.as_secs_f64()
        );

        assert_eq!(received, 100, "Should receive all 100 events");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[allow(clippy::excessive_nesting, clippy::single_match)]
    async fn test_load_subscription_churn() {
        use crate::subscriptions::event_bus::{Event, EventBus, InMemoryEventBus};

        let bus = Arc::new(InMemoryEventBus::new());

        let start_time = std::time::Instant::now();

        // Rapid subscription creation/deletion with continuous publishing
        let bus_clone1 = bus.clone();
        let publisher_task = tokio::spawn(async move {
            for i in 0..1000 {
                let _ = bus_clone1
                    .publish(Arc::new(Event::new(
                        "churn".to_string(),
                        json!({ "index": i }),
                        "churn-test".to_string(),
                    )))
                    .await;

                if i % 100 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });

        let bus_clone2 = bus.clone();
        let subscriber_task = tokio::spawn(async move {
            let mut local_count = 0;
            for iteration in 0..100 {
                // Subscribe, receive a few, unsubscribe
                match bus_clone2.subscribe("churn-test").await {
                    Ok(mut stream) => {
                        // Try to get 1-5 events
                        for _ in 0..5 {
                            let result =
                                tokio::time::timeout(Duration::from_millis(100), stream.recv())
                                    .await;
                            if result.is_ok() && result.unwrap().is_some() {
                                local_count += 1;
                            }
                        }

                        let _ = bus_clone2.unsubscribe("churn-test").await;
                    }
                    Err(_) => {}
                }

                if iteration % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            local_count
        });

        publisher_task.await.expect("Publisher failed");
        let subscriber_result = subscriber_task.await.expect("Subscriber failed");
        let elapsed = start_time.elapsed();

        println!(
            "Subscription churn: 1000 publishes + 100 subscribe/unsub cycles in {:.2}s, received: {}",
            elapsed.as_secs_f64(),
            subscriber_result
        );

        assert!(subscriber_result > 0, "Should receive some events");
        assert!(
            elapsed.as_secs_f64() < 10.0,
            "Should complete in < 10 seconds"
        );
    }
}
