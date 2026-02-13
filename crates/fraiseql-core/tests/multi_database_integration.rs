//! Multi-database integration tests for FraiseQL adapters.
//!
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
use fraiseql_core::db::mysql::MySqlAdapter;
#[cfg(feature = "sqlite")]
use fraiseql_core::db::sqlite::SqliteAdapter;
#[cfg(feature = "sqlserver")]
use fraiseql_core::db::sqlserver::SqlServerAdapter;
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
use fraiseql_core::db::traits::DatabaseAdapter;
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
use fraiseql_core::db::types::DatabaseType;
// Note: WhereClause and WhereOperator available for future WHERE tests
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(unused_imports)]
use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};

// ============================================================================
// MySQL Integration Tests
// ============================================================================

#[cfg(feature = "test-mysql")]
mod mysql_tests {
    use super::*;

    const MYSQL_URL: &str =
        "mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql";

    #[tokio::test]
    async fn test_mysql_adapter_creation() {
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        assert_eq!(adapter.database_type(), DatabaseType::MySQL);

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0, "Pool should have connections");
    }

    #[tokio::test]
    async fn test_mysql_health_check() {
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        adapter.health_check().await.expect("Health check should pass");
    }

    #[tokio::test]
    async fn test_mysql_execute_raw_query() {
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_raw_query("SELECT 1 as value")
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("value"));
    }

    #[tokio::test]
    async fn test_mysql_query_v_user_view() {
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(10), None)
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
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None)
            .await
            .expect("Query should succeed");

        assert!(results.len() <= 2, "Should respect LIMIT clause");
    }

    #[tokio::test]
    async fn test_mysql_query_with_offset() {
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        // Get all users first
        let all_results = adapter
            .execute_where_query("v_user", None, Some(10), None)
            .await
            .expect("Query should succeed");

        // Get users with offset
        let offset_results = adapter
            .execute_where_query("v_user", None, Some(10), Some(1))
            .await
            .expect("Query should succeed");

        if all_results.len() > 1 {
            assert_eq!(offset_results.len(), all_results.len() - 1, "Offset should skip first row");
        }
    }

    #[tokio::test]
    async fn test_mysql_query_v_post_with_nested_author() {
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_post", None, Some(5), None)
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
        let adapter = MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter");

        let metrics = adapter.pool_metrics();

        assert!(metrics.total_connections > 0, "Should have total connections");
        assert!(
            metrics.idle_connections <= metrics.total_connections,
            "Idle should not exceed total"
        );
    }

    #[tokio::test]
    async fn test_mysql_concurrent_queries() {
        let adapter =
            Arc::new(MySqlAdapter::new(MYSQL_URL).await.expect("Failed to create MySQL adapter"));

        let mut handles = Vec::new();

        for _ in 0..10 {
            let adapter_clone = Arc::clone(&adapter);
            let handle = tokio::spawn(async move {
                adapter_clone.execute_where_query("v_user", None, Some(5), None).await
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
                r#"CREATE VIEW v_user AS
                   SELECT id, json_object('id', id, 'name', name, 'email', email) AS data
                   FROM users"#,
            )
            .await
            .expect("Create view should succeed");

        // Query the view
        let results = adapter
            .execute_where_query("v_user", None, Some(10), None)
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
            .execute_where_query("v_items", None, Some(2), None)
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
            .execute_where_query("v_items", None, Some(10), Some(2))
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
                r#"CREATE VIEW v_post AS
                   SELECT p.id,
                          json_object(
                              'id', p.id,
                              'title', p.title,
                              'author', json_object('id', a.id, 'name', a.name)
                          ) AS data
                   FROM posts p
                   JOIN authors a ON p.author_id = a.id"#,
            )
            .await
            .expect("Create view should succeed");

        // Query the view
        let results = adapter
            .execute_where_query("v_post", None, Some(10), None)
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
                adapter_clone.execute_where_query("v_test", None, Some(5), None).await
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

#[cfg(feature = "test-sqlserver")]
mod sqlserver_tests {
    use super::*;

    const SQLSERVER_URL: &str = "server=localhost,1434;database=master;user=sa;password=FraiseQL_Test1234;TrustServerCertificate=true";
    const SQLSERVER_TEST_DB_URL: &str = "server=localhost,1434;database=test_fraiseql;user=sa;password=FraiseQL_Test1234;TrustServerCertificate=true";

    #[tokio::test]
    async fn test_sqlserver_adapter_creation() {
        let adapter = SqlServerAdapter::new(SQLSERVER_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        assert_eq!(adapter.database_type(), DatabaseType::SQLServer);

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0, "Pool should have connections");
    }

    #[tokio::test]
    async fn test_sqlserver_health_check() {
        let adapter = SqlServerAdapter::new(SQLSERVER_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        adapter.health_check().await.expect("Health check should pass");
    }

    #[tokio::test]
    async fn test_sqlserver_execute_raw_query() {
        let adapter = SqlServerAdapter::new(SQLSERVER_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        let results = adapter
            .execute_raw_query("SELECT 1 as value")
            .await
            .expect("Query should succeed");

        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("value"));
    }

    #[tokio::test]
    async fn test_sqlserver_query_v_user_view() {
        let adapter = match SqlServerAdapter::new(SQLSERVER_TEST_DB_URL).await {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!(
                    "Skipping test_sqlserver_query_v_user_view: test_fraiseql database not available: {e}"
                );
                return;
            },
        };

        let results = adapter
            .execute_where_query("v_user", None, Some(10), None)
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
        let adapter = SqlServerAdapter::new(SQLSERVER_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        let metrics = adapter.pool_metrics();

        assert!(metrics.total_connections > 0, "Should have total connections");
        assert!(
            metrics.idle_connections <= metrics.total_connections,
            "Idle should not exceed total"
        );
    }

    #[tokio::test]
    async fn test_sqlserver_concurrent_queries() {
        let adapter = Arc::new(
            SqlServerAdapter::new(SQLSERVER_URL)
                .await
                .expect("Failed to create SQL Server adapter"),
        );

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
// Cross-Database Tests (Database-Agnostic)
// ============================================================================

/// Trait for database-agnostic test execution
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(dead_code)]
async fn run_basic_health_check<A: DatabaseAdapter>(adapter: &A) -> bool {
    adapter.health_check().await.is_ok()
}

#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(dead_code)]
async fn verify_pool_metrics<A: DatabaseAdapter>(adapter: &A) -> bool {
    let metrics = adapter.pool_metrics();
    metrics.total_connections > 0 && metrics.idle_connections <= metrics.total_connections
}

// Helper to run queries and verify JSON structure
#[cfg(any(feature = "mysql", feature = "sqlite", feature = "sqlserver"))]
#[allow(dead_code)]
async fn verify_view_returns_json<A: DatabaseAdapter>(
    adapter: &A,
    view_name: &str,
    expected_fields: &[&str],
) -> bool {
    let results = adapter.execute_where_query(view_name, None, Some(1), None).await;

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
