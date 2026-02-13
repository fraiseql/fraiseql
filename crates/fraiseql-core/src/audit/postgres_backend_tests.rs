//! PostgreSQL audit backend tests
//!
//! Comprehensive tests for PostgreSQL-based audit logging with connection pooling,
//! JSONB operations, multi-tenancy, and error handling.

use super::*;
use deadpool_postgres::{Pool, Runtime};
use serde_json::json;

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a test connection pool
async fn create_test_pool() -> Pool {
    let _db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://fraiseql:fraiseql_password@localhost:5432/fraiseql_test".to_string()
    });

    // Use a simple config with direct connection parameters
    let mut config = deadpool_postgres::Config::new();
    config.dbname = Some("fraiseql_test".to_string());
    config.user = Some("fraiseql".to_string());
    config.password = Some("fraiseql_password".to_string());
    config.host = Some("localhost".to_string());
    config.port = Some(5432);

    config
        .create_pool(Some(Runtime::Tokio1), tokio_postgres::NoTls)
        .expect("Failed to create pool")
}

/// Clean audit table before test
async fn clean_audit_table(pool: &Pool) -> AuditResult<()> {
    let client = pool
        .get()
        .await
        .map_err(|e| AuditError::DatabaseError(format!("Failed to get connection: {}", e)))?;

    client
        .execute("DELETE FROM audit_log", &[])
        .await
        .map_err(|e| AuditError::DatabaseError(format!("Failed to clean table: {}", e)))?;

    Ok(())
}

/// Count rows in audit table
async fn count_audit_rows(pool: &Pool) -> i64 {
    let Ok(client) = pool.get().await else { return -1 };

    match client.query_one("SELECT COUNT(*) FROM audit_log", &[]).await {
        Ok(row) => row.get(0),
        Err(_) => -1,
    }
}

/// Check if event exists in database by ID
async fn event_exists_in_db(pool: &Pool, event_id: &str) -> bool {
    let Ok(client) = pool.get().await else { return false };

    let Ok(uuid) = uuid::Uuid::parse_str(event_id) else { return false };

    client
        .query_one("SELECT 1 FROM audit_log WHERE id = $1", &[&uuid])
        .await
        .is_ok()
}

// ============================================================================
// Test 1: Backend Creation
// ============================================================================

/// Test PostgreSQL backend creation
#[tokio::test]
async fn test_postgres_backend_creation() {
    let pool = create_test_pool().await;
    let result = PostgresAuditBackend::new(pool).await;
    assert!(result.is_ok(), "Backend should create successfully");
}

/// Test table creation with proper schema
#[tokio::test]
async fn test_postgres_table_creation() {
    let pool = create_test_pool().await;
    let _backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let client = pool.get().await.expect("Failed to get connection");

    // Check if table exists
    let result = client
        .query(
            "SELECT 1 FROM information_schema.tables WHERE table_name = 'audit_log'",
            &[],
        )
        .await;

    assert!(result.is_ok(), "audit_log table should exist");
}

/// Test index creation
#[tokio::test]
async fn test_postgres_index_creation() {
    let pool = create_test_pool().await;
    let _backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let client = pool.get().await.expect("Failed to get connection");

    // Check for expected indexes
    let expected_indexes = vec![
        "idx_audit_timestamp",
        "idx_audit_user_id",
        "idx_audit_event_type",
        "idx_audit_tenant_id",
        "idx_audit_tenant_time",
        "idx_audit_user_time",
        "idx_audit_status",
    ];

    for index_name in expected_indexes {
        let result = client
            .query(
                "SELECT 1 FROM pg_indexes WHERE indexname = $1",
                &[&index_name],
            )
            .await;

        assert!(
            result.is_ok() && !result.unwrap().is_empty(),
            "Index {} should exist",
            index_name
        );
    }
}

// ============================================================================
// Test 2: Basic Event Logging
// ============================================================================

/// Test logging single event
#[tokio::test]
async fn test_postgres_log_single_event() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );

    let event_id = event.id.clone();
    let result = backend.log_event(event).await;

    assert!(result.is_ok(), "Should log event successfully");
    assert!(
        event_exists_in_db(&pool, &event_id).await,
        "Event should be in database"
    );
}

