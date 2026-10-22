#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use std::collections::HashMap;

use super::*;
use crate::error::ObserverError;

#[test]
fn test_transport_kind_default() {
    let kind = TransportKind::default();
    assert_eq!(kind, TransportKind::Postgres);
}

#[test]
fn test_transport_config_default() {
    let config = TransportConfig::default();
    assert_eq!(config.transport, TransportKind::Postgres);
}

#[test]
fn test_transport_config_validation() {
    // Valid postgres config
    let config = TransportConfig::default();
    config
        .validate()
        .unwrap_or_else(|e| panic!("default transport config should be valid: {e}"));

    // Invalid: NATS transport without URL
    let config = TransportConfig {
        transport: TransportKind::Nats,
        nats:      NatsTransportConfig {
            url: String::new(),
            ..Default::default()
        },
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidConfig { .. })),
        "NATS without URL should fail: {result:?}"
    );
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
    config
        .validate()
        .unwrap_or_else(|e| panic!("default jetstream config should be valid: {e}"));

    // Invalid: dedup window = 0
    let config = JetStreamConfig {
        dedup_window_minutes: 0,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidConfig { .. })),
        "dedup_window=0 should fail: {result:?}"
    );

    // Invalid: dedup window > 60
    let config = JetStreamConfig {
        dedup_window_minutes: 61,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidConfig { .. })),
        "dedup_window=61 should fail: {result:?}"
    );

    // Invalid: ack_wait = 0
    let config = JetStreamConfig {
        ack_wait_secs: 0,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidConfig { .. })),
        "ack_wait=0 should fail: {result:?}"
    );
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
            url:                None,
            url_env:            None,
            method:             None,
            headers:            HashMap::new(),
            body_template:      None,
            signing_secret:     None,
            signing_secret_env: None,
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
        url:                None,
        url_env:            None,
        method:             None,
        headers:            HashMap::new(),
        body_template:      None,
        signing_secret:     None,
        signing_secret_env: None,
    };

    let result = invalid.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidActionConfig { .. })),
        "webhook without url should fail: {result:?}"
    );

    let valid = ActionConfig::Webhook {
        url:                Some("https://example.com".to_string()),
        url_env:            None,
        method:             None,
        headers:            HashMap::new(),
        body_template:      Some("{}".to_string()),
        signing_secret:     None,
        signing_secret_env: None,
    };

    valid
        .validate()
        .unwrap_or_else(|e| panic!("valid webhook config should pass: {e}"));
}

#[test]
fn test_webhook_signing_secret_env_deserializes() {
    // #345: the signing-secret env-var NAME is captured from config.
    let json = serde_json::json!({
        "type": "webhook",
        "url": "https://example.com/hook",
        "signing_secret_env": "MY_WEBHOOK_SECRET"
    });
    let action: ActionConfig = serde_json::from_value(json).expect("deserialize webhook");
    match action {
        ActionConfig::Webhook {
            signing_secret_env, ..
        } => {
            assert_eq!(signing_secret_env.as_deref(), Some("MY_WEBHOOK_SECRET"));
        },
        other => panic!("expected webhook, got {other:?}"),
    }
}

#[test]
fn test_webhook_signing_secret_env_defaults_to_none() {
    let json = serde_json::json!({ "type": "webhook", "url": "https://example.com/hook" });
    let action: ActionConfig = serde_json::from_value(json).expect("deserialize webhook");
    match action {
        ActionConfig::Webhook {
            signing_secret_env, ..
        } => {
            assert_eq!(signing_secret_env, None);
        },
        other => panic!("expected webhook, got {other:?}"),
    }
}

#[test]
fn test_webhook_empty_signing_secret_env_is_rejected() {
    // An empty NAME is a config error (it would silently never resolve a secret).
    let action = ActionConfig::Webhook {
        url:                Some("https://example.com".to_string()),
        url_env:            None,
        method:             None,
        headers:            HashMap::new(),
        body_template:      None,
        signing_secret:     None,
        signing_secret_env: Some(String::new()),
    };
    assert!(
        matches!(action.validate(), Err(ObserverError::InvalidActionConfig { .. })),
        "empty signing_secret_env must be rejected"
    );
}

#[test]
fn test_webhook_signing_secret_literal_deserializes() {
    // #467: a per-subscription signing-secret literal is captured from config
    // (round-trips through the observer `actions` JSONB).
    let json = serde_json::json!({
        "type": "webhook",
        "url": "https://example.com/hook",
        "signing_secret": "whsec_per_subscription"
    });
    let action: ActionConfig = serde_json::from_value(json).expect("deserialize webhook");
    match action {
        ActionConfig::Webhook { signing_secret, .. } => {
            assert_eq!(signing_secret.as_deref(), Some("whsec_per_subscription"));
        },
        other => panic!("expected webhook, got {other:?}"),
    }
}

