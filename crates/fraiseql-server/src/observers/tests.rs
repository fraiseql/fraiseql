mod redaction_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::redact_action_secrets;

    #[test]
    fn webhook_header_values_are_redacted_keys_preserved() {
        let mut actions = serde_json::json!([
            {
                "type": "webhook",
                "url": "https://hook.example/path",
                "method": "POST",
                "headers": { "Authorization": "Bearer super-secret", "X-Api-Key": "k-123" }
            }
        ]);
        redact_action_secrets(&mut actions);

        let headers = &actions[0]["headers"];
        assert_eq!(headers["Authorization"], "[REDACTED]");
        assert_eq!(headers["X-Api-Key"], "[REDACTED]");
        // Non-secret fields are preserved verbatim.
        assert_eq!(actions[0]["url"], "https://hook.example/path");
        assert_eq!(actions[0]["method"], "POST");
    }

    #[test]
    fn webhook_signing_secret_literal_is_redacted_env_name_preserved() {
        // #467: the per-subscription `signing_secret` literal is a secret at rest
        // and must never travel in a response body; `signing_secret_env` is only
        // an env-var NAME and stays visible.
        let mut actions = serde_json::json!([
            {
                "type": "webhook",
                "url": "https://hook.example/path",
                "signing_secret": "whsec_per_subscription",
                "signing_secret_env": "MY_WEBHOOK_SECRET"
            }
        ]);
        redact_action_secrets(&mut actions);

        assert_eq!(actions[0]["signing_secret"], "[REDACTED]");
        assert_eq!(actions[0]["signing_secret_env"], "MY_WEBHOOK_SECRET");
        assert_eq!(actions[0]["url"], "https://hook.example/path");
    }

    #[test]
    fn webhook_headers_env_names_are_kept_by_the_stored_type_and_shown() {
        // The admin API stores an action through this crate's own `ActionConfig`: a key
        // it did not carry would pass the runtime's check and be dropped on store.
        let written = serde_json::json!({
            "type": "webhook",
            "url": "https://hook.example/path",
            "headers_env": { "Authorization": "HOOK_TOKEN" }
        });
        let stored: super::super::ActionConfig = serde_json::from_value(written.clone()).unwrap();
        let mut actions = serde_json::json!([serde_json::to_value(&stored).unwrap()]);
        assert_eq!(actions[0]["headers_env"], written["headers_env"], "kept on store");

        redact_action_secrets(&mut actions);
        assert_eq!(actions[0]["headers_env"]["Authorization"], "HOOK_TOKEN", "a name is shown");

        let without: super::super::ActionConfig = serde_json::from_value(
            serde_json::json!({ "type": "webhook", "url": "https://hook.example/path" }),
        )
        .unwrap();
        let without = serde_json::to_value(&without).unwrap();
        assert!(without.get("headers_env").is_none(), "absent stays absent: {without}");
    }

    #[test]
    fn actions_without_headers_are_untouched() {
        let mut actions = serde_json::json!([
            { "type": "slack", "webhook_url": "https://slack/x", "message_template": "hi" },
            { "type": "log", "level": "info", "message_template": "m" }
        ]);
        let before = actions.clone();
        redact_action_secrets(&mut actions);
        assert_eq!(actions, before, "actions with no headers must be unchanged");
    }

    #[test]
    fn non_array_actions_are_ignored() {
        let mut actions = serde_json::json!({ "headers": { "k": "v" } });
        let before = actions.clone();
        redact_action_secrets(&mut actions);
        assert_eq!(actions, before, "a non-array actions value must not be mutated");
    }

    #[test]
    fn with_redacted_secrets_redacts_observer_actions() {
        use chrono::Utc;

        use super::super::Observer;

        let observer = Observer {
            pk_observer:          1,
            id:                   uuid::Uuid::nil(),
            name:                 "n".to_string(),
            description:          None,
            entity_type:          None,
            event_type:           None,
            condition_expression: None,
            actions:              serde_json::json!([
                { "type": "webhook", "url": "u", "headers": { "Authorization": "Bearer s" } }
            ]),
            enabled:              true,
            priority:             100,
            retry_config:         serde_json::json!({}),
            timeout_ms:           30_000,
            fk_customer_org:      None,
            created_at:           Utc::now(),
            updated_at:           Utc::now(),
            created_by:           None,
            updated_by:           None,
            deleted_at:           None,
            managed_by:           None,
        };

        let redacted = observer.with_redacted_secrets();
        assert_eq!(redacted.actions[0]["headers"]["Authorization"], "[REDACTED]");
    }
}

