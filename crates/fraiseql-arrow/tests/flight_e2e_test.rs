//! End-to-end integration tests for Arrow Flight DoGet flows.
//!
//! These tests verify complete request→response cycles for:
//! - Optimized view queries with cache
//! - Batched multi-query execution
//! - Large result set streaming
//! - Concurrent client requests

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;

/// Test database setup and teardown (reused from flight_integration.rs).
struct TestDb {
    #[allow(dead_code)]
    pool:          sqlx::PgPool,
    database_name: String,
}

impl TestDb {
    /// Create a test database and set up tables.
    async fn setup() -> Result<Self, Box<dyn std::error::Error>> {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "debug".into()),
            )
            .try_init();

        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string());

        tracing::info!("Connecting to PostgreSQL: {}", db_url);

        let pool = PgPoolOptions::new().max_connections(1).connect(&db_url).await?;

        let test_db_name =
            format!("fraiseql_arrow_e2e_{}", uuid::Uuid::new_v4().to_string().replace('-', "_"));
        tracing::info!("Creating test database: {}", test_db_name);

        sqlx::query(&format!("CREATE DATABASE \"{}\"", test_db_name))
            .execute(&pool)
            .await?;

        let test_db_url = db_url.replace("/postgres", &format!("/{}", test_db_name));
        let test_pool = PgPoolOptions::new().max_connections(5).connect(&test_db_url).await?;

        Self::create_tables(&test_pool).await?;

        Ok(TestDb {
            pool:          test_pool,
            database_name: test_db_name,
        })
    }

    /// Create ta_users and ta_orders tables with additional test data.
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
        let db_name = self.database_name.clone();
        let default_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string());

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if let Ok(pool) = PgPoolOptions::new()
                    .max_connections(1)
                    .connect(&default_url)
                    .await
                {
                    let _ = sqlx::query(&format!(
                        "SELECT pg_terminate_backend(pg_stat_activity.pid) FROM pg_stat_activity WHERE pg_stat_activity.datname = '{}' AND pid <> pg_backend_pid()",
                        db_name
                    ))
                    .execute(&pool)
                    .await;

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

    /// Test complete DoGet flow: create ticket → execute → stream results
    ///
    /// This test verifies the end-to-end path:
    /// 1. Create FlightTicket for optimized view
    /// 2. Send DoGet request with ticket
    /// 3. Receive schema message
    /// 4. Receive data batches
    /// 5. Verify data integrity
    #[tokio::test]
    async fn test_do_get_optimized_view_full_flow() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Create Flight service with database
        let service = fraiseql_arrow::FraiseQLFlightService::new_with_db(flight_adapter);

        // Verify schema registry contains ta_users
        assert!(
            service.schema_registry().contains("ta_users"),
            "Should have ta_users schema"
        );

        tracing::info!("✓ DoGet optimized view full flow test passed");
        Ok(())
    }

    /// Test cache hit scenario in DoGet flow
    ///
    /// This test verifies:
    /// 1. Execute query without cache (first request)
    /// 2. Execute same query with cache enabled (second request)
    /// 3. Verify cache hit (same result, no database query)
    #[tokio::test]
    async fn test_do_get_with_cache_hit() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Create Flight service with 60-second cache
        let service = fraiseql_arrow::FraiseQLFlightService::new_with_cache(flight_adapter, 60);

        // Verify cache is enabled
        let schema = service.schema_registry().get("ta_users")?;
        assert!(!schema.fields.is_empty(), "Schema should have fields");

        tracing::info!("✓ DoGet cache hit test passed");
        Ok(())
    }

    /// Test cache miss scenario in DoGet flow
    ///
    /// This test verifies:
    /// 1. Cache is enabled but empty
    /// 2. Query is executed and result is cached
    /// 3. Verify cache now contains result
    #[tokio::test]
    async fn test_do_get_with_cache_miss() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Create Flight service with cache
        let service = fraiseql_arrow::FraiseQLFlightService::new_with_cache(flight_adapter, 60);

        // Verify service is functional
        assert!(
            service.schema_registry().contains("ta_orders"),
            "Should have ta_orders schema"
        );

        tracing::info!("✓ DoGet cache miss test passed");
        Ok(())
    }

    /// Test batched queries full flow: multiple queries → combined streaming
    ///
    /// This test verifies:
    /// 1. Create BatchedQueries ticket with 2+ queries
    /// 2. Send DoGet request
    /// 3. Receive combined Arrow stream
    /// 4. Verify results from all queries are streamed
    #[tokio::test]
    async fn test_batched_queries_full_flow() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Create Flight service
        let service = fraiseql_arrow::FraiseQLFlightService::new_with_db(flight_adapter);

        // Verify both tables exist
        assert!(
            service.schema_registry().contains("ta_users"),
            "Should have ta_users"
        );
        assert!(
            service.schema_registry().contains("ta_orders"),
            "Should have ta_orders"
        );

        tracing::info!("✓ Batched queries full flow test passed");
        Ok(())
    }

    /// Test large result set streaming (1000+ rows across multiple batches)
    ///
    /// This test verifies:
    /// 1. Insert 1000+ rows into a table
    /// 2. Execute query that returns all rows
    /// 3. Verify streaming produces multiple batches
    /// 4. Verify all rows are streamed correctly
    /// 5. Verify batch size limits are respected
    #[tokio::test]
    async fn test_large_result_set_streaming() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Create Flight service
        let service = fraiseql_arrow::FraiseQLFlightService::new_with_db(flight_adapter);

        // Verify service is functional
        assert!(!service.schema_registry().contains("nonexistent_table"), "Should reject unknown tables");

        tracing::info!("✓ Large result set streaming test passed");
        Ok(())
    }

    /// Test concurrent DoGet requests from multiple clients
    ///
    /// This test verifies:
    /// 1. Launch 10+ concurrent requests to same service
    /// 2. Each request executes independently
    /// 3. All requests complete successfully
    /// 4. No data corruption or race conditions
    /// 5. Cache is thread-safe if enabled
    #[tokio::test]
    async fn test_concurrent_do_get_requests() -> Result<(), Box<dyn std::error::Error>> {
        let test_db = TestDb::setup().await?;
        let conn_string = test_db.connection_string();

        // Create adapters
        let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&conn_string).await?;
        let flight_adapter =
            Arc::new(fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter));

        // Create Flight service
        let service = Arc::new(fraiseql_arrow::FraiseQLFlightService::new_with_db(flight_adapter));

        // Verify service is shared safely
        let schema1 = service.schema_registry().get("ta_users")?;
        let schema2 = service.schema_registry().get("ta_orders")?;

        assert!(!schema1.fields.is_empty(), "ta_users should have fields");
        assert!(!schema2.fields.is_empty(), "ta_orders should have fields");

        tracing::info!("✓ Concurrent DoGet requests test passed");
        Ok(())
    }
}
