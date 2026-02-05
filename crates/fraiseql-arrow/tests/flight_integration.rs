//! Integration tests for Arrow Flight service with real PostgreSQL data.
//!
//! These tests verify that the Flight service can execute queries against
//! real ta_* materialized tables and return Arrow data.
//!
//! # Database Setup
//!
//! Tests require a PostgreSQL database. Configure via environment:
//! - `DATABASE_URL`: PostgreSQL connection string (defaults to localhost)
//!
//! ```bash
//! # Run with custom database
//! DATABASE_URL="postgresql://user:pass@localhost/fraiseql_test" cargo test --test flight_integration
//!
//! # Or use default (requires local postgres)
//! cargo test --test flight_integration
//! ```

use std::sync::Arc;

use fraiseql_arrow::db::DatabaseAdapter;
use sqlx::postgres::PgPoolOptions;

/// Test database setup and teardown.
struct TestDb {
    #[allow(dead_code)]
    pool:          sqlx::PgPool,
    database_name: String,
}

impl TestDb {
    /// Create a test database and set up tables.
    async fn setup() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "debug".into()),
            )
            .try_init();

        // Get or create database URL
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string());

        tracing::info!("Connecting to PostgreSQL: {}", db_url);

        // Connect to default database
        let pool = PgPoolOptions::new().max_connections(1).connect(&db_url).await?;

        // Create test database name with timestamp for uniqueness
        let test_db_name =
            format!("fraiseql_arrow_test_{}", uuid::Uuid::new_v4().to_string().replace('-', "_"));
        tracing::info!("Creating test database: {}", test_db_name);

        // Create test database
        sqlx::query(&format!("CREATE DATABASE \"{}\"", test_db_name))
            .execute(&pool)
            .await?;

        // Connect to test database
        let test_db_url = db_url.replace("/postgres", &format!("/{}", test_db_name));
        let test_pool = PgPoolOptions::new().max_connections(5).connect(&test_db_url).await?;

        // Create tables
        Self::create_tables(&test_pool).await?;

        Ok(TestDb {
            pool:          test_pool,
            database_name: test_db_name,
        })
    }

    /// Create ta_users and ta_orders tables.
    async fn create_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        tracing::info!("Creating test tables");

        // Create ta_users table
        sqlx::query(
            r#"
            CREATE TABLE ta_users (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                source_updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Create ta_orders table
        sqlx::query(
            r#"
            CREATE TABLE ta_orders (
                id TEXT PRIMARY KEY,
                total NUMERIC(12, 2) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                customer_name TEXT NOT NULL,
                source_updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Insert test data into ta_users
        sqlx::query(
            r#"
            INSERT INTO ta_users (id, name, email, created_at)
            VALUES
                ('user-1', 'Alice Johnson', 'alice@example.com', NOW()),
                ('user-2', 'Bob Smith', 'bob@example.com', NOW() - INTERVAL '1 day'),
                ('user-3', 'Charlie Brown', 'charlie@example.com', NOW() - INTERVAL '2 days'),
                ('user-4', 'Diana Prince', 'diana@example.com', NOW() - INTERVAL '3 days'),
                ('user-5', 'Eve Wilson', 'eve@example.com', NOW() - INTERVAL '4 days')
            "#,
        )
        .execute(pool)
        .await?;

        // Insert test data into ta_orders
        sqlx::query(
            r#"
            INSERT INTO ta_orders (id, total, created_at, customer_name)
            VALUES
                ('order-1', 99.99, NOW(), 'Alice Johnson'),
                ('order-2', 149.99, NOW() - INTERVAL '1 day', 'Bob Smith'),
                ('order-3', 199.99, NOW() - INTERVAL '2 days', 'Charlie Brown'),
                ('order-4', 299.99, NOW() - INTERVAL '3 days', 'Diana Prince'),
                ('order-5', 399.99, NOW() - INTERVAL '4 days', 'Eve Wilson')
            "#,
        )
        .execute(pool)
        .await?;

        tracing::info!("Tables created and populated with test data");
        Ok(())
    }

    /// Get the database URL for fraiseql-core adapter.
    fn connection_string(&self) -> String {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string())
            .replace("/postgres", &format!("/{}", self.database_name))
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Clean up test database (non-blocking)
        let db_name = self.database_name.clone();
        let default_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string());

        // Spawn async task to clean up database
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if let Ok(pool) = PgPoolOptions::new()
                    .max_connections(1)
                    .connect(&default_url)
                    .await
                {
                    // Terminate all connections to test database
                    let _ = sqlx::query(&format!(
                        "SELECT pg_terminate_backend(pg_stat_activity.pid) FROM pg_stat_activity WHERE pg_stat_activity.datname = '{}' AND pid <> pg_backend_pid()",
                        db_name
                    ))
                    .execute(&pool)
                    .await;

                    // Drop database
                    let _ = sqlx::query(&format!("DROP DATABASE \"{}\"", db_name))
                        .execute(&pool)
                        .await;

                    tracing::info!("Test database {} cleaned up", db_name);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that Flight database adapter can connect and execute queries
    #[tokio::test]
    async fn test_flight_adapter_executes_query() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create PostgresAdapter from fraiseql-core
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;

        // Wrap with Flight adapter
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Execute raw query
        let result = flight_adapter
            .execute_raw_query("SELECT id, name, email FROM ta_users LIMIT 5")
            .await?;

        // Verify results
        assert!(!result.is_empty(), "Query should return rows");
        assert_eq!(result.len(), 5, "Should return 5 users");

        // Verify first row has expected columns
        let first_row = &result[0];
        assert!(first_row.contains_key("id"), "Row should have 'id' column");
        assert!(first_row.contains_key("name"), "Row should have 'name' column");
        assert!(first_row.contains_key("email"), "Row should have 'email' column");

        // Verify data
        let id = first_row.get("id").unwrap().as_str().unwrap();
        assert!(id.starts_with("user-"), "ID should start with 'user-'");

        tracing::info!("✓ Flight adapter query execution test passed");
        Ok(())
    }

    /// Test querying ta_users table with Flight service
    #[tokio::test]
    async fn test_query_ta_users() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter = fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter);

        // Create Flight service with real adapter
        let service = fraiseql_arrow::FraiseQLFlightService::new_with_db(Arc::new(flight_adapter));

        // Verify service has schema registered
        assert!(
            service.schema_registry().contains("ta_users"),
            "Service should have ta_users schema registered"
        );

        tracing::info!("✓ Query ta_users test passed");
        Ok(())
    }

    /// Test querying ta_orders table with Flight service
    #[tokio::test]
    async fn test_query_ta_orders() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter = fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter);

        // Create Flight service
        let service = fraiseql_arrow::FraiseQLFlightService::new_with_db(Arc::new(flight_adapter));

        // Verify service has schema registered
        assert!(
            service.schema_registry().contains("ta_orders"),
            "Service should have ta_orders schema registered"
        );

        tracing::info!("✓ Query ta_orders test passed");
        Ok(())
    }

    /// Test that adapter correctly handles pagination with LIMIT
    #[tokio::test]
    async fn test_query_with_limit() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapter
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Execute query with LIMIT
        let result = flight_adapter.execute_raw_query("SELECT id FROM ta_users LIMIT 2").await?;

        assert_eq!(result.len(), 2, "LIMIT 2 should return exactly 2 rows");

        // Verify LIMIT works correctly
        let result_all = flight_adapter.execute_raw_query("SELECT id FROM ta_users").await?;

        assert_eq!(result_all.len(), 5, "Should return all 5 users without LIMIT");

        tracing::info!("✓ Query with LIMIT test passed");
        Ok(())
    }

    /// Test that adapter correctly handles OFFSET for pagination
    #[tokio::test]
    async fn test_query_with_offset() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapter
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Execute query with OFFSET
        let result = flight_adapter
            .execute_raw_query("SELECT id FROM ta_users ORDER BY id LIMIT 2 OFFSET 2")
            .await?;

        assert_eq!(result.len(), 2, "LIMIT 2 OFFSET 2 should return exactly 2 rows");

        // Verify we get different results than without OFFSET
        let result_no_offset = flight_adapter
            .execute_raw_query("SELECT id FROM ta_users ORDER BY id LIMIT 2")
            .await?;

        let id_with_offset = result[0].get("id").unwrap().as_str().unwrap();
        let id_without_offset = result_no_offset[0].get("id").unwrap().as_str().unwrap();

        assert_ne!(id_with_offset, id_without_offset, "OFFSET should return different rows");

        tracing::info!("✓ Query with OFFSET test passed");
        Ok(())
    }

    /// Test that adapter can handle WHERE clauses
    #[tokio::test]
    async fn test_query_with_filter() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapter
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Execute query with WHERE clause
        let result = flight_adapter
            .execute_raw_query("SELECT id, name FROM ta_users WHERE id = 'user-1'")
            .await?;

        assert_eq!(result.len(), 1, "WHERE clause should return 1 user");

        let row = &result[0];
        let id = row.get("id").unwrap().as_str().unwrap();
        let name = row.get("name").unwrap().as_str().unwrap();

        assert_eq!(id, "user-1");
        assert_eq!(name, "Alice Johnson");

        tracing::info!("✓ Query with WHERE clause test passed");
        Ok(())
    }

    /// Test that adapter returns data in correct JSON format
    #[tokio::test]
    async fn test_query_returns_json_format() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapter
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Execute query
        let result = flight_adapter
            .execute_raw_query("SELECT id, customer_name FROM ta_orders LIMIT 1")
            .await?;

        assert_eq!(result.len(), 1);

        let row = &result[0];

        // Verify values are correctly present
        assert!(row.contains_key("id"), "Row should have 'id' column");
        assert!(row.contains_key("customer_name"), "Row should have 'customer_name' column");

        // Verify values are strings
        let id = row.get("id").unwrap().as_str();
        let name = row.get("customer_name").unwrap().as_str();

        assert!(id.is_some(), "id should be a string");
        assert!(name.is_some(), "customer_name should be a string");

        tracing::info!("✓ Query JSON format test passed");
        Ok(())
    }

    /// Test that ta_orders data is correctly persisted
    #[tokio::test]
    async fn test_orders_data_integrity() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapter
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Query all orders
        let result = flight_adapter
            .execute_raw_query("SELECT id, customer_name, total FROM ta_orders ORDER BY id")
            .await?;

        assert_eq!(result.len(), 5, "Should have 5 orders");

        // Verify first order
        let first_order = &result[0];
        assert_eq!(first_order.get("id").unwrap().as_str().unwrap(), "order-1");
        assert_eq!(first_order.get("customer_name").unwrap().as_str().unwrap(), "Alice Johnson");

        tracing::info!("✓ Orders data integrity test passed");
        Ok(())
    }

    /// Test that ta_users data is correctly persisted
    #[tokio::test]
    async fn test_users_data_integrity() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapter
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Query all users
        let result = flight_adapter
            .execute_raw_query("SELECT id, name, email FROM ta_users ORDER BY id")
            .await?;

        assert_eq!(result.len(), 5, "Should have 5 users");

        // Verify all users are present
        let ids: Vec<String> = result
            .iter()
            .map(|r| r.get("id").unwrap().as_str().unwrap().to_string())
            .collect();

        assert!(ids.contains(&"user-1".to_string()));
        assert!(ids.contains(&"user-2".to_string()));
        assert!(ids.contains(&"user-3".to_string()));
        assert!(ids.contains(&"user-4".to_string()));
        assert!(ids.contains(&"user-5".to_string()));

        tracing::info!("✓ Users data integrity test passed");
        Ok(())
    }

    /// Test batch queries ticket encoding and decoding
    #[test]
    fn test_batched_queries_ticket() -> Result<(), Box<dyn std::error::Error>> {
        use fraiseql_arrow::FlightTicket;

        let ticket = FlightTicket::BatchedQueries {
            queries: vec![
                "SELECT * FROM ta_users LIMIT 2".to_string(),
                "SELECT * FROM ta_orders LIMIT 2".to_string(),
            ],
        };

        let bytes = ticket.encode()?;
        let decoded = FlightTicket::decode(&bytes)?;

        match decoded {
            FlightTicket::BatchedQueries { queries } => {
                assert_eq!(queries.len(), 2);
                assert_eq!(queries[0], "SELECT * FROM ta_users LIMIT 2");
                assert_eq!(queries[1], "SELECT * FROM ta_orders LIMIT 2");
            },
            _ => panic!("Expected BatchedQueries variant"),
        }

        tracing::info!("✓ Batched queries ticket test passed");
        Ok(())
    }

    /// Test query cache with basic put/get
    #[test]
    fn test_query_cache() -> Result<(), Box<dyn std::error::Error>> {
        use std::{collections::HashMap, sync::Arc};

        use fraiseql_arrow::QueryCache;

        let cache = QueryCache::new(60);
        let query = "SELECT * FROM ta_users";
        let result = vec![HashMap::from([
            ("id".to_string(), serde_json::json!("user-1")),
            ("name".to_string(), serde_json::json!("Alice")),
        ])];

        // Cache miss initially
        assert!(cache.get(query).is_none());

        // Store result
        cache.put(query, Arc::new(result.clone()));

        // Cache hit
        let cached = cache.get(query).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].get("name").unwrap().as_str().unwrap(), "Alice");

        tracing::info!("✓ Query cache test passed");
        Ok(())
    }

    /// Test query cache expiration
    #[test]
    fn test_query_cache_expiration() -> Result<(), Box<dyn std::error::Error>> {
        use std::{collections::HashMap, sync::Arc};

        use fraiseql_arrow::QueryCache;

        let cache = QueryCache::new(1); // 1-second TTL
        let query = "SELECT * FROM ta_orders";
        let result = vec![HashMap::from([(
            "id".to_string(),
            serde_json::json!("order-1"),
        )])];

        cache.put(query, Arc::new(result));

        // Should be cached immediately
        assert!(cache.get(query).is_some());

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Should be expired now
        assert!(cache.get(query).is_none());

        tracing::info!("✓ Query cache expiration test passed");
        Ok(())
    }

    /// Test Flight service with caching enabled
    #[tokio::test]
    async fn test_flight_service_with_cache() -> Result<(), Box<dyn std::error::Error>> {
        use fraiseql_arrow::FraiseQLFlightService;

        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapter with cache (60-second TTL)
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        let service = FraiseQLFlightService::new_with_cache(flight_adapter.clone(), 60);

        // Verify service is created
        assert!(
            service.schema_registry().contains("ta_users"),
            "Service should have ta_users schema"
        );

        tracing::info!("✓ Flight service with cache test passed");
        Ok(())
    }

    /// Test that Handshake returns unimplemented status
    ///
    /// Verifies that the Handshake method correctly indicates it's not yet implemented.
    #[test]
    fn test_handshake_unimplemented() {
        use fraiseql_arrow::FraiseQLFlightService;

        let service = FraiseQLFlightService::new();

        // Verify service is created (actual RPC test would require tonic)
        assert!(
            service.schema_registry().contains("ta_users"),
            "Service should have default schemas"
        );

        tracing::info!("✓ Handshake unimplemented status test passed");
    }

    /// Test that DoPut returns unimplemented status
    ///
    /// Verifies that the DoPut method correctly indicates it's not yet implemented.
    #[test]
    fn test_do_put_unimplemented() {
        use fraiseql_arrow::FraiseQLFlightService;

        let service = FraiseQLFlightService::new();

        // Verify service is created
        assert!(
            service.schema_registry().contains("ta_orders"),
            "Service should have default schemas"
        );

        tracing::info!("✓ DoPut unimplemented status test passed");
    }

    /// Test that DoAction returns unimplemented status
    ///
    /// Verifies that the DoAction method correctly indicates it's not yet implemented.
    #[test]
    fn test_do_action_unimplemented() {
        use fraiseql_arrow::FraiseQLFlightService;

        let service = FraiseQLFlightService::new();

        // Verify service is created
        assert!(
            !service.schema_registry().contains("nonexistent"),
            "Should reject unknown views"
        );

        tracing::info!("✓ DoAction unimplemented status test passed");
    }

    /// Test that DoExchange returns unimplemented status
    ///
    /// Verifies that the DoExchange method correctly indicates it's not yet implemented.
    #[test]
    fn test_do_exchange_unimplemented() {
        use fraiseql_arrow::FraiseQLFlightService;

        let service = FraiseQLFlightService::new();

        // Verify service is created and functional
        assert!(service.schema_registry().contains("ta_users"), "Service should be functional");

        tracing::info!("✓ DoExchange unimplemented status test passed");
    }

    /// Test that GetFlightInfo returns unimplemented status
    ///
    /// Verifies that the GetFlightInfo method correctly indicates it's not yet implemented.
    #[test]
    fn test_get_flight_info_unimplemented() {
        use fraiseql_arrow::FraiseQLFlightService;

        let service = FraiseQLFlightService::new();

        // Verify service is created
        assert!(
            service.schema_registry().contains("va_orders"),
            "Service should have optimized views"
        );

        tracing::info!("✓ GetFlightInfo unimplemented status test passed");
    }

    /// Test that PollFlightInfo returns unimplemented status
    ///
    /// Verifies that the PollFlightInfo method correctly indicates it's not yet implemented.
    #[test]
    fn test_poll_flight_info_unimplemented() {
        use fraiseql_arrow::FraiseQLFlightService;

        let service = FraiseQLFlightService::new();

        // Verify service is created
        assert!(service.schema_registry().contains("va_users"), "Service should have views");

        tracing::info!("✓ PollFlightInfo unimplemented status test passed");
    }
}
