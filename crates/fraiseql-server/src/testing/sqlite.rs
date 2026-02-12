//! In-memory SQLite testing utilities
//!
//! Provides helpers for creating and managing in-memory SQLite databases
//! for integration testing. Uses real SQL execution, not mocks.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite, Row};
use std::str::FromStr;

/// Creates an in-memory SQLite pool for testing
///
/// # Examples
///
/// ```rust,no_run
/// let pool = create_test_sqlite_pool().await.unwrap();
/// let row = sqlx::query("SELECT 1 as num")
///     .fetch_one(&pool)
///     .await
///     .unwrap();
/// let num: i32 = row.get("num");
/// assert_eq!(num, 1);
/// ```
pub async fn create_test_sqlite_pool() -> Result<Pool<Sqlite>, sqlx::Error> {
    let connect_options = SqliteConnectOptions::from_str("sqlite::memory:")?
        .create_if_missing(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
}

/// Creates an in-memory SQLite pool with schema initialized
pub async fn create_test_sqlite_pool_with_schema(
    schema_sql: &str,
) -> Result<Pool<Sqlite>, sqlx::Error> {
    let pool = create_test_sqlite_pool().await?;

    // Execute schema creation
    sqlx::raw_sql(schema_sql)
        .execute(&pool)
        .await?;

    Ok(pool)
}

/// Test schema for entity events table
pub const TEST_EVENTS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    data TEXT NOT NULL,
    user_id TEXT,
    tenant_id TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_entity_type ON events(entity_type);
CREATE INDEX idx_entity_id ON events(entity_id);
CREATE INDEX idx_timestamp ON events(timestamp);
CREATE INDEX idx_tenant_id ON events(tenant_id);
"#;

/// Test schema for configuration table
pub const TEST_CONFIG_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS configurations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    config_data TEXT NOT NULL,
    version INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_sqlite_pool() {
        let pool = create_test_sqlite_pool()
            .await
            .expect("Failed to create SQLite pool");

        let result = sqlx::query("SELECT 1 as num")
            .fetch_one(&pool)
            .await;

        assert!(result.is_ok());
        let row = result.unwrap();
        let num: i32 = row.get("num");
        assert_eq!(num, 1);
    }

    #[tokio::test]
    async fn test_create_test_sqlite_pool_with_schema() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create SQLite pool with schema");

        // Verify the table exists
        let result = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='events'")
            .fetch_one(&pool)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_and_query_events() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert a test event
        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("event-1")
        .bind("UserCreated")
        .bind("User")
        .bind("user-123")
        .bind(1234567890i64)
        .bind(r#"{"email": "test@example.com"}"#)
        .execute(&pool)
        .await
        .expect("Failed to insert event");

        // Query it back
        let row = sqlx::query("SELECT id, event_type, data FROM events WHERE id = ?")
            .bind("event-1")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch event");

        let id: String = row.get("id");
        let event_type: String = row.get("event_type");
        let data: String = row.get("data");

        assert_eq!(id, "event-1");
        assert_eq!(event_type, "UserCreated");
        assert!(data.contains("test@example.com"));
    }

    #[tokio::test]
    async fn test_query_with_null_fields() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert an event with null user_id
        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("event-2")
        .bind("EntityUpdated")
        .bind("Product")
        .bind("prod-456")
        .bind(1234567900i64)
        .bind(r#"{"name": "Widget"}"#)
        .execute(&pool)
        .await
        .expect("Failed to insert event");

        // Query and verify null handling
        let row = sqlx::query("SELECT user_id, tenant_id FROM events WHERE id = ?")
            .bind("event-2")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch event");

        let user_id: Option<String> = row.get("user_id");
        let tenant_id: Option<String> = row.get("tenant_id");

        assert!(user_id.is_none());
        assert!(tenant_id.is_none());
    }

    #[tokio::test]
    async fn test_batch_insert() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert multiple events
        for i in 1..=5 {
            sqlx::query(
                "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(format!("event-{}", i))
            .bind(format!("Event{}", i))
            .bind("TestEntity")
            .bind(format!("entity-{}", i))
            .bind(1234567890i64 + i as i64)
            .bind(format!(r#"{{"index": {}}}"#, i))
            .execute(&pool)
            .await
            .expect("Failed to insert event");
        }

        // Query all events
        let rows = sqlx::query("SELECT COUNT(*) as count FROM events")
            .fetch_one(&pool)
            .await
            .expect("Failed to count events");

        let count: i32 = rows.get("count");
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_configuration_table() {
        let pool = create_test_sqlite_pool_with_schema(TEST_CONFIG_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert configuration
        sqlx::query(
            "INSERT INTO configurations (name, config_data, version) VALUES (?, ?, ?)"
        )
        .bind("default")
        .bind(r#"{"database": "test"}"#)
        .bind(1)
        .execute(&pool)
        .await
        .expect("Failed to insert config");

        // Query it back
        let row = sqlx::query("SELECT config_data, version FROM configurations WHERE name = ?")
            .bind("default")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch config");

        let config_data: String = row.get("config_data");
        let version: i32 = row.get("version");

        assert!(config_data.contains("test"));
        assert_eq!(version, 1);
    }
}
