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
            running: false,
            observer_count: 0,
            last_checkpoint: None,
            events_processed: 0,
            errors: 0,
        };
        assert!(!health.running);
        assert_eq!(health.observer_count, 0);
    }
}
