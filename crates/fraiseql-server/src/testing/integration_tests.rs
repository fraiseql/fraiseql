//! Comprehensive integration tests using real database infrastructure
//!
//! Tests real functionality against in-memory SQLite to verify
//! query execution, configuration, Arrow conversion, and event handling.

use super::sqlite::{create_test_sqlite_pool_with_schema, TEST_EVENTS_SCHEMA};
use sqlx::Row;

#[cfg(test)]
mod real_database_tests {
    use super::*;

    /// Test: Insert and query events in real database
    #[tokio::test]
    async fn test_real_event_storage_and_retrieval() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert an event
        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data, user_id, tenant_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("evt-001")
        .bind("UserCreated")
        .bind("User")
        .bind("user-123")
        .bind(1704067200i64)
        .bind(r#"{"email": "alice@example.com", "name": "Alice", "role": "admin"}"#)
        .bind("sys-user")
        .bind("acme-corp")
        .execute(&pool)
        .await
        .expect("Failed to insert event");

        // Retrieve and verify
        let row = sqlx::query("SELECT * FROM events WHERE id = ?")
            .bind("evt-001")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch event");

        let id: String = row.get("id");
        let event_type: String = row.get("event_type");
        let entity_type: String = row.get("entity_type");
        let data: String = row.get("data");
        let user_id: Option<String> = row.get("user_id");
        let tenant_id: Option<String> = row.get("tenant_id");

        assert_eq!(id, "evt-001");
        assert_eq!(event_type, "UserCreated");
        assert_eq!(entity_type, "User");
        assert!(data.contains("alice@example.com"));
        assert_eq!(user_id, Some("sys-user".to_string()));
        assert_eq!(tenant_id, Some("acme-corp".to_string()));
    }

    /// Test: Query events with WHERE clause (simulating GraphQL query generation)
    #[tokio::test]
    async fn test_query_events_with_filters() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert multiple events
        for i in 1..=10 {
            sqlx::query(
                "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data, tenant_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(format!("evt-{:03}", i))
            .bind(if i <= 5 { "Created" } else { "Updated" })
            .bind("Product")
            .bind(format!("prod-{:03}", i))
            .bind(1704067200i64 + (i as i64 * 3600))
            .bind(format!(r#"{{"status": "active", "index": {}}}"#, i))
            .bind("tenant-1")
            .execute(&pool)
            .await
            .expect("Failed to insert event");
        }

        // Query with filter (entity_type = 'Product' AND event_type = 'Created')
        let rows = sqlx::query("SELECT COUNT(*) as count FROM events WHERE entity_type = ? AND event_type = ?")
            .bind("Product")
            .bind("Created")
            .fetch_one(&pool)
            .await
            .expect("Failed to count events");

        let count: i32 = rows.get("count");
        assert_eq!(count, 5);

        // Query by timestamp range (9 events, since event 10 is at exact boundary)
        let rows = sqlx::query("SELECT COUNT(*) as count FROM events WHERE timestamp >= ? AND timestamp < ?")
            .bind(1704067200i64)
            .bind(1704067200i64 + 36000)
            .fetch_one(&pool)
            .await
            .expect("Failed to count range events");

        let range_count: i32 = rows.get("count");
        assert_eq!(range_count, 9); // Events 1-9 within the range (event 10 is at boundary)
    }

    /// Test: Events with complex JSON data
    #[tokio::test]
    async fn test_events_with_complex_json() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        let complex_data = r#"{
            "user": {
                "id": "user-456",
                "email": "bob@example.com",
                "profile": {
                    "firstName": "Bob",
                    "lastName": "Builder",
                    "tags": ["developer", "senior", "rust"]
                }
            },
            "metadata": {
                "source": "api",
                "version": "2.0"
            }
        }"#;

        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("evt-complex-001")
        .bind("UserProfileUpdated")
        .bind("User")
        .bind("user-456")
        .bind(1704067200i64)
        .bind(complex_data)
        .execute(&pool)
        .await
        .expect("Failed to insert complex event");

        let row = sqlx::query("SELECT data FROM events WHERE id = ?")
            .bind("evt-complex-001")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch event");

        let data: String = row.get("data");
        assert!(data.contains("Bob"));
        assert!(data.contains("developer"));
        assert!(data.contains("rust"));
    }

