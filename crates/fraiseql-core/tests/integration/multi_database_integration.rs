//! Multi-database integration tests for FraiseQL adapters.
#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design
//! These tests validate that each database adapter works correctly against
//! real database instances. Tests are gated by feature flags and require
//! Docker containers to be running.
//!
//! # Running Tests
//!
//! ```bash
//! # Start test databases
//! docker compose -f docker-compose.test.yml up -d
//!
//! # Wait for databases to be ready
//! sleep 10
//!
//! # Run MySQL integration tests
//! cargo test -p fraiseql-core --features test-mysql --test multi_database_integration
//!
//! # Run SQLite tests (no Docker needed)
//! cargo test -p fraiseql-core --features sqlite --test multi_database_integration
//!
//! # Run SQL Server integration tests
//! cargo test -p fraiseql-core --features test-sqlserver --test multi_database_integration
//! ```

// Database adapters (conditionally compiled based on features)
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
use std::sync::Arc;

#[cfg(feature = "mysql")]
#[allow(unused_imports)]
// Reason: imported by conditional feature gate; used when test-mysql is enabled
use fraiseql_core::db::mysql::MySqlAdapter;
#[cfg(feature = "sqlite")]
#[allow(unused_imports)]
// Reason: imported by conditional feature gate; used when test-sqlite is enabled
use fraiseql_core::db::sqlite::SqliteAdapter;
#[cfg(feature = "sqlserver")]
#[allow(unused_imports)]
// Reason: imported by conditional feature gate; used when test-sqlserver is enabled
use fraiseql_core::db::sqlserver::SqlServerAdapter;
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
use fraiseql_core::db::traits::DatabaseAdapter;
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
use fraiseql_core::db::types::DatabaseType;
// Note: WhereClause and WhereOperator available for future WHERE tests
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(unused_imports)]
// Reason: WhereClause/WhereOperator reserved for future WHERE-clause tests; feature-gated
use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};

// ============================================================================
// MySQL Integration Tests
// ============================================================================

#[cfg(feature = "test-mysql")]
mod mysql_tests {
    use super::*;

    fn mysql_url() -> String {
        std::env::var("MYSQL_URL")
            .expect("MYSQL_URL must be set (e.g. via `dagger call test-integration --suite=mysql`)")
    }

