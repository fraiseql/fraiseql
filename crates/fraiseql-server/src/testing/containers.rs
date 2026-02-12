//! Testcontainers integration for production-identical testing
//!
//! Provides helpers for connecting to real PostgreSQL databases
//! for integration tests that need production-identical behavior.
//! Can be used with local Docker containers or managed PostgreSQL services.

use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;

/// Creates a PostgreSQL pool for integration testing
///
/// # Examples
///
/// ```rust,no_run
/// #[ignore]  // Run with: cargo test -- --ignored
/// #[tokio::test]
/// async fn test_with_postgres() {
///     let pool = create_test_postgres_pool().await.unwrap();
///     // Use the pool for testing
/// }
/// ```
pub async fn create_test_postgres_pool() -> Result<Pool<Postgres>, sqlx::Error> {
    // Try to get connection string from environment, or use local Docker
    let connection_string = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| {
            "postgresql://postgres:postgres@localhost:5432/fraiseql_test".to_string()
        });

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
}

/// Test schema for PostgreSQL events table
pub const TEST_POSTGRES_EVENTS_SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    timestamp BIGINT NOT NULL,
    data JSONB NOT NULL,
    user_id TEXT,
    tenant_id TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_events_entity_type ON events(entity_type);
CREATE INDEX IF NOT EXISTS idx_events_entity_id ON events(entity_id);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_tenant_id ON events(tenant_id);
CREATE INDEX IF NOT EXISTS idx_events_data ON events USING GIN(data);
";

/// Test schema for PostgreSQL configuration table
pub const TEST_POSTGRES_CONFIG_SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS configurations (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    config_data JSONB NOT NULL,
    version INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_config_name ON configurations(name);
";

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_postgres_connection() {
        // This test requires a running PostgreSQL instance
        // Skip in CI/CD unless explicitly enabled
        match create_test_postgres_pool().await {
            Ok(pool) => {
                let result = sqlx::query("SELECT 1 as num")
                    .fetch_one(&pool)
                    .await;

                assert!(result.is_ok());
                let row = result.unwrap();
                let num: i32 = row.get("num");
                assert_eq!(num, 1);
            }
            Err(_) => {
                // Skip if no database available
                println!("PostgreSQL not available, skipping test");
            }
        }
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_postgres_jsonb_query() {
        match create_test_postgres_pool().await {
            Ok(pool) => {
                // Test JSONB support
                let result = sqlx::query("SELECT '{\"key\": \"value\"}'::jsonb as data")
                    .fetch_one(&pool)
                    .await;

                assert!(result.is_ok());
            }
            Err(_) => {
                println!("PostgreSQL not available, skipping test");
            }
        }
    }
}
