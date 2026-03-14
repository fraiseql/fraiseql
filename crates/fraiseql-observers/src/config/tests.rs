#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use super::*;

#[test]
fn test_observer_runtime_config_defaults() {
    let config: ObserverRuntimeConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(config.channel_capacity, 1000);
    assert_eq!(config.max_concurrency, 50);
    assert_eq!(config.backlog_alert_threshold, 500);
    assert_eq!(config.shutdown_timeout, "30s");
}

#[test]
fn test_max_dlq_size_defaults_to_none() {
    let config: ObserverRuntimeConfig = serde_json::from_str("{}").unwrap();
    assert!(config.max_dlq_size.is_none());
    assert!(config.validate().is_ok());
}

#[test]
fn test_max_dlq_size_defaults_to_none_from_json() {
    let config: ObserverRuntimeConfig = serde_json::from_str("{}").unwrap();
    assert!(config.max_dlq_size.is_none());
}

#[test]
fn test_max_dlq_size_zero_is_invalid() {
    let config: ObserverRuntimeConfig = serde_json::from_str(r#"{"max_dlq_size": 0}"#).unwrap();
    assert_eq!(config.max_dlq_size, Some(0));
    assert!(config.validate().is_err(), "max_dlq_size = 0 must be rejected by validate()");
}

#[test]
fn test_max_dlq_size_positive_is_valid() {
    let config: ObserverRuntimeConfig = serde_json::from_str(r#"{"max_dlq_size": 10000}"#).unwrap();
    assert_eq!(config.max_dlq_size, Some(10_000));
    assert!(config.validate().is_ok());
}

#[test]
fn test_max_dlq_size_one_is_valid() {
    let config: ObserverRuntimeConfig = serde_json::from_str(r#"{"max_dlq_size": 1}"#).unwrap();
    assert_eq!(config.max_dlq_size, Some(1));
    assert!(config.validate().is_ok());
}

#[test]
fn test_transport_kind_default() {
    let kind = TransportKind::default();
    assert_eq!(kind, TransportKind::Postgres);
}

#[test]
fn test_transport_config_default() {
    let config = TransportConfig::default();
    assert_eq!(config.transport, TransportKind::Postgres);
    assert!(!config.run_bridge);
    assert!(config.run_executors);
}

#[test]
fn test_transport_config_validation() {
    // Valid postgres config
    let config = TransportConfig::default();
    assert!(config.validate().is_ok());

    // Invalid: NATS transport without URL
    let config = TransportConfig {
        transport: TransportKind::Nats,
        nats: NatsTransportConfig {
            url: String::new(),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: run_bridge=true with postgres transport
    let config = TransportConfig {
        transport: TransportKind::Postgres,
        run_bridge: true,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_nats_transport_config_default() {
    let config = NatsTransportConfig::default();
    assert!(config.url.contains("localhost:4222"));
    assert_eq!(config.subject_prefix, "fraiseql.mutation");
    assert_eq!(config.consumer_name, "fraiseql_observer_worker");
}

#[test]
fn test_jetstream_config_default() {
    let config = JetStreamConfig::default();
    assert_eq!(config.dedup_window_minutes, 5);
    assert_eq!(config.max_age_days, 7);
    assert_eq!(config.max_msgs, 10_000_000);
    assert_eq!(config.ack_wait_secs, 30);
    assert_eq!(config.max_deliver, 3);
}

#[test]
fn test_jetstream_config_validation() {
    // Valid config
    let config = JetStreamConfig::default();
    assert!(config.validate().is_ok());

    // Invalid: dedup window = 0
    let config = JetStreamConfig {
        dedup_window_minutes: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: dedup window > 60
    let config = JetStreamConfig {
        dedup_window_minutes: 61,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: ack_wait = 0
    let config = JetStreamConfig {
        ack_wait_secs: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_bridge_transport_config_default() {
    let config = BridgeTransportConfig::default();
    assert_eq!(config.transport_name, "pg_to_nats");
    assert_eq!(config.batch_size, 100);
    assert_eq!(config.poll_interval_secs, 1);
    assert_eq!(config.notify_channel, "fraiseql_events");
}

#[test]
fn test_bridge_transport_config_validation() {
    // Valid config
    let config = BridgeTransportConfig::default();
    assert!(config.validate().is_ok());

    // Invalid: empty transport_name
    let config = BridgeTransportConfig {
        transport_name: String::new(),
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: batch_size = 0
    let config = BridgeTransportConfig {
        batch_size: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: batch_size > 10000
    let config = BridgeTransportConfig {
        batch_size: 10001,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: poll_interval = 0
    let config = BridgeTransportConfig {
        poll_interval_secs: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_retry_config_defaults() {
    let config = RetryConfig::default();
    assert_eq!(config.max_attempts, 3);
    assert_eq!(config.initial_delay_ms, 100);
    assert_eq!(config.max_delay_ms, 30000);
}

#[test]
fn test_action_type_names() {
    assert_eq!(
        ActionConfig::Webhook {
            url:           None,
            url_env:       None,
            headers:       HashMap::new(),
            body_template: None,
        }
        .action_type(),
        "webhook"
    );

    assert_eq!(
        ActionConfig::Email {
            to:               None,
            to_template:      None,
            subject:          None,
            subject_template: None,
            body_template:    None,
            reply_to:         None,
        }
        .action_type(),
        "email"
    );
}

#[test]
fn test_webhook_action_validation() {
    let invalid = ActionConfig::Webhook {
        url:           None,
        url_env:       None,
        headers:       HashMap::new(),
        body_template: None,
    };

    assert!(invalid.validate().is_err());

    let valid = ActionConfig::Webhook {
        url:           Some("https://example.com".to_string()),
        url_env:       None,
        headers:       HashMap::new(),
        body_template: Some("{}".to_string()),
    };

    assert!(valid.validate().is_ok());
}

#[test]
fn test_email_action_validation() {
    let invalid = ActionConfig::Email {
        to:               None,
        to_template:      None,
        subject:          None,
        subject_template: None,
        body_template:    None,
        reply_to:         None,
    };

    assert!(invalid.validate().is_err());

    let valid = ActionConfig::Email {
        to:               Some("user@example.com".to_string()),
        to_template:      None,
        subject:          Some("Test".to_string()),
        subject_template: None,
        body_template:    Some("Body".to_string()),
        reply_to:         None,
    };

    assert!(valid.validate().is_ok());
}

#[test]
fn test_multi_listener_config_defaults() {
    let config = MultiListenerConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.lease_duration_ms, 30000);
    assert_eq!(config.health_check_interval_ms, 5000);
    assert_eq!(config.failover_threshold_ms, 60000);
    assert_eq!(config.max_listeners, 10);
}

#[test]
fn test_multi_listener_config_validation() {
    let valid_config = MultiListenerConfig::default();
    assert!(valid_config.validate().is_ok());

    let invalid_lease = MultiListenerConfig {
        lease_duration_ms: 0,
        ..Default::default()
    };
    assert!(invalid_lease.validate().is_err());

    let invalid_health_check = MultiListenerConfig {
        health_check_interval_ms: 0,
        ..Default::default()
    };
    assert!(invalid_health_check.validate().is_err());

    let invalid_threshold = MultiListenerConfig {
        failover_threshold_ms: 1000,
        health_check_interval_ms: 5000,
        ..Default::default()
    };
    assert!(invalid_threshold.validate().is_err());

    let invalid_max_listeners = MultiListenerConfig {
        max_listeners: 0,
        ..Default::default()
    };
    assert!(invalid_max_listeners.validate().is_err());
}

#[test]
fn test_multi_listener_config_builder() {
    let config = MultiListenerConfig::new()
        .enable()
        .with_listener_id("test-listener".to_string())
        .with_lease_duration_ms(20000);

    assert!(config.enabled);
    assert_eq!(config.listener_id, "test-listener");
    assert_eq!(config.lease_duration_ms, 20000);
}

#[test]
fn test_redis_config_defaults() {
    let config = RedisConfig::default();
    assert!(config.url.contains("localhost:6379"));
    assert_eq!(config.pool_size, 10);
    assert_eq!(config.connect_timeout_secs, 5);
    assert_eq!(config.command_timeout_secs, 2);
    assert_eq!(config.dedup_window_secs, 300);
    assert_eq!(config.cache_ttl_secs, 60);
}

#[test]
fn test_redis_config_validation() {
    // Valid config
    let config = RedisConfig::default();
    assert!(config.validate().is_ok());

    // Invalid: empty URL
    let config = RedisConfig {
        url: String::new(),
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: pool_size = 0
    let config = RedisConfig {
        pool_size: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Invalid: dedup_window too large
    let config = RedisConfig {
        dedup_window_secs: 3601,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_performance_config_defaults() {
    let config = PerformanceConfig::default();
    assert!(!config.enable_dedup);
    assert!(!config.enable_caching);
    assert!(config.enable_concurrent);
    assert_eq!(config.max_concurrent_actions, 10);
    assert_eq!(config.concurrent_timeout_ms, 30000);
}

#[test]
fn test_performance_config_validation() {
    // Valid config (no Redis features enabled)
    let config = PerformanceConfig::default();
    assert!(config.validate(false).is_ok());

    // Invalid: enable_dedup without Redis
    let config = PerformanceConfig {
        enable_dedup: true,
        ..Default::default()
    };
    assert!(config.validate(false).is_err());
    assert!(config.validate(true).is_ok()); // OK with Redis

    // Invalid: enable_caching without Redis
    let config = PerformanceConfig {
        enable_caching: true,
        ..Default::default()
    };
    assert!(config.validate(false).is_err());
    assert!(config.validate(true).is_ok()); // OK with Redis

    // Invalid: max_concurrent_actions = 0
    let config = PerformanceConfig {
        max_concurrent_actions: 0,
        ..Default::default()
    };
    assert!(config.validate(false).is_err());
}

// ========================================================================
// Environment variable override tests
//
// Each from_env / with_env_overrides method is tested with:
//   (a) env var set to a valid value — override is applied
//   (b) env var set to an invalid/unknown value — falls back to default
//   (c) env var unset — default is preserved
// ========================================================================

#[test]
fn transport_kind_from_env_postgres() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("postgres"))], || {
        assert_eq!(TransportKind::from_env(), Some(TransportKind::Postgres));
    });
}

#[test]
fn transport_kind_from_env_postgresql_alias() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("postgresql"))], || {
        assert_eq!(TransportKind::from_env(), Some(TransportKind::Postgres));
    });
}

#[test]
fn transport_kind_from_env_nats() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("nats"))], || {
        assert_eq!(TransportKind::from_env(), Some(TransportKind::Nats));
    });
}

#[test]
fn transport_kind_from_env_in_memory() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("in_memory"))], || {
        assert_eq!(TransportKind::from_env(), Some(TransportKind::InMemory));
    });
}

#[test]
fn transport_kind_from_env_memory_alias() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("memory"))], || {
        assert_eq!(TransportKind::from_env(), Some(TransportKind::InMemory));
    });
}

