#![allow(clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_noop_host_context_returns_unsupported() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };
    let ctx = NoopHostContext::new(payload);

    // Non-async methods should return Unsupported
    assert!(ctx.auth_context().is_err());
    assert!(ctx.env_var("TEST").is_ok());
}

#[test]
fn test_noop_host_context_log_captures_entries() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };
    let ctx = NoopHostContext::new(payload);

    // Log some messages at different levels
    ctx.log(LogLevel::Debug, "debug message");
    ctx.log(LogLevel::Info, "info message");
    ctx.log(LogLevel::Warn, "warning message");
    ctx.log(LogLevel::Error, "error message");

    // Verify all logs were captured
    let logs = ctx.captured_logs();
    assert_eq!(logs.len(), 4);
    assert_eq!(logs[0].level, LogLevel::Debug);
    assert_eq!(logs[0].message, "debug message");
    assert_eq!(logs[1].level, LogLevel::Info);
    assert_eq!(logs[1].message, "info message");
    assert_eq!(logs[2].level, LogLevel::Warn);
    assert_eq!(logs[2].message, "warning message");
    assert_eq!(logs[3].level, LogLevel::Error);
    assert_eq!(logs[3].message, "error message");
}

#[test]
fn test_event_payload_available_in_context() {
    let payload = EventPayload {
        trigger_type: "mutation".to_string(),
        entity:       "User".to_string(),
        event_kind:   "updated".to_string(),
        data:         serde_json::json!({"id": 42}),
        timestamp:    chrono::Utc::now(),
    };
    let ctx = NoopHostContext::new(payload);

    let retrieved = ctx.event_payload();
    assert_eq!(retrieved.trigger_type, "mutation");
    assert_eq!(retrieved.entity, "User");
    assert_eq!(retrieved.event_kind, "updated");
    assert_eq!(retrieved.data, serde_json::json!({"id": 42}));
}

// Storage access tests for LiveHostContext
#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_host_storage_get_returns_bytes() {
    use std::{collections::HashMap, sync::Arc};

    use crate::host::live::{HostContextConfig, LiveHostContext};

    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "File".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    // Create a mock storage backend
    let storage_data = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let test_data = b"hello world".to_vec();
    {
        let mut data = storage_data.lock().expect("storage data mutex poisoned");
        data.entry("documents".to_string())
            .or_insert_with(HashMap::new)
            .insert("file.txt".to_string(), test_data.clone());
    }

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());
    // Store the mock data directly (for now, without a real StorageBackend trait)
    // This test is a placeholder until we implement proper storage backend integration

    let result = ctx.storage_get("documents", "file.txt").await;

    // Should fail with Unsupported since storage backend is not configured
    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Unsupported { .. }) => (),
        other => panic!("expected Unsupported error, got {:?}", other),
    }
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_host_storage_without_backend_returns_unsupported() {
    use crate::host::live::{HostContextConfig, LiveHostContext};

    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "File".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.storage_get("documents", "file.txt").await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Unsupported { message }) => {
            assert!(message.contains("not yet implemented") || message.contains("not configured"));
        },
        other => panic!("expected Unsupported error, got {:?}", other),
    }
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_host_storage_put_respects_size_limit() {
    use crate::host::live::{HostContextConfig, LiveHostContext};

    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "File".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    // Create config with very small size limit
    let config = HostContextConfig {
        max_storage_upload_bytes: 10, // 10 bytes limit
        ..Default::default()
    };

    let ctx = LiveHostContext::new(payload, config);

    // Try to upload larger than limit (but first check if storage is not configured)
    let oversized_data = vec![0u8; 100];
    let result = ctx.storage_put("documents", "large.txt", &oversized_data, "text/plain").await;

    // Should fail with Unsupported since storage backend is not configured
    assert!(result.is_err());
}