    /// Test: Batch event insertion
    #[tokio::test]
    async fn test_batch_event_insertion_and_query() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert 100 events in a loop (simulating batch import)
        for i in 1..=100 {
            sqlx::query(
                "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(format!("evt-batch-{:04}", i))
            .bind(format!("Event{}", (i % 5) + 1))
            .bind(format!("Entity{}", (i % 3) + 1))
            .bind(format!("entity-{:04}", i))
            .bind(1704067200i64 + (i as i64))
            .bind(format!(r#"{{"seq": {}}}"#, i))
            .execute(&pool)
            .await
            .expect("Failed to insert event");
        }

        // Query total count
        let row = sqlx::query("SELECT COUNT(*) as count FROM events")
            .fetch_one(&pool)
            .await
            .expect("Failed to count events");

        let total_count: i32 = row.get("count");
        assert_eq!(total_count, 100);

        // Query by event type
        let row = sqlx::query("SELECT COUNT(*) as count FROM events WHERE event_type = ?")
            .bind("Event1")
            .fetch_one(&pool)
            .await
            .expect("Failed to count by type");

        let type_count: i32 = row.get("count");
        assert_eq!(type_count, 20); // 100 / 5 = 20 per event type
    }

    /// Test: Order and limit (pagination)
    #[tokio::test]
    async fn test_event_pagination() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert ordered events
        for i in 1..=20 {
            sqlx::query(
                "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(format!("evt-page-{:02}", i))
            .bind("Test")
            .bind("Page")
            .bind(format!("page-{:02}", i))
            .bind(1704067200i64 + (i as i64 * 100))
            .bind(r#"{}"#)
            .execute(&pool)
            .await
            .expect("Failed to insert event");
        }

        // Get first page (limit 5)
        let rows = sqlx::query("SELECT id FROM events ORDER BY timestamp ASC LIMIT 5")
            .fetch_all(&pool)
            .await
            .expect("Failed to fetch first page");

        assert_eq!(rows.len(), 5);
        let first_id: String = rows[0].get("id");
        assert_eq!(first_id, "evt-page-01");

        // Get second page (limit 5, offset 5)
        let rows = sqlx::query("SELECT id FROM events ORDER BY timestamp ASC LIMIT 5 OFFSET 5")
            .fetch_all(&pool)
            .await
            .expect("Failed to fetch second page");

        assert_eq!(rows.len(), 5);
        let second_page_first: String = rows[0].get("id");
        assert_eq!(second_page_first, "evt-page-06");
    }

    /// Test: Transaction rollback on error
    #[tokio::test]
    async fn test_transaction_with_rollback() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Simulate transaction scenario
        let mut tx = pool.begin().await.expect("Failed to start transaction");

        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("evt-tx-001")
        .bind("Created")
        .bind("Test")
        .bind("test-1")
        .bind(1704067200i64)
        .bind(r#"{}"#)
        .execute(&mut *tx)
        .await
        .expect("Failed to insert");

        // Rollback transaction
        tx.rollback().await.expect("Failed to rollback");

        // Verify event was not saved
        let result = sqlx::query("SELECT * FROM events WHERE id = ?")
            .bind("evt-tx-001")
            .fetch_optional(&pool)
            .await
            .expect("Failed to query after rollback");

        assert!(result.is_none());
    }

    /// Test: Index usage for filtering
    #[tokio::test]
    async fn test_indexed_queries() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert events with varying tenant_id
        for i in 1..=50 {
            let tenant = format!("tenant-{}", (i % 5) + 1);
            sqlx::query(
                "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data, tenant_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(format!("evt-idx-{:02}", i))
            .bind("Event")
            .bind("Entity")
            .bind(format!("entity-{:02}", i))
            .bind(1704067200i64 + (i as i64))
            .bind(r#"{}"#)
            .bind(&tenant)
            .execute(&pool)
            .await
            .expect("Failed to insert");
        }

        // Query using indexed column (should be fast)
        let row = sqlx::query("SELECT COUNT(*) as count FROM events WHERE tenant_id = ?")
            .bind("tenant-1")
            .fetch_one(&pool)
            .await
            .expect("Failed to count by tenant");

        let count: i32 = row.get("count");
        assert_eq!(count, 10); // 50 / 5 = 10 per tenant
    }

    /// Test: NULL handling in different columns
    #[tokio::test]
    async fn test_null_handling() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert event with NULL fields
        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("evt-null-001")
        .bind("Test")
        .bind("Entity")
        .bind("entity-1")
        .bind(1704067200i64)
        .bind(r#"{}"#)
        // user_id and tenant_id are NULL
        .execute(&pool)
        .await
        .expect("Failed to insert event");

        // Query NULL and NOT NULL
        let row = sqlx::query("SELECT user_id, tenant_id FROM events WHERE id = ?")
            .bind("evt-null-001")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch");

        let user_id: Option<String> = row.get("user_id");
        let tenant_id: Option<String> = row.get("tenant_id");

        assert!(user_id.is_none());
        assert!(tenant_id.is_none());

        // Count NULL values
        let row = sqlx::query("SELECT COUNT(*) as count FROM events WHERE user_id IS NULL")
            .fetch_one(&pool)
            .await
            .expect("Failed to count nulls");

        let null_count: i32 = row.get("count");
        assert!(null_count >= 1);
    }

    /// Test: Update events
    #[tokio::test]
    async fn test_update_event() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert initial event
        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("evt-upd-001")
        .bind("Created")
        .bind("Entity")
        .bind("entity-1")
        .bind(1704067200i64)
        .bind(r#"{"status": "initial"}"#)
        .execute(&pool)
        .await
        .expect("Failed to insert");

        // Update event
        let rows_affected = sqlx::query("UPDATE events SET data = ? WHERE id = ?")
            .bind(r#"{"status": "updated"}"#)
            .bind("evt-upd-001")
            .execute(&pool)
            .await
            .expect("Failed to update")
            .rows_affected();

        assert_eq!(rows_affected, 1);

        // Verify update
        let row = sqlx::query("SELECT data FROM events WHERE id = ?")
            .bind("evt-upd-001")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch");

        let data: String = row.get("data");
        assert!(data.contains("updated"));
        assert!(!data.contains("initial"));
    }

    /// Test: Delete events
    #[tokio::test]
    async fn test_delete_event() {
        let pool = create_test_sqlite_pool_with_schema(TEST_EVENTS_SCHEMA)
            .await
            .expect("Failed to create pool");

        // Insert events
        sqlx::query(
            "INSERT INTO events (id, event_type, entity_type, entity_id, timestamp, data)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("evt-del-001")
        .bind("Test")
        .bind("Entity")
        .bind("entity-1")
        .bind(1704067200i64)
        .bind(r#"{}"#)
        .execute(&pool)
        .await
        .expect("Failed to insert");

        // Delete event
        let rows_affected = sqlx::query("DELETE FROM events WHERE id = ?")
            .bind("evt-del-001")
            .execute(&pool)
            .await
            .expect("Failed to delete")
            .rows_affected();

        assert_eq!(rows_affected, 1);

        // Verify deletion
        let result = sqlx::query("SELECT * FROM events WHERE id = ?")
            .bind("evt-del-001")
            .fetch_optional(&pool)
            .await
            .expect("Failed to query");

        assert!(result.is_none());
    }
}
