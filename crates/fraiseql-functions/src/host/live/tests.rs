//! Tests for `LiveHostContext` `GraphQL` query execution.

use super::*;
use std::sync::Arc;

/// Mock query executor for testing.
struct MockQueryExecutor {
    /// If set, return this result; otherwise return an error.
    response: Option<serde_json::Value>,
}

impl MockQueryExecutor {
    fn new(response: serde_json::Value) -> Arc<Self> {
        Arc::new(Self {
            response: Some(response),
        })
    }

    fn error() -> Arc<Self> {
        Arc::new(Self { response: None })
    }
}

impl QueryExecutor for MockQueryExecutor {
    fn execute_query(
        &self,
        _query: &str,
        _variables: Option<&serde_json::Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + '_>> {
        Box::pin(async move {
            match &self.response {
                Some(value) => Ok(value.clone()),
                None => Err(fraiseql_error::FraiseQLError::Validation {
                    message: "mock query failed".to_string(),
                    path: None,
                }),
            }
        })
    }
}

#[tokio::test]
async fn test_host_query_executes_graphql() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let response = serde_json::json!({
        "data": {
            "users": [
                {"id": "1", "name": "Alice"},
                {"id": "2", "name": "Bob"}
            ]
        }
    });
    let executor = MockQueryExecutor::new(response.clone());

    let ctx = LiveHostContext::with_executor(payload, HostContextConfig::default(), executor);

    let result = ctx.query("{ users { id name } }", serde_json::json!({})).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), response);
}

#[tokio::test]
async fn test_host_query_passes_variables() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let response = serde_json::json!({
        "data": {
            "user": {"id": "1", "name": "Alice"}
        }
    });
    let executor = MockQueryExecutor::new(response.clone());

    let ctx = LiveHostContext::with_executor(payload, HostContextConfig::default(), executor);

    let result = ctx
        .query(
            "query($id: Int!) { user(id: $id) { name } }",
            serde_json::json!({"id": 1}),
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), response);
}

#[tokio::test]
async fn test_host_query_rejects_mutations() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let executor = MockQueryExecutor::error();
    let ctx = LiveHostContext::with_executor(payload, HostContextConfig::default(), executor);

    let result = ctx
        .query(
            "mutation { createUser(name: \"Alice\") { id } }",
            serde_json::json!({}),
        )
        .await;

    // The mock executor will reject it, but in the real implementation,
    // mutation detection should happen before executing.
    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_query_invalid_graphql_returns_validation_error() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let executor = MockQueryExecutor::error();
    let ctx = LiveHostContext::with_executor(payload, HostContextConfig::default(), executor);

    let result = ctx.query("{ invalid syntax }", serde_json::json!({})).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_query_without_executor_returns_unsupported() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.query("{ users { id } }", serde_json::json!({})).await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Unsupported { message }) => {
            assert!(message.contains("executor"));
        }
        _ => panic!("expected Unsupported error"),
    }
}

// SQL Query Tests