    #[tokio::test]
    async fn test_mysql_adapter_creation() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        assert_eq!(adapter.database_type(), DatabaseType::MySQL);

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0, "Pool should have connections");
    }

    #[tokio::test]
    async fn test_mysql_health_check() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        adapter.health_check().await.expect("Health check should pass");
    }

    #[tokio::test]
    async fn test_mysql_execute_raw_query() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_raw_query("SELECT 1 as value")
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("value"));
    }

    #[tokio::test]
    async fn test_mysql_query_v_user_view() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(10), None, None)
            .await
            .expect("Query should succeed");

        assert!(!results.is_empty(), "v_user view should have test data");

        // Verify JSON structure
        let first = results[0].as_value();
        assert!(first.get("id").is_some(), "Should have id field");
        assert!(first.get("name").is_some(), "Should have name field");
        assert!(first.get("email").is_some(), "Should have email field");
    }

    #[tokio::test]
    async fn test_mysql_query_with_limit() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None, None)
            .await
            .expect("Query should succeed");

        assert!(results.len() <= 2, "Should respect LIMIT clause");
    }

    #[tokio::test]
    async fn test_mysql_query_with_offset() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        // Get all users first
        let all_results = adapter
            .execute_where_query("v_user", None, Some(10), None, None)
            .await
            .expect("Query should succeed");

        // Get users with offset
        let offset_results = adapter
            .execute_where_query("v_user", None, Some(10), Some(1), None)
            .await
            .expect("Query should succeed");

        if all_results.len() > 1 {
            assert_eq!(offset_results.len(), all_results.len() - 1, "Offset should skip first row");
        }
    }

    #[tokio::test]
    async fn test_mysql_query_v_post_with_nested_author() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_post", None, Some(5), None, None)
            .await
            .expect("Query should succeed");

        assert!(!results.is_empty(), "v_post view should have test data");

        // Verify nested author object
        let first = results[0].as_value();
        assert!(first.get("id").is_some(), "Should have id field");
        assert!(first.get("title").is_some(), "Should have title field");
        assert!(first.get("author").is_some(), "Should have nested author object");

        let author = first.get("author").unwrap();
        assert!(author.get("id").is_some(), "Author should have id");
        assert!(author.get("name").is_some(), "Author should have name");
    }

    #[tokio::test]
    async fn test_mysql_pool_metrics() {
        let adapter =
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter");

        let metrics = adapter.pool_metrics();

        assert!(metrics.total_connections > 0, "Should have total connections");
        assert!(
            metrics.idle_connections <= metrics.total_connections,
            "Idle should not exceed total"
        );
    }

    #[tokio::test]
    async fn test_mysql_concurrent_queries() {
        let adapter = Arc::new(
            MySqlAdapter::new(&mysql_url()).await.expect("Failed to create MySQL adapter"),
        );

        let mut handles = Vec::new();

        for _ in 0..10 {
            let adapter_clone = Arc::clone(&adapter);
            let handle = tokio::spawn(async move {
                adapter_clone.execute_where_query("v_user", None, Some(5), None, None).await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles).await.into_iter().collect();

        for result in results {
            assert!(result.is_ok(), "Task should complete");
            assert!(result.unwrap().is_ok(), "Query should succeed");
        }
    }
}

// ============================================================================
// SQLite Integration Tests
// ============================================================================

#[cfg(feature = "sqlite")]
mod sqlite_tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_in_memory_adapter_creation() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        assert_eq!(adapter.database_type(), DatabaseType::SQLite);

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0, "Pool should have connections");
    }

    #[tokio::test]
    async fn test_sqlite_health_check() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        adapter.health_check().await.expect("Health check should pass");
    }

    #[tokio::test]
    async fn test_sqlite_execute_raw_query() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        let results = adapter
            .execute_raw_query("SELECT 1 as value")
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("value"));
    }

    #[tokio::test]
    async fn test_sqlite_create_and_query_view() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create test table
        adapter
            .execute_raw_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
            .await
            .expect("Create table should succeed");

        // Insert test data
        adapter
            .execute_raw_query(
                "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')",
            )
            .await
            .expect("Insert should succeed");

        adapter
            .execute_raw_query("INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')")
            .await
            .expect("Insert should succeed");

        // Create view returning JSON
        adapter
            .execute_raw_query(
                r"CREATE VIEW v_user AS
                   SELECT id, json_object('id', id, 'name', name, 'email', email) AS data
                   FROM users",
            )
            .await
            .expect("Create view should succeed");

        // Query the view
        let results = adapter
            .execute_where_query("v_user", None, Some(10), None, None)
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 2, "Should have 2 users");

        let first = results[0].as_value();
        assert!(first.get("id").is_some(), "Should have id field");
        assert!(first.get("name").is_some(), "Should have name field");
        assert!(first.get("email").is_some(), "Should have email field");
    }

    #[tokio::test]
    async fn test_sqlite_query_with_limit() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Setup test data
        adapter
            .execute_raw_query("CREATE TABLE items (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .expect("Create table should succeed");

        for i in 1..=5 {
            adapter
                .execute_raw_query(&format!(
                    "INSERT INTO items (data) VALUES ('{}')",
                    serde_json::json!({"value": i})
                ))
                .await
                .expect("Insert should succeed");
        }

        adapter
            .execute_raw_query("CREATE VIEW v_items AS SELECT id, data FROM items")
            .await
            .expect("Create view should succeed");

        // Query with limit
        let results = adapter
            .execute_where_query("v_items", None, Some(2), None, None)
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 2, "Should respect LIMIT clause");
    }

    #[tokio::test]
    async fn test_sqlite_query_with_offset() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Setup test data
        adapter
            .execute_raw_query("CREATE TABLE items (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .expect("Create table should succeed");

        for i in 1..=5 {
            adapter
                .execute_raw_query(&format!(
                    "INSERT INTO items (data) VALUES ('{}')",
                    serde_json::json!({"value": i})
                ))
                .await
                .expect("Insert should succeed");
        }

        adapter
            .execute_raw_query("CREATE VIEW v_items AS SELECT id, data FROM items")
            .await
            .expect("Create view should succeed");

        // Query with offset
        let results = adapter
            .execute_where_query("v_items", None, Some(10), Some(2), None)
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 3, "Should skip first 2 rows");
    }

    #[tokio::test]
    async fn test_sqlite_pool_metrics() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        let metrics = adapter.pool_metrics();

        assert!(metrics.total_connections > 0, "Should have total connections");
        assert!(
            metrics.idle_connections <= metrics.total_connections,
            "Idle should not exceed total"
        );
    }

    #[tokio::test]
    async fn test_sqlite_nested_json_view() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create tables
        adapter
            .execute_raw_query("CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .expect("Create authors table should succeed");

        adapter
            .execute_raw_query(
                "CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT, author_id INTEGER REFERENCES authors(id))",
            )
            .await
            .expect("Create posts table should succeed");

        // Insert test data
        adapter
            .execute_raw_query("INSERT INTO authors (id, name) VALUES (1, 'Alice')")
            .await
            .expect("Insert author should succeed");

        adapter
            .execute_raw_query("INSERT INTO posts (title, author_id) VALUES ('Hello World', 1)")
            .await
            .expect("Insert post should succeed");

        // Create view with nested JSON
        adapter
            .execute_raw_query(
                r"CREATE VIEW v_post AS
                   SELECT p.id,
                          json_object(
                              'id', p.id,
                              'title', p.title,
                              'author', json_object('id', a.id, 'name', a.name)
                          ) AS data
                   FROM posts p
                   JOIN authors a ON p.author_id = a.id",
            )
            .await
            .expect("Create view should succeed");

        // Query the view
        let results = adapter
            .execute_where_query("v_post", None, Some(10), None, None)
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 1, "Should have 1 post");

        let post = results[0].as_value();
        assert!(post.get("id").is_some(), "Should have id field");
        assert!(post.get("title").is_some(), "Should have title field");
        assert!(post.get("author").is_some(), "Should have nested author");

        let author = post.get("author").unwrap();
        assert!(author.get("id").is_some(), "Author should have id");
        assert!(author.get("name").is_some(), "Author should have name");
    }

    #[tokio::test]
    async fn test_sqlite_concurrent_queries() {
        let adapter =
            Arc::new(SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter"));

        // Setup test data
        adapter
            .execute_raw_query("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .expect("Create table should succeed");

        adapter
            .execute_raw_query(
                "CREATE VIEW v_test AS SELECT id, json_object('id', id) AS data FROM test",
            )
            .await
            .expect("Create view should succeed");

        for i in 1..=10 {
            adapter
                .execute_raw_query(&format!("INSERT INTO test (data) VALUES ('data{i}')"))
                .await
                .expect("Insert should succeed");
        }

        let mut handles = Vec::new();

        for _ in 0..10 {
            let adapter_clone = Arc::clone(&adapter);
            let handle = tokio::spawn(async move {
                adapter_clone.execute_where_query("v_test", None, Some(5), None, None).await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles).await.into_iter().collect();

        for result in results {
            assert!(result.is_ok(), "Task should complete");
            assert!(result.unwrap().is_ok(), "Query should succeed");
        }
    }
}

// ============================================================================
// SQL Server Integration Tests
// ============================================================================

/// Build a SQL Server connection string for `database` from the harness-provided
/// server. `SQLSERVER_URL` holds `server=…;user=…;password=…;TrustServerCertificate=true`
/// (no database); each test appends the database it needs. Returns `None` only when
/// `SQLSERVER_URL` is unset.
#[cfg(feature = "test-sqlserver")]
async fn sqlserver_conn(database: &str) -> Option<String> {
    let svc = fraiseql_test_support::sqlserver().await?;
    Some(format!("{};database={database}", svc.url().trim_end_matches(';')))
}

#[cfg(feature = "test-sqlserver")]
mod sqlserver_tests {
    use super::*;

    /// Adapter against the `master` database (server-level tests).
    async fn master_adapter() -> SqlServerAdapter {
        let url = sqlserver_conn("master").await.expect(
            "SQLSERVER_URL must be set (e.g. via `dagger call test-integration --suite=sqlserver`)",
        );
        SqlServerAdapter::new(&url).await.expect("Failed to create SQL Server adapter")
    }

    #[tokio::test]
    async fn test_sqlserver_adapter_creation() {
        let adapter = master_adapter().await;

        assert_eq!(adapter.database_type(), DatabaseType::SQLServer);

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0, "Pool should have connections");
    }

    #[tokio::test]
    async fn test_sqlserver_health_check() {
        let adapter = master_adapter().await;

        adapter.health_check().await.expect("Health check should pass");
    }

    #[tokio::test]
    async fn test_sqlserver_execute_raw_query() {
        let adapter = master_adapter().await;

        let results = adapter
            .execute_raw_query("SELECT 1 as value")
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("value"));
    }

    #[tokio::test]
    async fn test_sqlserver_query_v_user_view() {
        let Some(url) = sqlserver_conn("fraiseql_test").await else {
            eprintln!("Skipping test_sqlserver_query_v_user_view: SQLSERVER_URL not set");
            return;
        };
        let adapter = SqlServerAdapter::new(&url)
            .await
            .expect("Failed to connect to SQL Server (fraiseql_test)");

        let results = adapter
            .execute_where_query("v_user", None, Some(10), None, None)
            .await
            .expect("Query should succeed");

        assert!(!results.is_empty(), "v_user view should have test data");

        // Verify JSON structure
        let first = results[0].as_value();
        assert!(first.get("id").is_some(), "Should have id field");
        assert!(first.get("name").is_some(), "Should have name field");
        assert!(first.get("email").is_some(), "Should have email field");
    }

    #[tokio::test]
    async fn test_sqlserver_pool_metrics() {
        let adapter = master_adapter().await;

        let metrics = adapter.pool_metrics();

        assert!(metrics.total_connections > 0, "Should have total connections");
        assert!(
            metrics.idle_connections <= metrics.total_connections,
            "Idle should not exceed total"
        );
    }

    #[tokio::test]
    async fn test_sqlserver_concurrent_queries() {
        let adapter = Arc::new(master_adapter().await);

        let mut handles = Vec::new();

        for _ in 0..5 {
            let adapter_clone = Arc::clone(&adapter);
            let handle =
                tokio::spawn(
                    async move { adapter_clone.execute_raw_query("SELECT 1 as value").await },
                );
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles).await.into_iter().collect();

        for result in results {
            assert!(result.is_ok(), "Task should complete");
            assert!(result.unwrap().is_ok(), "Query should succeed");
        }
    }
}

// ============================================================================
// SQL Server Relay Pagination Integration Tests
// ============================================================================

#[cfg(feature = "test-sqlserver")]
mod sqlserver_relay_tests {
    use fraiseql_core::{
        db::{
            sqlserver::SqlServerAdapter,
            traits::{CursorValue, RelayDatabaseAdapter},
            where_clause::{WhereClause, WhereOperator},
        },
        error::FraiseQLError,
    };

    // UUID ids for v_relay_item rows (in ascending SQL Server UNIQUEIDENTIFIER order).
    // These UUIDs are of the form 00000000-0000-0000-0000-00000000000N where N is 1–a.
    // SQL Server compares bytes 10–15 first; for these UUIDs those bytes are
    // 000000000001 … 00000000000a, giving standard ascending order.
    const UUID_3: &str = "00000000-0000-0000-0000-000000000003";
    const UUID_5: &str = "00000000-0000-0000-0000-000000000005";
    const UUID_8: &str = "00000000-0000-0000-0000-000000000008";
    const UUID_10: &str = "00000000-0000-0000-0000-00000000000a";

    async fn adapter() -> SqlServerAdapter {
        let url = super::sqlserver_conn("fraiseql_test").await.expect(
            "SQLSERVER_URL must be set (e.g. via `dagger call test-integration --suite=sqlserver`)",
        );
        SqlServerAdapter::new(&url).await.expect("Failed to connect to SQL Server")
    }

    fn extract_label(row: &fraiseql_core::db::types::JsonbValue) -> String {
        row.as_value()
            .get("label")
            .and_then(|v| v.as_str())
            .expect("row must have 'label' field")
            .to_string()
    }

    fn extract_score(row: &fraiseql_core::db::types::JsonbValue) -> i64 {
        row.as_value()
            .get("score")
            .and_then(|v| v.as_i64())
            .expect("row must have 'score' field")
    }

    #[tokio::test]
    async fn test_sqlserver_relay_forward_first_page() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, true, None, None, false)
            .await
            .expect("forward first page");
        assert_eq!(result.rows().len(), 3);
        let labels: Vec<String> = result.rows().iter().map(extract_label).collect();
        assert_eq!(labels, vec!["item-1", "item-2", "item-3"]);
        assert_eq!(result.total_count(), None);
    }

    #[tokio::test]
    async fn test_sqlserver_relay_forward_with_after_cursor() {
        let a = adapter().await;
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                Some(CursorValue::Uuid(UUID_3.to_string())),
                None,
                3,
                true,
                None,
                None,
                false,
            )
            .await
            .expect("forward with after cursor");
        let labels: Vec<String> = result.rows().iter().map(extract_label).collect();
        assert_eq!(labels, vec!["item-4", "item-5", "item-6"]);
    }

    #[tokio::test]
    async fn test_sqlserver_relay_forward_exhausted() {
        let a = adapter().await;
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                Some(CursorValue::Uuid(UUID_8.to_string())),
                None,
                10,
                true,
                None,
                None,
                false,
            )
            .await
            .expect("forward exhausted");
        let labels: Vec<String> = result.rows().iter().map(extract_label).collect();
        assert_eq!(labels, vec!["item-9", "item-10"]);
    }

    #[tokio::test]
    async fn test_sqlserver_relay_backward_with_before_cursor() {
        let a = adapter().await;
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                None,
                Some(CursorValue::Uuid(UUID_5.to_string())),
                3,
                false,
                None,
                None,
                false,
            )
            .await
            .expect("backward with before cursor");
        // Rows before UUID-5 (exclusive), last 3, re-sorted ASC → items 2,3,4
        let labels: Vec<String> = result.rows().iter().map(extract_label).collect();
        assert_eq!(labels, vec!["item-2", "item-3", "item-4"]);
    }

    #[tokio::test]
    async fn test_sqlserver_relay_backward_first_page_no_cursor() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, false, None, None, false)
            .await
            .expect("backward first page no cursor");
        // Last 3 rows in ascending cursor order → items 8,9,10
        let labels: Vec<String> = result.rows().iter().map(extract_label).collect();
        assert_eq!(labels, vec!["item-8", "item-9", "item-10"]);
    }

    #[tokio::test]
    async fn test_sqlserver_relay_total_count_is_10() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, true, None, None, true)
            .await
            .expect("total count");
        assert_eq!(result.total_count(), Some(10));
    }

    #[tokio::test]
    async fn test_sqlserver_relay_total_count_ignores_cursor() {
        let a = adapter().await;
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                Some(CursorValue::Uuid(UUID_5.to_string())),
                None,
                3,
                true,
                None,
                None,
                true,
            )
            .await
            .expect("total count ignores cursor");
        // totalCount counts all matching rows, not just those after the cursor.
        assert_eq!(result.total_count(), Some(10));
    }

    #[tokio::test]
    async fn test_sqlserver_relay_total_count_absent_when_not_requested() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, true, None, None, false)
            .await
            .expect("no total count");
        assert_eq!(result.total_count(), None);
    }

    #[tokio::test]
    async fn test_sqlserver_relay_forward_with_where_clause() {
        let a = adapter().await;
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gte,
            value:    serde_json::json!(50),
        };
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                None,
                None,
                10,
                true,
                Some(&clause),
                None,
                false,
            )
            .await
            .expect("forward with where clause");
        // Scores ≥ 50: items 1(50), 3(70), 5(90), 7(60), 9(80) → 5 rows
        assert_eq!(result.rows().len(), 5);
        for row in result.rows() {
            let score = extract_score(row);
            assert!(score >= 50, "All rows must have score >= 50, got {score}");
        }
    }

    #[tokio::test]
    async fn test_sqlserver_relay_backward_custom_order_by_score_asc() {
        use fraiseql_core::compiler::aggregation::{OrderByClause, OrderDirection};

        let a = adapter().await;
        let order_by = vec![OrderByClause::new("score".to_string(), OrderDirection::Asc)];

        // before = UUID-5 (score=90), limit=3, forward=false, order_by score ASC.
        // Rows with UUID < UUID-5: item-1(50), item-2(30), item-3(70), item-4(10).
        // Sorted by score ASC: [10, 30, 50, 70]. Last 3 = [30, 50, 70].
        // After backward flip (inner DESC, outer ASC): returned in score ASC order.
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                None,
                Some(CursorValue::Uuid(UUID_5.to_string())),
                3,
                false,
                None,
                Some(&order_by),
                false,
            )
            .await
            .expect("backward custom order_by score asc");

        assert_eq!(result.rows().len(), 3, "Should return exactly 3 rows");

        // Verify scores are in ascending order (proves backward direction flip is correct).
        let scores: Vec<i64> = result.rows().iter().map(extract_score).collect();
        assert_eq!(scores, vec![30, 50, 70], "Rows must be in score ASC order");
    }

    #[tokio::test]
    async fn test_sqlserver_relay_forward_empty_result() {
        let a = adapter().await;
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                Some(CursorValue::Uuid(UUID_10.to_string())),
                None,
                10,
                true,
                None,
                None,
                false,
            )
            .await
            .expect("forward empty result");
        assert!(result.rows().is_empty(), "Should return 0 rows after the last UUID");
    }

    #[tokio::test]
    async fn test_sqlserver_relay_missing_view_returns_error() {
        // Validates count query robustness: a missing view must surface as
        // FraiseQLError::Database, NOT as Ok(total_count: 0).
        let a = adapter().await;
        let err = a
            .execute_relay_page("v_nonexistent", "id", None, None, 3, true, None, None, true)
            .await
            .expect_err("missing view must return Err");
        assert!(
            matches!(err, FraiseQLError::Database { .. }),
            "Expected Database error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_sqlserver_relay_uuid_cursor_invalid_format_returns_validation_error() {
        // Validates UUID validation: malformed UUID must return Validation error before
        // reaching SQL Server, rather than an opaque type-conversion database error.
        let a = adapter().await;
        let err = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                Some(CursorValue::Uuid("not-a-uuid".to_string())),
                None,
                3,
                true,
                None,
                None,
                false,
            )
            .await
            .expect_err("malformed UUID cursor must return Err");
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "Expected Validation error, got {err:?}"
        );
    }
}