#[test]
fn transport_kind_from_env_unknown_returns_none() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("kafka"))], || {
        assert_eq!(TransportKind::from_env(), None);
    });
}

#[test]
fn transport_kind_from_env_unset_returns_none() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", None::<&str>)], || {
        assert_eq!(TransportKind::from_env(), None);
    });
}

#[test]
fn redis_config_from_env_overrides_url() {
    temp_env::with_vars([("FRAISEQL_REDIS_URL", Some("redis://custom:6380"))], || {
        let cfg = RedisConfig::default().with_env_overrides();
        assert_eq!(cfg.url, "redis://custom:6380");
    });
}

#[test]
fn redis_config_from_env_overrides_pool_size() {
    temp_env::with_vars([("FRAISEQL_REDIS_POOL_SIZE", Some("25"))], || {
        let cfg = RedisConfig::default().with_env_overrides();
        assert_eq!(cfg.pool_size, 25);
    });
}

#[test]
fn redis_config_from_env_invalid_pool_size_preserves_default() {
    temp_env::with_vars([("FRAISEQL_REDIS_POOL_SIZE", Some("not_a_number"))], || {
        let default = RedisConfig::default();
        let cfg = RedisConfig::default().with_env_overrides();
        assert_eq!(cfg.pool_size, default.pool_size);
    });
}