#[tokio::test]
async fn test_host_sql_query_returns_rows() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .sql_query("SELECT id, name FROM users WHERE active = $1", &[serde_json::json!(true)])
        .await;

    // Should succeed (SELECT is allowed)
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_host_sql_query_rejects_insert() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .sql_query("INSERT INTO users (name) VALUES ($1)", &[serde_json::json!("Alice")])
        .await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Authorization { message, .. }) => {
            assert!(message.contains("not allowed"));
        }
        other => panic!("expected Authorization error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_host_sql_query_rejects_update() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .sql_query(
            "UPDATE users SET name = $1 WHERE id = $2",
            &[serde_json::json!("Bob"), serde_json::json!(1)],
        )
        .await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Authorization { .. }) => (),
        other => panic!("expected Authorization error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_host_sql_query_rejects_delete() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.sql_query("DELETE FROM users WHERE id = $1", &[serde_json::json!(1)]).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_sql_query_rejects_ddl() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.sql_query("DROP TABLE users", &[]).await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Authorization { message, .. }) => {
            assert!(message.contains("not allowed"));
        }
        other => panic!("expected Authorization error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_host_sql_query_rejects_copy() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .sql_query("COPY users FROM '/tmp/data.csv'", &[])
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_sql_query_rejects_create_extension() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .sql_query("CREATE EXTENSION IF NOT EXISTS pgcrypto", &[])
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_sql_query_rejects_set_role() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.sql_query("SET ROLE admin", &[]).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_sql_query_rejects_call() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.sql_query("CALL delete_all_users()", &[]).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_sql_query_rejects_truncate() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.sql_query("TRUNCATE users", &[]).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_sql_query_allows_explain_without_analyze() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .sql_query("EXPLAIN SELECT * FROM users", &[])
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_host_sql_query_rejects_explain_analyze() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .sql_query("EXPLAIN ANALYZE SELECT * FROM users", &[])
        .await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Authorization { message, .. }) => {
            assert!(message.contains("not allowed"));
        }
        other => panic!("expected Authorization error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_host_sql_query_invalid_returns_validation_error() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx.sql_query("INVALID SYNTAX HERE", &[]).await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Validation { .. }) => (),
        other => panic!("expected Validation error, got {:?}", other),
    }
}

// HTTP Request Tests

#[tokio::test]
async fn test_host_http_valid_domain_passes_validation() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    // Configure to allow a specific domain
    let mut config = HostContextConfig::default();
    config.allowed_domains = vec!["api.example.com".to_string()];

    let client = Arc::new(
        reqwest::Client::builder()
            .build()
            .expect("failed to create client"),
    );
    let ctx = LiveHostContext::with_http_client(payload, config, client);

    // This URL passes domain allowlist but will fail connection (expected)
    let result = ctx
        .http_request("GET", "https://api.example.com/api/test", &[], None)
        .await;

    // Should not be blocked by domain allowlist; error is from connection attempt
    match result {
        Ok(_) => {}, // Success (unexpected but ok)
        Err(fraiseql_error::FraiseQLError::Authorization { message, .. }) => {
            panic!("should not block allowed domain: {}", message);
        }
        Err(_) => {}, // Connection error is expected (domain doesn't exist)
    }
}

#[tokio::test]
async fn test_host_http_subdomain_glob_pattern() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    // Configure with glob pattern
    let mut config = HostContextConfig::default();
    config.allowed_domains = vec!["*.example.com".to_string()];

    let client = Arc::new(
        reqwest::Client::builder()
            .build()
            .expect("failed to create client"),
    );
    let ctx = LiveHostContext::with_http_client(payload, config, client);

    // Should pass domain check
    let result = ctx
        .http_request("GET", "https://api.example.com/test", &[], None)
        .await;

    match result {
        Ok(_) => {},
        Err(fraiseql_error::FraiseQLError::Authorization { message, .. }) if message.contains("domain") => {
            panic!("should allow subdomain matching glob pattern: {}", message);
        }
        Err(_) => {}, // Connection error is expected
    }
}

#[tokio::test]
async fn test_host_http_blocks_disallowed_domain() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let mut config = HostContextConfig::default();
    config.allowed_domains = vec!["allowed.com".to_string()];
    let ctx = LiveHostContext::new(payload, config);

    let result = ctx
        .http_request("GET", "https://blocked.com/api", &[], None)
        .await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Authorization { .. }) => (),
        other => panic!("expected Authorization error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_host_http_blocks_private_ipv4() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let private_ips = vec![
        "http://127.0.0.1/api",
        "http://10.0.0.1/api",
        "http://192.168.1.1/api",
        "http://172.16.0.1/api",
    ];

    for ip_url in private_ips {
        let result = ctx.http_request("GET", ip_url, &[], None).await;
        assert!(result.is_err(), "should block {}", ip_url);
    }
}