mod config_tests {
    use super::super::config::ObserverManagementConfig;

    #[test]
    fn test_default_config() {
        let config = ObserverManagementConfig::default();
        assert!(config.enabled);
        assert_eq!(config.base_path, "/api/observers");
        assert_eq!(config.max_page_size, 100);
        assert!(!config.log_payloads);
        assert_eq!(config.log_retention_days, 30);
        assert!(config.require_auth);
    }
}

mod repository_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::RetryConfig;

    /// The admin API refuses what the compiler refuses of a declared observer, with the
    /// same text: an action names one destination.
    #[test]
    fn a_created_action_naming_two_destinations_is_refused_naming_the_fix() {
        let err =
            serde_json::from_value::<super::super::CreateObserverRequest>(serde_json::json!({
                "name": "order_created",
                "actions": [{"type": "slack", "message_template": "m",
                             "webhook_url": "https://example.test/slack",
                             "webhook_url_env": "SLACK_WEBHOOK_URL"}]
            }))
            .expect_err("an action with two destinations must be refused");

        let message = err.to_string();
        assert!(message.contains("actions[0]"), "{message}");
        assert!(
            message.contains("set one of `webhook_url` / `webhook_url_env`, not both"),
            "{message}"
        );
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.backoff_strategy, "exponential");
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 60000);
    }

    // --- SQL structure unit tests (no database required) ---
    //
    // These verify the central injection-safety invariant: bound values produced by
    // push_bind() are assigned $N placeholders and never appear in the SQL string itself.

    #[test]
    fn test_list_entity_type_not_inlined() {
        let malicious = "' OR '1'='1";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM core.tb_observer WHERE 1=1");
        qb.push(" AND entity_type = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious), "user input must not appear in SQL string");
        assert!(sql.contains("$1"), "placeholder must be present");
    }

    #[test]
    fn test_list_event_type_not_inlined() {
        let malicious = "'; DROP TABLE tb_observer; --";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM core.tb_observer WHERE 1=1");
        qb.push(" AND event_type = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_logs_status_not_inlined() {
        let malicious = "' UNION SELECT * FROM secrets --";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM core.tb_observer_log WHERE 1=1");
        qb.push(" AND status = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_logs_trace_id_not_inlined() {
        let malicious = "x' OR fk_customer_org IS NOT NULL--";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM core.tb_observer_log WHERE 1=1");
        qb.push(" AND trace_id = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_no_filters_produces_minimal_sql() {
        let qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM core.tb_observer WHERE 1=1");
        let sql = qb.sql();
        assert!(!sql.contains("entity_type"));
        assert!(!sql.contains("event_type"));
        assert!(!sql.contains("enabled"));
        assert!(!sql.contains("fk_customer_org"));
        assert!(!sql.contains("deleted_at"));
    }

    #[test]
    fn test_list_exclude_deleted_adds_condition() {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM core.tb_observer WHERE 1=1");
        qb.push(" AND deleted_at IS NULL");
        let sql = qb.sql();
        assert!(sql.contains("deleted_at IS NULL"));
    }

    #[test]
    fn test_list_logs_observer_id_uses_placeholder() {
        let observer_id = uuid::Uuid::new_v4();
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM core.tb_observer_log WHERE 1=1");
        qb.push(" AND fk_observer = (SELECT pk_observer FROM core.tb_observer WHERE id = ")
            .push_bind(observer_id)
            .push(")");
        let sql = qb.sql();
        assert!(!sql.contains(&observer_id.to_string()), "UUID must not be inlined in SQL");
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_multiple_filters_use_sequential_placeholders() {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM core.tb_observer WHERE 1=1");
        qb.push(" AND entity_type = ").push_bind("Order");
        qb.push(" AND event_type = ").push_bind("INSERT");
        qb.push(" AND enabled = ").push_bind(true);
        let sql = qb.sql();
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
        assert!(sql.contains("$3"));
        assert!(!sql.contains("Order"));
        assert!(!sql.contains("INSERT"));
    }
}

mod routes_tests {
    // Note: Integration tests would require a test database
    // These are placeholder tests for route configuration
}

mod runtime_tests {
    use super::super::runtime::RuntimeHealth;

    #[test]
    fn test_runtime_health_default() {
        let health = RuntimeHealth {
            running:          false,
            observer_count:   0,
            last_checkpoint:  None,
            events_processed: 0,
            errors:           0,
        };
        assert!(!health.running);
        assert_eq!(health.observer_count, 0);
    }
}

/// Atomicity tests for the observer `entity_type_index` swap path.
///
/// These tests reproduce the exact `Arc<ArcSwap<HashMap<…>>>` pattern used
/// by `ObserverRuntime::entity_type_index` (the production type is the same;
/// the rebuild semantics are identical). Constructing a real `ObserverRuntime`
/// requires a `PgPool`, so the integration boundary is exercised against the
/// type alias directly — this is the same swap call the production reload
/// path takes (`self.entity_type_index.store(Arc::new(new_map))`).
mod runtime_index_atomicity_tests {
    use std::{collections::HashMap, sync::Arc, thread, time::Duration};

    use arc_swap::ArcSwap;

    type EntityTypeIndex = Arc<ArcSwap<HashMap<(String, String), Vec<i64>>>>;

    fn gen_a() -> HashMap<(String, String), Vec<i64>> {
        let mut m = HashMap::new();
        m.insert(("Order".to_string(), "INSERT".to_string()), vec![1, 2, 3]);
        m.insert(("Order".to_string(), "UPDATE".to_string()), vec![4]);
        m.insert(("User".to_string(), "INSERT".to_string()), vec![5, 6]);
        m
    }

    fn gen_b() -> HashMap<(String, String), Vec<i64>> {
        let mut m = HashMap::new();
        // Same key set, distinct values — so an observer of either generation
        // can be distinguished, but no key is ever missing.
        m.insert(("Order".to_string(), "INSERT".to_string()), vec![10, 20, 30]);
        m.insert(("Order".to_string(), "UPDATE".to_string()), vec![40]);
        m.insert(("User".to_string(), "INSERT".to_string()), vec![50, 60]);
        m
    }

    /// F056 acceptance: under concurrent reload, every lookup returns
    /// **exactly** one of the two known generations — never empty, never a
    /// partial union, never a value from neither.
    ///
    /// A `clear()` + per-key `insert()` rebuild would fail this test because
    /// readers would intermittently observe `None` for keys mid-rebuild or
    /// a per-key value from a generation different from another key in the
    /// same snapshot. The `ArcSwap` snapshot-swap pattern passes because
    /// every reader sees a `Guard` over one whole pre-swap or post-swap map.
    #[test]
    fn entity_type_index_swap_is_snapshot_atomic() {
        let index: EntityTypeIndex = Arc::new(ArcSwap::from_pointee(gen_a()));
        let a = gen_a();
        let b = gen_b();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Writer: alternate Gen_A / Gen_B as fast as possible.
        let writer_index = Arc::clone(&index);
        let writer_stop = Arc::clone(&stop);
        let writer = thread::spawn(move || {
            let mut flip = false;
            while !writer_stop.load(std::sync::atomic::Ordering::Relaxed) {
                let next = if flip { gen_b() } else { gen_a() };
                writer_index.store(Arc::new(next));
                flip = !flip;
            }
        });

        // Readers: 8 threads × ~12_500 lookups each = 100_000 total lookups.
        let mut readers = Vec::new();
        for _ in 0..8 {
            let reader_index = Arc::clone(&index);
            let expected_a = a.clone();
            let expected_b = b.clone();
            readers.push(thread::spawn(move || {
                let keys: Vec<(String, String)> = expected_a.keys().cloned().collect();
                for _ in 0..12_500 {
                    let snapshot = reader_index.load();
                    for key in &keys {
                        let observed = snapshot
                            .get(key)
                            .cloned()
                            .expect("key must be present in every generation");
                        let from_a = expected_a.get(key) == Some(&observed);
                        let from_b = expected_b.get(key) == Some(&observed);
                        assert!(
                            from_a || from_b,
                            "observed value {:?} for key {:?} is from neither Gen_A nor Gen_B",
                            observed,
                            key,
                        );
                    }
                }
            }));
        }

        for r in readers {
            r.join().expect("reader thread panicked");
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().expect("writer thread panicked");
    }

    /// F056 acceptance: after a swap completes, all subsequent loads observe
    /// the post-reload generation (visibility is prompt, not eventual).
    #[test]
    fn entity_type_index_swap_visibility_is_prompt() {
        let index: EntityTypeIndex = Arc::new(ArcSwap::from_pointee(gen_a()));

        // Pre-swap: every key must match Gen_A.
        {
            let pre = index.load();
            for (k, v) in &gen_a() {
                assert_eq!(pre.get(k), Some(v), "pre-swap value mismatch for {:?}", k);
            }
        }

        index.store(Arc::new(gen_b()));

        // Post-swap: every subsequent load (across threads) must match Gen_B.
        let mut handles = Vec::new();
        for _ in 0..4 {
            let reader = Arc::clone(&index);
            handles.push(thread::spawn(move || {
                // A tiny pause to ensure cross-thread visibility through the
                // ArcSwap acquire-load barrier on whichever scheduler runs us.
                thread::sleep(Duration::from_millis(1));
                let snap = reader.load();
                for (k, v) in &gen_b() {
                    assert_eq!(snap.get(k), Some(v), "post-swap value mismatch for {:?}", k);
                }
            }));
        }
        for h in handles {
            h.join().expect("reader thread panicked");
        }
    }
}

mod router_construction {
    //! Router-construction tests.
    //!
    //! Each test calls a router constructor and lets the returned `Router`
    //! drop. Axum validates path-capture syntax inside `Router::route`, so any
    //! lingering axum-0.7 `:param` literal panics here at build time — this is
    //! exactly the bug class behind issue #316.
    //!
    //! The platform E2E suite is gated behind `FRAISEQL_PLATFORM_E2E=1` and
    //! never mounts these routers in default `cargo test` runs, so without
    //! these tests the panic only surfaces at first server boot.

    #![allow(clippy::unwrap_used)] // Reason: test code; pool ctor errors must panic to surface test setup failures

    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
    use fraiseql_test_utils::failing_adapter::FailingAdapter;
    use sqlx::PgPool;
    use tokio::sync::RwLock;

    use crate::observers::{
        ChangelogState, DlqState, ObserverRepository, ObserverState, RuntimeHealthState,
        observer_changelog_routes, observer_dlq_routes, observer_routes, observer_runtime_routes,
        runtime::{ObserverRuntime, ObserverRuntimeConfig},
    };

    fn lazy_pool() -> PgPool {
        PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap()
    }

    fn stub_runtime() -> Arc<RwLock<ObserverRuntime>> {
        Arc::new(RwLock::new(ObserverRuntime::new(ObserverRuntimeConfig::new(lazy_pool()))))
    }

    #[tokio::test]
    async fn observer_routes_constructs() {
        let state = ObserverState {
            repository: ObserverRepository::new(lazy_pool()),
            runtime:    None,
            executor:   Arc::new(ArcSwap::from_pointee(Executor::read_only(
                CompiledSchema::new(),
                Arc::new(FailingAdapter::new()),
            ))),
        };
        let _ = observer_routes(state);
    }

    #[tokio::test]
    async fn observer_runtime_routes_constructs() {
        let state = RuntimeHealthState {
            runtime: stub_runtime(),
        };
        let _ = observer_runtime_routes(state);
    }

    #[tokio::test]
    async fn observer_dlq_routes_constructs() {
        let state = DlqState {
            runtime: stub_runtime(),
        };
        let _ = observer_dlq_routes(state);
    }

    #[tokio::test]
    async fn observer_changelog_routes_constructs() {
        let state = ChangelogState { pool: lazy_pool() };
        let _ = observer_changelog_routes(state);
    }
}

mod retry_config_backoff_wire {
    //! #612 item 11: the admin `RetryConfig` field is `backoff_strategy` (renamed
    //! from `backoff`), so the persisted `retry_config` JSONB the runtime reads
    //! carries the key the runtime actually consumes; `deny_unknown_fields` makes
    //! the dead `backoff` name (or a typo) fail loud instead of silently
    //! defaulting to exponential backoff.
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use fraiseql_observers::{BackoffStrategy, RetryConfig as RuntimeRetryConfig};

    use super::super::RetryConfig as AdminRetryConfig;

    #[test]
    fn admin_backoff_strategy_round_trips_into_the_runtime() {
        let admin = AdminRetryConfig {
            backoff_strategy: "linear".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_value(&admin).unwrap();
        // The persisted JSONB carries `backoff_strategy`, which the runtime reads —
        // the pre-#612 dead `backoff` key is gone.
        assert_eq!(json["backoff_strategy"], "linear");
        assert!(json.get("backoff").is_none(), "the dead `backoff` key is not emitted");

        let runtime: RuntimeRetryConfig = serde_json::from_value(json).unwrap();
        assert!(
            matches!(runtime.backoff_strategy, BackoffStrategy::Linear),
            "the runtime honors the admin-configured strategy instead of defaulting"
        );
    }

    #[test]
    fn admin_dto_rejects_the_dead_backoff_field() {
        let err =
            serde_json::from_value::<AdminRetryConfig>(serde_json::json!({ "backoff": "linear" }))
                .expect_err("the old `backoff` key must be rejected, not silently ignored");
        assert!(err.to_string().contains("backoff"), "error names the offending key: {err}");
    }

    #[test]
    fn runtime_retry_config_rejects_unknown_keys() {
        let err = serde_json::from_value::<RuntimeRetryConfig>(
            serde_json::json!({ "backoff": "linear", "backoff_strategy": "linear" }),
        )
        .expect_err("an unexpected key must fail loud at reload, not be silently dropped");
        assert!(err.to_string().contains("backoff"), "{err}");
    }
}

mod action_type_dispatch {
    //! #612 item 10: action types the runtime cannot dispatch are rejected at
    //! create/update (400) rather than accepted (201) then silently skipped.
    //! Since #632 every current DTO type has a real dispatcher; the gate stays
    //! for future variants.
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::{ActionConfig, handlers::reject_undispatchable_actions};

    fn action(json: serde_json::Value) -> ActionConfig {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn webhook_email_slack_are_dispatchable() {
        for (json, name) in [
            (serde_json::json!({ "type": "webhook", "url": "https://h/x" }), "webhook"),
            (
                serde_json::json!({ "type": "email", "to": "a@b.c", "subject": "s", "body_template": "b" }),
                "email",
            ),
            (
                serde_json::json!({ "type": "slack", "webhook_url": "https://h/x", "message_template": "m" }),
                "slack",
            ),
        ] {
            let a = action(json);
            assert_eq!(a.type_name(), name);
            assert!(a.is_runtime_dispatchable(), "{name} must be dispatchable");
        }
    }

    /// #632: the runtime now has real `database` and `log` dispatchers, so the
    /// admin API must accept those action types again (the #612-era 400 was the
    /// stopgap for "created then silently skipped at load").
    #[test]
    fn database_and_log_are_dispatchable_since_632() {
        for json in [
            serde_json::json!({ "type": "database", "function_name": "fn_notify" }),
            serde_json::json!({ "type": "log", "message_template": "m" }),
        ] {
            let a = action(json);
            assert!(
                a.is_runtime_dispatchable(),
                "{} has a real runtime dispatcher since #632",
                a.type_name()
            );
        }
    }

    #[test]
    fn reject_helper_passes_database_and_log_actions_since_632() {
        let actions = vec![
            action(serde_json::json!({
                "type": "database",
                "function_name": "fn_notify"
            })),
            action(serde_json::json!({
                "type": "log",
                "message_template": "order {{ id }} changed"
            })),
        ];
        assert!(
            reject_undispatchable_actions(&actions).is_none(),
            "#632: database/log actions have real dispatchers and must be accepted"
        );
    }

    #[test]
    fn reject_helper_passes_all_dispatchable_actions() {
        let actions = vec![action(
            serde_json::json!({ "type": "webhook", "url": "https://h/x" }),
        )];
        assert!(
            reject_undispatchable_actions(&actions).is_none(),
            "a webhook-only action set is accepted"
        );
    }
}

mod template_paths {
    //! A webhook `body_template` placeholder the observed entity cannot fill is
    //! refused at create/update (422) rather than dead-lettering every event at delivery.
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use axum::http::StatusCode;
    use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};

    use super::super::{
        ActionConfig,
        handlers::{captured_row_warnings, reject_unfillable_templates},
    };

    /// `Order { id, total, reviewStatus, tags: [String], meta: JSON, customer: Customer }`,
    /// `Customer { email, billingEmail, address: Address }`, `Address { city }`.
    fn schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        let mut order = TypeDefinition::new("Order", "v_order");
        order.fields = vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("total", FieldType::Float),
            FieldDefinition::new("reviewStatus", FieldType::String),
            FieldDefinition::new("tags", FieldType::list(FieldType::String)),
            FieldDefinition::new("meta", FieldType::Json),
            FieldDefinition::new("customer", FieldType::Object("Customer".to_string())),
        ];
        let mut customer = TypeDefinition::new("Customer", "v_customer");
        customer.fields = vec![
            FieldDefinition::new("email", FieldType::String),
            FieldDefinition::new("billingEmail", FieldType::String),
            FieldDefinition::new("address", FieldType::Object("Address".to_string())),
        ];
        let mut address = TypeDefinition::new("Address", "v_address");
        address.fields = vec![FieldDefinition::new("city", FieldType::String)];
        schema.types = vec![order, customer, address];
        schema.build_indexes();
        schema
    }

    fn webhook(template: &str) -> ActionConfig {
        serde_json::from_value(serde_json::json!({
            "type": "webhook", "url": "https://h/x", "body_template": template,
        }))
        .unwrap()
    }

    fn refused(entity: Option<&str>, template: &str) -> Option<String> {
        let response = reject_unfillable_templates(&schema(), entity, &[webhook(template)])?;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = futures::executor::block_on(axum::body::to_bytes(response.into_body(), 1 << 16))
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        Some(body["error"].as_str().unwrap().to_string())
    }

    #[test]
    fn an_unknown_field_is_refused_by_name() {
        let error = refused(Some("Order"), r#"{"x": "{{ nope }}"}"#).unwrap();
        assert!(error.contains("`{{ nope }}`") && error.contains("'Order'"), "{error}");
    }

    #[test]
    fn declared_fields_are_accepted_at_every_depth() {
        assert_eq!(
            refused(
                Some("Order"),
                r#"{"id": "{{ id }}", "to": "{{ customer.email }}", "c": "{{ customer.address.city }}", "t": "{{ tags }}"}"#
            ),
            None
        );
    }

    /// The renderer reads the changed row, and a row holds a field under its stored key:
    /// `reviewStatus` is `review_status` there.
    #[test]
    fn a_multi_word_field_is_named_by_the_key_the_row_holds() {
        assert_eq!(refused(Some("Order"), r#"{"s": "{{ review_status }}"}"#), None);
        assert_eq!(refused(Some("Order"), r#"{"to": "{{ customer.billing_email }}"}"#), None);
    }

    /// The published name is not a key of the row, so it is refused, and the refusal says
    /// which key is.
    #[test]
    fn the_published_name_of_a_multi_word_field_is_refused_naming_the_stored_key() {
        let error = refused(Some("Order"), r#"{"s": "{{ reviewStatus }}"}"#).unwrap();
        assert!(error.contains("`{{ reviewStatus }}`"), "{error}");
        assert!(error.contains("`review_status`"), "names the key the row holds: {error}");

        let error = refused(Some("Order"), r#"{"to": "{{ customer.billingEmail }}"}"#).unwrap();
        assert!(error.contains("`billing_email`"), "at depth too: {error}");
    }

    /// A name that is no field at all gets no such hint.
    #[test]
    fn an_unknown_field_is_not_told_of_a_stored_key() {
        let error = refused(Some("Order"), r#"{"x": "{{ nope }}"}"#).unwrap();
        assert!(!error.contains("row holds"), "{error}");
    }

    #[test]
    fn an_unknown_field_of_a_nested_object_is_refused() {
        let error = refused(Some("Order"), r#"{"c": "{{ customer.address.zip }}"}"#).unwrap();
        assert!(error.contains("customer.address.zip"), "{error}");
    }

    #[test]
    fn a_path_into_a_scalar_or_a_list_is_refused() {
        assert!(refused(Some("Order"), r#"{"x": "{{ total.amount }}"}"#).is_some());
        assert!(refused(Some("Order"), r#"{"x": "{{ tags.first }}"}"#).is_some());
    }

    /// The schema cannot say what a JSON field holds, so a path into it is left to delivery.
    #[test]
    fn a_path_into_json_is_left_to_delivery() {
        assert_eq!(refused(Some("Order"), r#"{"x": "{{ meta.anything }}"}"#), None);
    }

    #[test]
    fn a_text_template_is_checked_too() {
        assert!(refused(Some("Order"), r#"{"n": {{ nope }}}"#).is_some());
        assert_eq!(refused(Some("Order"), r#"{"n": {{ total }}}"#), None);
    }

    /// The JSON template's keys are not rendered, so they are not checked.
    #[test]
    fn a_placeholder_in_a_key_is_not_a_path() {
        assert_eq!(refused(Some("Order"), r#"{"{{ nope }}": "{{ id }}"}"#), None);
    }

    /// An observer on every entity, or on one the schema does not declare, has no fields to
    /// check against.
    #[test]
    fn an_undeclared_or_absent_entity_type_is_not_checked() {
        assert_eq!(refused(None, r#"{"x": "{{ nope }}"}"#), None);
        assert_eq!(refused(Some("Invoice"), r#"{"x": "{{ nope }}"}"#), None);
    }

    /// Every webhook of the observer is checked, not only the first.
    #[test]
    fn a_later_webhook_is_checked() {
        let actions = [
            webhook(r#"{"id": "{{ id }}"}"#),
            webhook(r#"{"x": "{{ nope }}"}"#),
        ];
        assert!(reject_unfillable_templates(&schema(), Some("Order"), &actions).is_some());
    }

    /// `schema()` with `Order` captured from `tb_order`, a table of plain columns.
    fn row_captured() -> CompiledSchema {
        let mut schema = schema();
        schema.subscribable.push(fraiseql_core::schema::SubscribableEntity {
            entity_type: "Order".to_string(),
            tables:      vec!["tb_order".to_string()],
            pre_image:   false,
        });
        schema
    }

    /// A captured row of a plain-column table holds no embedded field. The template is
    /// stored, since a mutation's event fills it, and the log says what a captured one
    /// will not.
    #[test]
    fn a_template_a_captured_row_cannot_fill_is_accepted_and_warned_of() {
        let actions = [webhook(
            r#"{"e": "{{ customer.email }}", "id": "{{ id }}"}"#,
        )];

        assert!(reject_unfillable_templates(&row_captured(), Some("Order"), &actions).is_none());
        let said = captured_row_warnings(&row_captured(), Some("Order"), &actions);

        assert_eq!(said.len(), 1, "one placeholder reads an embedded field: {said:?}");
        assert!(said[0].contains("`{{ customer.email }}`"), "{said:?}");
        assert!(said[0].contains("`tb_order`"), "names the table: {said:?}");
    }

    /// A column the row holds and the document does not is still refused: no mutation's
    /// event would fill it.
    #[test]
    fn a_column_only_the_captured_row_holds_is_still_refused() {
        let actions = [webhook(r#"{"c": "{{ fk_customer }}"}"#)];

        assert!(reject_unfillable_templates(&row_captured(), Some("Order"), &actions).is_some());
    }

    #[test]
    fn nothing_is_said_where_no_table_is_captured_as_its_row() {
        let actions = [webhook(r#"{"e": "{{ customer.email }}"}"#)];

        assert_eq!(captured_row_warnings(&schema(), Some("Order"), &actions), Vec::<String>::new());
        assert_eq!(captured_row_warnings(&row_captured(), None, &actions), Vec::<String>::new());
    }

    /// Every webhook of the observer is read, not only the first.
    #[test]
    fn a_later_webhook_is_warned_of() {
        let actions = [
            webhook(r#"{"id": "{{ id }}"}"#),
            webhook(r#"{"e": "{{ customer.address.city }}"}"#),
        ];

        assert_eq!(captured_row_warnings(&row_captured(), Some("Order"), &actions).len(), 1);
    }
}

mod declared_owner {
    use fraiseql_core::schema::{CompiledSchema, ObserverDefinition};

    use crate::{
        ServerError,
        observers::{MANAGED_BY_SCHEMA, declared_owner},
    };

    fn schema(project: Option<&str>, observers: &[&str]) -> CompiledSchema {
        CompiledSchema {
            project_name: project.map(str::to_owned),
            observers: observers
                .iter()
                .map(|name| ObserverDefinition::new(*name, "Order", "INSERT"))
                .collect(),
            ..CompiledSchema::default()
        }
    }

    #[test]
    fn a_named_project_owns_its_declared_observers_under_its_name() {
        for observers in [&["order_created"][..], &[]] {
            let owner = declared_owner(&schema(Some("billing"), observers));
            assert!(matches!(owner.as_deref(), Ok("schema:billing")), "{owner:?}");
        }
    }

    #[test]
    fn observers_declared_with_no_project_name_are_refused_naming_the_key() {
        let refused = declared_owner(&schema(None, &["order_created"]));

        let message = match refused {
            Err(ServerError::ConfigError(message)) => message,
            other => format!("not a configuration error: {other:?}"),
        };
        assert!(message.contains("`order_created`"), "names an observer: {message}");
        assert!(message.contains("[project] name"), "names the key to set: {message}");
    }

    #[test]
    fn a_schema_with_neither_removes_what_it_once_declared_under_the_unnamed_owner() {
        let owner = declared_owner(&schema(None, &[]));
        assert!(matches!(owner.as_deref(), Ok(MANAGED_BY_SCHEMA)), "{owner:?}");
    }
}

mod update_request_wire {
    use serde_json::json;

    use super::super::UpdateObserverRequest;

    fn read(body: serde_json::Value) -> UpdateObserverRequest {
        serde_json::from_value(body).expect("an update request")
    }

    #[test]
    fn a_condition_left_out_is_no_change() {
        assert_eq!(read(json!({})).condition_expression, None);
    }

    #[test]
    fn a_condition_sent_as_null_clears_it() {
        assert_eq!(read(json!({ "condition_expression": null })).condition_expression, Some(None));
    }

    #[test]
    fn a_condition_sent_as_text_replaces_it() {
        let request = read(json!({ "condition_expression": "total > 1" }));
        assert_eq!(request.condition_expression, Some(Some("total > 1".to_owned())));
    }

    /// Serialized and read back, each of the three keeps its meaning.
    #[test]
    fn each_shape_survives_a_round_trip() {
        for body in [
            json!({}),
            json!({ "condition_expression": null }),
            json!({ "condition_expression": "total > 1" }),
        ] {
            let written = serde_json::to_value(read(body.clone())).expect("serializes");
            assert_eq!(written.get("condition_expression"), body.get("condition_expression"));
        }
    }

    /// Clearing the condition is a change, so it is refused on a declared observer.
    #[test]
    fn clearing_the_condition_is_more_than_enabled() {
        let request = read(json!({ "condition_expression": null, "enabled": false }));
        assert!(!request.changes_only_enabled());
        assert!(read(json!({ "enabled": false })).changes_only_enabled());
    }

    const SETTINGS: [&str; 3] = ["description", "retry_config", "timeout_ms"];

    fn set(field: &str) -> serde_json::Value {
        match field {
            "description" => json!("pages the on-call"),
            "retry_config" => json!({
                "max_attempts": 9,
                "backoff_strategy": "linear",
                "initial_delay_ms": 5,
                "max_delay_ms": 50,
            }),
            _ => json!(1234),
        }
    }

    /// For each setting: whether it is left out, sent as `null`, or sent with a value.
    fn shapes(request: &UpdateObserverRequest) -> [(bool, bool); 3] {
        [
            (request.description.is_none(), request.description == Some(None)),
            (request.retry_config.is_none(), matches!(request.retry_config, Some(None))),
            (request.timeout_ms.is_none(), request.timeout_ms == Some(None)),
        ]
    }

    #[test]
    fn a_setting_left_out_is_no_change() {
        assert_eq!(shapes(&read(json!({}))), [(true, false); 3]);
    }

    #[test]
    fn a_setting_sent_as_null_clears_it() {
        for (i, field) in SETTINGS.into_iter().enumerate() {
            let shape = shapes(&read(json!({ field: null })))[i];
            assert_eq!(shape, (false, true), "{field}");
        }
    }

    #[test]
    fn a_setting_sent_with_a_value_replaces_it() {
        let request = read(json!({
            "description": set("description"),
            "retry_config": set("retry_config"),
            "timeout_ms": set("timeout_ms"),
        }));
        assert_eq!(request.description, Some(Some("pages the on-call".to_owned())));
        let retry = request.retry_config.flatten().expect("a retry config");
        assert_eq!((retry.max_attempts, retry.max_delay_ms), (9, 50));
        assert_eq!(request.timeout_ms, Some(Some(1234)));
    }

    /// Serialized and read back, each setting keeps its meaning in each of the three shapes.
    #[test]
    fn each_setting_shape_survives_a_round_trip() {
        for field in SETTINGS {
            for body in [
                json!({}),
                json!({ field: null }),
                json!({ field: set(field) }),
            ] {
                let written = serde_json::to_value(read(body.clone())).expect("serializes");
                assert_eq!(written.get(field), body.get(field), "{body}");
            }
        }
    }

    /// Clearing a setting is a change, so it is refused on a declared observer.
    #[test]
    fn clearing_a_setting_is_more_than_enabled() {
        for field in SETTINGS {
            let request = read(json!({ field: null, "enabled": false }));
            assert!(!request.changes_only_enabled(), "{field}");
        }
    }
}
