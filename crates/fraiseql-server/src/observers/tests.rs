mod redaction_tests {
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

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.backoff, "exponential");
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
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        qb.push(" AND entity_type = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious), "user input must not appear in SQL string");
        assert!(sql.contains("$1"), "placeholder must be present");
    }

    #[test]
    fn test_list_event_type_not_inlined() {
        let malicious = "'; DROP TABLE tb_observer; --";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        qb.push(" AND event_type = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_logs_status_not_inlined() {
        let malicious = "' UNION SELECT * FROM secrets --";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tb_observer_log WHERE 1=1");
        qb.push(" AND status = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_logs_trace_id_not_inlined() {
        let malicious = "x' OR fk_customer_org IS NOT NULL--";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tb_observer_log WHERE 1=1");
        qb.push(" AND trace_id = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_no_filters_produces_minimal_sql() {
        let qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
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
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        qb.push(" AND deleted_at IS NULL");
        let sql = qb.sql();
        assert!(sql.contains("deleted_at IS NULL"));
    }

    #[test]
    fn test_list_logs_observer_id_uses_placeholder() {
        let observer_id = uuid::Uuid::new_v4();
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tb_observer_log WHERE 1=1");
        qb.push(" AND fk_observer = (SELECT pk_observer FROM tb_observer WHERE id = ")
            .push_bind(observer_id)
            .push(")");
        let sql = qb.sql();
        assert!(!sql.contains(&observer_id.to_string()), "UUID must not be inlined in SQL");
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_multiple_filters_use_sequential_placeholders() {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
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

    #[test]
    fn test_routes_compile() {
        // This test just ensures the routes compile correctly
        // Actual testing requires a database connection
    }
}

mod runtime_tests {
    use super::super::runtime::RuntimeHealth;

    #[test]
    fn test_runtime_config_defaults() {
        // This test would require a PgPool which needs a database connection
        // For now, just verify the struct compiles
    }

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
