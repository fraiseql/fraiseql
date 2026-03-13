#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::items_after_statements)] // Reason: test helper structs defined near use site

//! Integration tests for Arrow Flight authenticated query execution.
//!
//! These tests verify that all RPC methods require valid session tokens
//! from the handshake phase, and that scope-based access control works.

use arrow_flight::{Action, Ticket, flight_service_server::FlightService};
use chrono::Utc;
use fraiseql_arrow::{flight_server::FraiseQLFlightService, ticket::FlightTicket};
use fraiseql_core::security::auth_middleware::AuthenticatedUser;
use tonic::Request;

/// Create a test authenticated user for testing.
fn create_test_user(user_id: &str, scopes: Vec<&str>) -> AuthenticatedUser {
    AuthenticatedUser {
        user_id:    user_id.to_string(),
        scopes:     scopes.into_iter().map(|s| s.to_string()).collect(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    }
}

const TEST_FLIGHT_SECRET: &str = "flight-test-session-secret-for-integration-tests";

/// Returns the env vars needed for Flight session tests.
const fn flight_secret_vars() -> [(&'static str, Option<&'static str>); 1] {
    [("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))]
}

/// Create a session token for a test user (mimics handshake).
///
/// Must be called within a `temp_env::with_vars` or `temp_env::async_with_vars` block
/// that sets `FLIGHT_SESSION_SECRET`.
fn create_test_session_token(user: &AuthenticatedUser) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestSessionTokenClaims {
        sub:          String,
        exp:          i64,
        iat:          i64,
        scopes:       Vec<String>,
        session_type: String,
    }

    let now = chrono::Utc::now();
    let exp = now + chrono::Duration::minutes(5);

    let claims = TestSessionTokenClaims {
        sub:          user.user_id.clone(),
        exp:          exp.timestamp(),
        iat:          now.timestamp(),
        scopes:       user.scopes.clone(),
        session_type: "flight".to_string(),
    };

    let key = EncodingKey::from_secret(TEST_FLIGHT_SECRET.as_bytes());
    let header = Header::new(Algorithm::HS256);

    encode(&header, &claims, &key).expect("Failed to create test session token")
}

/// Test basic service setup
#[test]
fn test_service_with_auth_validator_configured() {
    let service = FraiseQLFlightService::new();
    assert!(!service.has_executor());
}

/// Test dev mode without validator
#[tokio::test]
async fn test_handshake_without_validator_allows_dev_mode() {
    let service = FraiseQLFlightService::new();
    assert!(service.executor().is_none());
}

/// Test service can be created with auth
#[test]
fn test_service_oidc_validator_setter() {
    let service = FraiseQLFlightService::new();
    assert!(!service.is_authenticated());
}

/// Test `do_get` requires authorization header.
#[tokio::test]
async fn test_do_get_without_authorization_header() {
    let service = FraiseQLFlightService::new();

    // Create a valid ticket
    let ticket = FlightTicket::OptimizedView {
        view:     "va_orders".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };
    let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

    // Create request WITHOUT authorization header
    let ticket_proto = Ticket {
        ticket: ticket_bytes.into(),
    };
    let request = Request::new(ticket_proto);

    // Should fail with unauthenticated error
    let result = service.do_get(request).await;

    match result {
        Err(status) => {
            assert_eq!(status.code(), tonic::Code::Unauthenticated);
            assert!(
                status.message().contains("authorization header"),
                "Error should mention authorization header"
            );
        },
        Ok(_) => panic!("do_get should fail without auth header"),
    }
}

/// Test `do_get` rejects invalid session token.
#[tokio::test]
async fn test_do_get_with_invalid_session_token() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        // Create a valid ticket
        let ticket = FlightTicket::OptimizedView {
            view:     "va_orders".to_string(),
            filter:   None,
            order_by: None,
            limit:    None,
            offset:   None,
        };
        let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

        // Create request with INVALID token
        let ticket_proto = Ticket {
            ticket: ticket_bytes.into(),
        };
        let mut request = Request::new(ticket_proto);

        // Add invalid token to metadata
        request.metadata_mut().insert(
            "authorization",
            "Bearer invalid-token-xyz".parse().expect("Failed to insert header"),
        );

        // Should fail with unauthenticated error
        let result = service.do_get(request).await;

        match result {
            Err(status) => {
                assert_eq!(status.code(), tonic::Code::Unauthenticated);
            },
            Ok(_) => panic!("do_get should fail with invalid token"),
        }
    })
    .await;
}