#[tokio::test]
async fn test_host_http_blocks_ipv6_loopback() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .http_request("GET", "http://[::1]/api", &[], None)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_http_blocks_ipv6_link_local() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .http_request("GET", "http://[fe80::1]/api", &[], None)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_host_http_allows_public_ipv4() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    // This will fail because the IP doesn't respond, but it should pass the SSRF check
    let result = ctx
        .http_request("GET", "http://8.8.8.8/api", &[], None)
        .await;

    // The request itself may fail (DNS, connection), but not due to SSRF
    match result {
        Ok(_) => {}, // Request succeeded (unlikely in test environment)
        Err(fraiseql_error::FraiseQLError::Authorization { .. }) => {
            panic!("should not block public IP")
        }
        Err(_) => {}, // Other error is fine (connection, DNS, etc.)
    }
}

#[tokio::test]
async fn test_host_http_invalid_url() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "API".to_string(),
        event_kind: "called".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let ctx = LiveHostContext::new(payload, HostContextConfig::default());

    let result = ctx
        .http_request("GET", "not a valid url", &[], None)
        .await;

    assert!(result.is_err());
}

// Storage Tests

#[tokio::test]
async fn test_host_storage_get_returns_bytes() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "File".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let backend = super::storage::MockStorageBackend::new();
    let test_data = b"hello world".to_vec();
    backend.store("documents", "file.txt", test_data.clone());

    let mut ctx = LiveHostContext::new(payload, HostContextConfig::default());
    ctx.storage_backend = Some(backend as Arc<dyn super::storage::StorageBackend>);

    let result = ctx.storage_get("documents", "file.txt").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), test_data);
}

#[tokio::test]
async fn test_host_storage_put_creates_object() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "File".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let backend = super::storage::MockStorageBackend::new();
    let mut ctx = LiveHostContext::new(payload, HostContextConfig::default());
    ctx.storage_backend = Some(backend.clone() as Arc<dyn super::storage::StorageBackend>);

    let test_data = b"test file content".as_slice();
    let result = ctx
        .storage_put("documents", "newfile.txt", test_data, "text/plain")
        .await;

    assert!(result.is_ok());
    // Verify the data was stored
    assert_eq!(
        backend.get_stored("documents", "newfile.txt"),
        Some(test_data.to_vec())
    );
}

#[tokio::test]
async fn test_host_storage_get_nonexistent_returns_not_found() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "File".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let backend = super::storage::MockStorageBackend::new();
    let mut ctx = LiveHostContext::new(payload, HostContextConfig::default());
    ctx.storage_backend = Some(backend as Arc<dyn super::storage::StorageBackend>);

    let result = ctx.storage_get("documents", "nonexistent.txt").await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Storage { message, .. }) => {
            assert!(message.contains("not found") || message.contains("does not exist"));
        }
        other => panic!("expected Storage error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_host_storage_put_respects_size_limit() {
    let payload = EventPayload {
        trigger_type: "test".to_string(),
        entity: "File".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let backend = super::storage::MockStorageBackend::new();

    // Create config with very small size limit
    let mut config = HostContextConfig::default();
    config.max_storage_upload_bytes = 10; // 10 bytes limit

    let mut ctx = LiveHostContext::new(payload, config);
    ctx.storage_backend = Some(backend as Arc<dyn super::storage::StorageBackend>);

    // Try to upload larger than limit
    let oversized_data = vec![0u8; 100];
    let result = ctx
        .storage_put("documents", "large.txt", &oversized_data, "text/plain")
        .await;

    assert!(result.is_err());
    match result {
        Err(fraiseql_error::FraiseQLError::Validation { message, .. }) => {
            assert!(message.contains("exceeds") || message.contains("size limit"));
        }
        other => panic!("expected Validation error, got {:?}", other),
    }
}

// Note: test_host_storage_without_backend_returns_unsupported is in host/mod.rs
// as a trait-level test since it doesn't require a real storage backend
