// ── audit_tests ───────────────────────────────────────────────────────────────

#![allow(clippy::panic)] // Reason: test code, panics acceptable
mod audit_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use super::super::audit::*;

    #[tokio::test]
    async fn record_and_retrieve_event() {
        let log = InMemoryAuditLog::new();
        let actor = AuditActor {
            id:         Some("admin".to_string()),
            actor_type: Some("service_account".to_string()),
            acting_for: None,
        };
        log.record("tenant-abc", TenantEventKind::Created, Some(&actor), None)
            .await
            .unwrap();

        let events = log.events_for("tenant-abc", 10, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_key, "tenant-abc");
        assert_eq!(events[0].event, TenantEventKind::Created);
        assert_eq!(events[0].actor.as_deref(), Some("admin"));
        assert_eq!(events[0].actor_type.as_deref(), Some("service_account"));
        assert_eq!(events[0].acting_for_user_id, None);
    }

    #[tokio::test]
    async fn record_captures_delegated_actor() {
        let log = InMemoryAuditLog::new();
        let human = uuid::Uuid::new_v4();
        let actor = AuditActor {
            id:         Some("agent-7".to_string()),
            actor_type: Some("ai_agent".to_string()),
            acting_for: Some(human),
        };
        log.record("t", TenantEventKind::ConfigChanged, Some(&actor), None)
            .await
            .unwrap();

        let events = log.events_for("t", 10, 0).await.unwrap();
        assert_eq!(events[0].actor_type.as_deref(), Some("ai_agent"));
        assert_eq!(events[0].acting_for_user_id, Some(human));
    }

    #[tokio::test]
    async fn events_filtered_by_tenant_key() {
        let log = InMemoryAuditLog::new();
        log.record("tenant-a", TenantEventKind::Created, None, None).await.unwrap();
        log.record("tenant-b", TenantEventKind::Created, None, None).await.unwrap();
        log.record("tenant-a", TenantEventKind::Suspended, None, None).await.unwrap();

        let events_a = log.events_for("tenant-a", 10, 0).await.unwrap();
        assert_eq!(events_a.len(), 2);

        let events_b = log.events_for("tenant-b", 10, 0).await.unwrap();
        assert_eq!(events_b.len(), 1);
    }

    #[tokio::test]
    async fn events_returned_newest_first() {
        let log = InMemoryAuditLog::new();
        log.record("t", TenantEventKind::Created, None, None).await.unwrap();
        log.record("t", TenantEventKind::Suspended, None, None).await.unwrap();
        log.record("t", TenantEventKind::Resumed, None, None).await.unwrap();

        let events = log.events_for("t", 10, 0).await.unwrap();
        assert_eq!(events[0].event, TenantEventKind::Resumed);
        assert_eq!(events[1].event, TenantEventKind::Suspended);
        assert_eq!(events[2].event, TenantEventKind::Created);
    }

    #[tokio::test]
    async fn pagination_with_limit_and_offset() {
        let log = InMemoryAuditLog::new();
        for _ in 0..5 {
            log.record("t", TenantEventKind::Created, None, None).await.unwrap();
        }

        let page1 = log.events_for("t", 2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = log.events_for("t", 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = log.events_for("t", 2, 4).await.unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn config_changed_event_with_payload() {
        let log = InMemoryAuditLog::new();
        let payload = serde_json::json!({
            "max_concurrent": {"old": 5, "new": 10}
        });
        let actor = AuditActor {
            id: Some("user-42".to_string()),
            ..Default::default()
        };
        log.record(
            "tenant-abc",
            TenantEventKind::ConfigChanged,
            Some(&actor),
            Some(payload.clone()),
        )
        .await
        .unwrap();

        let events = log.events_for("tenant-abc", 10, 0).await.unwrap();
        assert_eq!(events[0].payload.as_ref(), Some(&payload));
    }

    #[tokio::test]
    async fn append_only_no_update_or_delete() {
        // Verify by API: there are no update/delete methods on TenantAuditLog.
        // This test records multiple events and confirms all are preserved.
        let log = InMemoryAuditLog::new();
        log.record("t", TenantEventKind::Created, None, None).await.unwrap();
        log.record("t", TenantEventKind::Suspended, None, None).await.unwrap();
        log.record("t", TenantEventKind::Deleted, None, None).await.unwrap();

        let events = log.events_for("t", 100, 0).await.unwrap();
        assert_eq!(events.len(), 3, "all events must be preserved (append-only)");
    }

    #[test]
    fn event_kind_as_str() {
        assert_eq!(TenantEventKind::Created.as_str(), "created");
        assert_eq!(TenantEventKind::ConfigChanged.as_str(), "config_changed");
        assert_eq!(TenantEventKind::Suspended.as_str(), "suspended");
        assert_eq!(TenantEventKind::Resumed.as_str(), "resumed");
        assert_eq!(TenantEventKind::Deleted.as_str(), "deleted");
    }

    #[test]
    fn event_kind_serializes_to_snake_case() {
        let json = serde_json::to_string(&TenantEventKind::ConfigChanged).unwrap();
        assert_eq!(json, "\"config_changed\"");
    }
}

// ── pool_factory_tests ────────────────────────────────────────────────────────

mod pool_factory_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            postgres::{PostgresTlsConfig, ReadReplicaPolicy},
            traits::{DatabaseAdapter, SupportsMutations},
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        schema::CompiledSchema,
    };
    use fraiseql_error::FraiseQLError;

    use super::super::pool_factory::*;

    /// Stub adapter that implements `FromPoolConfig` for testing.
    #[derive(Debug, Clone)]
    struct StubPoolAdapter;

    #[async_trait]
    impl DatabaseAdapter for StubPoolAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    // A tenant adapter stands in for `PostgresAdapter`, which is write-capable, so
    // the stub declares the marker too — a fixture that is read-only where its
    // producer is not exercises a shape no tenant ever has. It does not override
    // `supports_mutations()`, so the executor these build still refuses writes;
    // these tests are about pool construction, not dispatch.
    impl SupportsMutations for StubPoolAdapter {}

    #[async_trait]
    impl FromPoolConfig for StubPoolAdapter {
        async fn from_pool_config(_config: &TenantPoolConfig) -> FraiseQLResult<Self> {
            Ok(Self)
        }
    }

    fn test_pool_config() -> TenantPoolConfig {
        TenantPoolConfig {
            connection_string:    "stub://localhost/test".to_string(),
            max_connections:      5,
            connect_timeout_secs: 5,
            idle_timeout_secs:    300,
            search_path:          None,
            tls:                  PostgresTlsConfig::default(),
            read_replica_urls:    Vec::new(),
            read_replica_policy:  ReadReplicaPolicy::default(),
            vector_scan:          fraiseql_core::db::postgres::VectorScanConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_create_tenant_executor_success() {
        let schema = CompiledSchema::default();
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let executor = create_tenant_executor::<StubPoolAdapter>(
            "acme",
            &schema_json,
            &config,
            &fraiseql_core::runtime::RuntimeConfig::default(),
        )
        .await
        .unwrap();
        assert_eq!(executor.schema().types.len(), 0);
    }

    #[tokio::test]
    async fn test_create_tenant_executor_invalid_json() {
        let config = test_pool_config();
        let Err(err) = create_tenant_executor::<StubPoolAdapter>(
            "acme",
            "not valid json",
            &config,
            &fraiseql_core::runtime::RuntimeConfig::default(),
        )
        .await
        else {
            panic!("expected Err for invalid JSON");
        };
        assert!(matches!(err, FraiseQLError::Parse { .. }), "Expected Parse error, got: {err:?}");
    }

    #[tokio::test]
    async fn test_create_tenant_executor_from_another_build() {
        let schema = CompiledSchema {
            fraiseql_version: serde_json::from_value(serde_json::json!("2.14.0")).unwrap(),
            ..CompiledSchema::default()
        };
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let Err(err) = create_tenant_executor::<StubPoolAdapter>(
            "acme",
            &schema_json,
            &config,
            &fraiseql_core::runtime::RuntimeConfig::default(),
        )
        .await
        else {
            panic!("expected Err for an artifact produced by another build");
        };
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "Expected Validation error, got: {err:?}"
        );
    }

    /// Adapter that always fails to connect — simulates unreachable DB.
    #[derive(Debug, Clone)]
    struct FailingAdapter;

    #[async_trait]
    impl DatabaseAdapter for FailingAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Err(FraiseQLError::database("connection refused"))
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    // A tenant adapter stands in for `PostgresAdapter`, which is write-capable, so
    // the stub declares the marker too — a fixture that is read-only where its
    // producer is not exercises a shape no tenant ever has. It does not override
    // `supports_mutations()`, so the executor these build still refuses writes;
    // these tests are about pool construction, not dispatch.
    impl SupportsMutations for FailingAdapter {}

    #[async_trait]
    impl FromPoolConfig for FailingAdapter {
        async fn from_pool_config(_config: &TenantPoolConfig) -> FraiseQLResult<Self> {
            Err(FraiseQLError::ConnectionPool {
                message: "connection refused".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_create_tenant_executor_unreachable_db() {
        let schema = CompiledSchema::default();
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let Err(err) = create_tenant_executor::<FailingAdapter>(
            "acme",
            &schema_json,
            &config,
            &fraiseql_core::runtime::RuntimeConfig::default(),
        )
        .await
        else {
            panic!("expected Err for unreachable DB");
        };
        assert!(
            matches!(err, FraiseQLError::ConnectionPool { .. }),
            "Expected ConnectionPool error, got: {err:?}"
        );
    }

    #[test]
    fn test_pool_config_defaults() {
        let json = r#"{"connection_string": "postgres://localhost/test"}"#;
        let config: TenantPoolConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.connect_timeout_secs, 5);
        assert_eq!(config.idle_timeout_secs, 300);
    }
}

// ── schema_isolation_tests ────────────────────────────────────────────────────

mod schema_isolation_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use std::sync::Mutex;

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
    };
    use fraiseql_error::FraiseQLError;

    use super::super::schema_isolation::*;

    // ── tenant_schema_name ──────────────────────────────────────────────

    #[test]
    fn valid_key_produces_prefixed_name() {
        assert_eq!(tenant_schema_name("acme").unwrap(), "tenant_acme");
    }

    #[test]
    fn alphanumeric_key_accepted() {
        assert_eq!(tenant_schema_name("org123").unwrap(), "tenant_org123");
    }

    #[test]
    fn underscore_in_key_accepted() {
        assert_eq!(tenant_schema_name("my_org").unwrap(), "tenant_my_org");
    }

    #[test]
    fn empty_key_rejected() {
        let err = tenant_schema_name("").unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "expected Validation, got: {err:?}"
        );
    }

    #[test]
    fn key_with_hyphen_rejected() {
        let err = tenant_schema_name("my-org").unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn key_with_dot_rejected() {
        let err = tenant_schema_name("my.org").unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn key_with_space_rejected() {
        assert!(tenant_schema_name("my org").is_err());
    }

    #[test]
    fn key_with_semicolon_rejected() {
        assert!(tenant_schema_name("org; DROP TABLE").is_err());
    }

    #[test]
    fn key_exceeding_max_length_rejected() {
        // MAX_PG_IDENTIFIER_LEN = 63, prefix = "tenant_" (7 chars)
        // So key can be at most 56 chars
        let long_key = "a".repeat(57);
        let err = tenant_schema_name(&long_key).unwrap_err();
        assert!(err.to_string().contains("63-character"));
    }

    #[test]
    fn key_at_max_length_accepted() {
        let key = "a".repeat(56); // tenant_ (7) + 56 = 63 exactly
        let name = tenant_schema_name(&key).unwrap();
        assert_eq!(name.len(), 63);
    }

    // ── DDL generation ──────────────────────────────────────────────────

    #[test]
    fn create_schema_ddl_generates_correct_sql() {
        assert_eq!(create_schema_ddl("acme").unwrap(), "CREATE SCHEMA IF NOT EXISTS tenant_acme");
    }

    #[test]
    fn create_schema_idempotent() {
        // IF NOT EXISTS means calling twice produces the same SQL
        let ddl1 = create_schema_ddl("acme").unwrap();
        let ddl2 = create_schema_ddl("acme").unwrap();
        assert_eq!(ddl1, ddl2);
        assert!(ddl1.contains("IF NOT EXISTS"));
    }

    #[test]
    fn create_schema_ddl_rejects_invalid_key() {
        assert!(create_schema_ddl("").is_err());
        assert!(create_schema_ddl("org; DROP").is_err());
    }

    // ── search_path ─────────────────────────────────────────────────────

    #[test]
    fn tenant_search_path_puts_the_tenant_schema_first() {
        assert_eq!(tenant_search_path("acme").unwrap().as_str(), "tenant_acme,public");
    }

    #[test]
    fn tenant_search_path_rejects_invalid_key() {
        assert!(tenant_search_path("").is_err());
    }

    // ── Row mode skips DDL ──────────────────────────────────────────────
    // (Row mode never calls these functions — verified at the caller level)

    // ── Async adapter functions ────────────────────────────────────────

    /// Spy adapter that records all SQL passed to `execute_raw_query`, and can
    /// answer the isolation probe with a canned `reset_val` — standing in for a
    /// pool whose connections were established with that search path.
    ///
    /// One adapter rather than two: the `make lint-async-trait` ratchet counts
    /// async-trait attribute sites across `crates/*/src/` by grep, so a second test
    /// double would spend budget meant for production dyn-dispatch traits — and
    /// naming the attribute in prose spends it too.
    #[derive(Debug)]
    struct SpyAdapter {
        queries:   Mutex<Vec<String>>,
        reset_val: Option<String>,
    }

    impl SpyAdapter {
        fn new() -> Self {
            Self {
                queries:   Mutex::new(Vec::new()),
                reset_val: None,
            }
        }

        fn with_reset_val(value: &str) -> Self {
            Self {
                queries:   Mutex::new(Vec::new()),
                reset_val: Some(value.to_string()),
            }
        }

        fn recorded_queries(&self) -> Vec<String> {
            self.queries.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl DatabaseAdapter for SpyAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            self.queries.lock().unwrap().push(sql.to_string());
            Ok(self
                .reset_val
                .as_ref()
                .map(|v| {
                    vec![std::collections::HashMap::from([(
                        "reset_val".to_string(),
                        serde_json::Value::String(v.clone()),
                    )])]
                })
                .unwrap_or_default())
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn provision_executes_create_schema_ddl() {
        let adapter = SpyAdapter::new();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        // The adoption probe runs first, then the DDL.
        assert_eq!(queries.len(), 2, "{queries:?}");
        assert!(queries[0].contains("pg_class"), "{}", queries[0]);
        assert_eq!(queries[1], "CREATE SCHEMA IF NOT EXISTS tenant_acme");
    }

    #[tokio::test]
    async fn drop_tenant_schema_issues_the_cascade_ddl() {
        let adapter = std::sync::Arc::new(SpyAdapter::new());
        let executor = fraiseql_core::runtime::Executor::read_only(
            fraiseql_core::schema::CompiledSchema::default(),
            std::sync::Arc::clone(&adapter),
        );
        drop_tenant_schema("acme", &executor).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "DROP SCHEMA IF EXISTS tenant_acme CASCADE");
    }

    /// The verification probe must read the *established* value, not the current
    /// one. `SpyAdapter` returns no rows, so the check must refuse — an adapter
    /// that cannot answer the question is an adapter that cannot prove isolation.
    #[tokio::test]
    async fn verify_search_path_refuses_when_the_adapter_cannot_answer() {
        let adapter = SpyAdapter::new();
        let err = verify_search_path("acme", &adapter).await.unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Configuration { .. }),
            "expected Configuration, got: {err:?}"
        );
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert!(
            queries[0].contains("reset_val") && queries[0].contains("pg_settings"),
            "isolation must be verified against the connection's established value, \
             not `current_setting`, which a session `SET` would also satisfy: {}",
            queries[0]
        );
    }

    #[tokio::test]
    async fn verify_search_path_accepts_the_established_tenant_path() {
        let adapter = SpyAdapter::with_reset_val("tenant_acme, public");
        verify_search_path("acme", &adapter).await.unwrap();
    }

    #[tokio::test]
    async fn verify_search_path_rejects_a_foreign_path() {
        let adapter = SpyAdapter::with_reset_val("tenant_other,public");
        let err = verify_search_path("acme", &adapter).await.unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Configuration { .. }),
            "expected Configuration, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn provision_is_idempotent() {
        let adapter = SpyAdapter::new();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        let ddl: Vec<String> = adapter
            .recorded_queries()
            .into_iter()
            .filter(|q| q.starts_with("CREATE SCHEMA"))
            .collect();
        assert_eq!(ddl.len(), 2);
        // Both should be IF NOT EXISTS — idempotent
        assert!(ddl.iter().all(|q| q.contains("IF NOT EXISTS")), "{ddl:?}");
    }

    #[tokio::test]
    async fn provision_rejects_invalid_key() {
        let adapter = SpyAdapter::new();
        let err = provision_tenant_schema("my-org", &adapter).await.unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        // No SQL should have been executed
        assert!(adapter.recorded_queries().is_empty());
    }

    /// The adoption probe is advisory. An adapter that cannot answer it must not
    /// block provisioning — the DDL is the operation that has to happen.
    #[tokio::test]
    async fn provision_still_runs_when_the_adoption_probe_cannot_answer() {
        let adapter = SpyAdapter::with_reset_val("irrelevant");
        provision_tenant_schema("acme", &adapter).await.unwrap();
    }

    #[tokio::test]
    async fn drop_rejects_invalid_key() {
        let adapter = std::sync::Arc::new(SpyAdapter::new());
        let executor = fraiseql_core::runtime::Executor::read_only(
            fraiseql_core::schema::CompiledSchema::default(),
            std::sync::Arc::clone(&adapter),
        );
        let err = drop_tenant_schema("", &executor).await.unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        assert!(adapter.recorded_queries().is_empty());
    }
}

// ── #1333: a tenant executor must not drift from the server's ────────────────
//
// The defect is not any one missing setting — it is that a fourth constructor exists
// at all. `create_tenant_executor` ends in `Executor::new(...)`, which is
// `with_config(..., RuntimeConfig::default())`, while every HTTP constructor routes
// through `initialization::executor_runtime_config`, whose own doc calls itself "the
// single seam every server entry point routes through so the config can never drift by
// constructor (H16)".
//
// So this pins the *agreement*, field by field, with exhaustive destructuring: adding a
// field to `RuntimeConfig` is a compile error here until it is classified. Counting
// settings instead would have gone stale already — #1333's table lists eleven, and
// `query_function_resolver` (#1329) landed after it was written, making twelve.
mod runtime_config_drift {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::print_stderr)] // Reason: skip diagnostics for the DATABASE_URL-less leg

    use fraiseql_core::runtime::RuntimeConfig;

    /// Which `RuntimeConfig` fields the two executors disagree on.
    ///
    /// Compares by **presence** for the `Arc<dyn Trait>` gates: none of them implements
    /// `PartialEq`, and presence is the property that matters — a tenant request either
    /// has an `Authorizer` to consult or it does not. Identity is checked too, via
    /// `Arc::ptr_eq`, so "installed a different policy" is a disagreement as well as
    /// "installed none".
    ///
    /// `deliberately_per_tenant` names the fields a tenant legitimately owns: they come
    /// from the tenant's **own** compiled schema, not the server's.
    fn disagreements(server: &RuntimeConfig, tenant: &RuntimeConfig) -> Vec<&'static str> {
        // Exhaustive destructuring, deliberately without `..`, mirroring
        // `RuntimeConfig::with_compiled_schema`: a new field is a compile error here
        // until someone decides whether a tenant inherits it. A `..` tail would let the
        // next field be dropped on this path in silence — which is the whole of #1333.
        let RuntimeConfig {
            cache_query_plans,
            max_page_size,
            enable_tracing,
            field_filter,
            rls_policy,
            field_authorizer,
            authorizer,
            query_timeout_ms,
            jsonb_optimization,
            query_validation,
            max_operation_cost,
            audit_mutations,
            changelog_enabled,
            dry_run_mutations,
            cascade_limits,
            before_mutation_gate,
            query_function_resolver,
        } = server;

        let mut out = Vec::new();

        // Caller-installed policy: the operator decides it, so every tenant runs under
        // the same one. Presence *and* identity.
        if authorizer.is_some() != tenant.authorizer.is_some() {
            out.push("authorizer");
        }
        if before_mutation_gate.is_some() != tenant.before_mutation_gate.is_some() {
            out.push("before_mutation_gate");
        }
        if rls_policy.is_some() != tenant.rls_policy.is_some() {
            out.push("rls_policy");
        }
        if field_authorizer.is_some() != tenant.field_authorizer.is_some() {
            out.push("field_authorizer");
        }
        if field_filter.is_some() != tenant.field_filter.is_some() {
            out.push("field_filter");
        }
        if query_function_resolver.is_some() != tenant.query_function_resolver.is_some() {
            out.push("query_function_resolver");
        }

        // Operator-owned scalars: the same reason `database_tls`, `read_replica_policy`
        // and `vector_scan` are stamped onto every tenant pool (#801, #957, #1116).
        if *cache_query_plans != tenant.cache_query_plans {
            out.push("cache_query_plans");
        }
        if *enable_tracing != tenant.enable_tracing {
            out.push("enable_tracing");
        }
        if *query_timeout_ms != tenant.query_timeout_ms {
            out.push("query_timeout_ms");
        }
        if *dry_run_mutations != tenant.dry_run_mutations {
            out.push("dry_run_mutations");
        }
        if format!("{jsonb_optimization:?}") != format!("{:?}", tenant.jsonb_optimization) {
            out.push("jsonb_optimization");
        }
        if format!("{cascade_limits:?}") != format!("{:?}", tenant.cascade_limits) {
            out.push("cascade_limits");
        }

        // Schema-derived: a tenant's own `[validation]` / `[security.cost_budget]` /
        // `[changelog]` are its own, so these are compared only when the two executors
        // were built from the *same* schema — which is what the test below does.
        if *max_page_size != tenant.max_page_size {
            out.push("max_page_size");
        }
        if *max_operation_cost != tenant.max_operation_cost {
            out.push("max_operation_cost");
        }
        if *audit_mutations != tenant.audit_mutations {
            out.push("audit_mutations");
        }
        if *changelog_enabled != tenant.changelog_enabled {
            out.push("changelog_enabled");
        }
        if query_validation.is_some() != tenant.query_validation.is_some() {
            out.push("query_validation");
        }

        out
    }

    /// `disagreements` must actually see a dropped field, or the pin below is a
    /// decoration. Built by taking a fully-populated config and defaulting it — which is
    /// exactly what `Executor::new` does to a tenant.
    #[test]
    fn the_comparison_notices_a_config_that_was_defaulted() {
        let populated = RuntimeConfig {
            cache_query_plans: false,
            max_page_size: Some(17),
            enable_tracing: true,
            query_timeout_ms: 4321,
            max_operation_cost: Some(99),
            audit_mutations: true,
            changelog_enabled: false,
            dry_run_mutations: true,
            ..RuntimeConfig::default()
        };

        let found = disagreements(&populated, &RuntimeConfig::default());

        for expected in [
            "cache_query_plans",
            "enable_tracing",
            "query_timeout_ms",
            "dry_run_mutations",
            "max_page_size",
            "max_operation_cost",
            "audit_mutations",
            "changelog_enabled",
        ] {
            assert!(
                found.contains(&expected),
                "the drift comparison missed `{expected}` — a pin that cannot see a \
                 dropped field would pass over #1333 itself. Found: {found:?}"
            );
        }
    }

    // ── The pin itself, against a real tenant pool ──────────────────────────
    //
    // Self-skips without DATABASE_URL, so it is inert in the database-free `test` leg
    // and runs in `integration (postgres)`.

    use std::sync::Arc;

    use fraiseql_core::{
        db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy, VectorScanConfig},
        error::Result as CoreResult,
        security::{
            Authorizer, AuthzDecision, AuthzRequest, BeforeMutationGate, BeforeMutationOutcome,
            BeforeMutationRequest, SecurityContext,
        },
    };

    use crate::tenancy::{TenantPoolConfig, create_tenant_executor};

    /// An operator-installed policy. What it decides is irrelevant here — the pin is
    /// about whether a tenant request has one to consult at all.
    struct DenyAll;

    impl Authorizer for DenyAll {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> CoreResult<AuthzDecision> {
            Ok(AuthzDecision::Deny {
                reason: "pinned".to_string(),
            })
        }
    }

    struct AbortAll;

    #[async_trait::async_trait]
    impl BeforeMutationGate for AbortAll {
        async fn before_mutation(
            &self,
            _request: &BeforeMutationRequest<'_>,
        ) -> CoreResult<BeforeMutationOutcome> {
            Ok(BeforeMutationOutcome::Abort {
                reason: "pinned".to_string(),
            })
        }
    }

    /// The config a booting server ends up with: schema-derived settings plus the
    /// policy the operator installed programmatically.
    fn server_config() -> RuntimeConfig {
        RuntimeConfig {
            authorizer: Some(Arc::new(DenyAll)),
            before_mutation_gate: Some(Arc::new(AbortAll)),
            query_timeout_ms: 4321,
            ..RuntimeConfig::default()
        }
    }

    fn schema_json() -> String {
        serde_json::json!({
            "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
            "types": [],
            "queries": [],
            "mutations": [],
        })
        .to_string()
    }

    fn pool_config(url: &str) -> TenantPoolConfig {
        TenantPoolConfig {
            connection_string:    url.to_string(),
            max_connections:      2,
            connect_timeout_secs: 10,
            idle_timeout_secs:    300,
            search_path:          None,
            tls:                  PostgresTlsConfig::default(),
            read_replica_urls:    Vec::new(),
            read_replica_policy:  ReadReplicaPolicy::default(),
            vector_scan:          VectorScanConfig::default(),
        }
    }

    /// An `Authorizer` that allows — the positive twin's policy.
    struct AllowAll;

    impl Authorizer for AllowAll {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> CoreResult<AuthzDecision> {
            Ok(AuthzDecision::Allow)
        }
    }

    /// A schema with one query, so there is an operation for the `Authorizer` to be
    /// consulted about. The view does not exist: the gate is supposed to refuse
    /// *before* any SQL runs, so a missing relation is how the allow case proves the
    /// request got past the gate rather than never reaching the database.
    fn schema_json_with_query() -> String {
        serde_json::json!({
            "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
            "types": [{
                "name": "Widget",
                "sql_source": "v_p37_widget",
                "fields": [{"name": "id", "field_type": "Int", "nullable": false}],
            }],
            "queries": [{
                "name": "widgets",
                "return_type": "Widget",
                "sql_source": "v_p37_widget",
                "returns_list": true,
            }],
            "mutations": [],
        })
        .to_string()
    }

    async fn tenant_with_authorizer(
        url: &str,
        key: &str,
        authorizer: Arc<dyn Authorizer>,
    ) -> Arc<fraiseql_core::runtime::Executor> {
        create_tenant_executor::<PostgresAdapter>(
            key,
            &schema_json_with_query(),
            &pool_config(url),
            &RuntimeConfig {
                authorizer: Some(authorizer),
                ..RuntimeConfig::default()
            },
        )
        .await
        .expect("provision the tenant executor")
    }

    /// One factory, two registrations, two different configs — the property that
    /// separates design (B) from the one #1333's own Scope section proposed.
    ///
    /// Capturing the config inside `make_executor_factory`, beside the three
    /// operator-owned pool settings it already stamps (#801, #957, #1116), reads as the
    /// obvious symmetry. It is wrong: `prepare_functions_runtime` rebuilds the server's
    /// executor at *serve* time to install the `before:mutation` gate (#1327) and the
    /// function-query resolver (#1329), and `main.rs` builds the factory before that. A
    /// captured config would be missing exactly the gate #1327 exists for — and a test
    /// that only asserted "the authorizer binds" would have passed anyway, because the
    /// authorizer *is* installed before the factory is built.
    ///
    /// So the config travels per invocation. If it were ever captured, both executors
    /// below would carry the same one and this fails.
    #[tokio::test]
    async fn the_factory_reads_the_config_per_registration_rather_than_capturing_it() {
        let Some(url) = fraiseql_test_support::try_database_url() else {
            eprintln!("skipping #1333 capture pin: DATABASE_URL not set");
            return;
        };

        let factory = crate::tenancy::make_executor_factory::<PostgresAdapter>(
            PostgresTlsConfig::default(),
            ReadReplicaPolicy::default(),
            VectorScanConfig::default(),
        );

        // The config as it stands when the factory is built: no gate yet.
        let before = factory(
            "p37early".to_string(),
            schema_json(),
            pool_config(&url),
            RuntimeConfig::default(),
        )
        .await
        .expect("register the first tenant");

        // The config after the server's serve-time rebuild installed one.
        let after = factory(
            "p37late".to_string(),
            schema_json(),
            pool_config(&url),
            RuntimeConfig {
                before_mutation_gate: Some(Arc::new(AbortAll)),
                ..RuntimeConfig::default()
            },
        )
        .await
        .expect("register the second tenant");

        assert!(
            before.config().before_mutation_gate.is_none(),
            "the first registration predates the gate, so it must not have one — if it \
             does, this test is not measuring what it claims"
        );
        assert!(
            after.config().before_mutation_gate.is_some(),
            "#1333: a tenant registered after the server installed its before:mutation \
             gate must run under it. A factory that captured the config when it was built \
             would hand this tenant the earlier one, and the write gate #1327 exists for \
             would be absent on every tenant-keyed mutation."
        );
    }

    /// The behaviour half: the config arriving is not the same claim as the gate
    /// binding. A value can be carried and never consulted.
    #[tokio::test]
    async fn the_authorizer_binds_on_a_tenant_executor() {
        let Some(url) = fraiseql_test_support::try_database_url() else {
            eprintln!("skipping #1333 tenant Authorizer pin: DATABASE_URL not set");
            return;
        };
        let principal = SecurityContext::system_job("p37", "req-1", vec![], vec![], None);

        let denied = tenant_with_authorizer(&url, "p37deny", Arc::new(DenyAll))
            .await
            .execute_with_security("{ widgets { id } }", None, &principal)
            .await;
        assert!(
            matches!(denied, Err(fraiseql_core::error::FraiseQLError::Authorization { .. })),
            "#1333: an operation Authorizer must be consulted on a tenant-keyed request. \
             It never was — the tenant executor had none to consult. Got: {denied:?}"
        );

        // The twin. Without it, a tenant executor that refused everything — or one whose
        // missing view made every query fail — would satisfy the assertion above.
        let allowed = tenant_with_authorizer(&url, "p37allow", Arc::new(AllowAll))
            .await
            .execute_with_security("{ widgets { id } }", None, &principal)
            .await;
        assert!(
            !matches!(allowed, Err(fraiseql_core::error::FraiseQLError::Authorization { .. })),
            "an allowing Authorizer must let the request past the gate; it may still fail \
             on the missing view, which is what shows the gate was the only thing \
             refusing before. Got: {allowed:?}"
        );
    }

    #[tokio::test]
    async fn a_tenant_executor_runs_under_the_servers_runtime_config() {
        let Some(url) = fraiseql_test_support::try_database_url() else {
            eprintln!("skipping #1333 tenant RuntimeConfig pin: DATABASE_URL not set");
            return;
        };

        let tenant = create_tenant_executor::<PostgresAdapter>(
            "p37drift",
            &schema_json(),
            &pool_config(&url),
            // The whole point of this case: the tenant is built from the *server's*
            // config, the way `upsert_tenant_handler` supplies it.
            &server_config(),
        )
        .await
        .expect("provision the tenant executor");

        let found = disagreements(&server_config(), tenant.config());

        assert!(
            found.is_empty(),
            "#1333: a tenant-keyed request runs with none of these — the tenant factory \
             ends in `Executor::new`, which is `RuntimeConfig::default()`, while every \
             HTTP constructor routes through `executor_runtime_config`, the seam whose \
             own doc calls itself the single one every entry point takes. Drifted: {found:?}"
        );
    }
}