#[test]
fn redis_config_from_env_url_unset_preserves_default() {
    temp_env::with_vars([("FRAISEQL_REDIS_URL", None::<&str>)], || {
        let default = RedisConfig::default();
        let cfg = RedisConfig::default().with_env_overrides();
        assert_eq!(cfg.url, default.url);
    });
}

#[test]
fn nats_transport_config_from_env_overrides_url() {
    temp_env::with_vars([("FRAISEQL_NATS_URL", Some("nats://custom-nats:4222"))], || {
        let cfg = NatsTransportConfig::default().with_env_overrides();
        assert_eq!(cfg.url, "nats://custom-nats:4222");
    });
}

#[test]
fn nats_transport_config_from_env_overrides_subject_prefix() {
    temp_env::with_vars([("FRAISEQL_NATS_SUBJECT_PREFIX", Some("myapp.events"))], || {
        let cfg = NatsTransportConfig::default().with_env_overrides();
        assert_eq!(cfg.subject_prefix, "myapp.events");
    });
}

#[test]
fn nats_transport_config_from_env_overrides_consumer_name() {
    temp_env::with_vars([("FRAISEQL_NATS_CONSUMER_NAME", Some("my_consumer"))], || {
        let cfg = NatsTransportConfig::default().with_env_overrides();
        assert_eq!(cfg.consumer_name, "my_consumer");
    });
}