// ============================================================================
// MySQL Relay Pagination Tests
// ============================================================================

#[cfg(feature = "test-mysql")]
mod mysql_relay_tests {
    use fraiseql_core::db::{
        mysql::MySqlAdapter,
        traits::{CursorValue, RelayDatabaseAdapter},
        where_clause::{WhereClause, WhereOperator},
    };

    fn mysql_url() -> String {
        std::env::var("MYSQL_URL")
            .expect("MYSQL_URL must be set (e.g. via `dagger call test-integration --suite=mysql`)")
    }

    async fn adapter() -> MySqlAdapter {
        MySqlAdapter::new(&mysql_url()).await.expect("Failed to connect to MySQL")
    }

    fn extract_label(row: &fraiseql_core::db::types::JsonbValue) -> String {
        row.as_value()
            .get("label")
            .and_then(|v| v.as_str())
            .expect("row must have 'label' field")
            .to_string()
    }

    /// Forward pagination returns the first page.
    #[tokio::test]
    async fn test_mysql_relay_forward_first_page() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, true, None, None, false)
            .await
            .expect("forward first page");
        assert_eq!(result.rows().len(), 3);
        // First page has no previous entries (cursor starts at beginning)
        assert!(!result.rows().is_empty(), "first page must return rows");
    }

    /// Forward pagination with an `after` cursor skips earlier rows.
    #[tokio::test]
    async fn test_mysql_relay_forward_with_after_cursor() {
        let a = adapter().await;
        // Fetch first page to get a cursor
        let first = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, true, None, None, false)
            .await
            .expect("first page");
        assert_eq!(first.rows().len(), 3);

        // Extract cursor from the last row's id field (MySQL relay_item uses CHAR(36) UUIDs)
        let last_id = first
            .rows
            .last()
            .and_then(|row| row.as_value().get("id"))
            .and_then(|v| v.as_str())
            .expect("last row must have string id for cursor");
        let cursor_val = CursorValue::Uuid(last_id.to_string());
        let second = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                Some(cursor_val),
                None,
                3,
                true,
                None,
                None,
                false,
            )
            .await
            .expect("second page");
        assert!(!second.rows().is_empty(), "second page must have rows after cursor");
    }

    /// Requesting more rows than exist returns no further pages.
    #[tokio::test]
    async fn test_mysql_relay_forward_exhausted() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 100, true, None, None, false)
            .await
            .expect("over-limit page");
        assert_eq!(result.rows().len(), 10, "all 10 rows returned");
        // Requesting more than total rows means no further pages
        assert!(result.rows().len() <= 100, "rows must not exceed requested limit");
    }

    /// Backward pagination returns the last page.
    #[tokio::test]
    async fn test_mysql_relay_backward_last_page() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, false, None, None, false)
            .await
            .expect("backward last page");
        assert_eq!(result.rows().len(), 3);
        // Backward page of 3 from 10 rows returns exactly 3 rows
        assert!(result.rows().len() <= 3, "must not exceed requested limit");
    }

    /// Total count is returned when requested.
    #[tokio::test]
    async fn test_mysql_relay_total_count() {
        let a = adapter().await;
        let result = a
            .execute_relay_page("v_relay_item", "id", None, None, 3, true, None, None, true)
            .await
            .expect("total count query");
        assert_eq!(result.total_count(), Some(10), "must count all 10 rows");
    }

    /// WHERE filter reduces the result set.
    #[tokio::test]
    async fn test_mysql_relay_forward_with_where_clause() {
        use serde_json::json;
        let a = adapter().await;
        // Filter: only items whose label is "item-1"
        let where_clause = WhereClause::Field {
            path:     vec!["label".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("item-1"),
        };
        let result = a
            .execute_relay_page(
                "v_relay_item",
                "id",
                None,
                None,
                10,
                true,
                Some(&where_clause),
                None,
                true,
            )
            .await
            .expect("filtered relay page");
        assert_eq!(result.total_count(), Some(1), "only item-1 matches");
        assert_eq!(result.rows().len(), 1);
        assert_eq!(extract_label(&result.rows()[0]), "item-1");
    }

    /// Querying a non-existent view returns a database error.
    #[tokio::test]
    async fn test_mysql_relay_missing_view_returns_error() {
        use fraiseql_core::error::FraiseQLError;
        let a = adapter().await;
        let err = a
            .execute_relay_page("v_nonexistent_view", "id", None, None, 3, true, None, None, false)
            .await
            .expect_err("missing view must return Err");
        assert!(
            matches!(err, FraiseQLError::Database { .. }),
            "Expected Database error, got {err:?}"
        );
    }
}

// ============================================================================
// MySQL Advanced Query Tests (window functions, CTEs, aggregations)
// ============================================================================

#[cfg(feature = "test-mysql")]
mod mysql_advanced_tests {
    use fraiseql_core::db::mysql::MySqlAdapter;
    use fraiseql_db::DatabaseAdapter;

    fn mysql_url() -> String {
        std::env::var("MYSQL_URL")
            .expect("MYSQL_URL must be set (e.g. via `dagger call test-integration --suite=mysql`)")
    }

    async fn adapter() -> MySqlAdapter {
        MySqlAdapter::new(&mysql_url()).await.expect("Failed to connect to MySQL")
    }

