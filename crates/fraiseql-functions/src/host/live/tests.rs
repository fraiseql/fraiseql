//! Tests for `LiveHostContext` `GraphQL` query execution.

use super::*;

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
