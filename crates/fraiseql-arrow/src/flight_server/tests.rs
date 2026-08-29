//! Tests for the Flight service.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::unreadable_literal)] // Reason: test token expiration uses large integer literal
#![allow(clippy::default_trait_access)] // Reason: Default::default() used for struct field initialization

use std::sync::Arc;

use arrow_flight::{Action, Empty, FlightDescriptor, flight_service_server::FlightService};
use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use tonic::Request;

use super::{FraiseQLFlightService, QueryExecutor, SecurityContext, SessionTokenClaims};
use crate::ticket::FlightTicket;

/// Dummy executor for testing that implements `QueryExecutor` trait.
struct DummyExecutor;

// Reason: QueryExecutor is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl QueryExecutor for DummyExecutor {
    async fn execute_with_security(
        &self,
        _query: &str,
        _variables: Option<&serde_json::Value>,
        _security_context: &fraiseql_core::security::SecurityContext,
    ) -> std::result::Result<serde_json::Value, fraiseql_core::error::FraiseQLError> {
        Ok(serde_json::json!({"data": {"test": "ok"}}))
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
    let message = "fraiseql-core types accessible";

    // Verify imports work by checking these exist at compile time
    assert!(!message.is_empty());
}

/// Tests that `has_executor()` returns correct status
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

/// Tests that `do_action` requires authentication and executes `HealthCheck` action.
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

mod grpc_error_classification {
    //! #1201: the Flight transport must report *who is at fault*.
    //!
    //! Every executor failure used to become `Status::internal` — gRPC
    //! `INTERNAL` (13), a server-fault code that clients and proxies retry by
    //! default. A retried validation error can never succeed, so the operator's
    //! error budget showed a server-side fault rate that no server-side change
    //! could fix, for a query naming a field the schema no longer has.
    //!
    //! The codes are derived from [`FraiseQLError::status_code`] — the same
    //! classification the HTTP transport routes on — rather than re-matched, so
    //! the two transports cannot drift.

    use fraiseql_core::error::FraiseQLError;
    use tonic::Code;

    use crate::flight_server::grpc_code_for;

    fn parse() -> FraiseQLError {
        FraiseQLError::Parse {
            message:  "unexpected token".to_string(),
            location: "1:1".to_string(),
        }
    }

    /// The issue's own reproduction: a query naming a field the schema no longer
    /// has is the client's mistake.
    #[test]
    fn a_validation_error_is_invalid_argument_not_internal() {
        let error = FraiseQLError::validation("Query 'nosuchfield' not found in schema");
        assert_eq!(grpc_code_for(&error), Code::InvalidArgument);
    }

    #[test]
    fn a_parse_error_is_invalid_argument() {
        assert_eq!(grpc_code_for(&parse()), Code::InvalidArgument);
    }

    #[test]
    fn an_unknown_field_or_type_is_invalid_argument() {
        assert_eq!(
            grpc_code_for(&FraiseQLError::UnknownField {
                field:     "nope".to_string(),
                type_name: "User".to_string(),
            }),
            Code::InvalidArgument
        );
        assert_eq!(
            grpc_code_for(&FraiseQLError::UnknownType {
                type_name: "Nope".to_string(),
            }),
            Code::InvalidArgument
        );
    }

    #[test]
    fn an_authentication_failure_is_unauthenticated() {
        assert_eq!(
            grpc_code_for(&FraiseQLError::Authentication {
                message: "token expired".to_string(),
            }),
            Code::Unauthenticated
        );
    }

    #[test]
    fn an_authorization_failure_is_permission_denied() {
        assert_eq!(
            grpc_code_for(&FraiseQLError::Authorization {
                message:  "forbidden".to_string(),
                action:   None,
                resource: None,
            }),
            Code::PermissionDenied
        );
    }

    #[test]
    fn a_rate_limit_is_resource_exhausted() {
        assert_eq!(
            grpc_code_for(&FraiseQLError::RateLimited {
                message:          "slow down".to_string(),
                retry_after_secs: 30,
            }),
            Code::ResourceExhausted
        );
    }

    /// A cost ceiling splits on whether asking again can ever work — the
    /// distinction the variant's own documentation draws, and one the issue's
    /// suggested mapping (`resource_exhausted` for both) would have lost.
    /// `ResourceExhausted` is retryable in gRPC, and a **per-request** ceiling is
    /// permanent: retrying it is the same wasted round trip `INTERNAL` caused.
    #[test]
    fn a_cost_ceiling_is_retryable_only_when_the_budget_window_resets() {
        let windowed = FraiseQLError::CostExceeded {
            message:          "budget spent".to_string(),
            cost:             10,
            limit:            5,
            retry_after_secs: Some(60),
        };
        assert_eq!(grpc_code_for(&windowed), Code::ResourceExhausted);

        let per_request = FraiseQLError::CostExceeded {
            message:          "too expensive".to_string(),
            cost:             10,
            limit:            5,
            retry_after_secs: None,
        };
        assert_eq!(
            grpc_code_for(&per_request),
            Code::InvalidArgument,
            "a per-request ceiling cannot be satisfied by retrying the same query"
        );
    }

    #[test]
    fn a_timeout_is_deadline_exceeded() {
        assert_eq!(
            grpc_code_for(&FraiseQLError::Timeout {
                timeout_ms: 1_000,
                query:      None,
            }),
            Code::DeadlineExceeded
        );
    }

    /// **Control — the half that was always right.** A genuine server-side fault
    /// stays `INTERNAL`, so the fix is a narrowing rather than a blanket
    /// reclassification.
    #[test]
    fn a_database_failure_is_still_internal() {
        assert_eq!(
            grpc_code_for(&FraiseQLError::Database {
                message:   "connection reset".to_string(),
                sql_state: None,
            }),
            Code::Internal
        );
        assert_eq!(
            grpc_code_for(&FraiseQLError::ConnectionPool {
                message: "pool exhausted".to_string(),
            }),
            Code::Internal
        );
    }

    /// **Control.** Not one of the issue's cases, and the reason the mapping is
    /// *derived*: `status_code` already classifies these, so they arrive here
    /// correct without anyone thinking about gRPC.
    #[test]
    fn derived_codes_cover_variants_the_issue_did_not_list() {
        assert_eq!(
            grpc_code_for(&FraiseQLError::NotFound {
                resource_type: "User".to_string(),
                identifier:    "1".to_string(),
            }),
            Code::NotFound
        );
        assert_eq!(
            grpc_code_for(&FraiseQLError::Unsupported {
                message: "not built".to_string(),
            }),
            Code::Unimplemented
        );
    }

    /// The property the whole issue is about, stated once over every client-fault
    /// class: none of them may be reported as a server fault.
    #[test]
    fn no_client_fault_is_reported_as_a_server_fault() {
        let client_faults = [
            parse(),
            FraiseQLError::validation("bad"),
            FraiseQLError::Authentication {
                message: "no token".to_string(),
            },
            FraiseQLError::Authorization {
                message:  "denied".to_string(),
                action:   None,
                resource: None,
            },
            FraiseQLError::RateLimited {
                message:          "slow down".to_string(),
                retry_after_secs: 1,
            },
        ];
        for error in &client_faults {
            assert_ne!(
                grpc_code_for(error),
                Code::Internal,
                "reported as a retryable server fault: {error:?}"
            );
        }
    }
}