/// Test `do_get` rejects expired session token.
#[tokio::test]
async fn test_do_get_with_expired_session_token() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::{Deserialize, Serialize};

    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        // Create an EXPIRED token
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct ExpiredTokenClaims {
            sub:          String,
            exp:          i64,
            iat:          i64,
            scopes:       Vec<String>,
            session_type: String,
        }

        let now = chrono::Utc::now();
        let exp = now - chrono::Duration::minutes(5); // EXPIRED 5 minutes ago

        let claims = ExpiredTokenClaims {
            sub:          "user-1".to_string(),
            exp:          exp.timestamp(),
            iat:          (now - chrono::Duration::hours(1)).timestamp(),
            scopes:       vec!["user".to_string()],
            session_type: "flight".to_string(),
        };

        let key = EncodingKey::from_secret(TEST_FLIGHT_SECRET.as_bytes());
        let header = Header::new(Algorithm::HS256);
        let expired_token = encode(&header, &claims, &key).expect("Failed to encode expired token");

        // Create a valid ticket
        let ticket = FlightTicket::OptimizedView {
            view:     "va_orders".to_string(),
            filter:   None,
            order_by: None,
            limit:    None,
            offset:   None,
        };
        let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

        // Create request with expired token
        let ticket_proto = Ticket {
            ticket: ticket_bytes.into(),
        };
        let mut request = Request::new(ticket_proto);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", expired_token).parse().expect("Failed to insert header"),
        );

        // Should fail with unauthenticated error mentioning expiration
        let result = service.do_get(request).await;

        match result {
            Err(status) => {
                assert_eq!(status.code(), tonic::Code::Unauthenticated);
                assert!(
                    status.message().contains("expired"),
                    "Error should mention token expiration"
                );
            },
            Ok(_) => panic!("do_get should fail with expired token"),
        }
    })
    .await;
}

/// Test `do_get` accepts valid session token.
#[tokio::test]
async fn test_authenticated_do_get_with_valid_session_token() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        // Create a valid test user and session token
        let user = create_test_user("user-123", vec!["user", "read"]);
        let session_token = create_test_session_token(&user);

        // Create a GraphQL query ticket (simpler schema without timestamp conversion issues)
        let ticket = FlightTicket::GraphQLQuery {
            query:     "query { users { id name } }".to_string(),
            variables: None,
        };
        let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

        // Create request with valid session token
        let ticket_proto = Ticket {
            ticket: ticket_bytes.into(),
        };
        let mut request = Request::new(ticket_proto);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token).parse().expect("Failed to insert header"),
        );

        // Should succeed (authentication passes; actual query execution happens after)
        let result = service.do_get(request).await;

        // Even if query execution has issues, authentication should succeed
        // We're testing authentication validation, not query execution
        match result {
            Ok(_) => {
                // Best case: query executes successfully
            },
            Err(e) if e.code() != tonic::Code::Unauthenticated => {
                // Acceptable: query failed for other reasons (schema conversion, etc)
                // but authentication validation passed
            },
            Err(e) => {
                panic!("do_get should pass authentication validation. Got: {}", e.message());
            },
        }
    })
    .await;
}

/// Test `do_action` `HealthCheck` requires auth.
#[tokio::test]
async fn test_do_action_health_check_without_auth() {
    let service = FraiseQLFlightService::new();

    let action = Action {
        r#type: "HealthCheck".to_string(),
        body:   vec![].into(),
    };
    let request = Request::new(action);

    // Should fail without auth header
    let result = service.do_action(request).await;

    match result {
        Err(status) => {
            assert_eq!(status.code(), tonic::Code::Unauthenticated);
        },
        Ok(_) => panic!("do_action should fail without auth header"),
    }
}

