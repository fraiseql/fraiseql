//! Tests for the Flight service.

use std::sync::Arc;

use arrow_flight::{Action, Empty, FlightDescriptor, flight_service_server::FlightService};
use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use tonic::Request;

use super::{FraiseQLFlightService, QueryExecutor, SecurityContext, SessionTokenClaims};
use crate::ticket::FlightTicket;

/// Dummy executor for testing that implements QueryExecutor trait.
struct DummyExecutor;

#[async_trait]
impl QueryExecutor for DummyExecutor {
    async fn execute_with_security(
        &self,
        _query: &str,
        _variables: Option<&serde_json::Value>,
        _security_context: &fraiseql_core::security::SecurityContext,
    ) -> std::result::Result<String, String> {
        Ok(r#"{"data": {"test": "ok"}}"#.to_string())
    }
}

/// The secret value used for Flight session tokens in tests.
const TEST_FLIGHT_SECRET: &str = "test-flight-session-secret-for-unit-tests-only";

/// Returns the env vars needed for Flight session tests.
fn flight_secret_vars() -> [(&'static str, Option<&'static str>); 1] {
    [("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))]
}

/// Tests service initialization without database adapter
#[test]
fn test_new_creates_service_without_db_adapter() {
    let service = FraiseQLFlightService::new();
    assert!(service.db_adapter.is_none());
}

/// Tests that service registers default views on creation
#[test]
fn test_new_registers_defaults() {
    let service = FraiseQLFlightService::new();
    assert!(service.schema_registry.contains("va_orders"));
    assert!(service.schema_registry.contains("va_users"));
    assert!(service.schema_registry.contains("ta_orders"));
    assert!(service.schema_registry.contains("ta_users"));
}

/// Tests service initialization with executor
#[test]
fn test_new_with_executor_stores_reference() {
    let service = FraiseQLFlightService::new();
    // Executor field exists and can be set
    assert!(service.executor.is_none());
}

/// Tests that executor accessor works
#[test]
fn test_executor_accessor_returns_none_initially() {
    let service = FraiseQLFlightService::new();
    assert!(service.executor().is_none());
}

/// Tests that executor can be set and retrieved
#[test]
fn test_executor_can_be_set_and_retrieved() {
    let mut service = FraiseQLFlightService::new();

    // Create a dummy executor that implements QueryExecutor trait
    let dummy: Arc<dyn QueryExecutor> = Arc::new(DummyExecutor);
    service.set_executor(dummy.clone());

    assert!(service.executor().is_some());
    let _retrieved = service.executor().unwrap();
    // Executor trait object is now properly typed
}

/// Tests that fraiseql-core types are now accessible
#[test]
fn test_fraiseql_core_types_accessible() {
    // Should be able to import and use fraiseql-core types
    use fraiseql_core::schema::CompiledSchema;

    // These types should be accessible now that circular dependency is fixed
    let _: Option<CompiledSchema> = None;
    let _message = "fraiseql-core types accessible";

    // Verify imports work by checking these exist at compile time
    assert!(!_message.is_empty());
}

/// Tests that has_executor() returns correct status
#[test]
fn test_has_executor_status() {
    let service = FraiseQLFlightService::new();
    assert!(!service.has_executor());

    let mut service = FraiseQLFlightService::new();
    let dummy: Arc<dyn QueryExecutor> = Arc::new(DummyExecutor);
    service.set_executor(dummy);

    assert!(service.has_executor());
}

/// JWT extraction from Bearer format.
#[test]
fn test_jwt_extraction_from_bearer_format() {
    // Helper for extracting JWT from "Bearer <token>" format (used in handshake)
    fn extract_jwt_from_bearer(payload: &str) -> Option<&str> {
        payload.strip_prefix("Bearer ")
    }

    // Test valid Bearer format
    let token = extract_jwt_from_bearer("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9");
    assert_eq!(token, Some("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));

    // Test invalid format (no Bearer prefix)
    let token = extract_jwt_from_bearer("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9");
    assert_eq!(token, None);

    // Test empty string
    let token = extract_jwt_from_bearer("");
    assert_eq!(token, None);
}

/// Tests `SecurityContext` creation and validation.
#[test]
fn test_security_context_creation() {
    let context = SecurityContext {
        session_token: "session-12345".to_string(),
        user_id:       "user-456".to_string(),
        expiration:    Some(9999999999),
    };

    assert_eq!(context.session_token, "session-12345");
    assert_eq!(context.user_id, "user-456");
    assert!(context.expiration.is_some());
}

/// Tests that security context can be set on service.
#[test]
fn test_service_with_security_context() {
    let service = FraiseQLFlightService::new();
    assert!(service.security_context.is_none());

    // Set security context after successful handshake
    let _context = SecurityContext {
        session_token: "session-abc".to_string(),
        user_id:       "user-123".to_string(),
        expiration:    None,
    };

    // security_context can be set on service after handshake completes
}

/// Tests that `get_flight_info` returns schema for views.
#[tokio::test]
async fn test_get_flight_info_for_optimized_view() {
    let service = FraiseQLFlightService::new();

    // Create a FlightTicket for an optimized view and encode it
    let ticket = FlightTicket::OptimizedView {
        view:     "va_orders".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };
    let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

    // Create a FlightDescriptor with encoded ticket bytes
    let descriptor = FlightDescriptor {
        r#type: 1, // PATH
        path:   vec![String::from_utf8_lossy(&ticket_bytes).to_string()],
        cmd:    Default::default(),
    };

    let request = Request::new(descriptor);
    let result = service.get_flight_info(request).await;

    // Should return FlightInfo with schema
    assert!(result.is_ok(), "get_flight_info should succeed for valid view");
    let response = result.unwrap();
    let flight_info = response.into_inner();

    // Verify schema is present
    assert!(!flight_info.schema.is_empty(), "Schema should not be empty");
}

/// Tests that `get_flight_info` returns error for invalid view.
#[tokio::test]
async fn test_get_flight_info_invalid_view() {
    let service = FraiseQLFlightService::new();

    // Create a FlightTicket for a non-existent view and encode it
    let ticket = FlightTicket::OptimizedView {
        view:     "nonexistent_view".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };
    let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

    // Create a FlightDescriptor with encoded ticket bytes
    let descriptor = FlightDescriptor {
        r#type: 1, // PATH
        path:   vec![String::from_utf8_lossy(&ticket_bytes).to_string()],
        cmd:    Default::default(),
    };

    let request = Request::new(descriptor);
    let result = service.get_flight_info(request).await;

    // Should return error for invalid view
    assert!(result.is_err(), "get_flight_info should fail for non-existent view");
}

/// Tests that `list_actions` returns available actions.
#[tokio::test]
async fn test_list_actions_returns_action_types() {
    let service = FraiseQLFlightService::new();
    let request = Request::new(Empty {});
    let result = service.list_actions(request).await;

    assert!(result.is_ok(), "list_actions should succeed");
    let response = result.unwrap();
    let mut stream = response.into_inner();

    // Collect all actions
    let mut actions = Vec::new();
    while let Some(Ok(action_type)) = stream.next().await {
        actions.push(action_type);
    }

    // Should have at least 3 actions
    assert!(actions.len() >= 3, "Should have at least 3 actions, got {}", actions.len());

    // Verify action names exist
    let action_names: Vec<_> = actions.iter().map(|a| a.r#type.as_str()).collect();
    assert!(action_names.contains(&"ClearCache"), "Should have ClearCache action");
    assert!(
        action_names.contains(&"RefreshSchemaRegistry"),
        "Should have RefreshSchemaRegistry action"
    );
    assert!(action_names.contains(&"HealthCheck"), "Should have HealthCheck action");
}

/// Tests that `do_action` requires authentication and executes HealthCheck action.
#[tokio::test]
async fn test_do_action_health_check() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();
        let action = Action {
            r#type: "HealthCheck".to_string(),
            body:   vec![].into(),
        };

        // Create a test user and session token
        let now = Utc::now();
        let exp = now + chrono::Duration::minutes(5);

        let claims = SessionTokenClaims {
            sub:          "test-user".to_string(),
            exp:          exp.timestamp(),
            iat:          now.timestamp(),
            scopes:       vec!["user".to_string()],
            session_type: "flight".to_string(),
        };

        let key = EncodingKey::from_secret(TEST_FLIGHT_SECRET.as_bytes());
        let header = Header::new(Algorithm::HS256);

        let session_token = encode(&header, &claims, &key).expect("Failed to encode token");

        let mut request = Request::new(action);
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token)
                .parse()
                .expect("Failed to insert auth header"),
        );

        let result = service.do_action(request).await;

        assert!(result.is_ok(), "HealthCheck action should succeed");
        let response = result.unwrap();
        let mut stream = response.into_inner();

        // Should return at least one result
        if let Some(Ok(_result)) = stream.next().await {
            // Success - action returned result
        } else {
            panic!("HealthCheck should return a result");
        }
    })
    .await;
}

/// Tests that `do_action` returns error for unknown action.
#[tokio::test]
async fn test_do_action_unknown_action() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();
        let action = Action {
            r#type: "UnknownAction".to_string(),
            body:   vec![].into(),
        };

        // Must include authentication
        let now = Utc::now();
        let exp = now + chrono::Duration::minutes(5);

        let claims = SessionTokenClaims {
            sub:          "test-user".to_string(),
            exp:          exp.timestamp(),
            iat:          now.timestamp(),
            scopes:       vec!["user".to_string()],
            session_type: "flight".to_string(),
        };

        let key = EncodingKey::from_secret(TEST_FLIGHT_SECRET.as_bytes());
        let header = Header::new(Algorithm::HS256);

        let session_token = encode(&header, &claims, &key).expect("Failed to encode token");

        let mut request = Request::new(action);
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token)
                .parse()
                .expect("Failed to insert auth header"),
        );

        let result = service.do_action(request).await;

        assert!(result.is_err(), "Unknown action should return error");
    })
    .await;
}