#[test]
fn nats_transport_config_from_env_url_unset_preserves_default() {
    temp_env::with_vars([("FRAISEQL_NATS_URL", None::<&str>)], || {
        let default = NatsTransportConfig::default();
        let cfg = NatsTransportConfig::default().with_env_overrides();
        assert_eq!(cfg.url, default.url);
    });
}

#[test]
fn jetstream_config_from_env_overrides_dedup_window() {
    temp_env::with_vars([("FRAISEQL_NATS_DEDUP_WINDOW_MINUTES", Some("10"))], || {
        let cfg = JetStreamConfig::default().with_env_overrides();
        assert_eq!(cfg.dedup_window_minutes, 10);
    });
}

#[test]
fn jetstream_config_from_env_overrides_max_age_days() {
    temp_env::with_vars([("FRAISEQL_NATS_MAX_AGE_DAYS", Some("14"))], || {
        let cfg = JetStreamConfig::default().with_env_overrides();
        assert_eq!(cfg.max_age_days, 14);
    });
}

#[test]
fn jetstream_config_from_env_unset_preserves_defaults() {
    temp_env::with_vars(
        [
            ("FRAISEQL_NATS_DEDUP_WINDOW_MINUTES", None::<&str>),
            ("FRAISEQL_NATS_MAX_AGE_DAYS", None::<&str>),
        ],
        || {
            let default = JetStreamConfig::default();
            let cfg = JetStreamConfig::default().with_env_overrides();
            assert_eq!(cfg.dedup_window_minutes, default.dedup_window_minutes);
            assert_eq!(cfg.max_age_days, default.max_age_days);
        },
    );
}

#[test]
fn job_queue_config_from_env_overrides_batch_size() {
    temp_env::with_vars([("FRAISEQL_JOB_QUEUE_BATCH_SIZE", Some("200"))], || {
        let cfg = JobQueueConfig::default().with_env_overrides();
        assert_eq!(cfg.batch_size, 200);
    });
}

#[test]
fn job_queue_config_from_env_overrides_worker_concurrency() {
    temp_env::with_vars([("FRAISEQL_JOB_QUEUE_WORKER_CONCURRENCY", Some("8"))], || {
        let cfg = JobQueueConfig::default().with_env_overrides();
        assert_eq!(cfg.worker_concurrency, 8);
    });
}

#[test]
fn job_queue_config_from_env_unset_preserves_defaults() {
    temp_env::with_vars([("FRAISEQL_JOB_QUEUE_BATCH_SIZE", None::<&str>)], || {
        let default = JobQueueConfig::default();
        let cfg = JobQueueConfig::default().with_env_overrides();
        assert_eq!(cfg.batch_size, default.batch_size);
    });
}