    /// MySQL 8+ `RANK()` window function partitioned by category.
    #[tokio::test]
    async fn test_mysql_window_function_rank() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "SELECT category, score, label,
                        RANK() OVER (PARTITION BY category ORDER BY score DESC) AS rnk
                 FROM v_score
                 ORDER BY category, rnk",
            )
            .await
            .expect("RANK() window function must succeed on MySQL 8+");
        // 8 rows in tb_score
        assert_eq!(results.len(), 8, "all 8 scored rows returned");
        let first = &results[0];
        assert!(first.contains_key("rnk"), "must include rank column");
        // Category A: alpha(95), beta(80), gamma(80) — alpha has rank 1
        let cat = first.get("category").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(cat, "A");
        let rnk = first.get("rnk").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(rnk, 1, "highest score in category A must have rank 1");
    }

    /// MySQL 8+ `ROW_NUMBER()` window function.
    #[tokio::test]
    async fn test_mysql_window_function_row_number() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "SELECT id, label,
                        ROW_NUMBER() OVER (ORDER BY score DESC) AS row_num
                 FROM v_score",
            )
            .await
            .expect("ROW_NUMBER() must succeed on MySQL 8+");
        assert_eq!(results.len(), 8);
        // Each row has a unique row_num
        let row_nums_count = results
            .iter()
            .filter(|r| r.get("row_num").and_then(|v| v.as_u64()).is_some())
            .count();
        assert_eq!(row_nums_count, 8, "all rows must have row_num");
    }

    /// CTE (WITH clause) is supported on MySQL 8+.
    #[tokio::test]
    async fn test_mysql_cte_basic() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "WITH top_scores AS (
                     SELECT id, label, score FROM v_score WHERE score >= 80
                 )
                 SELECT * FROM top_scores ORDER BY score DESC",
            )
            .await
            .expect("CTE must be supported on MySQL 8+");
        // Scores >= 80: alpha(95), beta(80), gamma(80), zeta(90) → 4 rows
        assert_eq!(results.len(), 4, "four rows have score >= 80");
    }

    /// Recursive CTE returns expected depth.
    #[tokio::test]
    async fn test_mysql_cte_recursive() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "WITH RECURSIVE counter(n) AS (
                     SELECT 1
                     UNION ALL
                     SELECT n + 1 FROM counter WHERE n < 5
                 )
                 SELECT n FROM counter",
            )
            .await
            .expect("recursive CTE must succeed");
        assert_eq!(results.len(), 5, "recursive CTE must return 5 rows");
    }

    /// COUNT, SUM, AVG, MIN, MAX aggregations.
    #[tokio::test]
    async fn test_mysql_aggregations() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "SELECT
                     COUNT(*) AS cnt,
                     SUM(score) AS total,
                     AVG(score) AS avg_score,
                     MIN(score) AS min_score,
                     MAX(score) AS max_score
                 FROM v_score",
            )
            .await
            .expect("aggregations must succeed");
        assert_eq!(results.len(), 1, "aggregation returns one row");
        let row = &results[0];
        let cnt = row.get("cnt").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(cnt, 8, "8 score rows");
        let max = row.get("max_score").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(max, 95, "max score is 95 (alpha)");
        let min = row.get("min_score").and_then(|v| v.as_u64()).unwrap_or(999);
        assert_eq!(min, 50, "min score is 50 (eta)");
    }

    /// GROUP BY aggregation per category.
    #[tokio::test]
    async fn test_mysql_group_by_aggregation() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "SELECT category, COUNT(*) AS cnt, MAX(score) AS max_score
                 FROM v_score
                 GROUP BY category
                 ORDER BY category",
            )
            .await
            .expect("GROUP BY must succeed");
        // 3 categories: A(3 rows), B(3 rows), C(2 rows)
        assert_eq!(results.len(), 3, "3 distinct categories");
        let first = &results[0];
        let cat = first.get("category").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(cat, "A");
        let cnt = first.get("cnt").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(cnt, 3, "category A has 3 rows");
    }
}

// ============================================================================
// MySQL Mutation Tests
// ============================================================================

#[cfg(feature = "test-mysql")]
mod mysql_mutation_tests {
    use fraiseql_core::db::mysql::MySqlAdapter;
    use fraiseql_db::DatabaseAdapter;

    fn mysql_url() -> String {
        std::env::var("MYSQL_URL")
            .expect("MYSQL_URL must be set (e.g. via `dagger call test-integration --suite=mysql`)")
    }

    /// MySQL mutation via stored procedure: insert returns the new row.
    #[tokio::test]
    async fn test_mysql_mutation_insert_via_procedure() {
        let a = MySqlAdapter::new(&mysql_url()).await.expect("connect");
        let result = a
            .execute_function_call("fn_create_tag", &[serde_json::json!("test-tag-plan03")])
            .await
            .expect("stored procedure call must succeed");
        // Procedure returns one row with id and name
        assert!(!result.is_empty(), "INSERT must return the new row");
        let row = &result[0];
        assert!(row.contains_key("id"), "returned row must have id");
        let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(name, "test-tag-plan03");
    }

    /// Calling a non-existent procedure returns a database error.
    #[tokio::test]
    async fn test_mysql_mutation_nonexistent_procedure_returns_error() {
        use fraiseql_core::error::FraiseQLError;
        let a = MySqlAdapter::new(&mysql_url()).await.expect("connect");
        let err = a
            .execute_function_call("fn_does_not_exist", &[])
            .await
            .expect_err("non-existent procedure must return Err");
        assert!(
            matches!(err, FraiseQLError::Database { .. }),
            "Expected Database error, got {err:?}"
        );
    }

    /// C1 regression (CRITICAL SQL injection): a stored-procedure argument that
    /// looks like an injection payload — backslash-quote breakout + statement
    /// terminator + comment — must be **bound as a literal string**, not parsed
    /// as SQL. The procedure echoes its argument back via a SELECT, so the value
    /// must round-trip byte-for-byte. Under the pre-fix inline text-protocol
    /// escaping (which doubled `'` only and left `\` alone), MySQL's default
    /// backslash mode let `\'` close the quote and the trailing `; …` execute as
    /// raw SQL, so the call errored or stored a mangled value; the parameterized
    /// CALL binds the exact bytes.
    #[tokio::test]
    async fn test_mysql_function_call_arg_is_not_sql_injectable() {
        let a = MySqlAdapter::new(&mysql_url()).await.expect("connect");
        let payload = r"\', SELECT 1; -- injected";
        let result = a
            .execute_function_call("fn_create_tag", &[serde_json::json!(payload)])
            .await
            .expect("parameterized CALL must succeed even with an injection-shaped argument");
        assert!(!result.is_empty(), "procedure must return the inserted row");
        let name = result[0].get("name").and_then(|v| v.as_str()).unwrap_or_default();
        assert_eq!(
            name, payload,
            "argument must round-trip as a literal string, proving it was bound, not executed"
        );
    }
}

// ============================================================================
// MySQL Error Path Tests
// ============================================================================

#[cfg(feature = "test-mysql")]
mod mysql_error_tests {
    use fraiseql_core::{db::mysql::MySqlAdapter, error::FraiseQLError};
    use fraiseql_db::DatabaseAdapter;

    /// A completely bad connection URL returns a database error.
    #[tokio::test]
    async fn test_mysql_connection_failure_returns_database_error() {
        // Port 1 is almost certainly closed; connection attempt must fail.
        let result =
            MySqlAdapter::new("mysql://bad_user:bad_pass@127.0.0.1:1/nonexistent_db").await;
        assert!(result.is_err(), "connection to bad URL must fail");
        if let Err(err) = result {
            assert!(
                matches!(
                    err,
                    FraiseQLError::Database { .. } | FraiseQLError::ConnectionPool { .. }
                ),
                "Expected Database or ConnectionPool error on bad connection, got {err:?}"
            );
        }
    }

    /// Querying a non-existent view returns a database error.
    #[tokio::test]
    async fn test_mysql_missing_view_returns_database_error() {
        let url = std::env::var("MYSQL_URL").expect(
            "MYSQL_URL must be set (e.g. via `dagger call test-integration --suite=mysql`)",
        );
        let a = MySqlAdapter::new(&url).await.expect("connect");
        let err = a
            .execute_where_query("v_view_that_does_not_exist", None, Some(1), None, None)
            .await
            .expect_err("non-existent view must return Err");
        assert!(
            matches!(err, FraiseQLError::Database { .. }),
            "Expected Database error for missing view, got {err:?}"
        );
    }
}

// ============================================================================
// MySQL Change-Spine Outbox Tests
// ============================================================================

/// Behavioural proof of the Change Spine transactional outbox on MySQL: the
/// MySQL adapter's `execute_function_call_with_changelog` runs the mutation
/// procedure and writes exactly one `tb_entity_change_log` row **in the same
/// transaction**, atomically, and only for an effective change. The portable
/// path (no PG `MATERIALIZED` CTE): CALL the proc, parse its `mutation_response`
/// row in Rust, then INSERT the outbox row before commit. `duration_ms` /
/// `started_at` are legitimately NULL (no request-scoped DB clock on MySQL).
///
/// Self-provisions the contract table + procedures at runtime (mirrors the PG
/// `changelog_outbox_test.rs::provision`); each test isolates on a unique
/// `object_type`. Run with `--test-threads=1` (the file's contract).
#[cfg(feature = "test-mysql")]
mod mysql_outbox_tests {
    use fraiseql_core::db::mysql::MySqlAdapter;
    use fraiseql_db::{ChangeLogWrite, DatabaseAdapter};
    use serde_json::json;
    use sqlx::{MySqlPool, Row};

    fn mysql_url() -> String {
        std::env::var("MYSQL_URL")
            .expect("MYSQL_URL must be set (e.g. via `dagger call test-integration --suite=mysql`)")
    }

