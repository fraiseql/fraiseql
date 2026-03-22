//! Tests for the SQL Server database adapter.

use super::*;

mod error_code_tests {
    use super::*;

    #[test]
    fn test_unique_constraint_violation_2627() {
        // 2627 = unique constraint → ANSI unique violation 23505
        assert_eq!(map_mssql_error_code(2627), Some("23505".to_string()));
    }

    #[test]
    fn test_duplicate_key_2601() {
        // 2601 = duplicate key in unique index → same ANSI code 23505
        assert_eq!(map_mssql_error_code(2601), Some("23505".to_string()));
    }

    #[test]
    fn test_not_null_violation_515() {
        // 515 = NOT NULL violation → ANSI not-null violation 23502
        assert_eq!(map_mssql_error_code(515), Some("23502".to_string()));
    }

    #[test]
    fn test_foreign_key_violation_547() {
        // 547 = FK violation → ANSI FK violation 23503 (unchanged)
        assert_eq!(map_mssql_error_code(547), Some("23503".to_string()));
    }

    #[test]
    fn test_deadlock_1205() {
        // 1205 = deadlock victim → ANSI serialization failure 40001 (NOT PostgreSQL-vendor 40P01)
        assert_eq!(map_mssql_error_code(1205), Some("40001".to_string()));
    }

    #[test]
    fn test_string_truncation_8152() {
        // 8152 = string truncation → ANSI value too long 22001 (unchanged)
        assert_eq!(map_mssql_error_code(8152), Some("22001".to_string()));
    }

    #[test]
    fn test_out_of_memory_701_returns_none() {
        // 701 = insufficient memory — no ANSI equivalent; must return None
        // (previously incorrectly returned the PostgreSQL-vendor code "53200")
        assert_eq!(map_mssql_error_code(701), None);
    }

    #[test]
    fn test_unknown_code_returns_none() {
        assert_eq!(map_mssql_error_code(9999), None);
        assert_eq!(map_mssql_error_code(0), None);
        assert_eq!(map_mssql_error_code(u32::MAX), None);
    }
}

mod relay_sql_tests {
    use crate::sqlserver::helpers::{
        build_relay_backward_outer_order_sql, build_relay_order_sql, build_relay_where_sql,
        is_valid_uuid_format,
    };
    use crate::types::sql_hints::{OrderByClause, OrderDirection};

    // ── build_relay_order_sql ──────────────────────────────────────────────

    #[test]
    fn test_build_relay_order_sql_forward_no_order_by() {
        let sql = build_relay_order_sql("[id]", None, true);
        assert_eq!(sql, " ORDER BY [id] ASC");
    }

    #[test]
    fn test_build_relay_order_sql_backward_no_order_by() {
        let sql = build_relay_order_sql("[id]", None, false);
        assert_eq!(sql, " ORDER BY [id] DESC");
    }

    #[test]
    fn test_build_relay_order_sql_forward_custom_order_by_asc() {
        let order_by = vec![OrderByClause {
            field:     "score".to_string(),
            direction: OrderDirection::Asc,
        }];
        let sql = build_relay_order_sql("[id]", Some(&order_by), true);
        assert_eq!(sql, " ORDER BY JSON_VALUE(data, '$.score') ASC, [id] ASC");
    }

    #[test]
    fn test_build_relay_order_sql_backward_custom_order_by_asc_flips_to_desc() {
        // KEY TEST: backward pagination must flip ASC → DESC so the inner
        // FETCH NEXT subquery retrieves the correct N rows before the cursor.
        let order_by = vec![OrderByClause {
            field:     "score".to_string(),
            direction: OrderDirection::Asc,
        }];
        let sql = build_relay_order_sql("[id]", Some(&order_by), false);
        assert_eq!(sql, " ORDER BY JSON_VALUE(data, '$.score') DESC, [id] DESC");
    }