#[test]
fn performance_config_from_env_enables_dedup() {
    temp_env::with_vars([("FRAISEQL_ENABLE_DEDUP", Some("true"))], || {
        let cfg = PerformanceConfig::default().with_env_overrides();
        assert!(cfg.enable_dedup, "FRAISEQL_ENABLE_DEDUP=true should enable dedup");
    });
}

#[test]
fn performance_config_from_env_enables_caching() {
    temp_env::with_vars([("FRAISEQL_ENABLE_CACHING", Some("1"))], || {
        let cfg = PerformanceConfig::default().with_env_overrides();
        assert!(cfg.enable_caching, "FRAISEQL_ENABLE_CACHING=1 should enable caching");
    });
}

#[test]
fn performance_config_from_env_overrides_max_concurrent_actions() {
    temp_env::with_vars([("FRAISEQL_MAX_CONCURRENT_ACTIONS", Some("20"))], || {
        let cfg = PerformanceConfig::default().with_env_overrides();
        assert_eq!(cfg.max_concurrent_actions, 20);
    });
}

#[test]
fn performance_config_from_env_unset_preserves_defaults() {
    temp_env::with_vars(
        [
            ("FRAISEQL_ENABLE_DEDUP", None::<&str>),
            ("FRAISEQL_ENABLE_CACHING", None::<&str>),
            ("FRAISEQL_MAX_CONCURRENT_ACTIONS", None::<&str>),
        ],
        || {
            let default = PerformanceConfig::default();
            let cfg = PerformanceConfig::default().with_env_overrides();
            assert_eq!(cfg.enable_dedup, default.enable_dedup);
            assert_eq!(cfg.enable_caching, default.enable_caching);
            assert_eq!(cfg.max_concurrent_actions, default.max_concurrent_actions);
        },
    );
}

#[test]
fn clickhouse_config_from_env_overrides_url() {
    temp_env::with_vars([("FRAISEQL_CLICKHOUSE_URL", Some("http://ch-server:8123"))], || {
        let cfg = ClickHouseConfig::default().with_env_overrides();
        assert_eq!(cfg.url, "http://ch-server:8123");
    });
}

#[test]
fn clickhouse_config_from_env_overrides_database() {
    temp_env::with_vars([("FRAISEQL_CLICKHOUSE_DATABASE", Some("analytics"))], || {
        let cfg = ClickHouseConfig::default().with_env_overrides();
        assert_eq!(cfg.database, "analytics");
    });
}

#[test]
fn clickhouse_config_from_env_unset_preserves_defaults() {
    temp_env::with_vars(
        [
            ("FRAISEQL_CLICKHOUSE_URL", None::<&str>),
            ("FRAISEQL_CLICKHOUSE_DATABASE", None::<&str>),
        ],
        || {
            let default = ClickHouseConfig::default();
            let cfg = ClickHouseConfig::default().with_env_overrides();
            assert_eq!(cfg.url, default.url);
            assert_eq!(cfg.database, default.database);
        },
    );
}

#[test]
fn transport_config_from_env_overrides_kind() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("nats"))], || {
        let cfg = TransportConfig::default().with_env_overrides();
        assert_eq!(cfg.transport, TransportKind::Nats);
    });
}

#[test]
fn transport_config_from_env_enables_bridge() {
    temp_env::with_vars(
        [
            ("FRAISEQL_OBSERVER_TRANSPORT", Some("nats")),
            ("FRAISEQL_NATS_ENABLE_BRIDGE", Some("true")),
        ],
        || {
            let cfg = TransportConfig::default().with_env_overrides();
            assert!(cfg.run_bridge, "FRAISEQL_NATS_ENABLE_BRIDGE=true should enable bridge");
        },
    );
}

#[test]
fn transport_config_from_env_unset_preserves_defaults() {
    temp_env::with_vars(
        [
            ("FRAISEQL_OBSERVER_TRANSPORT", None::<&str>),
            ("FRAISEQL_NATS_ENABLE_BRIDGE", None::<&str>),
        ],
        || {
            let default = TransportConfig::default();
            let cfg = TransportConfig::default().with_env_overrides();
            assert_eq!(cfg.transport, default.transport);
            assert_eq!(cfg.run_bridge, default.run_bridge);
        },
    );
}