    /// A raw sqlx pool (provisioning + assertions) plus the adapter under test.
    async fn connect() -> (MySqlPool, MySqlAdapter) {
        let url = mysql_url();
        let pool = MySqlPool::connect(&url).await.expect("raw sqlx pool");
        let adapter = MySqlAdapter::new(&url).await.expect("Failed to create MySQL adapter");
        (pool, adapter)
    }

    /// DROP+CREATE the MySQL change-log contract table (the `09_*` DDL shape,
    /// trimmed to the columns these tests assert). `id` carries `DEFAULT (UUID())`
    /// — the portable INSERT omits it, exactly as on PG/MSSQL.
    async fn provision(pool: &MySqlPool) {
        sqlx::raw_sql("DROP TABLE IF EXISTS tb_entity_change_log")
            .execute(pool)
            .await
            .expect("drop contract table");
        sqlx::raw_sql(
            "CREATE TABLE tb_entity_change_log (
                 pk_entity_change_log BIGINT AUTO_INCREMENT PRIMARY KEY,
                 object_type       VARCHAR(255) NOT NULL,
                 modification_type VARCHAR(50)  NOT NULL,
                 id                CHAR(36)     NOT NULL DEFAULT (UUID()),
                 created_at        TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
                 tenant_id         CHAR(36)     NULL,
                 object_id         CHAR(36)     NULL,
                 object_data       JSON         NULL,
                 updated_fields    JSON         NULL,
                 `cascade`         JSON         NULL,
                 duration_ms       INT          NULL,
                 started_at        TIMESTAMP(6) NULL,
                 trace_id          VARCHAR(64)  NULL,
                 schema_version    VARCHAR(64)  NULL,
                 trace_context     JSON         NULL,
                 actor_type        VARCHAR(50)  NULL,
                 acting_for        CHAR(36)     NULL,
                 commit_time       TIMESTAMP(6) NULL,
                 seq               BIGINT       NULL)",
        )
        .execute(pool)
        .await
        .expect("create contract table");
    }

    /// (Re)create a stored procedure returning a `mutation_response`-shaped row.
    async fn create_proc(pool: &MySqlPool, name: &str, create_sql: &str) {
        sqlx::raw_sql(&format!("DROP PROCEDURE IF EXISTS {name}"))
            .execute(pool)
            .await
            .expect("drop proc");
        // A lone CREATE PROCEDURE statement via COM_QUERY needs no DELIMITER.
        sqlx::raw_sql(create_sql).execute(pool).await.expect("create proc");
    }

    async fn count_rows(pool: &MySqlPool, object_type: &str) -> i64 {
        sqlx::query("SELECT COUNT(*) AS c FROM tb_entity_change_log WHERE object_type = ?")
            .bind(object_type)
            .fetch_one(pool)
            .await
            .expect("count")
            .get::<i64, _>("c")
    }

    #[tokio::test]
    async fn mysql_executor_writes_changelog_in_txn() {
        let (pool, adapter) = connect().await;
        provision(&pool).await;
        let obj_type = "MyOutboxUser";

        create_proc(
            &pool,
            "fn_my_outbox_create",
            "CREATE PROCEDURE fn_my_outbox_create(IN p_id CHAR(36))
             BEGIN
               SELECT TRUE AS succeeded, TRUE AS state_changed,
                      p_id AS entity_id, 'MyOutboxUser' AS entity_type,
                      JSON_OBJECT('id', p_id, 'name', 'Ada') AS entity,
                      JSON_ARRAY('name') AS updated_fields,
                      NULL AS `cascade`;
             END",
        )
        .await;

        let id = uuid::Uuid::new_v4().to_string();
        let changelog = ChangeLogWrite::new(obj_type, "INSERT");
        let rows = adapter
            .execute_function_call_with_changelog(
                "fn_my_outbox_create",
                &[json!(id)],
                &[],
                Some(&changelog),
            )
            .await
            .expect("mutation + outbox write");

        // The procedure's row is still returned to the caller, unchanged.
        assert_eq!(rows.len(), 1, "procedure row returned to the caller");

        // Exactly one outbox row, with the mutation's identity + payload.
        let row = sqlx::query(
            "SELECT object_type, modification_type, object_id, object_data, updated_fields, \
             duration_ms FROM tb_entity_change_log WHERE object_id = ?",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("exactly one outbox row");
        assert_eq!(row.get::<String, _>("object_type"), obj_type);
        assert_eq!(row.get::<String, _>("modification_type"), "INSERT");
        assert_eq!(row.get::<String, _>("object_id"), id);
        let data: serde_json::Value = row.get("object_data");
        assert_eq!(data["name"], json!("Ada"), "object_data is the entity payload");
        let updated: serde_json::Value = row.get("updated_fields");
        assert_eq!(updated, json!(["name"]), "updated_fields carried through");
        // duration_ms is legitimately NULL on the portable path (no DB-clock GUC).
        assert!(
            row.get::<Option<i32>, _>("duration_ms").is_none(),
            "duration_ms NULL on MySQL (no request-scoped clock)"
        );
    }

    #[tokio::test]
    async fn mysql_changelog_row_atomic_with_mutation() {
        let (pool, adapter) = connect().await;
        provision(&pool).await;
        let obj_type = "MyOutboxAtomic";

        // The procedure raises — the whole txn (incl. any outbox INSERT) rolls back.
        create_proc(
            &pool,
            "fn_my_outbox_boom",
            "CREATE PROCEDURE fn_my_outbox_boom()
             BEGIN
               SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'boom';
             END",
        )
        .await;

        let changelog = ChangeLogWrite::new(obj_type, "INSERT");
        let result = adapter
            .execute_function_call_with_changelog("fn_my_outbox_boom", &[], &[], Some(&changelog))
            .await;

        assert!(result.is_err(), "raising procedure surfaces an error");
        assert_eq!(count_rows(&pool, obj_type).await, 0, "no outbox row after rollback");
    }

    #[tokio::test]
    async fn mysql_noop_and_failed_mutations_write_no_changelog_row() {
        let (pool, adapter) = connect().await;
        provision(&pool).await;

        // succeeded=true but state_changed=false (a no-op) → no spine event.
        create_proc(
            &pool,
            "fn_my_outbox_noop",
            "CREATE PROCEDURE fn_my_outbox_noop()
             BEGIN
               SELECT TRUE AS succeeded, FALSE AS state_changed,
                      NULL AS entity_id, 'MyOutboxNoop' AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS `cascade`;
             END",
        )
        .await;
        // succeeded=false (a business-logic failure that still commits) → no event.
        create_proc(
            &pool,
            "fn_my_outbox_fail",
            "CREATE PROCEDURE fn_my_outbox_fail()
             BEGIN
               SELECT FALSE AS succeeded, FALSE AS state_changed,
                      NULL AS entity_id, 'MyOutboxFail' AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS `cascade`;
             END",
        )
        .await;

        adapter
            .execute_function_call_with_changelog(
                "fn_my_outbox_noop",
                &[],
                &[],
                Some(&ChangeLogWrite::new("MyOutboxNoop", "UPDATE")),
            )
            .await
            .unwrap();
        adapter
            .execute_function_call_with_changelog(
                "fn_my_outbox_fail",
                &[],
                &[],
                Some(&ChangeLogWrite::new("MyOutboxFail", "INSERT")),
            )
            .await
            .unwrap();

        assert_eq!(count_rows(&pool, "MyOutboxNoop").await, 0, "no-op writes no spine event");
        assert_eq!(count_rows(&pool, "MyOutboxFail").await, 0, "failure writes no spine event");
    }

    #[tokio::test]
    async fn mysql_object_type_falls_back_to_return_type_when_entity_type_is_null() {
        let (pool, adapter) = connect().await;
        provision(&pool).await;
        let obj_type = "MyOutboxFallback";

        // A state-changing mutation that returns NO entity_type — the NOT-NULL
        // object_type must fall back to the threaded value (the GraphQL return type).
        create_proc(
            &pool,
            "fn_my_outbox_noetype",
            "CREATE PROCEDURE fn_my_outbox_noetype(IN p_id CHAR(36))
             BEGIN
               SELECT TRUE AS succeeded, TRUE AS state_changed,
                      p_id AS entity_id, NULL AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS `cascade`;
             END",
        )
        .await;

        let id = uuid::Uuid::new_v4().to_string();
        adapter
            .execute_function_call_with_changelog(
                "fn_my_outbox_noetype",
                &[json!(id)],
                &[],
                Some(&ChangeLogWrite::new(obj_type, "DELETE")),
            )
            .await
            .unwrap();

        let object_type: String =
            sqlx::query("SELECT object_type FROM tb_entity_change_log WHERE object_id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get("object_type");
        assert_eq!(object_type, obj_type, "object_type falls back to the return type");
    }

    #[tokio::test]
    async fn mysql_outbox_atomic_with_procedure_dml() {
        async fn probe_count(pool: &MySqlPool, id: &str) -> i64 {
            sqlx::query("SELECT COUNT(*) AS c FROM tb_my_probe WHERE id = ?")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap()
                .get::<i64, _>("c")
        }

        let (pool, adapter) = connect().await;
        provision(&pool).await;
        // A probe table the procedure writes to: proves the procedure's OWN DML and
        // the outbox row commit (or roll back) together, and that a procedure doing
        // DML-then-SELECT does not desync the connection before the outbox INSERT.
        sqlx::raw_sql("DROP TABLE IF EXISTS tb_my_probe").execute(&pool).await.unwrap();
        sqlx::raw_sql("CREATE TABLE tb_my_probe (id CHAR(36) PRIMARY KEY, note VARCHAR(50))")
            .execute(&pool)
            .await
            .unwrap();

        // Commit path: the procedure INSERTs a probe row, then returns an effective
        // change → BOTH the probe row and the outbox row persist.
        create_proc(
            &pool,
            "fn_my_dml_ok",
            "CREATE PROCEDURE fn_my_dml_ok(IN p_id CHAR(36))
             BEGIN
               INSERT INTO tb_my_probe (id, note) VALUES (p_id, 'ok');
               SELECT TRUE AS succeeded, TRUE AS state_changed, p_id AS entity_id,
                      'MyDml' AS entity_type, NULL AS entity, NULL AS updated_fields,
                      NULL AS `cascade`;
             END",
        )
        .await;
        let ok_id = uuid::Uuid::new_v4().to_string();
        adapter
            .execute_function_call_with_changelog(
                "fn_my_dml_ok",
                &[json!(ok_id)],
                &[],
                Some(&ChangeLogWrite::new("MyDml", "INSERT")),
            )
            .await
            .unwrap();
        assert_eq!(probe_count(&pool, &ok_id).await, 1, "procedure DML committed");
        assert_eq!(
            count_rows(&pool, "MyDml").await,
            1,
            "outbox row committed atomically with the procedure DML"
        );

        // Rollback path: the procedure INSERTs a probe row, then RAISES → neither the
        // probe row nor the outbox row survives.
        create_proc(
            &pool,
            "fn_my_dml_boom",
            "CREATE PROCEDURE fn_my_dml_boom(IN p_id CHAR(36))
             BEGIN
               INSERT INTO tb_my_probe (id, note) VALUES (p_id, 'boom');
               SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'boom';
             END",
        )
        .await;
        let boom_id = uuid::Uuid::new_v4().to_string();
        let res = adapter
            .execute_function_call_with_changelog(
                "fn_my_dml_boom",
                &[json!(boom_id)],
                &[],
                Some(&ChangeLogWrite::new("MyDmlBoom", "INSERT")),
            )
            .await;
        assert!(res.is_err(), "raising procedure surfaces an error");
        assert_eq!(probe_count(&pool, &boom_id).await, 0, "procedure DML rolled back");
        assert_eq!(count_rows(&pool, "MyDmlBoom").await, 0, "no outbox row after rollback");
    }

    #[tokio::test]
    async fn mysql_tenant_id_stamped_and_null_paths() {
        let (pool, adapter) = connect().await;
        provision(&pool).await;
        let obj_type = "MyOutboxTenant";

        create_proc(
            &pool,
            "fn_my_outbox_tenant",
            "CREATE PROCEDURE fn_my_outbox_tenant(IN p_id CHAR(36))
             BEGIN
               SELECT TRUE AS succeeded, TRUE AS state_changed,
                      p_id AS entity_id, 'MyOutboxTenant' AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS `cascade`;
             END",
        )
        .await;

        // Stamped explicitly from the envelope (NOT reconstructed from any session).
        let tenant = uuid::Uuid::new_v4().to_string();
        let stamped_id = uuid::Uuid::new_v4().to_string();
        adapter
            .execute_function_call_with_changelog(
                "fn_my_outbox_tenant",
                &[json!(stamped_id)],
                &[],
                Some(
                    &ChangeLogWrite::new(obj_type, "INSERT")
                        .with_tenant_id(Some(tenant.parse().unwrap())),
                ),
            )
            .await
            .unwrap();
        let got: String =
            sqlx::query("SELECT tenant_id FROM tb_entity_change_log WHERE object_id = ?")
                .bind(&stamped_id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get("tenant_id");
        assert_eq!(got, tenant, "tenant_id stamped verbatim from the envelope");

        // No tenant on the envelope → the column is NULL (never a lossy cast).
        let null_id = uuid::Uuid::new_v4().to_string();
        adapter
            .execute_function_call_with_changelog(
                "fn_my_outbox_tenant",
                &[json!(null_id)],
                &[],
                Some(&ChangeLogWrite::new(obj_type, "INSERT")),
            )
            .await
            .unwrap();
        let got: Option<String> =
            sqlx::query("SELECT tenant_id FROM tb_entity_change_log WHERE object_id = ?")
                .bind(&null_id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get("tenant_id");
        assert_eq!(got, None, "tenant_id is NULL when the envelope carries none");
    }
}

// ============================================================================
// SQL Server Advanced Query Tests (window functions, CTEs, aggregations)
// ============================================================================

#[cfg(feature = "test-sqlserver")]
mod sqlserver_advanced_tests {
    use fraiseql_core::db::sqlserver::SqlServerAdapter;
    use fraiseql_db::DatabaseAdapter;

    async fn adapter() -> SqlServerAdapter {
        let url = super::sqlserver_conn("fraiseql_test").await.expect(
            "SQLSERVER_URL must be set (e.g. via `dagger call test-integration --suite=sqlserver`)",
        );
        SqlServerAdapter::new(&url).await.expect("Failed to connect to SQL Server")
    }

    /// SQL Server `RANK()` window function partitioned by category.
    #[tokio::test]
    async fn test_sqlserver_window_function_rank() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "SELECT category, score, label,
                        RANK() OVER (PARTITION BY category ORDER BY score DESC) AS rnk
                 FROM v_score
                 ORDER BY category, rnk",
            )
            .await
            .expect("RANK() must succeed on SQL Server 2012+");
        assert_eq!(results.len(), 8, "all 8 scored rows returned");
        let first = &results[0];
        assert!(first.contains_key("rnk"), "must include rank column");
    }

    /// SQL Server `ROW_NUMBER()` window function.
    #[tokio::test]
    async fn test_sqlserver_window_function_row_number() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "SELECT id, label,
                        ROW_NUMBER() OVER (ORDER BY score DESC) AS row_num
                 FROM v_score",
            )
            .await
            .expect("ROW_NUMBER() must succeed");
        assert_eq!(results.len(), 8);
    }

    /// CTE (WITH clause) is fully supported on SQL Server.
    #[tokio::test]
    async fn test_sqlserver_cte_basic() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "WITH top_scores AS (
                     SELECT id, label, score FROM v_score WHERE score >= 80
                 )
                 SELECT * FROM top_scores ORDER BY score DESC",
            )
            .await
            .expect("CTE must succeed on SQL Server");
        assert_eq!(results.len(), 4, "four rows have score >= 80");
    }

    /// Recursive CTE on SQL Server.
    #[tokio::test]
    async fn test_sqlserver_cte_recursive() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "WITH counter(n) AS (
                     SELECT 1
                     UNION ALL
                     SELECT n + 1 FROM counter WHERE n < 5
                 )
                 SELECT n FROM counter",
            )
            .await
            .expect("recursive CTE must succeed on SQL Server");
        assert_eq!(results.len(), 5, "recursive CTE must return 5 rows");
    }

    /// COUNT, SUM, AVG, MIN, MAX aggregations on SQL Server.
    #[tokio::test]
    async fn test_sqlserver_aggregations() {
        let a = adapter().await;
        let results = a
            .execute_raw_query(
                "SELECT
                     COUNT(*) AS cnt,
                     SUM(score) AS total,
                     AVG(CAST(score AS FLOAT)) AS avg_score,
                     MIN(score) AS min_score,
                     MAX(score) AS max_score
                 FROM v_score",
            )
            .await
            .expect("aggregations must succeed");
        assert_eq!(results.len(), 1);
        let row = &results[0];
        let cnt = row.get("cnt").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(cnt, 8);
        let max = row.get("max_score").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(max, 95, "max score is 95 (alpha)");
    }
}

