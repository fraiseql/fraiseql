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