/// Test logging event with all optional fields
#[tokio::test]
async fn test_postgres_log_event_with_all_fields() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let event = AuditEvent::new_user_action("user456", "bob", "192.168.1.2", "posts", "update", "success")
        .with_resource_id("post_123")
        .with_before_state(json!({"title": "Old Title", "content": "Old content"}))
        .with_after_state(json!({"title": "New Title", "content": "New content"}))
        .with_tenant_id("tenant_1")
        .with_metadata("user_agent", json!("Mozilla/5.0"))
        .with_metadata("correlation_id", json!("req-789"));

    let event_id = event.id.clone();
    let result = backend.log_event(event).await;

    assert!(result.is_ok(), "Should log event with all fields");
    assert!(
        event_exists_in_db(&pool, &event_id).await,
        "Event should exist in database"
    );
}

// ============================================================================
// Test 3: Event Querying
// ============================================================================

/// Test querying all events
#[tokio::test]
async fn test_postgres_query_all_events() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    // Log 5 events
    for i in 0..5 {
        let event = AuditEvent::new_user_action(
            format!("user{}", i),
            format!("user{}", i),
            "192.168.1.1",
            "users",
            "create",
            "success",
        );
        backend.log_event(event).await.ok();
    }

    let filters = AuditQueryFilters::default();
    let events = backend.query_events(filters).await;

    assert!(events.is_ok());
    assert_eq!(events.unwrap().len(), 5, "Should retrieve all 5 events");
}

/// Test querying by event_type
#[tokio::test]
async fn test_postgres_query_by_event_type() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    // Log events of different types
    let e1 = AuditEvent::new_user_action("user1", "alice", "192.168.1.1", "users", "create", "success");
    let e2 = AuditEvent::new_user_action("user2", "bob", "192.168.1.2", "posts", "delete", "success");
    backend.log_event(e1).await.ok();
    backend.log_event(e2).await.ok();

    let filters = AuditQueryFilters {
        event_type: Some("users_create".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "users_create");
}

/// Test querying by user_id
#[tokio::test]
async fn test_postgres_query_by_user_id() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let e1 = AuditEvent::new_user_action("alice", "alice", "192.168.1.1", "users", "create", "success");
    let e2 = AuditEvent::new_user_action("bob", "bob", "192.168.1.2", "posts", "delete", "success");
    backend.log_event(e1).await.ok();
    backend.log_event(e2).await.ok();

    let filters = AuditQueryFilters {
        user_id: Some("alice".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user_id, "alice");
}

/// Test querying by resource_type
#[tokio::test]
async fn test_postgres_query_by_resource_type() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let e1 = AuditEvent::new_user_action("u1", "u1", "192.168.1.1", "users", "create", "success");
    let e2 = AuditEvent::new_user_action("u2", "u2", "192.168.1.2", "posts", "create", "success");
    backend.log_event(e1).await.ok();
    backend.log_event(e2).await.ok();

    let filters = AuditQueryFilters {
        resource_type: Some("posts".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].resource_type, "posts");
}

/// Test querying by status
#[tokio::test]
async fn test_postgres_query_by_status() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let e1 = AuditEvent::new_user_action("u1", "u1", "192.168.1.1", "users", "create", "success");
    let e2 = AuditEvent::new_user_action("u2", "u2", "192.168.1.2", "users", "delete", "failure")
        .with_error("Access denied");
    backend.log_event(e1).await.ok();
    backend.log_event(e2).await.ok();

    let filters = AuditQueryFilters {
        status: Some("failure".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].status, "failure");
}

/// Test querying with pagination limit
#[tokio::test]
async fn test_postgres_query_with_limit() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    // Log 10 events
    for i in 0..10 {
        let event =
            AuditEvent::new_user_action(format!("u{}", i), format!("u{}", i), "192.168.1.1", "users", "create", "success");
        backend.log_event(event).await.ok();
    }

    let filters = AuditQueryFilters {
        limit: Some(5),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 5, "Should return only 5 events");
}

/// Test querying with pagination offset
#[tokio::test]
async fn test_postgres_query_with_offset() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    for i in 0..10 {
        let event =
            AuditEvent::new_user_action(format!("u{}", i), format!("u{}", i), "192.168.1.1", "users", "create", "success");
        backend.log_event(event).await.ok();
    }

    // Get first 5
    let filters1 = AuditQueryFilters {
        limit: Some(5),
        offset: Some(0),
        ..Default::default()
    };
    let events1 = backend.query_events(filters1).await.unwrap();

    // Get next 5
    let filters2 = AuditQueryFilters {
        limit: Some(5),
        offset: Some(5),
        ..Default::default()
    };
    let events2 = backend.query_events(filters2).await.unwrap();

    assert_eq!(events1.len(), 5);
    assert_eq!(events2.len(), 5);
    // Events should be different (ordered by timestamp DESC)
    assert_ne!(events1[0].id, events2[0].id);
}