// ============================================================================
// SQL Server Mutation Tests
// ============================================================================

#[cfg(feature = "test-sqlserver")]
mod sqlserver_mutation_tests {
    use fraiseql_core::db::sqlserver::SqlServerAdapter;
    use fraiseql_db::DatabaseAdapter;

    async fn adapter() -> SqlServerAdapter {
        let url = super::sqlserver_conn("fraiseql_test").await.expect(
            "SQLSERVER_URL must be set (e.g. via `dagger call test-integration --suite=sqlserver`)",
        );
        SqlServerAdapter::new(&url).await.expect("connect")
    }

    /// SQL Server mutation via stored procedure using OUTPUT INSERTED.*.
    #[tokio::test]
    async fn test_sqlserver_mutation_insert_via_procedure() {
        let a = adapter().await;
        let result = a
            .execute_function_call("fn_create_tag", &[serde_json::json!("test-tag-sqlserver")])
            .await
            .expect("stored procedure call must succeed");
        assert!(!result.is_empty(), "INSERT must return the new row");
        let row = &result[0];
        assert!(row.contains_key("id"), "returned row must have id");
        let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(name, "test-tag-sqlserver");
    }

    /// Calling a non-existent procedure returns a database error.
    #[tokio::test]
    async fn test_sqlserver_mutation_nonexistent_procedure_returns_error() {
        use fraiseql_core::error::FraiseQLError;
        let a = adapter().await;
        let err = a
            .execute_function_call("fn_does_not_exist", &[])
            .await
            .expect_err("non-existent procedure must return Err");
        assert!(
            matches!(err, FraiseQLError::Database { .. }),
            "Expected Database error, got {err:?}"
        );
    }
}

// ============================================================================
// SQL Server Change-Spine Outbox Tests
// ============================================================================

