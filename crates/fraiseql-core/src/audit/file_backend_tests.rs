//! File backend tests
//!
//! Tests for JSON lines file-based audit logging

use tempfile::TempDir;

use super::*;

// ============================================================================
// Test 1: File Backend Creation
// ============================================================================

/// Test creating file backend
#[tokio::test]
async fn test_file_backend_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    // Create backend - this will fail until implemented
    let _backend = FileAuditBackend::new(file_path).await;
}

/// Test file backend with invalid path
#[tokio::test]
async fn test_file_backend_invalid_path() {
    // Path that cannot be created should return error
    let invalid_path = "/invalid/nonexistent/path/audit.log";

    let result = FileAuditBackend::new(invalid_path).await;
    assert!(result.is_err(), "Should error on invalid path");
}

// ============================================================================
// Test 2: File Backend Event Logging
// ============================================================================

/// Test logging single event to file
#[tokio::test]
async fn test_file_backend_log_event() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );

    let result = backend.log_event(event).await;
    assert!(result.is_ok(), "Should log event successfully");
}

/// Test file backend writes JSON lines format
#[tokio::test]
async fn test_file_backend_json_lines_format() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    // Log first event
    let event1 =
        AuditEvent::new_user_action("user1", "alice", "192.168.1.1", "users", "create", "success");
    backend.log_event(event1).await.ok();

    // Log second event
    let event2 =
        AuditEvent::new_user_action("user2", "bob", "192.168.1.2", "posts", "delete", "success");
    backend.log_event(event2).await.ok();

    // Read file and verify format
    let content = std::fs::read_to_string(&file_path).expect("Failed to read file");
    let lines: Vec<&str> = content.lines().collect();

    assert_eq!(lines.len(), 2, "Should have 2 lines");

    // Each line should be valid JSON
    for line in &lines {
        let result: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(result.is_ok(), "Each line should be valid JSON");
    }
}

/// Test file backend appends to existing file
#[tokio::test]
async fn test_file_backend_appends() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    // Create backend and log first event
    {
        let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

        let event = AuditEvent::new_user_action(
            "user1",
            "alice",
            "192.168.1.1",
            "users",
            "create",
            "success",
        );
        backend.log_event(event).await.ok();
    }

    // Create new backend on same file and log second event
    {
        let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

        let event = AuditEvent::new_user_action(
            "user2",
            "bob",
            "192.168.1.2",
            "posts",
            "delete",
            "success",
        );
        backend.log_event(event).await.ok();
    }

    // Verify both events are in file
    let content = std::fs::read_to_string(&file_path).expect("Failed to read file");
    let lines: Vec<&str> = content.lines().collect();

    assert!(lines.len() >= 2, "Should have at least 2 lines");
}

// ============================================================================
// Test 3: File Backend Event Querying
// ============================================================================

/// Test querying events from file
#[tokio::test]
async fn test_file_backend_query_events() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    // Log multiple events
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

    // Query all events
    let filters = AuditQueryFilters::default();
    let events = backend.query_events(filters).await;

    assert!(events.is_ok(), "Query should succeed");
    let events = events.unwrap();
    assert_eq!(events.len(), 5, "Should return all 5 events");
}

/// Test querying with user_id filter
#[tokio::test]
async fn test_file_backend_query_by_user() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    // Log events for different users
    let event1 =
        AuditEvent::new_user_action("alice", "alice", "192.168.1.1", "users", "create", "success");
    backend.log_event(event1).await.ok();

    let event2 =
        AuditEvent::new_user_action("bob", "bob", "192.168.1.2", "posts", "delete", "success");
    backend.log_event(event2).await.ok();

    // Query for alice's events
    let filters = AuditQueryFilters {
        user_id: Some("alice".to_string()),
        ..Default::default()
    };

    let events = backend.query_events(filters).await;
    assert!(events.is_ok());
    let events = events.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user_id, "alice");
}

/// Test querying with limit
#[tokio::test]
async fn test_file_backend_query_with_limit() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    // Log 10 events
    for i in 0..10 {
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

    // Query with limit
    let filters = AuditQueryFilters {
        limit: Some(5),
        ..Default::default()
    };

    let events = backend.query_events(filters).await;
    assert!(events.is_ok());
    let events = events.unwrap();
    assert_eq!(events.len(), 5, "Should respect limit");
}

// ============================================================================
// Test 4: File Backend Error Handling
// ============================================================================

/// Test file backend handles write errors gracefully
#[tokio::test]
async fn test_file_backend_write_error_handling() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    let event =
        AuditEvent::new_user_action("user1", "alice", "192.168.1.1", "users", "create", "success");

    // First write should succeed
    let result = backend.log_event(event.clone()).await;
    assert!(result.is_ok());

    // Simulate file deletion and attempt another write
    std::fs::remove_file(&file_path).ok();

    // Should handle error gracefully
    let result = backend.log_event(event).await;
    // May succeed (recreates file) or fail with proper error
    if result.is_err() {
        // Error should be descriptive
        let err_msg = format!("{:?}", result);
        assert!(
            err_msg.contains("File") || err_msg.contains("IO"),
            "Error should be descriptive"
        );
    }
}

// ============================================================================
// Test 5: File Backend Performance
// ============================================================================

/// Test file backend performance with many events
#[tokio::test]
async fn test_file_backend_bulk_logging() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    // Log 100 events
    for i in 0..100 {
        let event = AuditEvent::new_user_action(
            format!("user{}", i % 10),
            format!("user{}", i % 10),
            "192.168.1.1",
            "users",
            "create",
            "success",
        );
        let result = backend.log_event(event).await;
        assert!(result.is_ok(), "Should log event {}", i);
    }

    // Query all events
    let filters = AuditQueryFilters::default();
    let events = backend.query_events(filters).await;

    assert!(events.is_ok());
    let events = events.unwrap();
    assert_eq!(events.len(), 100, "Should have all 100 events");
}

// ============================================================================
// Test 6: File Backend Concurrency
// ============================================================================

/// Test file backend handles concurrent writes
#[tokio::test]
async fn test_file_backend_concurrent_writes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("audit.log");

    let backend = FileAuditBackend::new(&file_path).await.expect("Failed to create backend");

    let backend = std::sync::Arc::new(backend);

    // Spawn multiple tasks logging events concurrently
    let mut handles: Vec<tokio::task::JoinHandle<AuditResult<()>>> = vec![];
    for i in 0..10 {
        let backend_clone = backend.clone();
        let handle = tokio::spawn(async move {
            let event = AuditEvent::new_user_action(
                format!("user{}", i),
                format!("user{}", i),
                "192.168.1.1",
                "users",
                "create",
                "success",
            );
            backend_clone.log_event(event).await
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "Task should complete successfully");
        assert!(result.unwrap().is_ok(), "Log should succeed");
    }

    // Verify all events were logged
    let filters = AuditQueryFilters::default();
    let events = backend.query_events(filters).await;
    assert!(events.is_ok());
    let events = events.unwrap();
    assert_eq!(events.len(), 10, "Should have all 10 concurrent events");
}