/// Test events are ordered by timestamp DESC
#[tokio::test]
async fn test_postgres_query_ordering() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    // Log events with slight delays to ensure different timestamps
    for i in 0..3 {
        let event =
            AuditEvent::new_user_action(format!("u{}", i), format!("u{}", i), "192.168.1.1", "users", "create", "success");
        backend.log_event(event).await.ok();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    let filters = AuditQueryFilters::default();
    let events = backend.query_events(filters).await.unwrap();

    // Verify descending order
    for i in 1..events.len() {
        assert!(
            events[i - 1].timestamp >= events[i].timestamp,
            "Events should be ordered DESC by timestamp"
        );
    }
}

/// Test query with no matching results
#[tokio::test]
async fn test_postgres_query_no_results() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let filters = AuditQueryFilters {
        user_id: Some("nonexistent_user".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 0, "Should return empty result");
}

// ============================================================================
// Test 4: JSONB Operations
// ============================================================================

/// Test storing and retrieving complex JSONB metadata
#[tokio::test]
async fn test_postgres_jsonb_metadata() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let event = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "create", "success")
        .with_metadata("user_agent", json!("Mozilla/5.0"))
        .with_metadata("correlation_id", json!("req-123"))
        .with_metadata("request_path", json!("/api/users"));

    backend.log_event(event.clone()).await.ok();

    let filters = AuditQueryFilters {
        user_id: Some("u1".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    let retrieved = &events[0];
    assert!(retrieved.metadata.get("user_agent").is_some());
}

/// Test storing before_state and after_state
#[tokio::test]
async fn test_postgres_jsonb_state_snapshots() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let before = json!({"status": "inactive", "count": 0});
    let after = json!({"status": "active", "count": 5});

    let event = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "update", "success")
        .with_before_state(before.clone())
        .with_after_state(after.clone());

    backend.log_event(event).await.ok();

    let filters = AuditQueryFilters {
        user_id: Some("u1".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    let retrieved = &events[0];
    assert_eq!(retrieved.before_state, Some(before));
    assert_eq!(retrieved.after_state, Some(after));
}

/// Test null before_state and after_state
#[tokio::test]
async fn test_postgres_null_state_snapshots() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let event = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "read", "success");

    backend.log_event(event).await.ok();

    let filters = AuditQueryFilters {
        user_id: Some("u1".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    let retrieved = &events[0];
    assert!(retrieved.before_state.is_none());
    assert!(retrieved.after_state.is_none());
}

// ============================================================================
// Test 5: Multi-tenancy
// ============================================================================

/// Test tenant isolation
#[tokio::test]
async fn test_postgres_tenant_isolation() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let e1 = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "create", "success")
        .with_tenant_id("tenant_1");
    let e2 = AuditEvent::new_user_action("u2", "bob", "192.168.1.2", "users", "create", "success")
        .with_tenant_id("tenant_2");

    backend.log_event(e1).await.ok();
    backend.log_event(e2).await.ok();

    // Query tenant_1
    let filters = AuditQueryFilters {
        tenant_id: Some("tenant_1".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tenant_id, Some("tenant_1".to_string()));
}

/// Test null tenant_id handling
#[tokio::test]
async fn test_postgres_null_tenant_id() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let e1 = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "create", "success");
    let e2 =
        AuditEvent::new_user_action("u2", "bob", "192.168.1.2", "users", "create", "success").with_tenant_id("tenant_1");

    backend.log_event(e1).await.ok();
    backend.log_event(e2).await.ok();

    let all_events = backend.query_events(AuditQueryFilters::default()).await.unwrap();
    assert_eq!(all_events.len(), 2);

    // Count null and non-null
    let with_tenant = all_events.iter().filter(|e| e.tenant_id.is_some()).count();
    let without_tenant = all_events.iter().filter(|e| e.tenant_id.is_none()).count();

    assert_eq!(with_tenant, 1);
    assert_eq!(without_tenant, 1);
}

// ============================================================================
// Test 6: Error Handling
// ============================================================================

/// Test validation error: invalid status
#[tokio::test]
async fn test_postgres_validation_error_invalid_status() {
    let pool = create_test_pool().await;
    let backend = PostgresAuditBackend::new(pool).await.expect("Failed to create backend");

    // Create event with invalid status (must be success/failure/denied)
    let mut event = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "create", "success");
    event.status = "invalid_status".to_string();

    let result = backend.log_event(event).await;
    assert!(result.is_err(), "Should reject invalid status");
}

