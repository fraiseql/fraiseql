#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design
mod transport_mod_tests {
    use super::super::*;

    #[test]
    fn test_event_filter_all_tenants_narrows_nothing() {
        let filter = EventFilter::all_tenants();
        assert!(filter.entity_type.is_none());
        assert!(filter.operation.is_none());
        assert_eq!(filter.tenant, TenantScope::AllTenants);
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
            tenant_id:            None,
            duration_ms:          None,
            seq:                  None,
            actor_type:           None,
            acting_for:           None,
            schema_version:       None,
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.user_id, Some("456".to_string()));
    }

    #[test]
    fn pg_bridge_surfaces_full_change_spine_envelope() {
        // The NATS bridge must carry the full Change-Spine envelope to out-of-session
        // consumers, not just user_id — including #377 schema_version and the #390
        // actor columns. UUID columns project to their string form (like tenant_id).
        let tenant = Uuid::new_v4();
        let human = Uuid::new_v4();
        let entry = ChangeLogEntry {
            pk_entity_change_log: 9,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          Some(serde_json::json!({"total": 1})),
            extra_metadata:       None,
            created_at:           Utc::now(),
            tenant_id:            Some(tenant),
            duration_ms:          Some(42),
            seq:                  Some(1_007),
            actor_type:           Some("ai_agent".to_string()),
            acting_for:           Some(human),
            schema_version:       Some("v2.7.0".to_string()),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();

        assert_eq!(event.tenant_id, Some(tenant.to_string()));
        assert_eq!(event.duration_ms, Some(42));
        assert_eq!(event.seq, Some(1_007));
        assert_eq!(event.actor_type.as_deref(), Some("ai_agent"));
        assert_eq!(event.acting_for, Some(human.to_string()));
        assert_eq!(event.schema_version.as_deref(), Some("v2.7.0"));
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
            tenant_id:            None,
            duration_ms:          None,
            seq:                  None,
            actor_type:           None,
            acting_for:           None,
            schema_version:       None,
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
            tenant_id:            None,
            duration_ms:          None,
            seq:                  None,
            actor_type:           None,
            acting_for:           None,
            schema_version:       None,
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
            tenant_id:            None,
            duration_ms:          None,
            seq:                  None,
            actor_type:           None,
            acting_for:           None,
            schema_version:       None,
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
        let mut stream = transport.subscribe(EventFilter::all_tenants()).await.unwrap();

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
        let mut stream = transport.subscribe(EventFilter::all_tenants()).await.unwrap();

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

        let mut stream = transport.subscribe(EventFilter::all_tenants()).await.unwrap();

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

    #[test]
    fn validate_nats_url_rejects_plaintext_by_default() {
        // L-nats-plaintext: a public-host plaintext nats:// URL must be refused
        // when the plaintext opt-in is absent, before any DNS resolution. Clear
        // the opt-in and production markers so the result is deterministic.
        temp_env::with_vars(
            [
                ("FRAISEQL_NATS_ALLOW_PLAINTEXT", None::<&str>),
                ("FRAISEQL_ENV", None),
                ("FRAISEQL_PROFILE", None),
                ("KUBERNETES_SERVICE_HOST", None),
            ],
            || {
                let result = validate_nats_url("nats://nats.example.com:4222");
                assert!(result.is_err(), "plaintext nats:// must be refused by default");
                if let Err(e) = result {
                    let msg = e.to_string();
                    assert!(
                        msg.contains("tls://") || msg.contains("plaintext"),
                        "error should explain the TLS requirement: {msg}"
                    );
                }
            },
        );
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
            .subscribe(EventFilter::all_tenants())
            .await
            .expect("subscribe should succeed");
        drop(stream);
    }

    /// #1113: this transport took `_filter` and returned an unfiltered stream — not
    /// even `entity_type` applied. It is the transport a single-node deployment is
    /// most likely to be running, so it is the one where an ignored tenant filter
    /// hands one caller every tenant's change events, `data` payload included.
    ///
    /// Named `postgres_notify_*` on purpose: `integration (observers)` reaches these
    /// by the name filter `--lib postgres_notify`, and a test outside that prefix
    /// would never bind a database.
    #[tokio::test]
    async fn postgres_notify_delivers_only_the_subscribed_tenant_and_entity_type() {
        use futures::StreamExt;

        let Some(pool) = try_test_pool().await else {
            eprintln!("Skipping: TEST_DATABASE_URL not set");
            return;
        };

        // The one provisioner for this table (#942/#982) — a private CREATE would be
        // an eleventh flavour of the contract.
        sqlx::raw_sql(&fraiseql_test_support::changelog::entity_change_log_provision_sql())
            .execute(&pool)
            .await
            .expect("provision core.tb_entity_change_log");

        let mine = uuid::Uuid::new_v4();
        let theirs = uuid::Uuid::new_v4();

        // Published in an order where an unfiltered stream yields the wrong row first.
        for (object_type, tenant) in [
            ("Order", theirs),
            ("Invoice", mine),
            ("Order", mine),
            ("Order", theirs),
        ] {
            sqlx::query(
                "INSERT INTO core.tb_entity_change_log \
                 (object_type, modification_type, object_id, object_data, tenant_id) \
                 VALUES ($1, 'INSERT', gen_random_uuid(), $2, $3)",
            )
            .bind(object_type)
            .bind(serde_json::json!({"payload": "present"}))
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("seed change-log row");
        }

        // A listener id nothing else uses: the dispatch ledger is keyed by it, so a
        // shared id would split these rows with whatever ran before.
        let config = ChangeLogListenerConfig::new(pool)
            .with_poll_interval(25)
            .with_listener_id(format!("t1113-{}", uuid::Uuid::new_v4()));
        let transport = PostgresNotifyTransport::from_config(config);

        let mut stream = transport
            .subscribe(EventFilter::for_tenant(mine.to_string()).with_entity_type("Order"))
            .await
            .expect("subscribe should succeed");

        let received = tokio::time::timeout(std::time::Duration::from_secs(10), stream.next())
            .await
            .expect("the matching row must be delivered within the poll window")
            .expect("stream must not end")
            .expect("delivery must not error");

        assert_eq!(received.entity_type, "Order", "entity_type filter must apply");
        assert_eq!(
            received.tenant_id.as_deref(),
            Some(mine.to_string().as_str()),
            "a tenant-scoped subscription must not receive another tenant's event"
        );

        // Only one seeded row matches both narrowings; anything further means the
        // filter admitted a row it should not have.
        let extra = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next()).await;
        assert!(
            extra.is_err(),
            "exactly one seeded row is inside the filter; got a second: {:?}",
            extra
                .ok()
                .flatten()
                .and_then(std::result::Result::ok)
                .map(|e| (e.entity_type, e.tenant_id))
        );
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

// ---------------------------------------------------------------------------
// #1113: `EventFilter` is a filter, in every transport
// ---------------------------------------------------------------------------

/// `subscribe` takes an `EventFilter`, and two of the three transports took it as
/// `_filter` and returned an unfiltered stream. A filter that only one
/// implementation applies is not a filter: the same subscription returned one
/// tenant's events under NATS and every tenant's under the two transports a
/// single-node deployment is most likely to be running.
///
/// These pin the *transport's* application of the filter. `EventFilter::matches`
/// has its own tests; asserting only there would pass with the call site removed.
#[allow(clippy::unwrap_used)] // Reason: test code
mod filter_is_honoured_by_every_transport {
    use futures::StreamExt;
    use serde_json::json;
    use uuid::Uuid;

    use super::super::{EventFilter, EventTransport, in_memory::InMemoryTransport};
    use crate::event::{EntityEvent, EventKind};

    /// How long a "nothing must arrive" assertion waits before concluding nothing did.
    const QUIET_WINDOW: std::time::Duration = std::time::Duration::from_millis(250);

    fn event(entity_type: &str, kind: EventKind, tenant: Option<&str>) -> EntityEvent {
        let mut e = EntityEvent::new(
            kind,
            entity_type.to_string(),
            Uuid::new_v4(),
            json!({"payload": "present"}),
        );
        e.tenant_id = tenant.map(String::from);
        e
    }

    #[tokio::test]
    async fn in_memory_delivers_only_the_subscribed_tenant() {
        let transport = std::sync::Arc::new(InMemoryTransport::new());
        let mut stream = transport
            .subscribe(EventFilter::for_tenant("tenant-a").with_entity_type("Order"))
            .await
            .unwrap();

        // Another tenant's event is published FIRST, so a transport that ignores the
        // filter yields it before the subscriber's own.
        transport
            .publish(event("Order", EventKind::Created, Some("tenant-b")))
            .await
            .unwrap();
        let mine = event("Order", EventKind::Created, Some("tenant-a"));
        let mine_id = mine.id;
        transport.publish(mine).await.unwrap();

        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(
            received.id, mine_id,
            "a tenant-scoped subscription must not receive another tenant's event \
             (got tenant {:?})",
            received.tenant_id
        );
    }

    #[tokio::test]
    async fn in_memory_does_not_treat_an_untenanted_event_as_the_subscribed_tenant() {
        let transport = std::sync::Arc::new(InMemoryTransport::new());
        let mut stream = transport.subscribe(EventFilter::for_tenant("tenant-a")).await.unwrap();

        transport.publish(event("Order", EventKind::Created, None)).await.unwrap();

        let quiet = tokio::time::timeout(QUIET_WINDOW, stream.next()).await;
        assert!(
            quiet.is_err(),
            "an event carrying no tenant must not satisfy a tenant-scoped subscription; \
             got {:?}",
            quiet.ok().flatten().map(|r| r.map(|e| e.tenant_id))
        );
    }

    #[tokio::test]
    async fn in_memory_delivers_only_the_subscribed_entity_type() {
        let transport = std::sync::Arc::new(InMemoryTransport::new());
        let mut stream = transport
            .subscribe(EventFilter::all_tenants().with_entity_type("Order"))
            .await
            .unwrap();

        transport.publish(event("Invoice", EventKind::Created, None)).await.unwrap();
        let mine = event("Order", EventKind::Created, None);
        let mine_id = mine.id;
        transport.publish(mine).await.unwrap();

        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(
            received.id, mine_id,
            "subscribing to Order must not deliver an Invoice (got {})",
            received.entity_type
        );
    }

    #[tokio::test]
    async fn in_memory_delivers_only_the_subscribed_operation() {
        let transport = std::sync::Arc::new(InMemoryTransport::new());
        let mut stream = transport
            .subscribe(EventFilter::all_tenants().with_operation(EventKind::Deleted))
            .await
            .unwrap();

        transport.publish(event("Order", EventKind::Created, None)).await.unwrap();
        let mine = event("Order", EventKind::Deleted, None);
        let mine_id = mine.id;
        transport.publish(mine).await.unwrap();

        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(
            received.id, mine_id,
            "subscribing to DELETE must not deliver an INSERT (got {:?})",
            received.event_type
        );
    }

    /// The server-internal consumers — the observer runtime and the retry loop —
    /// subscribe across the whole deployment on purpose. Filtering must not have
    /// narrowed them.
    #[tokio::test]
    async fn in_memory_all_tenants_still_delivers_every_tenant() {
        let transport = std::sync::Arc::new(InMemoryTransport::new());
        let mut stream = transport.subscribe(EventFilter::all_tenants()).await.unwrap();

        for tenant in [Some("tenant-a"), Some("tenant-b"), None] {
            let e = event("Order", EventKind::Created, tenant);
            let id = e.id;
            transport.publish(e).await.unwrap();
            let received = stream.next().await.unwrap().unwrap();
            assert_eq!(received.id, id, "AllTenants must deliver tenant {tenant:?}");
        }
    }
}

/// The one definition of "is this event inside this filter" (#1113). The transports
/// delegate here; `filter_is_honoured_by_every_transport` pins that they do.
#[allow(clippy::unwrap_used)] // Reason: test code
mod event_filter_matches {
    use serde_json::json;
    use uuid::Uuid;

    use super::super::{EventFilter, TenantScope};
    use crate::event::{EntityEvent, EventKind};

    fn event(entity_type: &str, kind: EventKind, tenant: Option<&str>) -> EntityEvent {
        let mut e = EntityEvent::new(kind, entity_type.to_string(), Uuid::new_v4(), json!({}));
        e.tenant_id = tenant.map(String::from);
        e
    }

    #[test]
    fn all_tenants_matches_every_tenant_and_the_untenanted() {
        let filter = EventFilter::all_tenants();
        assert!(filter.matches(&event("Order", EventKind::Created, Some("a"))));
        assert!(filter.matches(&event("Order", EventKind::Created, Some("b"))));
        assert!(filter.matches(&event("Order", EventKind::Created, None)));
    }

    #[test]
    fn a_tenant_scope_matches_only_that_tenant() {
        let filter = EventFilter::for_tenant("a");
        assert!(filter.matches(&event("Order", EventKind::Created, Some("a"))));
        assert!(!filter.matches(&event("Order", EventKind::Created, Some("b"))));
    }

    /// A missing stamp is not a wildcard. The same fail-closed rule the GraphQL
    /// subscription gate applies in multi-tenant mode.
    #[test]
    fn a_tenant_scope_does_not_match_an_untenanted_event() {
        assert!(!EventFilter::for_tenant("a").matches(&event("Order", EventKind::Created, None)));
    }

    /// A tenant id is compared whole: `"a"` must not admit `"ab"`, and vice versa.
    #[test]
    fn a_tenant_scope_compares_the_whole_id() {
        assert!(!EventFilter::for_tenant("a").matches(&event(
            "Order",
            EventKind::Created,
            Some("ab")
        )));
        assert!(!EventFilter::for_tenant("ab").matches(&event(
            "Order",
            EventKind::Created,
            Some("a")
        )));
    }

    #[test]
    fn entity_type_is_matched_exactly() {
        let filter = EventFilter::all_tenants().with_entity_type("Order");
        assert!(filter.matches(&event("Order", EventKind::Created, None)));
        assert!(!filter.matches(&event("Invoice", EventKind::Created, None)));
        assert!(!filter.matches(&event("order", EventKind::Created, None)));
    }

    #[test]
    fn operation_is_matched_exactly() {
        let filter = EventFilter::all_tenants().with_operation(EventKind::Deleted);
        assert!(filter.matches(&event("Order", EventKind::Deleted, None)));
        assert!(!filter.matches(&event("Order", EventKind::Created, None)));
        assert!(!filter.matches(&event("Order", EventKind::Updated, None)));
        assert!(!filter.matches(&event("Order", EventKind::Custom, None)));
    }

    /// Every narrowing must hold at once: an event matching two of three is out.
    /// A filter built by `&&`-ing the wrong way round would pass the single-dimension
    /// tests above and let a foreign tenant through on an entity-type match.
    #[test]
    fn every_narrowing_must_hold_at_once() {
        let filter = EventFilter::for_tenant("a")
            .with_entity_type("Order")
            .with_operation(EventKind::Created);

        assert!(filter.matches(&event("Order", EventKind::Created, Some("a"))));
        assert!(!filter.matches(&event("Order", EventKind::Created, Some("b"))), "wrong tenant");
        assert!(!filter.matches(&event("Invoice", EventKind::Created, Some("a"))), "wrong type");
        assert!(
            !filter.matches(&event("Order", EventKind::Deleted, Some("a"))),
            "wrong operation"
        );
    }

    #[test]
    fn narrowing_is_recorded_on_the_filter() {
        let filter = EventFilter::for_tenant("a")
            .with_entity_type("Order")
            .with_operation(EventKind::Updated);
        assert_eq!(filter.entity_type.as_deref(), Some("Order"));
        assert_eq!(filter.operation, Some(EventKind::Updated));
        assert_eq!(filter.tenant, TenantScope::Tenant("a".to_string()));
    }
}