/// Behavioural proof of the Change Spine transactional outbox on SQL Server: the
/// tiberius adapter's `execute_function_call_with_changelog` runs the mutation
/// procedure and writes exactly one `core.tb_entity_change_log` row **in the same
/// transaction** (`SET XACT_ABORT ON; BEGIN TRAN … COMMIT`), atomically, and only
/// for an effective change. `duration_ms`/`started_at` are legitimately NULL (no
/// request-scoped DB clock on SQL Server).
///
/// Self-provisions the contract table + procedures at runtime; each test isolates
/// on a unique `object_type`. Run with `--test-threads=1`.
#[cfg(feature = "test-sqlserver")]
mod sqlserver_outbox_tests {
    use fraiseql_core::db::sqlserver::SqlServerAdapter;
    use fraiseql_db::{ChangeLogWrite, DatabaseAdapter};
    use serde_json::json;

    async fn adapter() -> SqlServerAdapter {
        let url = super::sqlserver_conn("fraiseql_test").await.expect(
            "SQLSERVER_URL must be set (e.g. via `dagger call test-integration --suite=sqlserver`)",
        );
        SqlServerAdapter::new(&url).await.expect("Failed to connect to SQL Server")
    }

    /// DROP+CREATE the SQL Server change-log contract table (the `10_*` DDL shape,
    /// trimmed to the columns these tests assert). `id`/`seq` carry defaults; the
    /// portable INSERT omits them. `[cascade]` is bracket-quoted (reserved word).
    async fn provision(a: &SqlServerAdapter) {
        a.execute_raw_query("IF SCHEMA_ID('core') IS NULL EXEC('CREATE SCHEMA core')")
            .await
            .expect("create core schema");
        a.execute_raw_query(
            "IF OBJECT_ID('core.tb_entity_change_log','U') IS NOT NULL \
                 DROP TABLE core.tb_entity_change_log",
        )
        .await
        .expect("drop contract table");
        a.execute_raw_query(
            "IF OBJECT_ID('core.seq_entity_change_log') IS NOT NULL \
                 DROP SEQUENCE core.seq_entity_change_log",
        )
        .await
        .expect("drop sequence");
        a.execute_raw_query(
            "CREATE SEQUENCE core.seq_entity_change_log AS BIGINT START WITH 1 INCREMENT BY 1",
        )
        .await
        .expect("create sequence");
        a.execute_raw_query(
            "CREATE TABLE core.tb_entity_change_log (
                 pk_entity_change_log BIGINT IDENTITY(1,1) PRIMARY KEY,
                 object_type       NVARCHAR(255) NOT NULL,
                 modification_type NVARCHAR(50)  NOT NULL,
                 id                UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID(),
                 created_at        DATETIME2     NOT NULL DEFAULT SYSUTCDATETIME(),
                 tenant_id         UNIQUEIDENTIFIER NULL,
                 object_id         UNIQUEIDENTIFIER NULL,
                 object_data       NVARCHAR(MAX) NULL,
                 updated_fields    NVARCHAR(MAX) NULL,
                 [cascade]         NVARCHAR(MAX) NULL,
                 duration_ms       INT           NULL,
                 started_at        DATETIME2     NULL,
                 trace_id          NVARCHAR(64)  NULL,
                 schema_version    NVARCHAR(64)  NULL,
                 trace_context     NVARCHAR(MAX) NULL,
                 actor_type        NVARCHAR(50)  NULL,
                 acting_for        UNIQUEIDENTIFIER NULL,
                 commit_time       DATETIME2     NULL,
                 seq               BIGINT NOT NULL DEFAULT (NEXT VALUE FOR core.seq_entity_change_log))",
        )
        .await
        .expect("create contract table");
    }

    async fn create_proc(a: &SqlServerAdapter, body: &str) {
        a.execute_raw_query(body).await.expect("create proc");
    }

    async fn count_rows(a: &SqlServerAdapter, object_type: &str) -> i64 {
        let rows = a
            .execute_raw_query(&format!(
                "SELECT COUNT(*) AS c FROM core.tb_entity_change_log WHERE object_type = '{object_type}'"
            ))
            .await
            .expect("count");
        rows[0].get("c").and_then(serde_json::Value::as_i64).expect("count value")
    }

    #[tokio::test]
    async fn sqlserver_executor_writes_changelog_in_txn() {
        let a = adapter().await;
        provision(&a).await;
        let obj_type = "SsOutboxUser";
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_outbox_create @p_id NVARCHAR(36) AS
             BEGIN
               SET NOCOUNT ON;
               SELECT CAST(1 AS BIT) AS succeeded, CAST(1 AS BIT) AS state_changed,
                      @p_id AS entity_id, 'SsOutboxUser' AS entity_type,
                      '{\"name\":\"Ada\"}' AS entity, '[\"name\"]' AS updated_fields,
                      NULL AS [cascade];
             END",
        )
        .await;

        let id = uuid::Uuid::new_v4().to_string();
        let rows = a
            .execute_function_call_with_changelog(
                "fn_ss_outbox_create",
                &[json!(id)],
                &[],
                Some(&ChangeLogWrite::new(obj_type, "INSERT")),
            )
            .await
            .expect("mutation + outbox write");
        assert_eq!(rows.len(), 1, "procedure row returned to the caller");

        let got = a
            .execute_raw_query(&format!(
                "SELECT object_type, modification_type, CONVERT(NVARCHAR(36), object_id) AS object_id, \
                 object_data, updated_fields, duration_ms \
                 FROM core.tb_entity_change_log WHERE object_id = '{id}'"
            ))
            .await
            .expect("read outbox row");
        assert_eq!(got.len(), 1, "exactly one outbox row");
        let row = &got[0];
        assert_eq!(row.get("object_type"), Some(&json!(obj_type)));
        assert_eq!(row.get("modification_type"), Some(&json!("INSERT")));
        // SQL Server canonicalises UNIQUEIDENTIFIER to uppercase → compare case-insensitively.
        assert_eq!(
            row.get("object_id").and_then(serde_json::Value::as_str).map(str::to_lowercase),
            Some(id.clone()),
            "object_id round-trips through the UNIQUEIDENTIFIER column"
        );
        assert_eq!(row["object_data"]["name"], json!("Ada"), "object_data is the entity payload");
        assert_eq!(row.get("updated_fields"), Some(&json!(["name"])), "updated_fields carried");
        // duration_ms is legitimately NULL on the portable path (no DB-clock GUC).
        assert!(
            row.get("duration_ms").is_none_or(serde_json::Value::is_null),
            "duration_ms NULL on SQL Server (no request-scoped clock)"
        );
    }

    #[tokio::test]
    async fn sqlserver_changelog_row_atomic_with_mutation() {
        let a = adapter().await;
        provision(&a).await;
        let obj_type = "SsOutboxAtomic";
        // The procedure raises — XACT_ABORT rolls back the whole transaction.
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_outbox_boom AS
             BEGIN
               SET NOCOUNT ON;
               THROW 50000, 'boom', 1;
             END",
        )
        .await;
        let result = a
            .execute_function_call_with_changelog(
                "fn_ss_outbox_boom",
                &[],
                &[],
                Some(&ChangeLogWrite::new(obj_type, "INSERT")),
            )
            .await;
        assert!(result.is_err(), "raising procedure surfaces an error");
        assert_eq!(count_rows(&a, obj_type).await, 0, "no outbox row after rollback");
    }

    #[tokio::test]
    async fn sqlserver_noop_and_failed_mutations_write_no_changelog_row() {
        let a = adapter().await;
        provision(&a).await;
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_outbox_noop AS
             BEGIN
               SET NOCOUNT ON;
               SELECT CAST(1 AS BIT) AS succeeded, CAST(0 AS BIT) AS state_changed,
                      NULL AS entity_id, 'SsOutboxNoop' AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS [cascade];
             END",
        )
        .await;
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_outbox_fail AS
             BEGIN
               SET NOCOUNT ON;
               SELECT CAST(0 AS BIT) AS succeeded, CAST(0 AS BIT) AS state_changed,
                      NULL AS entity_id, 'SsOutboxFail' AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS [cascade];
             END",
        )
        .await;
        a.execute_function_call_with_changelog(
            "fn_ss_outbox_noop",
            &[],
            &[],
            Some(&ChangeLogWrite::new("SsOutboxNoop", "UPDATE")),
        )
        .await
        .unwrap();
        a.execute_function_call_with_changelog(
            "fn_ss_outbox_fail",
            &[],
            &[],
            Some(&ChangeLogWrite::new("SsOutboxFail", "INSERT")),
        )
        .await
        .unwrap();
        assert_eq!(count_rows(&a, "SsOutboxNoop").await, 0, "no-op writes no spine event");
        assert_eq!(count_rows(&a, "SsOutboxFail").await, 0, "failure writes no spine event");
    }

    #[tokio::test]
    async fn sqlserver_object_type_falls_back_to_return_type_when_entity_type_is_null() {
        let a = adapter().await;
        provision(&a).await;
        let obj_type = "SsOutboxFallback";
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_outbox_noetype @p_id NVARCHAR(36) AS
             BEGIN
               SET NOCOUNT ON;
               SELECT CAST(1 AS BIT) AS succeeded, CAST(1 AS BIT) AS state_changed,
                      @p_id AS entity_id, NULL AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS [cascade];
             END",
        )
        .await;
        let id = uuid::Uuid::new_v4().to_string();
        a.execute_function_call_with_changelog(
            "fn_ss_outbox_noetype",
            &[json!(id)],
            &[],
            Some(&ChangeLogWrite::new(obj_type, "DELETE")),
        )
        .await
        .unwrap();
        let got = a
            .execute_raw_query(&format!(
                "SELECT object_type FROM core.tb_entity_change_log WHERE object_id = '{id}'"
            ))
            .await
            .unwrap();
        assert_eq!(got[0].get("object_type"), Some(&json!(obj_type)), "object_type falls back");
    }

    #[tokio::test]
    async fn sqlserver_outbox_atomic_with_procedure_dml() {
        async fn probe_count(a: &SqlServerAdapter, id: &str) -> i64 {
            let rows = a
                .execute_raw_query(&format!(
                    "SELECT COUNT(*) AS c FROM dbo.tb_ss_probe WHERE id = '{id}'"
                ))
                .await
                .unwrap();
            rows[0].get("c").and_then(serde_json::Value::as_i64).unwrap()
        }

        let a = adapter().await;
        provision(&a).await;
        // A probe table the procedure writes to: proves the procedure's OWN DML and
        // the outbox row commit (or roll back) together.
        a.execute_raw_query(
            "IF OBJECT_ID('dbo.tb_ss_probe','U') IS NOT NULL DROP TABLE dbo.tb_ss_probe",
        )
        .await
        .unwrap();
        a.execute_raw_query(
            "CREATE TABLE dbo.tb_ss_probe (id NVARCHAR(36) PRIMARY KEY, note NVARCHAR(50))",
        )
        .await
        .unwrap();

        // Commit path: procedure INSERTs a probe row + returns an effective change.
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_dml_ok @p_id NVARCHAR(36) AS
             BEGIN
               SET NOCOUNT ON;
               INSERT INTO dbo.tb_ss_probe (id, note) VALUES (@p_id, 'ok');
               SELECT CAST(1 AS BIT) AS succeeded, CAST(1 AS BIT) AS state_changed,
                      @p_id AS entity_id, 'SsDml' AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS [cascade];
             END",
        )
        .await;
        let ok_id = uuid::Uuid::new_v4().to_string();
        a.execute_function_call_with_changelog(
            "fn_ss_dml_ok",
            &[json!(ok_id)],
            &[],
            Some(&ChangeLogWrite::new("SsDml", "INSERT")),
        )
        .await
        .unwrap();
        assert_eq!(probe_count(&a, &ok_id).await, 1, "procedure DML committed");
        assert_eq!(count_rows(&a, "SsDml").await, 1, "outbox row committed atomically");

        // Rollback path: procedure INSERTs then THROWs → neither survives.
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_dml_boom @p_id NVARCHAR(36) AS
             BEGIN
               SET NOCOUNT ON;
               INSERT INTO dbo.tb_ss_probe (id, note) VALUES (@p_id, 'boom');
               THROW 50000, 'boom', 1;
             END",
        )
        .await;
        let boom_id = uuid::Uuid::new_v4().to_string();
        let res = a
            .execute_function_call_with_changelog(
                "fn_ss_dml_boom",
                &[json!(boom_id)],
                &[],
                Some(&ChangeLogWrite::new("SsDmlBoom", "INSERT")),
            )
            .await;
        assert!(res.is_err(), "raising procedure surfaces an error");
        assert_eq!(probe_count(&a, &boom_id).await, 0, "procedure DML rolled back");
        assert_eq!(count_rows(&a, "SsDmlBoom").await, 0, "no outbox row after rollback");
    }

    #[tokio::test]
    async fn sqlserver_tenant_id_stamped_and_null_paths() {
        let a = adapter().await;
        provision(&a).await;
        let obj_type = "SsOutboxTenant";
        create_proc(
            &a,
            "CREATE OR ALTER PROCEDURE dbo.fn_ss_outbox_tenant @p_id NVARCHAR(36) AS
             BEGIN
               SET NOCOUNT ON;
               SELECT CAST(1 AS BIT) AS succeeded, CAST(1 AS BIT) AS state_changed,
                      @p_id AS entity_id, 'SsOutboxTenant' AS entity_type,
                      NULL AS entity, NULL AS updated_fields, NULL AS [cascade];
             END",
        )
        .await;

        // Stamped explicitly from the envelope.
        let tenant = uuid::Uuid::new_v4().to_string();
        let stamped_id = uuid::Uuid::new_v4().to_string();
        a.execute_function_call_with_changelog(
            "fn_ss_outbox_tenant",
            &[json!(stamped_id)],
            &[],
            Some(
                &ChangeLogWrite::new(obj_type, "INSERT")
                    .with_tenant_id(Some(tenant.parse().unwrap())),
            ),
        )
        .await
        .unwrap();
        let got = a
            .execute_raw_query(&format!(
                "SELECT CONVERT(NVARCHAR(36), tenant_id) AS tenant_id \
                 FROM core.tb_entity_change_log WHERE object_id = '{stamped_id}'"
            ))
            .await
            .unwrap();
        // MSSQL upper-cases UNIQUEIDENTIFIER → compare case-insensitively.
        assert_eq!(
            got[0]
                .get("tenant_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_lowercase),
            Some(tenant.clone()),
            "tenant_id stamped verbatim from the envelope"
        );

        // No tenant on the envelope → the column is NULL.
        let null_id = uuid::Uuid::new_v4().to_string();
        a.execute_function_call_with_changelog(
            "fn_ss_outbox_tenant",
            &[json!(null_id)],
            &[],
            Some(&ChangeLogWrite::new(obj_type, "INSERT")),
        )
        .await
        .unwrap();
        let got = a
            .execute_raw_query(&format!(
                "SELECT CONVERT(NVARCHAR(36), tenant_id) AS tenant_id \
                 FROM core.tb_entity_change_log WHERE object_id = '{null_id}'"
            ))
            .await
            .unwrap();
        assert!(
            got[0].get("tenant_id").is_none_or(serde_json::Value::is_null),
            "tenant_id is NULL when the envelope carries none"
        );
    }
}

