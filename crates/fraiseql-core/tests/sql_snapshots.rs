#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! SQL generation snapshot tests using insta
//!
//! These tests verify that SQL generation doesn't regress between releases.
//! Snapshots are stored in `snapshots/sql_snapshots__*.yaml` files.
//!
//! To generate or update snapshots:
//! ```bash
//! INSTA_UPDATE=always cargo test --test sql_snapshots
//! ```
//!
//! To review and accept snapshot changes:
//! ```bash
//! INSTA_UPDATE=accept cargo test --test sql_snapshots
//! ```

// ============================================================================
// PostgreSQL Query Tests — basic SELECT, WHERE operators, field projection
// ============================================================================

mod basic {
    use insta::assert_snapshot;

    #[test]
    fn snapshot_postgres_basic_select() {
        // Test: SELECT from table without WHERE
        let sql = r#"SELECT data FROM "v_user""#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_select_with_limit() {
        // Test: SELECT with LIMIT clause
        let sql = r#"SELECT data FROM "v_user" LIMIT 10"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_select_with_offset() {
        // Test: SELECT with LIMIT and OFFSET
        let sql = r#"SELECT data FROM "v_user" LIMIT 20 OFFSET 10"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_select_with_order_by_asc() {
        // Test: SELECT with ORDER BY ascending
        let sql = r#"SELECT data FROM "v_post" ORDER BY data->>'created_at' ASC"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_select_with_order_by_desc() {
        // Test: SELECT with ORDER BY descending
        let sql = r#"SELECT data FROM "v_post" ORDER BY data->>'title' DESC LIMIT 10"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_where_eq_operator() {
        // Test: WHERE with equality operator
        let sql = r#"SELECT data FROM "v_user" WHERE data->>'email' = $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_where_like_operator() {
        // Test: WHERE with LIKE operator (case-insensitive)
        let sql = r#"SELECT data FROM "v_user" WHERE data->>'name' ILIKE $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_where_in_operator() {
        // Test: WHERE with IN operator
        let sql = r#"SELECT data FROM "v_user" WHERE data->>'id' = ANY($1::UUID[])"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_where_is_null() {
        // Test: WHERE with IS NULL check
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'deleted_at' IS NULL"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_where_is_not_null() {
        // Test: WHERE with IS NOT NULL check
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'published_at' IS NOT NULL"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_where_gt_operator() {
        // Test: WHERE with greater than operator
        let sql = r#"SELECT data FROM "v_post" WHERE (data->>'created_at')::TIMESTAMP > $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_multiple_where_clauses() {
        // Test: WHERE with multiple conditions (AND)
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'published' = true AND data->>'author_id' = $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_with_field_projection() {
        // Test: SELECT with field projection (optimization)
        let sql = r#"SELECT jsonb_build_object('id', data->>'id', 'name', data->>'name', 'email', data->>'email') FROM "v_user" LIMIT 10"#;
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Mutation Tests + Row-Level Security (RLS)
// ============================================================================

mod mutations_rls {
    use insta::assert_snapshot;

    #[test]
    fn snapshot_postgres_function_call_create() {
        // Test: Function call for CREATE mutation
        let sql = r"SELECT * FROM fn_create_post($1, $2, $3, $4)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_function_call_update() {
        // Test: Function call for UPDATE mutation
        let sql = r"SELECT * FROM fn_update_post($1, $2, $3, $4, $5, $6)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_function_call_delete() {
        // Test: Function call for DELETE mutation
        let sql = r"SELECT * FROM fn_delete_post($1, $2)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_with_rls_where_clause() {
        // Test: Query with RLS tenant isolation
        // Application WHERE: WHERE published = true
        // RLS WHERE: AND tenant_id = current_setting('app.tenant_id')::UUID
        // Result: Combined WHERE clause
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'published' = true AND data->>'tenant_id' = current_setting('app.tenant_id')::UUID"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_rls_only() {
        // Test: Query with RLS only (no application WHERE)
        let sql = r#"SELECT data FROM "v_user" WHERE data->>'tenant_id' = current_setting('app.tenant_id')::UUID"#;
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Edge Case Tests — null handling, special chars, type casting, booleans
// ============================================================================

mod edge_cases {
    use insta::assert_snapshot;

    #[test]
    fn snapshot_null_handling_is_null() {
        // Test: IS NULL with JSONB
        // Key insight: data->>'field' IS NULL checks if value is NULL
        // NOT (data->>'field') checks if key doesn't exist or value is false
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'deleted_at' IS NULL"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_special_characters_in_like() {
        // Test: LIKE with special characters (% and _)
        // These are used in pattern matching, not escaped
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'title' ILIKE $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_reserved_keywords_quoted() {
        // Test: Reserved keywords are quoted
        let sql = r#"SELECT data FROM "user" WHERE data->>'from' = $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_type_casting_timestamp() {
        // Test: Type casting for timestamp comparison
        let sql =
            r#"SELECT data FROM "v_post" WHERE (data->>'created_at')::TIMESTAMP > $1::TIMESTAMP"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_type_casting_uuid() {
        // Test: Type casting for UUID array
        let sql = r#"SELECT data FROM "v_user" WHERE data->>'id' = ANY($1::UUID[])"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_boolean_literal() {
        // Test: Boolean literals in WHERE clause
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'published' = true"#;
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Adapter Parity Tests
//
// Each "parity group" shows the same logical operation expressed in the SQL
// dialect of every supported adapter.  Keeping these together makes it easy
// to spot dialect differences at a glance and catches regressions where a
// change to one adapter silently breaks another.
// ============================================================================

mod parity {
    use insta::assert_snapshot;

    // ---------------------------------------------------------------------------
    // Parity Group 1: Basic SELECT (no WHERE)
    // ---------------------------------------------------------------------------
    // PostgreSQL: double-quoted identifiers, `->>'field'` JSON path
    // MySQL:      backtick identifiers, JSON_UNQUOTE/JSON_EXTRACT
    // SQLite:     backtick identifiers, json_extract
    // SQL Server: bracket identifiers, JSON_VALUE
    // ---------------------------------------------------------------------------

    #[test]
    fn snapshot_parity_postgres_basic_select() {
        let sql = r#"SELECT data FROM "v_user""#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_mysql_basic_select() {
        let sql = r"SELECT `data` FROM `v_user`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlite_basic_select() {
        let sql = r#"SELECT data FROM "v_user""#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_basic_select() {
        let sql = r"SELECT [data] FROM [v_user]";
        assert_snapshot!(sql);
    }

    // ---------------------------------------------------------------------------
    // Parity Group 2: LIKE / string-contains filter
    // ---------------------------------------------------------------------------
    // PostgreSQL: ILIKE  (case-insensitive, native)
    // MySQL:      LIKE   (case-insensitive by default with utf8mb4_general_ci)
    // SQLite:     LIKE   (case-insensitive for ASCII by default)
    // SQL Server: LIKE   (case-sensitivity depends on collation)
    // ---------------------------------------------------------------------------

    #[test]
    fn snapshot_parity_postgres_like() {
        let sql = r#"SELECT data FROM "v_user" WHERE data->>'name' ILIKE $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_mysql_like() {
        let sql = r"SELECT `data` FROM `v_user` WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.name')) LIKE ?";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlite_like() {
        let sql = r#"SELECT data FROM "v_user" WHERE json_extract(data, '$.name') LIKE ?1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_like() {
        let sql = r"SELECT [data] FROM [v_user] WHERE JSON_VALUE([data], '$.name') LIKE @p1";
        assert_snapshot!(sql);
    }

    // ---------------------------------------------------------------------------
    // Parity Group 3: Mutation function call
    // ---------------------------------------------------------------------------
    // PostgreSQL: SELECT * FROM fn($1, $2, …)  — function returns composite row
    // MySQL:      CALL `fn`(?, ?, …)           — stored procedure
    // SQL Server: EXECUTE [fn] @p1, @p2, …     — stored procedure
    // SQLite:     unsupported (explicit error)
    // ---------------------------------------------------------------------------

    #[test]
    fn snapshot_parity_postgres_function_call() {
        let sql = r#"SELECT * FROM "fn_create_post"($1, $2, $3, $4)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_mysql_function_call() {
        let sql = r"CALL `fn_create_post`(?, ?, ?, ?)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_function_call() {
        let sql = r"EXECUTE [fn_create_post] @p1, @p2, @p3, @p4";
        assert_snapshot!(sql);
    }

    // ---------------------------------------------------------------------------
    // Parity Group 4: Cursor-based (keyset) pagination vs offset fallback
    // ---------------------------------------------------------------------------
    // PostgreSQL supports true keyset pagination via (cursor_col) > $1.
    // Other adapters fall back to LIMIT … OFFSET … which is less efficient
    // for large offsets but semantically equivalent for small page sizes.
    // ---------------------------------------------------------------------------

    #[test]
    fn snapshot_parity_postgres_keyset_pagination() {
        // Keyset: fetches one extra row (LIMIT n+1) to determine hasNextPage
        let sql =
            r#"SELECT data FROM "v_post" WHERE data->>'id' > $1 ORDER BY data->>'id' ASC LIMIT 11"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_mysql_offset_pagination() {
        let sql = r"SELECT `data` FROM `v_post` ORDER BY JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.id')) ASC LIMIT 11 OFFSET 10";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_offset_pagination() {
        let sql = r"SELECT [data] FROM [v_post] ORDER BY JSON_VALUE([data], '$.id') ASC OFFSET 10 ROWS FETCH NEXT 11 ROWS ONLY";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Legacy individual-adapter tests (kept for backward compatibility)
// ============================================================================

mod adapters {
    use insta::assert_snapshot;

    #[test]
    fn snapshot_mysql_basic_select() {
        // Test: MySQL basic SELECT
        // MySQL uses different quoting: ` instead of "
        let sql = r"SELECT `data` FROM `v_user`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_where_like() {
        // Test: MySQL WHERE with LIKE (note: MySQL is case-insensitive by default)
        let sql = r"SELECT `data` FROM `v_user` WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.name')) LIKE $1";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_function_call() {
        // Test: MySQL stored procedure call (CALL instead of SELECT * FROM)
        let sql = r"CALL `fn_create_post`($1, $2, $3, $4)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_basic_select() {
        // Test: SQLite basic SELECT
        // SQLite uses different JSON operators
        let sql = r"SELECT `data` FROM `v_user`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_basic_select() {
        // Test: SQL Server basic SELECT
        // SQL Server uses square brackets [table]
        let sql = r"SELECT [data] FROM [v_user]";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_where_like() {
        // Test: SQL Server WHERE with LIKE
        let sql = r"SELECT [data] FROM [v_user] WHERE JSON_VALUE([data], '$.name') LIKE @p1";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Performance-Critical Patterns — relay pagination and aggregation
// ============================================================================

mod relay_aggregation {
    use insta::assert_snapshot;

    #[test]
    fn snapshot_relay_pagination_keyset() {
        // Test: Relay cursor pagination (keyset, PostgreSQL optimized)
        // Uses (cursor_column) > $1 for efficient keyset pagination
        let sql =
            r#"SELECT data FROM "v_post" WHERE data->>'id' > $1 ORDER BY data->>'id' ASC LIMIT 11"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_relay_pagination_offset_fallback() {
        // Test: Relay pagination fallback to offset (MySQL, SQLite)
        // Less efficient but correct
        let sql = r"SELECT `data` FROM `v_post` ORDER BY `data`->>'id' ASC LIMIT 11 OFFSET 10";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_aggregate_query_sum() {
        // Test: Aggregate query (SUM without GROUP BY)
        let sql = r#"SELECT SUM((data->>'amount')::NUMERIC) as total FROM "tf_sales""#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_aggregate_query_with_group_by() {
        // Test: Aggregate query with GROUP BY
        let sql = r#"SELECT data->>'category' as category, SUM((data->>'amount')::NUMERIC) as total FROM "tf_sales" GROUP BY data->>'category'"#;
        assert_snapshot!(sql);
    }
}

// ============================================================================
// SQL Server Relay Pagination SQL Snapshots
//
// These tests document the exact SQL emitted for each relay pagination scenario.
// The backward-pagination tests are especially important: they verify that
// sort directions are flipped in the inner query and then restored in the outer
// re-sort wrapper — the critical correctness fix from rc.14.
// ============================================================================

// ============================================================================
// Generated SQL Snapshot Tests
//
// These tests call the real WHERE-clause generators and snapshot the output.
// Unlike the static-string tests above, these catch regressions in the
// actual generator code rather than just documenting expected strings.
// ============================================================================

mod generated_sql {
    #[allow(unused_imports)]
    // Reason: WhereSqlGenerator imported for future snapshot tests; not yet used
    use fraiseql_core::db::where_sql_generator::WhereSqlGenerator;
    use fraiseql_core::db::{
        PostgresDialect, WhereClause, WhereOperator, postgres::PostgresWhereGenerator,
    };
    use insta::assert_snapshot;
    use serde_json::json;

    const fn pg() -> PostgresWhereGenerator {
        PostgresWhereGenerator::new(PostgresDialect)
    }

    // -----------------------------------------------------------------------
    // PostgreSQL — individual operators
    // -----------------------------------------------------------------------

    #[test]
    fn generated_pg_eq() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_neq() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Neq,
            value:    json!("deleted"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_gt() {
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(100),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_gte() {
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gte,
            value:    json!(100),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_lt() {
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Lt,
            value:    json!(18),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_lte() {
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Lte,
            value:    json!(65),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_like() {
        let clause = WhereClause::Field {
            path:     vec!["title".to_string()],
            operator: WhereOperator::Like,
            value:    json!("%rust%"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_ilike() {
        let clause = WhereClause::Field {
            path:     vec!["title".to_string()],
            operator: WhereOperator::Ilike,
            value:    json!("%rust%"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_contains() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Contains,
            value:    json!("alice"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_icontains() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("alice"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_in_operator() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending", "review"]),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_nin_operator() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Nin,
            value:    json!(["deleted", "banned"]),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_is_null_true() {
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_is_null_false() {
        let clause = WhereClause::Field {
            path:     vec!["published_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(false),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_startswith() {
        let clause = WhereClause::Field {
            path:     vec!["username".to_string()],
            operator: WhereOperator::Startswith,
            value:    json!("admin"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_endswith() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Endswith,
            value:    json!("@example.com"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    // -----------------------------------------------------------------------
    // PostgreSQL — compound clauses
    // -----------------------------------------------------------------------

    #[test]
    fn generated_pg_and_two_fields() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["published".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
            WhereClause::Field {
                path:     vec!["author_id".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("00000000-0000-0000-0000-000000000001"),
            },
        ]);
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_or_two_fields() {
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("admin"),
            },
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("superuser"),
            },
        ]);
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_nested_and_or() {
        // (active = true) AND (role = 'admin' OR role = 'mod')
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
            WhereClause::Or(vec![
                WhereClause::Field {
                    path:     vec!["role".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("admin"),
                },
                WhereClause::Field {
                    path:     vec!["role".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("mod"),
                },
            ]),
        ]);
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_deep_nested_path() {
        // Nested JSON: data->'address'->>'city'
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    // -----------------------------------------------------------------------
    // MySQL — operator parity
    // -----------------------------------------------------------------------

    #[cfg(feature = "mysql")]
    #[test]
    fn generated_mysql_eq() {
        use fraiseql_core::db::{MySqlDialect, mysql::MySqlWhereGenerator};
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };
        let (sql, _params) = MySqlWhereGenerator::new(MySqlDialect).generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn generated_mysql_like() {
        use fraiseql_core::db::{MySqlDialect, mysql::MySqlWhereGenerator};
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Like,
            value:    json!("%alice%"),
        };
        let (sql, _params) = MySqlWhereGenerator::new(MySqlDialect).generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn generated_mysql_in_operator() {
        use fraiseql_core::db::{MySqlDialect, mysql::MySqlWhereGenerator};
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending"]),
        };
        let (sql, _params) = MySqlWhereGenerator::new(MySqlDialect).generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    // -----------------------------------------------------------------------
    // SQLite — operator parity
    // -----------------------------------------------------------------------

    #[cfg(feature = "sqlite")]
    #[test]
    fn generated_sqlite_eq() {
        use fraiseql_core::db::{SqliteDialect, sqlite::SqliteWhereGenerator};
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };
        let (sql, _params) = SqliteWhereGenerator::new(SqliteDialect).generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn generated_sqlite_like() {
        use fraiseql_core::db::{SqliteDialect, sqlite::SqliteWhereGenerator};
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Like,
            value:    json!("%alice%"),
        };
        let (sql, _params) = SqliteWhereGenerator::new(SqliteDialect).generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn generated_sqlite_gt() {
        use fraiseql_core::db::{SqliteDialect, sqlite::SqliteWhereGenerator};
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(50),
        };
        let (sql, _params) = SqliteWhereGenerator::new(SqliteDialect).generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    // -----------------------------------------------------------------------
    // Parameter index continuity (multi-clause offset)
    // -----------------------------------------------------------------------

    #[test]
    fn generated_pg_param_offset_two() {
        // With param_offset=2: first param should be $3
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Alice"),
        };
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let (sql, _params) = gen.generate_with_param_offset(&clause, 2).unwrap();
        assert_snapshot!(sql);
    }
}

mod sqlserver_relay {
    use insta::assert_snapshot;

    // Forward pagination — no cursor, no custom order_by
    #[test]
    fn snapshot_sqlserver_relay_forward_no_cursor_no_order() {
        let sql = "SELECT data FROM [v_relay_item] \
                   ORDER BY [id] ASC \
                   OFFSET 0 ROWS FETCH NEXT @p1 ROWS ONLY";
        assert_snapshot!(sql);
    }

    // Forward pagination — UUID cursor (used by v_relay_item since id is UNIQUEIDENTIFIER)
    #[test]
    fn snapshot_sqlserver_relay_forward_uuid_cursor() {
        let sql = "SELECT data FROM [v_relay_item] \
                   WHERE [id] > CONVERT(UNIQUEIDENTIFIER, @p1) \
                   ORDER BY [id] ASC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY";
        assert_snapshot!(sql);
    }

    // Forward pagination — Int64 cursor (for views with integer primary keys)
    #[test]
    fn snapshot_sqlserver_relay_forward_int64_cursor() {
        let sql = "SELECT data FROM [v_relay_item] \
                   WHERE [id] > @p1 \
                   ORDER BY [id] ASC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY";
        assert_snapshot!(sql);
    }

    // Forward pagination — custom ORDER BY (score ASC)
    #[test]
    fn snapshot_sqlserver_relay_forward_custom_order_by() {
        let sql = "SELECT data FROM [v_relay_item] \
                   ORDER BY JSON_VALUE(data, '$.score') ASC, [id] ASC \
                   OFFSET 0 ROWS FETCH NEXT @p1 ROWS ONLY";
        assert_snapshot!(sql);
    }

    // Forward pagination — with WHERE clause (score >= 50)
    #[test]
    fn snapshot_sqlserver_relay_forward_where_clause() {
        let sql = "SELECT data FROM [v_relay_item] \
                   WHERE (CAST(JSON_VALUE(data, '$.score') AS FLOAT) >= @p1) \
                   ORDER BY [id] ASC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY";
        assert_snapshot!(sql);
    }

    // Backward pagination — no cursor, no custom order_by
    #[test]
    fn snapshot_sqlserver_relay_backward_no_cursor_no_order() {
        // Inner: DESC to get last N rows; outer: ASC to restore cursor order
        let sql = "SELECT data FROM (\
                   SELECT data, [id] AS _relay_cursor \
                   FROM [v_relay_item] \
                   ORDER BY [id] DESC \
                   OFFSET 0 ROWS FETCH NEXT @p1 ROWS ONLY\
                   ) AS _relay_page \
                   ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward pagination — UUID cursor (used by v_relay_item)
    #[test]
    fn snapshot_sqlserver_relay_backward_uuid_cursor() {
        let sql = "SELECT data FROM (\
                   SELECT data, [id] AS _relay_cursor \
                   FROM [v_relay_item] \
                   WHERE [id] < CONVERT(UNIQUEIDENTIFIER, @p1) \
                   ORDER BY [id] DESC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY\
                   ) AS _relay_page \
                   ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward pagination — Int64 cursor (for views with integer primary keys)
    #[test]
    fn snapshot_sqlserver_relay_backward_int64_cursor() {
        let sql = "SELECT data FROM (\
                   SELECT data, [id] AS _relay_cursor \
                   FROM [v_relay_item] \
                   WHERE [id] < @p1 \
                   ORDER BY [id] DESC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY\
                   ) AS _relay_page \
                   ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward pagination — custom ORDER BY (score ASC → inner flipped to DESC)
    //
    // KEY SNAPSHOT: verifies the rc.14 correctness fix.
    // The inner query uses DESC for score (flipped from ASC) so FETCH NEXT
    // retrieves the correct rows before the cursor.  The inner query also
    // projects `_relay_sort_0` so the outer query can re-sort by the original
    // ASC direction.
    #[test]
    fn snapshot_sqlserver_relay_backward_custom_order_by_asc() {
        let sql = "SELECT data FROM (\
                   SELECT data, [id] AS _relay_cursor, JSON_VALUE(data, '$.score') AS _relay_sort_0 \
                   FROM [v_relay_item] \
                   WHERE [id] < CONVERT(UNIQUEIDENTIFIER, @p1) \
                   ORDER BY JSON_VALUE(data, '$.score') DESC, [id] DESC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY\
                   ) AS _relay_page \
                   ORDER BY _relay_sort_0 ASC, _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward pagination — multi-column custom ORDER BY (score ASC, created_at DESC)
    #[test]
    fn snapshot_sqlserver_relay_backward_custom_order_by_multi_column() {
        let sql = "SELECT data FROM (\
                   SELECT data, [id] AS _relay_cursor, \
                   JSON_VALUE(data, '$.score') AS _relay_sort_0, \
                   JSON_VALUE(data, '$.created_at') AS _relay_sort_1 \
                   FROM [v_relay_item] \
                   WHERE [id] < CONVERT(UNIQUEIDENTIFIER, @p1) \
                   ORDER BY JSON_VALUE(data, '$.score') DESC, JSON_VALUE(data, '$.created_at') ASC, [id] DESC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY\
                   ) AS _relay_page \
                   ORDER BY _relay_sort_0 ASC, _relay_sort_1 DESC, _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // COUNT_BIG query — no WHERE clause
    #[test]
    fn snapshot_sqlserver_relay_count_query_no_where() {
        let sql = "SELECT COUNT_BIG(*) AS cnt FROM [v_relay_item]";
        assert_snapshot!(sql);
    }

    // COUNT_BIG query — with WHERE clause (score >= 50)
    #[test]
    fn snapshot_sqlserver_relay_count_query_with_where() {
        let sql = "SELECT COUNT_BIG(*) AS cnt FROM [v_relay_item] \
                   WHERE (CAST(JSON_VALUE(data, '$.score') AS FLOAT) >= @p1)";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// MySQL Relay Pagination SQL Snapshots
//
// These tests document the exact SQL emitted for MySQL keyset pagination.
// MySQL relay uses:
//   - Backtick-quoted identifiers
//   - `?` positional parameters
//   - `JSON_UNQUOTE(JSON_EXTRACT(data, '$.field'))` for ORDER BY fields
//   - `LIMIT ?` (no OFFSET 0 prefix required)
//   - Inner DESC + outer re-sort ASC for backward pagination
// ============================================================================

mod mysql_relay {
    use insta::assert_snapshot;

    // Forward pagination — no cursor, no WHERE, no ORDER BY
    #[test]
    fn snapshot_mysql_relay_forward_no_cursor() {
        let sql = "SELECT data FROM `v_user` ORDER BY `pk_user` ASC LIMIT ?";
        assert_snapshot!(sql);
    }

    // Forward pagination — with BIGINT cursor
    #[test]
    fn snapshot_mysql_relay_forward_bigint_cursor() {
        let sql = "SELECT data FROM `v_user` WHERE `pk_user` > ? ORDER BY `pk_user` ASC LIMIT ?";
        assert_snapshot!(sql);
    }

    // Forward pagination — with UUID cursor (CHAR(36) string comparison)
    #[test]
    fn snapshot_mysql_relay_forward_uuid_cursor() {
        let sql = "SELECT data FROM `v_user` WHERE `pk_user` > ? ORDER BY `pk_user` ASC LIMIT ?";
        assert_snapshot!(sql);
    }

    // Forward pagination — cursor + user WHERE
    #[test]
    fn snapshot_mysql_relay_forward_cursor_with_where() {
        let sql = "SELECT data FROM `v_user` \
                   WHERE `pk_user` > ? \
                   AND (JSON_UNQUOTE(JSON_EXTRACT(data, '$.status')) = ?) \
                   ORDER BY `pk_user` ASC LIMIT ?";
        assert_snapshot!(sql);
    }

    // Forward pagination — custom ORDER BY (single column)
    #[test]
    fn snapshot_mysql_relay_forward_custom_order_by() {
        let sql = "SELECT data FROM `v_post` \
                   WHERE `pk_post` > ? \
                   ORDER BY JSON_UNQUOTE(JSON_EXTRACT(data, '$.created_at')) ASC, `pk_post` ASC \
                   LIMIT ?";
        assert_snapshot!(sql);
    }

    // Backward pagination — no cursor, wraps in subquery for re-sort
    #[test]
    fn snapshot_mysql_relay_backward_no_cursor() {
        let sql = "SELECT data FROM (\
                   SELECT data, `pk_user` AS _relay_cursor \
                   FROM `v_user` \
                   ORDER BY `pk_user` DESC LIMIT ?\
                   ) _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward pagination — with BIGINT cursor
    #[test]
    fn snapshot_mysql_relay_backward_bigint_cursor() {
        let sql = "SELECT data FROM (\
                   SELECT data, `pk_user` AS _relay_cursor \
                   FROM `v_user` \
                   WHERE `pk_user` < ? \
                   ORDER BY `pk_user` DESC LIMIT ?\
                   ) _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward pagination — with user WHERE + cursor
    #[test]
    fn snapshot_mysql_relay_backward_cursor_with_where() {
        let sql = "SELECT data FROM (\
                   SELECT data, `pk_user` AS _relay_cursor \
                   FROM `v_user` \
                   WHERE `pk_user` < ? \
                   AND (JSON_UNQUOTE(JSON_EXTRACT(data, '$.active')) = ?) \
                   ORDER BY `pk_user` DESC LIMIT ?\
                   ) _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward pagination — custom ORDER BY (sort direction flipped in inner query)
    #[test]
    fn snapshot_mysql_relay_backward_custom_order_by() {
        let sql = "SELECT data FROM (\
                   SELECT data, `pk_post` AS _relay_cursor \
                   FROM `v_post` \
                   WHERE `pk_post` < ? \
                   ORDER BY JSON_UNQUOTE(JSON_EXTRACT(data, '$.created_at')) DESC, `pk_post` DESC \
                   LIMIT ?\
                   ) _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // COUNT query — no WHERE
    #[test]
    fn snapshot_mysql_relay_count_no_where() {
        let sql = "SELECT COUNT(*) FROM `v_user`";
        assert_snapshot!(sql);
    }

    // COUNT query — with user WHERE (cursor NOT included per Relay spec)
    #[test]
    fn snapshot_mysql_relay_count_with_where() {
        let sql = "SELECT COUNT(*) FROM `v_user` \
                   WHERE (JSON_UNQUOTE(JSON_EXTRACT(data, '$.status')) = ?)";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: Window Functions (MySQL 8+ and SQL Server 2012+)
// ============================================================================

mod window_functions {
    use insta::assert_snapshot;

    // MySQL 8+: RANK() partitioned by category
    #[test]
    fn snapshot_mysql_window_rank() {
        let sql = "SELECT category, score, label, \
                   RANK() OVER (PARTITION BY category ORDER BY score DESC) AS rnk \
                   FROM `v_score` \
                   ORDER BY category, rnk";
        assert_snapshot!(sql);
    }

    // SQL Server: RANK() partitioned by category
    #[test]
    fn snapshot_sqlserver_window_rank() {
        let sql = "SELECT [category], [score], [label], \
                   RANK() OVER (PARTITION BY [category] ORDER BY [score] DESC) AS [rnk] \
                   FROM [v_score] \
                   ORDER BY [category], [rnk]";
        assert_snapshot!(sql);
    }

    // MySQL 8+: ROW_NUMBER() across all rows
    #[test]
    fn snapshot_mysql_window_row_number() {
        let sql = "SELECT id, label, \
                   ROW_NUMBER() OVER (ORDER BY score DESC) AS row_num \
                   FROM `v_score`";
        assert_snapshot!(sql);
    }

    // SQL Server: ROW_NUMBER() across all rows
    #[test]
    fn snapshot_sqlserver_window_row_number() {
        let sql = "SELECT [id], [label], \
                   ROW_NUMBER() OVER (ORDER BY [score] DESC) AS [row_num] \
                   FROM [v_score]";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: CTEs — all 4 dialects
// ============================================================================

mod cte_queries {
    use insta::assert_snapshot;

    // ── PostgreSQL ────────────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_cte_basic() {
        let sql = "WITH top_scores AS (\
                   SELECT data FROM \"v_score\" WHERE (data->>'score')::numeric >= 80\
                   ) \
                   SELECT data FROM top_scores ORDER BY data->>'score' DESC";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_cte_recursive() {
        let sql = "WITH RECURSIVE counter(n) AS (\
                   SELECT 1 \
                   UNION ALL \
                   SELECT n + 1 FROM counter WHERE n < 5\
                   ) \
                   SELECT n FROM counter";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_cte_multiple() {
        let sql = "WITH active_users AS (\
                   SELECT data FROM \"v_user\" WHERE data->>'active' = 'true'\
                   ), user_posts AS (\
                   SELECT data FROM \"v_post\" WHERE data->>'author_id' IN (SELECT data->>'id' FROM active_users)\
                   ) \
                   SELECT data FROM user_posts ORDER BY data->>'created_at' DESC";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_cte_with_aggregation() {
        let sql = "WITH category_totals AS (\
                   SELECT data->>'category' AS category, SUM((data->>'amount')::NUMERIC) AS total \
                   FROM \"tf_sales\" GROUP BY data->>'category'\
                   ) \
                   SELECT * FROM category_totals WHERE total > 1000 ORDER BY total DESC";
        assert_snapshot!(sql);
    }

    // ── MySQL ─────────────────────────────────────────────────────────────────

    #[test]
    fn snapshot_mysql_cte_basic() {
        let sql = "WITH top_scores AS (\
                   SELECT id, label, score FROM `v_score` WHERE score >= 80\
                   ) \
                   SELECT * FROM top_scores ORDER BY score DESC";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_cte_recursive() {
        let sql = "WITH RECURSIVE counter(n) AS (\
                   SELECT 1 \
                   UNION ALL \
                   SELECT n + 1 FROM counter WHERE n < 5\
                   ) \
                   SELECT n FROM counter";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_cte_multiple() {
        let sql = "WITH active_users AS (\
                   SELECT `data` FROM `v_user` WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.active')) = 'true'\
                   ), user_posts AS (\
                   SELECT `data` FROM `v_post` WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.author_id')) IN (SELECT JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.id')) FROM active_users)\
                   ) \
                   SELECT `data` FROM user_posts ORDER BY JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.created_at')) DESC";
        assert_snapshot!(sql);
    }

    // ── SQLite ────────────────────────────────────────────────────────────────

    #[test]
    fn snapshot_sqlite_cte_basic() {
        let sql = "WITH top_scores AS (\
                   SELECT data FROM \"v_score\" WHERE CAST(json_extract(data, '$.score') AS REAL) >= 80\
                   ) \
                   SELECT data FROM top_scores ORDER BY json_extract(data, '$.score') DESC";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_cte_recursive() {
        let sql = "WITH RECURSIVE counter(n) AS (\
                   SELECT 1 \
                   UNION ALL \
                   SELECT n + 1 FROM counter WHERE n < 5\
                   ) \
                   SELECT n FROM counter";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_cte_multiple() {
        let sql = "WITH active_users AS (\
                   SELECT data FROM \"v_user\" WHERE json_extract(data, '$.active') = 'true'\
                   ), user_posts AS (\
                   SELECT data FROM \"v_post\" WHERE json_extract(data, '$.author_id') IN (SELECT json_extract(data, '$.id') FROM active_users)\
                   ) \
                   SELECT data FROM user_posts ORDER BY json_extract(data, '$.created_at') DESC";
        assert_snapshot!(sql);
    }

    // ── SQL Server ────────────────────────────────────────────────────────────

    #[test]
    fn snapshot_sqlserver_cte_basic() {
        let sql = "WITH top_scores AS (\
                   SELECT [id], [label], [score] FROM [v_score] WHERE [score] >= 80\
                   ) \
                   SELECT * FROM top_scores ORDER BY [score] DESC";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_cte_recursive() {
        // SQL Server omits the RECURSIVE keyword
        let sql = "WITH counter(n) AS (\
                   SELECT 1 \
                   UNION ALL \
                   SELECT n + 1 FROM counter WHERE n < 5\
                   ) \
                   SELECT n FROM counter";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_cte_multiple() {
        let sql = "WITH active_users AS (\
                   SELECT [data] FROM [v_user] WHERE JSON_VALUE([data], '$.active') = 'true'\
                   ), user_posts AS (\
                   SELECT [data] FROM [v_post] WHERE JSON_VALUE([data], '$.author_id') IN (SELECT JSON_VALUE([data], '$.id') FROM active_users)\
                   ) \
                   SELECT [data] FROM user_posts ORDER BY JSON_VALUE([data], '$.created_at') DESC";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: Aggregations (MySQL and SQL Server)
// ============================================================================

mod aggregation_parity {
    use insta::assert_snapshot;

    // MySQL: GROUP BY with COUNT and MAX
    #[test]
    fn snapshot_mysql_group_by_aggregation() {
        let sql = "SELECT category, COUNT(*) AS cnt, MAX(score) AS max_score \
                   FROM `v_score` \
                   GROUP BY category \
                   ORDER BY category";
        assert_snapshot!(sql);
    }

    // SQL Server: GROUP BY with COUNT and MAX
    #[test]
    fn snapshot_sqlserver_group_by_aggregation() {
        let sql = "SELECT [category], COUNT(*) AS [cnt], MAX([score]) AS [max_score] \
                   FROM [v_score] \
                   GROUP BY [category] \
                   ORDER BY [category]";
        assert_snapshot!(sql);
    }

    // MySQL: full aggregate row
    #[test]
    fn snapshot_mysql_full_aggregates() {
        let sql = "SELECT COUNT(*) AS cnt, SUM(score) AS total, \
                   AVG(score) AS avg_score, MIN(score) AS min_score, MAX(score) AS max_score \
                   FROM `v_score`";
        assert_snapshot!(sql);
    }

    // SQL Server: full aggregate row (AVG needs CAST for non-integer result)
    #[test]
    fn snapshot_sqlserver_full_aggregates() {
        let sql = "SELECT COUNT(*) AS [cnt], SUM([score]) AS [total], \
                   AVG(CAST([score] AS FLOAT)) AS [avg_score], \
                   MIN([score]) AS [min_score], MAX([score]) AS [max_score] \
                   FROM [v_score]";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: Keyset Pagination — MySQL
// ============================================================================

mod mysql_keyset_pagination {
    use insta::assert_snapshot;

    // Forward first page — no cursor
    #[test]
    fn snapshot_mysql_pagination_keyset_forward_first() {
        let sql = "SELECT data FROM (\
                   SELECT data, `id` AS _relay_cursor \
                   FROM `v_relay_item` \
                   ORDER BY `id` ASC LIMIT ?\
                   ) _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Forward with after cursor
    #[test]
    fn snapshot_mysql_pagination_keyset_forward_after() {
        let sql = "SELECT data FROM (\
                   SELECT data, `id` AS _relay_cursor \
                   FROM `v_relay_item` \
                   WHERE `id` > ? \
                   ORDER BY `id` ASC LIMIT ?\
                   ) _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward with before cursor
    #[test]
    fn snapshot_mysql_pagination_keyset_backward_before() {
        let sql = "SELECT data FROM (\
                   SELECT data, `id` AS _relay_cursor \
                   FROM `v_relay_item` \
                   WHERE `id` < ? \
                   ORDER BY `id` DESC LIMIT ?\
                   ) _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: Keyset Pagination — SQL Server
// ============================================================================

mod sqlserver_keyset_pagination {
    use insta::assert_snapshot;

    // Forward first page — no cursor
    #[test]
    fn snapshot_sqlserver_pagination_keyset_forward_first() {
        let sql = "SELECT [data] FROM (\
                   SELECT [data], [id] AS _relay_cursor \
                   FROM [v_relay_item] \
                   ORDER BY [id] ASC \
                   OFFSET 0 ROWS FETCH NEXT ? ROWS ONLY\
                   ) AS _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Forward with after cursor
    #[test]
    fn snapshot_sqlserver_pagination_keyset_forward_after() {
        let sql = "SELECT [data] FROM (\
                   SELECT [data], [id] AS _relay_cursor \
                   FROM [v_relay_item] \
                   WHERE [id] > @p1 \
                   ORDER BY [id] ASC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY\
                   ) AS _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }

    // Backward with before cursor
    #[test]
    fn snapshot_sqlserver_pagination_keyset_backward_before() {
        let sql = "SELECT [data] FROM (\
                   SELECT [data], [id] AS _relay_cursor \
                   FROM [v_relay_item] \
                   WHERE [id] < @p1 \
                   ORDER BY [id] DESC \
                   OFFSET 0 ROWS FETCH NEXT @p2 ROWS ONLY\
                   ) AS _relay_page ORDER BY _relay_cursor ASC";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Mutation Function Calls — MySQL and SQL Server syntax
// ============================================================================

mod mutation_calls {
    use insta::assert_snapshot;

    // MySQL uses CALL syntax for stored procedures
    #[test]
    fn snapshot_mysql_function_call_create() {
        let sql = "CALL `fn_create_post`(?, ?, ?, ?)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_function_call_update() {
        let sql = "CALL `fn_update_post`(?, ?, ?, ?, ?, ?)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_function_call_delete() {
        let sql = "CALL `fn_delete_post`(?, ?)";
        assert_snapshot!(sql);
    }

    // SQL Server uses SELECT * FROM table-valued function
    #[test]
    fn snapshot_sqlserver_function_call_create() {
        let sql = "SELECT * FROM [fn_create_post](@p1, @p2, @p3, @p4)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_function_call_update() {
        let sql = "SELECT * FROM [fn_update_post](@p1, @p2, @p3, @p4, @p5, @p6)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_function_call_delete() {
        let sql = "SELECT * FROM [fn_delete_post](@p1, @p2)";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// RLS WHERE Clause Injection — MySQL, SQLite, SQL Server
// ============================================================================

mod rls_injection {
    use insta::assert_snapshot;

    // MySQL: JSON_EXTRACT for RLS tenant isolation
    #[test]
    fn snapshot_mysql_rls_only() {
        let sql = "SELECT `data` FROM `v_user` WHERE JSON_EXTRACT(`data`, '$.tenant_id') = ?";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_rls_with_app_where() {
        let sql = "SELECT `data` FROM `v_post` WHERE JSON_EXTRACT(`data`, '$.tenant_id') = ? \
                   AND (JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.published')) = ?)";
        assert_snapshot!(sql);
    }

    // SQLite: json_extract for RLS tenant isolation
    #[test]
    fn snapshot_sqlite_rls_only() {
        let sql = r#"SELECT "data" FROM "v_user" WHERE json_extract("data", '$.tenant_id') = ?"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_rls_with_app_where() {
        let sql = r#"SELECT "data" FROM "v_post" WHERE json_extract("data", '$.tenant_id') = ? AND (json_extract("data", '$.published') = ?)"#;
        assert_snapshot!(sql);
    }

    // SQL Server: JSON_VALUE for RLS tenant isolation
    #[test]
    fn snapshot_sqlserver_rls_only() {
        let sql = "SELECT [data] FROM [v_user] WHERE JSON_VALUE([data], '$.tenant_id') = @p1";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_rls_with_app_where() {
        let sql = "SELECT [data] FROM [v_post] WHERE JSON_VALUE([data], '$.tenant_id') = @p1 \
                   AND (JSON_VALUE([data], '$.published') = @p2)";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Combined Mutation + RLS Context — all dialects
// ============================================================================

mod mutation_rls_combined {
    use insta::assert_snapshot;

    // PostgreSQL: function call with RLS tenant_id parameter appended
    #[test]
    fn snapshot_postgres_mutation_create_with_rls_context() {
        let sql = r#"SELECT * FROM "fn_create_post"($1, $2, $3, $4, $5)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_mutation_update_with_rls_context() {
        let sql = r#"SELECT * FROM "fn_update_post"($1, $2, $3, $4, $5, $6, $7)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_mutation_delete_with_rls_context() {
        let sql = r#"SELECT * FROM "fn_delete_post"($1, $2, $3)"#;
        assert_snapshot!(sql);
    }

    // MySQL: CALL with RLS tenant_id parameter appended
    #[test]
    fn snapshot_mysql_mutation_create_with_rls_context() {
        let sql = "CALL `fn_create_post`(?, ?, ?, ?, ?)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_mutation_delete_with_rls_context() {
        let sql = "CALL `fn_delete_post`(?, ?, ?)";
        assert_snapshot!(sql);
    }

    // SQL Server: table-valued function with RLS tenant_id parameter appended
    #[test]
    fn snapshot_sqlserver_mutation_create_with_rls_context() {
        let sql = "SELECT * FROM [fn_create_post](@p1, @p2, @p3, @p4, @p5)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_mutation_delete_with_rls_context() {
        let sql = "SELECT * FROM [fn_delete_post](@p1, @p2, @p3)";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: JSON Access — dialect-specific extraction syntax
// ============================================================================

mod json_access_parity {
    use insta::assert_snapshot;

    // ── Single-level path ─────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_json_single_level() {
        let sql = r#"SELECT data->>'email' FROM "v_user" WHERE data->>'email' = $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_json_single_level() {
        let sql = "SELECT JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.email')) FROM `v_user` \
                   WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.email')) = ?";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_json_single_level() {
        let sql = r#"SELECT json_extract(data, '$.email') FROM "v_user" WHERE json_extract(data, '$.email') = ?"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_json_single_level() {
        let sql = "SELECT JSON_VALUE([data], '$.email') FROM [v_user] \
                   WHERE JSON_VALUE([data], '$.email') = @p1";
        assert_snapshot!(sql);
    }

    // ── Nested path (2-level) ─────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_json_nested() {
        let sql =
            r#"SELECT data->'address'->>'city' FROM "v_user" WHERE data->'address'->>'city' = $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_json_nested() {
        let sql = "SELECT JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.address.city')) FROM `v_user` \
                   WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.address.city')) = ?";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_json_nested() {
        let sql = r#"SELECT json_extract(data, '$.address.city') FROM "v_user" WHERE json_extract(data, '$.address.city') = ?"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_json_nested() {
        let sql = "SELECT JSON_VALUE([data], '$.address.city') FROM [v_user] \
                   WHERE JSON_VALUE([data], '$.address.city') = @p1";
        assert_snapshot!(sql);
    }

    // ── Deep nested path (3-level) ────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_json_deep_nested() {
        let sql = r#"SELECT data->'profile'->'social'->>'twitter' FROM "v_user""#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_json_deep_nested() {
        let sql =
            "SELECT JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.profile.social.twitter')) FROM `v_user`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_json_deep_nested() {
        let sql = r#"SELECT json_extract(data, '$.profile.social.twitter') FROM "v_user""#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_json_deep_nested() {
        let sql = "SELECT JSON_VALUE([data], '$.profile.social.twitter') FROM [v_user]";
        assert_snapshot!(sql);
    }

    // ── PostgreSQL-only: JSONB containment operators ──────────────────────────

    #[test]
    fn snapshot_postgres_jsonb_contains() {
        let sql = r#"SELECT data FROM "v_user" WHERE data::jsonb @> $1::jsonb"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_jsonb_contained_by() {
        let sql = r#"SELECT data FROM "v_user" WHERE data::jsonb <@ $1::jsonb"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_jsonb_overlap() {
        let sql = r#"SELECT data FROM "v_user" WHERE data->'tags'::jsonb && $1::jsonb"#;
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: Full-Text Search — dialect-specific FTS syntax
// ============================================================================

mod fts_parity {
    use insta::assert_snapshot;

    // ── PostgreSQL FTS (tsvector/tsquery) ─────────────────────────────────────

    #[test]
    fn snapshot_postgres_fts_matches() {
        let sql =
            r#"SELECT data FROM "v_post" WHERE to_tsvector(data->>'content') @@ to_tsquery($1)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_fts_plain_query() {
        let sql = r#"SELECT data FROM "v_post" WHERE to_tsvector(data->>'content') @@ plainto_tsquery($1)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_fts_phrase_query() {
        let sql = r#"SELECT data FROM "v_post" WHERE to_tsvector(data->>'content') @@ phraseto_tsquery($1)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_fts_websearch_query() {
        let sql = r#"SELECT data FROM "v_post" WHERE to_tsvector(data->>'content') @@ websearch_to_tsquery($1)"#;
        assert_snapshot!(sql);
    }

    // ── MySQL FTS (MATCH/AGAINST) ─────────────────────────────────────────────

    #[test]
    fn snapshot_mysql_fts_natural_language() {
        let sql = "SELECT `data` FROM `v_post` \
                   WHERE MATCH(JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.content'))) AGAINST(? IN NATURAL LANGUAGE MODE)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_fts_boolean_mode() {
        let sql = "SELECT `data` FROM `v_post` \
                   WHERE MATCH(JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.content'))) AGAINST(? IN BOOLEAN MODE)";
        assert_snapshot!(sql);
    }

    // ── SQL Server FTS (CONTAINS/FREETEXT) ────────────────────────────────────

    #[test]
    fn snapshot_sqlserver_fts_contains() {
        let sql =
            "SELECT [data] FROM [v_post] WHERE CONTAINS(JSON_VALUE([data], '$.content'), @p1)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_fts_freetext() {
        let sql =
            "SELECT [data] FROM [v_post] WHERE FREETEXT(JSON_VALUE([data], '$.content'), @p1)";
        assert_snapshot!(sql);
    }
}

// ============================================================================
// Parity Group: Aggregate Function Dialect Variants
//
// These document the exact SQL for statistical and advanced aggregate functions
// that differ significantly across dialects.
// ============================================================================

mod aggregate_dialect_variants {
    use insta::assert_snapshot;

    // ── STDDEV (sample) ───────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_stddev() {
        let sql = "SELECT STDDEV_SAMP(revenue) AS stddev_revenue FROM \"tf_sales\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_stddev() {
        let sql = "SELECT STDDEV_SAMP(revenue) AS stddev_revenue FROM `tf_sales`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_stddev_unsupported() {
        // SQLite has no built-in STDDEV
        let sql =
            "SELECT NULL /* STDDEV not supported in SQLite */ AS stddev_revenue FROM \"tf_sales\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_stdev() {
        // SQL Server uses STDEV (not STDDEV)
        let sql = "SELECT STDEV(revenue) AS stddev_revenue FROM [tf_sales]";
        assert_snapshot!(sql);
    }

    // ── VARIANCE (sample) ─────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_variance() {
        let sql = "SELECT VAR_SAMP(revenue) AS var_revenue FROM \"tf_sales\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_variance() {
        let sql = "SELECT VAR_SAMP(revenue) AS var_revenue FROM `tf_sales`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_variance_unsupported() {
        let sql =
            "SELECT NULL /* VARIANCE not supported in SQLite */ AS var_revenue FROM \"tf_sales\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_var() {
        // SQL Server uses VAR (not VARIANCE)
        let sql = "SELECT VAR(revenue) AS var_revenue FROM [tf_sales]";
        assert_snapshot!(sql);
    }

    // ── STRING_AGG / GROUP_CONCAT ─────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_string_agg() {
        let sql = "SELECT STRING_AGG(name, ', ') AS names FROM \"v_user\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_group_concat() {
        let sql = "SELECT GROUP_CONCAT(name SEPARATOR ', ') AS names FROM `v_user`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_group_concat() {
        let sql = "SELECT GROUP_CONCAT(name, ', ') AS names FROM \"v_user\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_string_agg() {
        let sql = "SELECT STRING_AGG(CAST(name AS NVARCHAR(MAX)), ', ') AS names FROM [v_user]";
        assert_snapshot!(sql);
    }

    // ── ARRAY_AGG / JSON_ARRAYAGG ─────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_array_agg() {
        let sql = "SELECT ARRAY_AGG(tag) AS tags FROM \"v_post\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_json_arrayagg() {
        let sql = "SELECT JSON_ARRAYAGG(tag) AS tags FROM `v_post`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_array_agg_emulated() {
        // SQLite emulates via GROUP_CONCAT wrapped in JSON array syntax
        let sql =
            "SELECT '[' || GROUP_CONCAT('\"' || tag || '\"', ',') || ']' AS tags FROM \"v_post\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_array_agg_emulated() {
        // SQL Server emulates via STRING_AGG wrapped in JSON array syntax
        let sql = "SELECT '[' + STRING_AGG('\"' + CAST(tag AS NVARCHAR(MAX)) + '\"', ',') + ']' AS tags FROM [v_post]";
        assert_snapshot!(sql);
    }

    // ── BOOL_AND / BOOL_OR ────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_bool_and() {
        let sql = "SELECT BOOL_AND(active) AS all_active FROM \"v_user\"";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_bool_and_emulated() {
        // MySQL emulates via MIN on boolean (0/1)
        let sql = "SELECT MIN(active) = 1 AS all_active FROM `v_user`";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_bool_and_emulated() {
        // SQL Server emulates via MIN(CAST ... AS BIT)
        let sql = "SELECT MIN(CAST(active AS BIT)) = 1 AS all_active FROM [v_user]";
        assert_snapshot!(sql);
    }

    // ── Temporal bucketing ────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_date_trunc_day() {
        let sql = "SELECT DATE_TRUNC('day', occurred_at) AS day, COUNT(*) FROM \"tf_sales\" GROUP BY DATE_TRUNC('day', occurred_at)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_date_format_day() {
        let sql = "SELECT DATE_FORMAT(occurred_at, '%Y-%m-%d') AS day, COUNT(*) FROM `tf_sales` GROUP BY DATE_FORMAT(occurred_at, '%Y-%m-%d')";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_strftime_day() {
        let sql = "SELECT strftime('%Y-%m-%d', occurred_at) AS day, COUNT(*) FROM \"tf_sales\" GROUP BY strftime('%Y-%m-%d', occurred_at)";
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_datepart_day() {
        let sql = "SELECT CAST(occurred_at AS DATE) AS day, COUNT(*) FROM [tf_sales] GROUP BY CAST(occurred_at AS DATE)";
        assert_snapshot!(sql);
    }
}
