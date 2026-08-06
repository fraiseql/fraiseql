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

// ============================================================================
// CTEs
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
}

// ============================================================================
// JSON Access
// ============================================================================

mod json_access_parity {
    use insta::assert_snapshot;

    // ── Single-level path ─────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_json_single_level() {
        let sql = r#"SELECT data->>'email' FROM "v_user" WHERE data->>'email' = $1"#;
        assert_snapshot!(sql);
    }

    // ── Nested path (2-level) ─────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_json_nested() {
        let sql =
            r#"SELECT data->'address'->>'city' FROM "v_user" WHERE data->'address'->>'city' = $1"#;
        assert_snapshot!(sql);
    }

    // ── Deep nested path (3-level) ────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_json_deep_nested() {
        let sql = r#"SELECT data->'profile'->'social'->>'twitter' FROM "v_user""#;
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
// Full-Text Search
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
}

// ============================================================================
// Aggregate Function Shapes
// ============================================================================

mod aggregate_dialect_variants {
    use insta::assert_snapshot;

    // ── STDDEV (sample) ───────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_stddev() {
        let sql = "SELECT STDDEV_SAMP(revenue) AS stddev_revenue FROM \"tf_sales\"";
        assert_snapshot!(sql);
    }

    // ── VARIANCE (sample) ─────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_variance() {
        let sql = "SELECT VAR_SAMP(revenue) AS var_revenue FROM \"tf_sales\"";
        assert_snapshot!(sql);
    }

    // ── STRING_AGG / GROUP_CONCAT ─────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_string_agg() {
        let sql = "SELECT STRING_AGG(name, ', ') AS names FROM \"v_user\"";
        assert_snapshot!(sql);
    }

    // ── ARRAY_AGG / JSON_ARRAYAGG ─────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_array_agg() {
        let sql = "SELECT ARRAY_AGG(tag) AS tags FROM \"v_post\"";
        assert_snapshot!(sql);
    }

    // ── BOOL_AND / BOOL_OR ────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_bool_and() {
        let sql = "SELECT BOOL_AND(active) AS all_active FROM \"v_user\"";
        assert_snapshot!(sql);
    }

    // ── Temporal bucketing ────────────────────────────────────────────────────

    #[test]
    fn snapshot_postgres_date_trunc_day() {
        let sql = "SELECT DATE_TRUNC('day', occurred_at) AS day, COUNT(*) FROM \"tf_sales\" GROUP BY DATE_TRUNC('day', occurred_at)";
        assert_snapshot!(sql);
    }
}
