use super::*;
use crate::{identifier::quote_mysql_identifier, types::DatabaseType};

// Unit tests for MySQL adapter internals.
// These tests do NOT require a live MySQL connection.
// Integration tests in the `tests` module below cover actual query execution.

// ========================================================================
// DatabaseType Invariant
// ========================================================================

#[test]
fn mysql_database_type_as_str() {
    assert_eq!(DatabaseType::MySQL.as_str(), "mysql");
    assert_eq!(DatabaseType::MySQL.to_string(), "mysql");
}

#[test]
fn mysql_database_type_differs_from_others() {
    assert_ne!(DatabaseType::MySQL, DatabaseType::PostgreSQL);
    assert_ne!(DatabaseType::MySQL, DatabaseType::SQLite);
    assert_ne!(DatabaseType::MySQL, DatabaseType::SQLServer);
}

// ========================================================================
// MySQL Error Code Mapping
// ========================================================================

#[test]
fn mysql_error_1062_maps_to_unique_violation() {
    assert_eq!(map_mysql_error_code(1062), Some("23505".to_string()));
}

#[test]
fn mysql_error_1169_also_maps_to_unique_violation() {
    assert_eq!(map_mysql_error_code(1169), Some("23505".to_string()));
}

#[test]
fn mysql_error_1048_maps_to_not_null_violation() {
    assert_eq!(map_mysql_error_code(1048), Some("23502".to_string()));
}

#[test]
fn mysql_error_1451_maps_to_foreign_key_violation() {
    assert_eq!(map_mysql_error_code(1451), Some("23503".to_string()));
}

#[test]
fn mysql_error_1452_also_maps_to_foreign_key_violation() {
    assert_eq!(map_mysql_error_code(1452), Some("23503".to_string()));
}

#[test]
fn mysql_error_1205_maps_to_lock_timeout() {
    assert_eq!(map_mysql_error_code(1205), Some("40001".to_string()));
}

#[test]
fn mysql_error_1213_maps_to_deadlock() {
    assert_eq!(map_mysql_error_code(1213), Some("40001".to_string()));
}

#[test]
fn unknown_mysql_error_code_returns_none() {
    assert_eq!(map_mysql_error_code(9999), None);
    assert_eq!(map_mysql_error_code(0), None);
    assert_eq!(map_mysql_error_code(1064), None);
}

// ========================================================================
// Relay Helper Functions
// ========================================================================

#[test]
fn relay_where_both_none_returns_empty() {
    assert_eq!(build_mysql_relay_where(None, None), "");
}

#[test]
fn relay_where_cursor_only() {
    assert_eq!(build_mysql_relay_where(Some("`id` > ?"), None), " WHERE `id` > ?");
}

#[test]
fn relay_where_user_only_wraps_in_parens() {
    assert_eq!(build_mysql_relay_where(None, Some("active = ?")), " WHERE (active = ?)");
}

#[test]
fn relay_where_both_combines_with_and() {
    assert_eq!(
        build_mysql_relay_where(Some("`id` > ?"), Some("active = ?")),
        " WHERE `id` > ? AND (active = ?)"
    );
}

#[test]
fn relay_order_sql_forward_no_custom_order() {
    let quoted_col = quote_mysql_identifier("id");
    let result = build_mysql_relay_order_sql(&quoted_col, None, true);
    assert_eq!(result, " ORDER BY `id` ASC");
}

#[test]
fn relay_order_sql_backward_no_custom_order() {
    let quoted_col = quote_mysql_identifier("id");
    let result = build_mysql_relay_order_sql(&quoted_col, None, false);
    assert_eq!(result, " ORDER BY `id` DESC");
}

#[test]
fn relay_order_sql_forward_with_desc_custom_order() {
    use crate::types::sql_hints::{OrderByClause, OrderDirection};
    let quoted_col = quote_mysql_identifier("id");
    let order_by = vec![OrderByClause::new(
        "created_at".to_string(),
        OrderDirection::Desc,
    )];
    let result = build_mysql_relay_order_sql(&quoted_col, Some(&order_by), true);
    assert!(result.contains("JSON_UNQUOTE(JSON_EXTRACT(data, '$.created_at')) DESC"));
    assert!(result.ends_with("`id` ASC"));
}

#[test]
fn relay_order_sql_backward_flips_asc_to_desc() {
    use crate::types::sql_hints::{OrderByClause, OrderDirection};
    let quoted_col = quote_mysql_identifier("id");
    let order_by = vec![OrderByClause::new(
        "created_at".to_string(),
        OrderDirection::Asc,
    )];
    let result = build_mysql_relay_order_sql(&quoted_col, Some(&order_by), false);
    assert!(result.contains("JSON_UNQUOTE(JSON_EXTRACT(data, '$.created_at')) DESC"));
    assert!(result.ends_with("`id` DESC"));
}

// ========================================================================
// MySQL Identifier Quoting
// ========================================================================

#[test]
fn mysql_identifier_wraps_in_backticks() {
    assert_eq!(quote_mysql_identifier("v_user"), "`v_user`");
}

#[test]
fn mysql_identifier_escapes_embedded_backtick() {
    assert_eq!(quote_mysql_identifier("bad`name"), "`bad``name`");
}

#[test]
fn mysql_identifier_schema_qualified_name() {
    assert_eq!(quote_mysql_identifier("mydb.v_user"), "`mydb`.`v_user`");
}

// ── EP-6: Connection pool failure paths ───────────────────────────────────

#[tokio::test]
async fn test_new_with_malformed_url_returns_connection_pool_error() {
    // sqlx parses the URL immediately; an unparseable string fails before
    // any network I/O occurs and the error is mapped to ConnectionPool.
    let result = MySqlAdapter::new("not-a-mysql-url").await;
    assert!(result.is_err(), "expected error for malformed URL");
    let err = result.err().expect("error confirmed above");
    assert!(
        matches!(err, FraiseQLError::ConnectionPool { .. }),
        "expected ConnectionPool error for malformed URL, got: {err:?}"
    );
}

#[tokio::test]
async fn test_with_pool_size_malformed_url_returns_connection_pool_error() {
    let result = MySqlAdapter::with_pool_size("://bad-url", 1).await;
    assert!(result.is_err(), "expected error for bad URL");
    let err = result.err().expect("error confirmed above");
    assert!(
        matches!(err, FraiseQLError::ConnectionPool { .. }),
        "expected ConnectionPool error for bad URL with custom pool size, got: {err:?}"
    );
}

#[cfg(feature = "test-mysql")]
mod tests {
    use super::*;

    // Note: These tests require a running MySQL instance with test data.
    // Run with: cargo test --features test-mysql -p fraiseql-core db::mysql::adapter

    const TEST_DB_URL: &str =
        "mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql";

    #[tokio::test]
    async fn test_adapter_creation() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::MySQL);
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_parameterized_limit_only() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None, None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }

    #[tokio::test]
    async fn test_parameterized_offset_only() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, None, Some(1), None)
            .await
            .expect("Failed to execute query");

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1), None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }
}
