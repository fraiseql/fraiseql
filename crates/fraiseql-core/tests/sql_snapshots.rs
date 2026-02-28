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

use insta::assert_snapshot;

// ============================================================================
// PostgreSQL Query Tests
// ============================================================================

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

// ============================================================================
// Mutation Tests
// ============================================================================

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

// ============================================================================
// Row-Level Security (RLS) Tests
// ============================================================================

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

// ============================================================================
// Edge Case Tests
// ============================================================================

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

// ============================================================================
// Adapter Parity Tests
//
// Each "parity group" shows the same logical operation expressed in the SQL
// dialect of every supported adapter.  Keeping these together makes it easy
// to spot dialect differences at a glance and catches regressions where a
// change to one adapter silently breaks another.
// ============================================================================

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

// ---------------------------------------------------------------------------
// (Legacy individual-adapter tests kept for backward compatibility)
// ---------------------------------------------------------------------------

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

// ============================================================================
// Performance-Critical Patterns
// ============================================================================

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

// ============================================================================
// SQL Server Relay Pagination SQL Snapshots
//
// These tests document the exact SQL emitted for each relay pagination scenario.
// The backward-pagination tests are especially important: they verify that
// sort directions are flipped in the inner query and then restored in the outer
// re-sort wrapper — the critical correctness fix from rc.14.
// ============================================================================

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