#[test]
fn test_webhook_empty_signing_secret_literal_is_rejected() {
    let action = ActionConfig::Webhook {
        url:                Some("https://example.com".to_string()),
        url_env:            None,
        method:             None,
        headers:            HashMap::new(),
        body_template:      None,
        signing_secret:     Some(String::new()),
        signing_secret_env: None,
    };
    assert!(
        matches!(action.validate(), Err(ObserverError::InvalidActionConfig { .. })),
        "empty signing_secret literal must be rejected"
    );
}

#[test]
fn test_webhook_both_signing_secret_sources_rejected() {
    // #467: a literal and an env-var name on the same action is ambiguous.
    let action = ActionConfig::Webhook {
        url:                Some("https://example.com".to_string()),
        url_env:            None,
        method:             None,
        headers:            HashMap::new(),
        body_template:      None,
        signing_secret:     Some("whsec_literal".to_string()),
        signing_secret_env: Some("MY_WEBHOOK_SECRET".to_string()),
    };
    assert!(
        matches!(action.validate(), Err(ObserverError::InvalidActionConfig { .. })),
        "setting both signing_secret and signing_secret_env must be rejected"
    );
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

    let result = invalid.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidActionConfig { .. })),
        "email without fields should fail: {result:?}"
    );

    let valid = ActionConfig::Email {
        to:               Some("user@example.com".to_string()),
        to_template:      None,
        subject:          Some("Test".to_string()),
        subject_template: None,
        body_template:    Some("Body".to_string()),
        reply_to:         None,
    };

    valid
        .validate()
        .unwrap_or_else(|e| panic!("valid email config should pass: {e}"));
}

#[test]
fn test_redis_config_defaults() {
    let config = RedisConfig::default();
    assert!(config.url.contains("localhost:6379"));
    assert_eq!(config.pool_size, 10);
    assert_eq!(config.connect_timeout_secs, 5);
    assert_eq!(config.command_timeout_secs, 2);
    assert_eq!(config.cache_ttl_secs, 60);
}

#[test]
fn test_redis_config_validation() {
    // Valid config
    let config = RedisConfig::default();
    config
        .validate()
        .unwrap_or_else(|e| panic!("default redis config should be valid: {e}"));

    // Invalid: empty URL
    let config = RedisConfig {
        url: String::new(),
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidConfig { .. })),
        "empty redis URL should fail: {result:?}"
    );

    // Invalid: pool_size = 0
    let config = RedisConfig {
        pool_size: 0,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidConfig { .. })),
        "pool_size=0 should fail: {result:?}"
    );

    // Invalid: cache_ttl too large
    let config = RedisConfig {
        cache_ttl_secs: 3601,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(ObserverError::InvalidConfig { .. })),
        "cache_ttl=3601 should fail: {result:?}"
    );
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
fn transport_config_from_env_overrides_kind() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", Some("nats"))], || {
        let cfg = TransportConfig::default().with_env_overrides();
        assert_eq!(cfg.transport, TransportKind::Nats);
    });
}

#[test]
fn transport_config_from_env_unset_preserves_defaults() {
    temp_env::with_vars([("FRAISEQL_OBSERVER_TRANSPORT", None::<&str>)], || {
        let default = TransportConfig::default();
        let cfg = TransportConfig::default().with_env_overrides();
        assert_eq!(cfg.transport, default.transport);
    });
}

// ── #428: SMS, push and search are not action types ──
//
// They had no transport (H24): kept "so existing configs still deserialize", then refused
// by `validate()`, a compatibility shim. A config naming one is refused when it is parsed,
// naming the type and the types that exist.

#[test]
fn an_action_type_with_no_transport_is_refused_when_parsed() {
    for (kind, action) in [
        ("sms", serde_json::json!({ "type": "sms", "phone": "+15551234567" })),
        ("push", serde_json::json!({ "type": "push", "device_token": "token" })),
        ("search", serde_json::json!({ "type": "search", "index": "users" })),
    ] {
        let err = serde_json::from_value::<ActionConfig>(action).expect_err(kind).to_string();
        assert!(
            err.contains(&format!("`{kind}`")) && err.contains("webhook") && err.contains("email"),
            "{kind}: {err}"
        );
    }
}

// ── #632: database / log actions validate for real ──