    #[test]
    fn test_build_relay_order_sql_backward_custom_order_by_desc_flips_to_asc() {
        let order_by = vec![OrderByClause {
            field:     "created_at".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = build_relay_order_sql("[id]", Some(&order_by), false);
        assert_eq!(sql, " ORDER BY JSON_VALUE(data, '$.created_at') ASC, [id] DESC");
    }

    #[test]
    fn test_build_relay_order_sql_multi_column_forward() {
        let order_by = vec![
            OrderByClause {
                field:     "a".to_string(),
                direction: OrderDirection::Asc,
            },
            OrderByClause {
                field:     "b".to_string(),
                direction: OrderDirection::Desc,
            },
        ];
        let sql = build_relay_order_sql("[id]", Some(&order_by), true);
        assert_eq!(
            sql,
            " ORDER BY JSON_VALUE(data, '$.a') ASC, JSON_VALUE(data, '$.b') DESC, [id] ASC"
        );
    }

    #[test]
    fn test_build_relay_order_sql_multi_column_backward_all_flipped() {
        let order_by = vec![
            OrderByClause {
                field:     "a".to_string(),
                direction: OrderDirection::Asc,
            },
            OrderByClause {
                field:     "b".to_string(),
                direction: OrderDirection::Desc,
            },
        ];
        let sql = build_relay_order_sql("[id]", Some(&order_by), false);
        assert_eq!(
            sql,
            " ORDER BY JSON_VALUE(data, '$.a') DESC, JSON_VALUE(data, '$.b') ASC, [id] DESC"
        );
    }

    // ── build_relay_backward_outer_order_sql ──────────────────────────────

    #[test]
    fn test_build_relay_backward_outer_order_sql_no_order_by() {
        let sql = build_relay_backward_outer_order_sql(None);
        assert_eq!(sql, " ORDER BY _relay_cursor ASC");
    }

    #[test]
    fn test_build_relay_backward_outer_order_sql_with_custom_asc() {
        let order_by = vec![OrderByClause {
            field:     "score".to_string(),
            direction: OrderDirection::Asc,
        }];
        let sql = build_relay_backward_outer_order_sql(Some(&order_by));
        assert_eq!(sql, " ORDER BY _relay_sort_0 ASC, _relay_cursor ASC");
    }

    #[test]
    fn test_build_relay_backward_outer_order_sql_desc_preserved() {
        let order_by = vec![OrderByClause {
            field:     "score".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = build_relay_backward_outer_order_sql(Some(&order_by));
        assert_eq!(sql, " ORDER BY _relay_sort_0 DESC, _relay_cursor ASC");
    }

    // ── build_relay_where_sql ─────────────────────────────────────────────

    #[test]
    fn test_build_relay_where_sql_none_none() {
        let sql = build_relay_where_sql(None, None);
        assert_eq!(sql, "");
    }

    #[test]
    fn test_build_relay_where_sql_cursor_only() {
        let sql = build_relay_where_sql(Some("cur > @p1"), None);
        assert_eq!(sql, " WHERE cur > @p1");
    }

    #[test]
    fn test_build_relay_where_sql_user_only() {
        let sql = build_relay_where_sql(None, Some("user_filter"));
        assert_eq!(sql, " WHERE (user_filter)");
    }

    #[test]
    fn test_build_relay_where_sql_both() {
        let sql = build_relay_where_sql(Some("cur > @p1"), Some("user_filter"));
        assert_eq!(sql, " WHERE cur > @p1 AND (user_filter)");
    }

    // ── is_valid_uuid_format ──────────────────────────────────────────────

    #[test]
    fn test_is_valid_uuid_format_accepts_valid_uuid() {
        assert!(is_valid_uuid_format("550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn test_is_valid_uuid_format_rejects_malformed() {
        assert!(!is_valid_uuid_format("not-a-uuid"));
        assert!(!is_valid_uuid_format("550e8400-e29b-41d4-a716")); // too short
        assert!(!is_valid_uuid_format("550e8400-e29b-41d4-a716-44665544000Z")); // invalid char
    }

    #[test]
    fn test_is_valid_uuid_format_rejects_empty() {
        assert!(!is_valid_uuid_format(""));
    }
}

#[cfg(feature = "test-sqlserver")]
mod integration_tests {
    use super::*;

    // Note: These tests require a running SQL Server instance with test data.
    // Run with: cargo test --features test-sqlserver -p fraiseql-core db::sqlserver::adapter

    const TEST_DB_URL: &str = "server=localhost,1434;database=fraiseql_test;user=sa;password=FraiseQL_Test1234;TrustServerCertificate=true";

    #[tokio::test]
    async fn test_adapter_creation() {
        let adapter = SqlServerAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::SQLServer);
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = SqlServerAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = SqlServerAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        // SQL Server requires ORDER BY for OFFSET...FETCH
        // This test just ensures parameterization works
        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1))
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }
}
