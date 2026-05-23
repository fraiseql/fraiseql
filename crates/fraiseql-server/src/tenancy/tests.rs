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
        log.record("tenant-abc", TenantEventKind::Created, Some("admin"), None)
            .await
            .unwrap();

        let events = log.events_for("tenant-abc", 10, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_key, "tenant-abc");
        assert_eq!(events[0].event, TenantEventKind::Created);
        assert_eq!(events[0].actor.as_deref(), Some("admin"));
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
        log.record(
            "tenant-abc",
            TenantEventKind::ConfigChanged,
            Some("user-42"),
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
            traits::DatabaseAdapter,
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
            DatabaseType::SQLite
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
        }
    }

    #[tokio::test]
    async fn test_create_tenant_executor_success() {
        let schema = CompiledSchema::default();
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let executor = create_tenant_executor::<StubPoolAdapter>("acme", &schema_json, &config)
            .await
            .unwrap();
        assert_eq!(executor.schema().types.len(), 0);
    }

    #[tokio::test]
    async fn test_create_tenant_executor_invalid_json() {
        let config = test_pool_config();
        let Err(err) =
            create_tenant_executor::<StubPoolAdapter>("acme", "not valid json", &config).await
        else {
            panic!("expected Err for invalid JSON");
        };
        assert!(matches!(err, FraiseQLError::Parse { .. }), "Expected Parse error, got: {err:?}");
    }

    #[tokio::test]
    async fn test_create_tenant_executor_bad_format_version() {
        let schema = CompiledSchema {
            schema_format_version: Some(999),
            ..CompiledSchema::default()
        };
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let Err(err) =
            create_tenant_executor::<StubPoolAdapter>("acme", &schema_json, &config).await
        else {
            panic!("expected Err for bad format version");
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

        let Err(err) =
            create_tenant_executor::<FailingAdapter>("acme", &schema_json, &config).await
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
    fn drop_schema_ddl_generates_correct_sql() {
        assert_eq!(drop_schema_ddl("acme").unwrap(), "DROP SCHEMA IF EXISTS tenant_acme CASCADE");
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

    #[test]
    fn drop_schema_ddl_rejects_invalid_key() {
        assert!(drop_schema_ddl("").is_err());
    }

    // ── search_path ─────────────────────────────────────────────────────

    #[test]
    fn search_path_sql_generates_correct_statement() {
        assert_eq!(search_path_sql("acme").unwrap(), "SET search_path TO tenant_acme, public");
    }

    #[test]
    fn search_path_sql_rejects_invalid_key() {
        assert!(search_path_sql("").is_err());
    }

    // ── Row mode skips DDL ──────────────────────────────────────────────
    // (Row mode never calls these functions — verified at the caller level)

    // ── Async adapter functions ────────────────────────────────────────

    /// Spy adapter that records all SQL passed to `execute_raw_query`.
    #[derive(Debug)]
    struct SpyAdapter {
        queries: Mutex<Vec<String>>,
    }

    impl SpyAdapter {
        fn new() -> Self {
            Self {
                queries: Mutex::new(Vec::new()),
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

    #[tokio::test]
    async fn provision_executes_create_schema_ddl() {
        let adapter = SpyAdapter::new();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "CREATE SCHEMA IF NOT EXISTS tenant_acme");
    }

    #[tokio::test]
    async fn drop_executes_drop_schema_ddl() {
        let adapter = SpyAdapter::new();
        drop_tenant_schema("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "DROP SCHEMA IF EXISTS tenant_acme CASCADE");
    }

    #[tokio::test]
    async fn configure_search_path_executes_set_statement() {
        let adapter = SpyAdapter::new();
        configure_search_path("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "SET search_path TO tenant_acme, public");
    }

    #[tokio::test]
    async fn provision_is_idempotent() {
        let adapter = SpyAdapter::new();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 2);
        // Both should be IF NOT EXISTS — idempotent
        assert!(queries[0].contains("IF NOT EXISTS"));
        assert!(queries[1].contains("IF NOT EXISTS"));
    }

    #[tokio::test]
    async fn provision_rejects_invalid_key() {
        let adapter = SpyAdapter::new();
        let err = provision_tenant_schema("my-org", &adapter).await.unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        // No SQL should have been executed
        assert!(adapter.recorded_queries().is_empty());
    }

    #[tokio::test]
    async fn drop_rejects_invalid_key() {
        let adapter = SpyAdapter::new();
        let err = drop_tenant_schema("", &adapter).await.unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        assert!(adapter.recorded_queries().is_empty());
    }
}