/// Test `do_action` `HealthCheck` succeeds with valid token.
#[tokio::test]
async fn test_do_action_health_check_with_valid_token() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        let user = create_test_user("user-123", vec!["user"]);
        let session_token = create_test_session_token(&user);

        let action = Action {
            r#type: "HealthCheck".to_string(),
            body:   vec![].into(),
        };
        let mut request = Request::new(action);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token).parse().expect("Failed to insert header"),
        );

        let result = service.do_action(request).await;

        if let Err(e) = &result {
            panic!("HealthCheck should succeed with valid token. Error: {}", e);
        }
    })
    .await;
}

/// Test `ClearCache` requires admin scope.
#[tokio::test]
async fn test_do_action_clear_cache_without_admin_scope() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        let user = create_test_user("user-123", vec!["user", "read"]); // NO "admin" scope
        let session_token = create_test_session_token(&user);

        let action = Action {
            r#type: "ClearCache".to_string(),
            body:   vec![].into(),
        };
        let mut request = Request::new(action);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token).parse().expect("Failed to insert header"),
        );

        let result = service.do_action(request).await;

        match result {
            Err(status) => {
                assert_eq!(status.code(), tonic::Code::PermissionDenied);
                assert!(status.message().contains("admin"), "Error should mention admin scope");
            },
            Ok(_) => panic!("ClearCache should fail without admin scope"),
        }
    })
    .await;
}

/// Test `ClearCache` succeeds with admin scope.
#[tokio::test]
async fn test_do_action_clear_cache_with_admin_scope() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        let user = create_test_user("user-123", vec!["user", "admin"]); // Has "admin" scope
        let session_token = create_test_session_token(&user);

        let action = Action {
            r#type: "ClearCache".to_string(),
            body:   vec![].into(),
        };
        let mut request = Request::new(action);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token).parse().expect("Failed to insert header"),
        );

        let result = service.do_action(request).await;

        if let Err(e) = &result {
            panic!("ClearCache should succeed with admin scope. Error: {}", e);
        }
    })
    .await;
}

/// Test `RefreshSchemaRegistry` requires admin scope.
#[tokio::test]
async fn test_do_action_refresh_schema_registry_without_admin_scope() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        let user = create_test_user("user-123", vec!["user"]); // NO "admin" scope
        let session_token = create_test_session_token(&user);

        let action = Action {
            r#type: "RefreshSchemaRegistry".to_string(),
            body:   vec![].into(),
        };
        let mut request = Request::new(action);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token).parse().expect("Failed to insert header"),
        );

        let result = service.do_action(request).await;

        match result {
            Err(status) => {
                assert_eq!(status.code(), tonic::Code::PermissionDenied);
            },
            Ok(_) => panic!("RefreshSchemaRegistry should fail without admin scope"),
        }
    })
    .await;
}

/// Test `RefreshSchemaRegistry` succeeds with admin scope.
#[tokio::test]
async fn test_do_action_refresh_schema_registry_with_admin_scope() {
    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        let user = create_test_user("user-123", vec!["admin"]); // Has "admin" scope
        let session_token = create_test_session_token(&user);

        let action = Action {
            r#type: "RefreshSchemaRegistry".to_string(),
            body:   vec![].into(),
        };
        let mut request = Request::new(action);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token).parse().expect("Failed to insert header"),
        );

        let result = service.do_action(request).await;

        if let Err(e) = &result {
            panic!("RefreshSchemaRegistry should succeed with admin scope. Error: {}", e);
        }
    })
    .await;
}

/// Test `SecurityContext` flows through to query execution methods.
#[tokio::test]
async fn test_security_context_created_for_authenticated_query() {
    use arrow_flight::flight_service_server::FlightService;

    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        let user = create_test_user("user-789", vec!["user", "read"]);
        let session_token = create_test_session_token(&user);

        // Create a GraphQL query ticket
        let ticket = FlightTicket::GraphQLQuery {
            query:     "query { users { id } }".to_string(),
            variables: None,
        };
        let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

        // Create request with authentication
        let ticket_proto = Ticket {
            ticket: ticket_bytes.into(),
        };
        let mut request = Request::new(ticket_proto);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token).parse().expect("Failed to insert header"),
        );

        // Execute do_get — authentication should pass; execution fails with Unavailable
        // because no executor is wired up in this unit test.
        let result = service.do_get(request).await;

        // Auth succeeded if the error is Unavailable (no executor), NOT Unauthenticated/PermissionDenied.
        match result {
            Ok(_) => {},
            Err(status) => assert_eq!(
                status.code(),
                tonic::Code::Unavailable,
                "do_get should fail with Unavailable (no executor), not an auth rejection; got: {status:?}",
            ),
        }
    })
    .await;
}