/// Test validation error: failure without error_message
#[tokio::test]
async fn test_postgres_validation_error_failure_no_message() {
    let pool = create_test_pool().await;
    let backend = PostgresAuditBackend::new(pool).await.expect("Failed to create backend");

    let mut event = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "create", "failure");
    event.error_message = None;

    let result = backend.log_event(event).await;
    assert!(result.is_err(), "Should require error_message for failure status");
}

/// Test UUID parsing error
#[tokio::test]
async fn test_postgres_uuid_parsing_error() {
    let pool = create_test_pool().await;
    let backend = PostgresAuditBackend::new(pool).await.expect("Failed to create backend");

    let mut event = AuditEvent::new_user_action("u1", "alice", "192.168.1.1", "users", "create", "success");
    event.id = "invalid-uuid".to_string();

    let result = backend.log_event(event).await;
    assert!(result.is_err(), "Should reject invalid UUID");
}

// ============================================================================
// Test 7: Performance and Concurrency
// ============================================================================

/// Test bulk logging (500 events)
#[tokio::test]
async fn test_postgres_bulk_logging() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    for i in 0..500 {
        let event = AuditEvent::new_user_action(
            format!("u{}", i % 10),
            format!("user{}", i % 10),
            "192.168.1.1",
            "users",
            "create",
            "success",
        );
        backend.log_event(event).await.ok();
    }

    let count = count_audit_rows(&pool).await;
    assert_eq!(count, 500, "Should have 500 events logged");
}

/// Test concurrent writes
#[tokio::test]
async fn test_postgres_concurrent_writes() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = std::sync::Arc::new(PostgresAuditBackend::new(pool.clone()).await.expect("Failed to create backend"));

    let mut handles = vec![];
    for i in 0..20 {
        let backend_clone = backend.clone();
        let handle = tokio::spawn(async move {
            for j in 0..5 {
                let event = AuditEvent::new_user_action(
                    format!("u{}", i * 5 + j),
                    format!("user{}", i),
                    "192.168.1.1",
                    "users",
                    "create",
                    "success",
                );
                let _ = backend_clone.log_event(event).await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.ok();
    }

    let count = count_audit_rows(&pool).await;
    assert_eq!(count, 100, "Should have 100 events from 20 concurrent tasks");
}

// ============================================================================
// Test 8: Schema Idempotency
// ============================================================================

/// Test table creation idempotency
#[tokio::test]
async fn test_postgres_table_creation_idempotent() {
    let pool = create_test_pool().await;

    // Create backend twice
    let _backend1 = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("First creation should succeed");

    let _backend2 = PostgresAuditBackend::new(pool)
        .await
        .expect("Second creation should succeed");
}

/// Test index creation idempotency
#[tokio::test]
async fn test_postgres_index_creation_idempotent() {
    let pool = create_test_pool().await;

    // Create backend multiple times
    for _ in 0..3 {
        let result = PostgresAuditBackend::new(pool.clone()).await;
        assert!(result.is_ok(), "Should succeed even if indexes already exist");
    }
}

// ============================================================================
// Test 9: Complex Queries
// ============================================================================

/// Test query with multiple combined filters
#[tokio::test]
async fn test_postgres_query_multiple_filters() {
    let pool = create_test_pool().await;
    clean_audit_table(&pool).await.ok();

    let backend = PostgresAuditBackend::new(pool.clone())
        .await
        .expect("Failed to create backend");

    let e1 = AuditEvent::new_user_action("alice", "alice", "192.168.1.1", "users", "create", "success")
        .with_tenant_id("tenant_1");
    let e2 = AuditEvent::new_user_action("alice", "alice", "192.168.1.1", "posts", "create", "success")
        .with_tenant_id("tenant_1");
    let e3 = AuditEvent::new_user_action("bob", "bob", "192.168.1.2", "users", "create", "success")
        .with_tenant_id("tenant_2");

    backend.log_event(e1).await.ok();
    backend.log_event(e2).await.ok();
    backend.log_event(e3).await.ok();

    let filters = AuditQueryFilters {
        user_id: Some("alice".to_string()),
        tenant_id: Some("tenant_1".to_string()),
        resource_type: Some("users".to_string()),
        ..Default::default()
    };
    let events = backend.query_events(filters).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user_id, "alice");
    assert_eq!(events[0].tenant_id, Some("tenant_1".to_string()));
    assert_eq!(events[0].resource_type, "users");
}