#[test]
fn test_database_action_accepts_identifiers_and_rejects_injection() {
    for good in ["fn_notify", "app.fn_notify", "_private", "a1.b2"] {
        let action = ActionConfig::Database {
            function_name: good.to_string(),
            params:        None,
        };
        assert!(action.validate().is_ok(), "{good:?} is a valid SQL function identifier");
    }
    for bad in [
        "fn; DROP TABLE tb_observer; --",
        "pg_sleep(10)",
        "a.b.c",
        "fn name",
        "",
        "1fn",
        "schema.",
        ".fn",
    ] {
        let action = ActionConfig::Database {
            function_name: bad.to_string(),
            params:        None,
        };
        assert!(
            matches!(action.validate(), Err(ObserverError::InvalidActionConfig { .. })),
            "{bad:?} must be rejected — the name is interpolated into SQL (#632)"
        );
    }
}

#[test]
fn test_log_action_validates_level_and_template() {
    for level in ["trace", "debug", "info", "warn", "error"] {
        let action = ActionConfig::Log {
            level:            level.to_string(),
            message_template: "m".to_string(),
        };
        assert!(action.validate().is_ok(), "{level:?} is a valid log level");
    }
    let bad_level = ActionConfig::Log {
        level:            "verbose".to_string(),
        message_template: "m".to_string(),
    };
    assert!(matches!(bad_level.validate(), Err(ObserverError::InvalidActionConfig { .. })));
    let empty_template = ActionConfig::Log {
        level:            "info".to_string(),
        message_template: String::new(),
    };
    assert!(matches!(
        empty_template.validate(),
        Err(ObserverError::InvalidActionConfig { .. })
    ));
}

/// The admin-API DTO shape (`tb_observer.actions` JSONB) must deserialize into
/// the runtime variants — that JSONB is exactly what `convert_observer` feeds
/// through `serde_json::from_value` at load (#632).
#[test]
fn test_database_and_log_deserialize_from_admin_dto_shape() {
    let actions: Vec<ActionConfig> = serde_json::from_value(serde_json::json!([
        {"type": "database", "function_name": "app.fn_notify", "params": {"channel": "ops"}},
        {"type": "log", "level": "warn", "message_template": "order {{ id }} changed"},
        {"type": "log", "message_template": "level defaults to info"}
    ]))
    .expect("admin DTO action JSONB must deserialize into runtime ActionConfig");
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0].action_type(), "database");
    assert_eq!(actions[1].action_type(), "log");
    assert!(
        matches!(&actions[2], ActionConfig::Log { level, .. } if level == "info"),
        "level must default to info"
    );
    for a in &actions {
        assert!(a.validate().is_ok(), "{} from DTO shape must validate", a.action_type());
    }
}

// ── #428: cache invalidation now has a real Redis transport ──
//
// A well-formed `cache`/invalidate config validates (transport availability is
// enforced at dispatch, like the email action). `refresh` and an empty pattern
// still fail loud at config-load.

#[test]
fn test_cache_invalidate_action_config_is_accepted() {
    let action = ActionConfig::Cache {
        key_pattern: "app:user:{{ id }}".to_string(),
        action:      "invalidate".to_string(),
    };
    assert!(
        action.validate().is_ok(),
        "a well-formed cache/invalidate action config must validate (#428)"
    );
}

#[test]
fn test_cache_refresh_action_config_is_rejected() {
    let action = ActionConfig::Cache {
        key_pattern: "user:*".to_string(),
        action:      "refresh".to_string(),
    };
    assert!(
        matches!(action.validate(), Err(ObserverError::InvalidActionConfig { .. })),
        "cache 'refresh' has no transport and must fail loud (#428)"
    );
}

#[test]
fn test_cache_action_config_rejects_empty_key_pattern() {
    let action = ActionConfig::Cache {
        key_pattern: String::new(),
        action:      "invalidate".to_string(),
    };
    assert!(
        matches!(action.validate(), Err(ObserverError::InvalidActionConfig { .. })),
        "an empty key_pattern must fail loud (#428)"
    );
}

/// The keys that configured the removed PostgreSQL → NATS bridge and Redis dedup store
/// are refused, not accepted and ignored.
#[test]
fn a_key_of_a_removed_subsystem_is_refused() {
    for (key, value) in [
        ("run_bridge", serde_json::json!(true)),
        ("run_executors", serde_json::json!(true)),
        ("bridge", serde_json::json!({})),
    ] {
        let err = serde_json::from_value::<TransportConfig>(serde_json::json!({ key: value }))
            .expect_err(key);
        assert!(err.to_string().contains(key), "{key}: {err}");
    }
    let err =
        serde_json::from_value::<RedisConfig>(serde_json::json!({ "dedup_window_secs": 300 }))
            .expect_err("dedup_window_secs");
    assert!(err.to_string().contains("dedup_window_secs"), "{err}");
}