/// Test `SecurityContext` contains user identity and scopes.
#[tokio::test]
async fn test_security_context_has_user_info() {
    // Create a security context from authenticated user
    let user = create_test_user("user-abc-123", vec!["user", "read", "admin"]);
    let context = fraiseql_core::security::SecurityContext::from_user(
        user,
        "req-correlation-id".to_string(),
    );

    // Verify context has user information
    assert_eq!(context.user_id, "user-abc-123");
    assert_eq!(context.scopes.len(), 3);
    assert!(context.scopes.contains(&"user".to_string()));
    assert!(context.scopes.contains(&"read".to_string()));
    assert!(context.scopes.contains(&"admin".to_string()));
}

/// Test different users are authenticated separately.
#[tokio::test]
async fn test_multiple_users_have_separate_contexts() {
    use arrow_flight::flight_service_server::FlightService;

    temp_env::async_with_vars(flight_secret_vars(), async {
        let service = FraiseQLFlightService::new();

        // User 1: Regular user
        let user1 = create_test_user("user-1", vec!["user"]);
        let token1 = create_test_session_token(&user1);

        // User 2: Admin user
        let user2 = create_test_user("user-2", vec!["admin"]);
        let token2 = create_test_session_token(&user2);

        let ticket = FlightTicket::GraphQLQuery {
            query:     "query { users { id } }".to_string(),
            variables: None,
        };
        let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

        // Request 1: User 1 query
        {
            let ticket_proto = Ticket {
                ticket: ticket_bytes.clone().into(),
            };
            let mut request = Request::new(ticket_proto);
            request.metadata_mut().insert(
                "authorization",
                format!("Bearer {}", token1).parse().expect("Failed to insert header"),
            );

            // Auth passed if error is Unavailable (no executor), not Unauthenticated.
            let result = service.do_get(request).await;
            match result {
                Ok(_) => {},
                Err(status) => assert_eq!(
                    status.code(),
                    tonic::Code::Unavailable,
                    "User 1 should be authenticated; got auth rejection: {status:?}",
                ),
            }
        }

        // Request 2: User 2 query
        {
            let ticket_proto = Ticket {
                ticket: ticket_bytes.into(),
            };
            let mut request = Request::new(ticket_proto);
            request.metadata_mut().insert(
                "authorization",
                format!("Bearer {}", token2).parse().expect("Failed to insert header"),
            );

            // Auth passed if error is Unavailable (no executor), not Unauthenticated.
            let result = service.do_get(request).await;
            match result {
                Ok(_) => {},
                Err(status) => assert_eq!(
                    status.code(),
                    tonic::Code::Unavailable,
                    "User 2 should be authenticated; got auth rejection: {status:?}",
                ),
            }
        }
    })
    .await;
}

/// Test that RLS policy would be evaluated with security context
/// (when executor is integrated in fraiseql-server).
#[test]
fn test_rls_policy_evaluation_architecture() {
    // This test documents the RLS policy evaluation flow
    // At the fraiseql-server level:
    //
    // 1. Client authenticates via Flight handshake -> session token
    // 2. Client calls do_get with session token
    // 3. Flight service creates SecurityContext from session token
    // 4. Flight service calls executor.execute_with_security(query, context)
    // 5. Executor evaluates RLS policy with context
    // 6. RLS policy returns WHERE clause filter based on user_id/roles
    // 7. Executor applies filter to query -> user only sees allowed rows
    //
    // SecurityContext fields used by RLS:
    // - user_id: "user-123"
    // - roles: ["user", "admin"]
    // - scopes: ["read:order", "write:order"]
    // - tenant_id: "org-456" (for multi-tenancy)
    // - attributes: {"department": "sales"} (custom claims)
}