// ============================================================================
// DialectCapabilityGuard Error Path Tests
// ============================================================================

#[cfg(any(feature = "mysql", feature = "sqlserver"))]
mod dialect_guard_error_tests {
    use fraiseql_db::{DialectCapabilityGuard, Feature, types::DatabaseType};
    use fraiseql_error::FraiseQLError;

    /// JSONB path ops are unsupported on MySQL — guard returns Unsupported.
    #[cfg(feature = "mysql")]
    #[test]
    fn test_mysql_jsonb_returns_unsupported() {
        let result = DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps);
        assert!(
            matches!(result, Err(FraiseQLError::Unsupported { .. })),
            "JSONB ops on MySQL must return Unsupported, got {result:?}"
        );
    }

    /// Subscriptions are unsupported on MySQL — guard returns Unsupported.
    #[cfg(feature = "mysql")]
    #[test]
    fn test_mysql_subscriptions_returns_unsupported() {
        let result = DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::Subscriptions);
        assert!(
            matches!(result, Err(FraiseQLError::Unsupported { .. })),
            "Subscriptions on MySQL must return Unsupported"
        );
    }

    /// JSONB path ops are unsupported on SQL Server — guard returns Unsupported.
    #[cfg(feature = "sqlserver")]
    #[test]
    fn test_sqlserver_jsonb_returns_unsupported() {
        let result = DialectCapabilityGuard::check(DatabaseType::SQLServer, Feature::JsonbPathOps);
        assert!(
            matches!(result, Err(FraiseQLError::Unsupported { .. })),
            "JSONB ops on SQL Server must return Unsupported"
        );
    }

    /// Mutations are supported on both MySQL and SQL Server — guard returns Ok.
    #[cfg(feature = "mysql")]
    #[test]
    fn test_mysql_mutations_are_supported() {
        assert!(
            DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::Mutations).is_ok(),
            "Mutations must be supported on MySQL"
        );
    }

    /// Window functions are supported on both MySQL 8+ and SQL Server 2012+.
    #[cfg(feature = "mysql")]
    #[test]
    fn test_mysql_window_functions_are_supported() {
        assert!(
            DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::WindowFunctions).is_ok(),
            "Window functions must be supported on MySQL 8+"
        );
    }
}

// ============================================================================
// Cross-Database Tests (Database-Agnostic)
// ============================================================================

/// Trait for database-agnostic test execution
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(dead_code)] // Reason: called by subset of multi-database tests; Clippy false-positive (multi-binary)
async fn run_basic_health_check<A: DatabaseAdapter>(adapter: &A) -> bool {
    adapter.health_check().await.is_ok()
}

#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(dead_code)] // Reason: called by subset of multi-database tests; Clippy false-positive (multi-binary)
async fn verify_pool_metrics<A: DatabaseAdapter>(adapter: &A) -> bool {
    let metrics = adapter.pool_metrics();
    metrics.total_connections > 0 && metrics.idle_connections <= metrics.total_connections
}

// Helper to run queries and verify JSON structure
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(dead_code)] // Reason: called by subset of multi-database tests; Clippy false-positive (multi-binary)
async fn verify_view_returns_json<A: DatabaseAdapter>(
    adapter: &A,
    view_name: &str,
    expected_fields: &[&str],
) -> bool {
    let results = adapter.execute_where_query(view_name, None, Some(1), None, None).await;

    if let Ok(rows) = results {
        if rows.is_empty() {
            return false;
        }

        let value = rows[0].as_value();
        expected_fields.iter().all(|field| value.get(*field).is_some())
    } else {
        false
    }
}
