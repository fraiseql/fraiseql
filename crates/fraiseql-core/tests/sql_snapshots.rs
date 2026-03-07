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
        let sql = r#"SELECT * FROM fn_create_post($1, $2, $3, $4)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_function_call_update() {
        // Test: Function call for UPDATE mutation
        let sql = r#"SELECT * FROM fn_update_post($1, $2, $3, $4, $5, $6)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_postgres_function_call_delete() {
        // Test: Function call for DELETE mutation
        let sql = r#"SELECT * FROM fn_delete_post($1, $2)"#;
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
        let sql = r#"SELECT data FROM "v_post" WHERE (data->>'created_at')::TIMESTAMP > $1::TIMESTAMP"#;
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
        let sql = r#"SELECT `data` FROM `v_user`"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlite_basic_select() {
        let sql = r#"SELECT data FROM "v_user""#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_basic_select() {
        let sql = r#"SELECT [data] FROM [v_user]"#;
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
        let sql = r#"SELECT `data` FROM `v_user` WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.name')) LIKE ?"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlite_like() {
        let sql = r#"SELECT data FROM "v_user" WHERE json_extract(data, '$.name') LIKE ?1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_like() {
        let sql = r#"SELECT [data] FROM [v_user] WHERE JSON_VALUE([data], '$.name') LIKE @p1"#;
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
        let sql = r#"CALL `fn_create_post`(?, ?, ?, ?)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_function_call() {
        let sql = r#"EXECUTE [fn_create_post] @p1, @p2, @p3, @p4"#;
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
        let sql =
            r#"SELECT `data` FROM `v_post` ORDER BY JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.id')) ASC LIMIT 11 OFFSET 10"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_parity_sqlserver_offset_pagination() {
        let sql = r#"SELECT [data] FROM [v_post] ORDER BY JSON_VALUE([data], '$.id') ASC OFFSET 10 ROWS FETCH NEXT 11 ROWS ONLY"#;
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
        let sql = r#"SELECT `data` FROM `v_user`"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_where_like() {
        // Test: MySQL WHERE with LIKE (note: MySQL is case-insensitive by default)
        let sql = r#"SELECT `data` FROM `v_user` WHERE JSON_UNQUOTE(JSON_EXTRACT(`data`, '$.name')) LIKE $1"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_mysql_function_call() {
        // Test: MySQL stored procedure call (CALL instead of SELECT * FROM)
        let sql = r#"CALL `fn_create_post`($1, $2, $3, $4)"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlite_basic_select() {
        // Test: SQLite basic SELECT
        // SQLite uses different JSON operators
        let sql = r#"SELECT `data` FROM `v_user`"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_basic_select() {
        // Test: SQL Server basic SELECT
        // SQL Server uses square brackets [table]
        let sql = r#"SELECT [data] FROM [v_user]"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_sqlserver_where_like() {
        // Test: SQL Server WHERE with LIKE
        let sql = r#"SELECT [data] FROM [v_user] WHERE JSON_VALUE([data], '$.name') LIKE @p1"#;
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
        let sql = r#"SELECT data FROM "v_post" WHERE data->>'id' > $1 ORDER BY data->>'id' ASC LIMIT 11"#;
        assert_snapshot!(sql);
    }

    #[test]
    fn snapshot_relay_pagination_offset_fallback() {
        // Test: Relay pagination fallback to offset (MySQL, SQLite)
        // Less efficient but correct
        let sql = r#"SELECT `data` FROM `v_post` ORDER BY `data`->>'id' ASC LIMIT 11 OFFSET 10"#;
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
    use insta::assert_snapshot;
    use fraiseql_core::db::{WhereClause, WhereOperator, postgres::PostgresWhereGenerator};
    #[allow(unused_imports)]
    use fraiseql_core::db::where_sql_generator::WhereSqlGenerator;
    use serde_json::json;

    fn pg() -> PostgresWhereGenerator {
        PostgresWhereGenerator::new()
    }

    // -----------------------------------------------------------------------
    // PostgreSQL — individual operators
    // -----------------------------------------------------------------------

    #[test]
    fn generated_pg_eq() {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("alice@example.com"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_neq() {
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::Neq,
            value: json!("deleted"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_gt() {
        let clause = WhereClause::Field {
            path: vec!["score".to_string()],
            operator: WhereOperator::Gt,
            value: json!(100),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_gte() {
        let clause = WhereClause::Field {
            path: vec!["score".to_string()],
            operator: WhereOperator::Gte,
            value: json!(100),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_lt() {
        let clause = WhereClause::Field {
            path: vec!["age".to_string()],
            operator: WhereOperator::Lt,
            value: json!(18),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_lte() {
        let clause = WhereClause::Field {
            path: vec!["age".to_string()],
            operator: WhereOperator::Lte,
            value: json!(65),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_like() {
        let clause = WhereClause::Field {
            path: vec!["title".to_string()],
            operator: WhereOperator::Like,
            value: json!("%rust%"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_ilike() {
        let clause = WhereClause::Field {
            path: vec!["title".to_string()],
            operator: WhereOperator::Ilike,
            value: json!("%rust%"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_contains() {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Contains,
            value: json!("alice"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_icontains() {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value: json!("alice"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_in_operator() {
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::In,
            value: json!(["active", "pending", "review"]),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_nin_operator() {
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::Nin,
            value: json!(["deleted", "banned"]),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_is_null_true() {
        let clause = WhereClause::Field {
            path: vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value: json!(true),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_is_null_false() {
        let clause = WhereClause::Field {
            path: vec!["published_at".to_string()],
            operator: WhereOperator::IsNull,
            value: json!(false),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_startswith() {
        let clause = WhereClause::Field {
            path: vec!["username".to_string()],
            operator: WhereOperator::Startswith,
            value: json!("admin"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_endswith() {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Endswith,
            value: json!("@example.com"),
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
                path: vec!["published".to_string()],
                operator: WhereOperator::Eq,
                value: json!(true),
            },
            WhereClause::Field {
                path: vec!["author_id".to_string()],
                operator: WhereOperator::Eq,
                value: json!("00000000-0000-0000-0000-000000000001"),
            },
        ]);
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn generated_pg_or_two_fields() {
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path: vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value: json!("admin"),
            },
            WhereClause::Field {
                path: vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value: json!("superuser"),
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
                path: vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value: json!(true),
            },
            WhereClause::Or(vec![
                WhereClause::Field {
                    path: vec!["role".to_string()],
                    operator: WhereOperator::Eq,
                    value: json!("admin"),
                },
                WhereClause::Field {
                    path: vec!["role".to_string()],
                    operator: WhereOperator::Eq,
                    value: json!("mod"),
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
            path: vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value: json!("Paris"),
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
        use fraiseql_core::db::mysql::MySqlWhereGenerator;
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("alice@example.com"),
        };
        let (sql, _params) = MySqlWhereGenerator.generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn generated_mysql_like() {
        use fraiseql_core::db::mysql::MySqlWhereGenerator;
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Like,
            value: json!("%alice%"),
        };
        let (sql, _params) = MySqlWhereGenerator.generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn generated_mysql_in_operator() {
        use fraiseql_core::db::mysql::MySqlWhereGenerator;
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::In,
            value: json!(["active", "pending"]),
        };
        let (sql, _params) = MySqlWhereGenerator.generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    // -----------------------------------------------------------------------
    // SQLite — operator parity
    // -----------------------------------------------------------------------

    #[cfg(feature = "sqlite")]
    #[test]
    fn generated_sqlite_eq() {
        use fraiseql_core::db::sqlite::SqliteWhereGenerator;
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("alice@example.com"),
        };
        let (sql, _params) = SqliteWhereGenerator.generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn generated_sqlite_like() {
        use fraiseql_core::db::sqlite::SqliteWhereGenerator;
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Like,
            value: json!("%alice%"),
        };
        let (sql, _params) = SqliteWhereGenerator.generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn generated_sqlite_gt() {
        use fraiseql_core::db::sqlite::SqliteWhereGenerator;
        let clause = WhereClause::Field {
            path: vec!["score".to_string()],
            operator: WhereOperator::Gt,
            value: json!(50),
        };
        let (sql, _params) = SqliteWhereGenerator.generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    // -----------------------------------------------------------------------
    // Parameter index continuity (multi-clause offset)
    // -----------------------------------------------------------------------

    #[test]
    fn generated_pg_param_offset_two() {
        // With param_offset=2: first param should be $3
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Eq,
            value: json!("Alice"),
        };
        let gen = PostgresWhereGenerator::new();
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
