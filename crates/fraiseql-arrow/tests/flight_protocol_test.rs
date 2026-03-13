//! Protocol-level tests for the Arrow Flight service that run without a live database.
//!
//! These tests verify:
//! - `list_flights` enumerates the registered default views
//! - `get_schema` returns correct schemas and informative errors
//! - `get_flight_info` returns consistent errors for unsupported ticket types
//! - `poll_flight_info` names the target version in its error message
//! - `do_get` streams placeholder data when no database adapter is configured
//! - `list_actions` advertises all four documented admin actions
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::default_trait_access)] // Reason: test code uses Default::default() for struct field initialization
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers

use std::{collections::HashMap, sync::Arc};

use arrow_flight::{
    Criteria, Empty, FlightDescriptor, Ticket,
    flight_service_server::FlightService as _,
};
use async_trait::async_trait;
use chrono::Utc;
use fraiseql_arrow::{DatabaseAdapter, DatabaseResult, FlightTicket, FraiseQLFlightService};
use futures::StreamExt as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use tonic::Request;

// ---------------------------------------------------------------------------
// Mock database adapter (no live DB required)
// ---------------------------------------------------------------------------

/// Minimal mock adapter that returns two hard-coded rows for any query.
///
/// Rows match the `ta_users` schema (`id`, `email`, `name`, `created_at` — all Utf8)
/// to avoid unsupported Arrow type conversions (e.g. `Timestamp(Microsecond, UTC)`).
struct MockAdapter;

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for MockAdapter {
    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![
            [
                ("id".to_string(), serde_json::json!("1")),
                ("email".to_string(), serde_json::json!("alice@example.com")),
                ("name".to_string(), serde_json::json!("Alice")),
                ("created_at".to_string(), serde_json::json!("2024-01-01T00:00:00Z")),
            ]
            .into_iter()
            .collect(),
            [
                ("id".to_string(), serde_json::json!("2")),
                ("email".to_string(), serde_json::json!("bob@example.com")),
                ("name".to_string(), serde_json::json!("Bob")),
                ("created_at".to_string(), serde_json::json!("2024-01-02T00:00:00Z")),
            ]
            .into_iter()
            .collect(),
        ])
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const TEST_SECRET: &str = "flight-protocol-test-secret-32-x";

/// JWT claims matching the shape expected by `validate_session_token`.
#[derive(Serialize)]
struct TestClaims {
    sub:          String,
    exp:          i64,
    iat:          i64,
    scopes:       Vec<String>,
    session_type: String,
}

/// Create a valid HS256 session token signed with the test secret.
fn make_session_token() -> String {
    let now = Utc::now();
    let exp = now + chrono::Duration::minutes(5);
    let claims = TestClaims {
        sub:          "protocol-test-user".to_string(),
        exp:          exp.timestamp(),
        iat:          now.timestamp(),
        scopes:       vec!["user".to_string()],
        session_type: "flight".to_string(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
    )
    .expect("Failed to encode test session token")
}

/// Encode a `FlightTicket` into a `FlightDescriptor` path entry.
fn descriptor_for_ticket(ticket: &FlightTicket) -> FlightDescriptor {
    let bytes = ticket.encode().expect("Failed to encode ticket");
    FlightDescriptor {
        r#type: 1, // PATH
        path:   vec![String::from_utf8_lossy(&bytes).to_string()],
        cmd:    Default::default(),
    }
}

/// Build a `Request<Ticket>` with a valid session token in the Authorization header.
fn authenticated_ticket_request(ticket: &FlightTicket) -> Request<Ticket> {
    let bytes = ticket.encode().expect("Failed to encode ticket");
    let mut req = Request::new(Ticket { ticket: bytes.into() });
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", make_session_token())
            .parse()
            .expect("Failed to set auth header"),
    );
    req
}

// ---------------------------------------------------------------------------
// Tests: list_flights
// ---------------------------------------------------------------------------

/// `list_flights` enumerates the four built-in views without authentication.
#[tokio::test]
async fn test_list_flights_enumerates_four_default_views() {
    let service = FraiseQLFlightService::new();
    let result =
        service.list_flights(Request::new(Criteria { expression: vec![].into() })).await;

    assert!(result.is_ok(), "list_flights should succeed without auth");
    let mut stream = result.unwrap().into_inner();

    let mut names: Vec<String> = Vec::new();
    while let Some(Ok(info)) = stream.next().await {
        if let Some(desc) = &info.flight_descriptor {
            if let Some(path) = desc.path.first() {
                names.push(path.clone());
            }
        }
    }

    assert!(names.contains(&"va_orders".to_string()), "va_orders must be listed");
    assert!(names.contains(&"va_users".to_string()), "va_users must be listed");
    assert!(names.contains(&"ta_orders".to_string()), "ta_orders must be listed");
    assert!(names.contains(&"ta_users".to_string()), "ta_users must be listed");
    assert_eq!(names.len(), 4, "Exactly 4 default views expected, got: {:?}", names);
}

// ---------------------------------------------------------------------------
// Tests: get_schema
// ---------------------------------------------------------------------------

/// `get_schema` returns a non-empty IPC schema for a `GraphQLQuery` ticket.
#[tokio::test]
async fn test_get_schema_for_graphql_query_returns_schema() {
    let service = FraiseQLFlightService::new();
    let ticket = FlightTicket::GraphQLQuery {
        query:     "{ users { id name } }".to_string(),
        variables: None,
    };
    let result = service.get_schema(Request::new(descriptor_for_ticket(&ticket))).await;

    assert!(result.is_ok(), "get_schema for GraphQLQuery must succeed");
    let schema_result = result.unwrap().into_inner();
    assert!(!schema_result.schema.is_empty(), "Schema bytes must be non-empty");
}

/// `get_schema` for an `OptimizedView` ticket returns the registered Arrow schema.
#[tokio::test]
async fn test_get_schema_for_optimized_view_returns_schema() {
    let service = FraiseQLFlightService::new();
    let ticket = FlightTicket::OptimizedView {
        view:     "va_orders".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };
    let result = service.get_schema(Request::new(descriptor_for_ticket(&ticket))).await;
    assert!(result.is_ok(), "get_schema for va_orders must succeed");
    assert!(!result.unwrap().into_inner().schema.is_empty());
}

/// `get_schema` for a `BulkExport` ticket returns `Unimplemented` naming `do_get`.
#[tokio::test]
async fn test_get_schema_bulk_export_returns_informative_error() {
    let service = FraiseQLFlightService::new();
    let ticket = FlightTicket::BulkExport {
        table:  "ta_orders".to_string(),
        filter: None,
        limit:  None,
        format: None,
    };
    let result = service.get_schema(Request::new(descriptor_for_ticket(&ticket))).await;
    assert!(result.is_err(), "Must fail for BulkExport");

    let err = result.err().unwrap();
    assert_eq!(err.code(), tonic::Code::Unimplemented);
    assert!(
        err.message().contains("do_get"),
        "Error must mention do_get as the alternative; got: {:?}",
        err.message()
    );
}

// ---------------------------------------------------------------------------
// Tests: get_flight_info
// ---------------------------------------------------------------------------

/// `get_flight_info` for `BulkExport` must return the exact same message as `get_schema`.
#[tokio::test]
async fn test_get_flight_info_bulk_export_message_matches_get_schema() {
    let service = FraiseQLFlightService::new();
    let ticket = FlightTicket::BulkExport {
        table:  "ta_orders".to_string(),
        filter: None,
        limit:  None,
        format: None,
    };

    let get_schema_err = service
        .get_schema(Request::new(descriptor_for_ticket(&ticket)))
        .await
        .expect_err("get_schema must fail for BulkExport");

    let get_flight_info_err = service
        .get_flight_info(Request::new(descriptor_for_ticket(&ticket)))
        .await
        .expect_err("get_flight_info must fail for BulkExport");

    assert_eq!(
        get_schema_err.message(),
        get_flight_info_err.message(),
        "BulkExport error message must be identical in get_schema and get_flight_info"
    );
}

// ---------------------------------------------------------------------------
// Tests: poll_flight_info
// ---------------------------------------------------------------------------

/// `poll_flight_info` for a known `OptimizedView` ticket must return a completed
/// `PollInfo`: `flight_descriptor = None` (signals done) and `progress = 1.0`.
#[tokio::test]
async fn test_poll_flight_info_returns_completed_poll_info() {
    let service = FraiseQLFlightService::new();
    let ticket = FlightTicket::OptimizedView {
        view:     "va_orders".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };

    let poll_info = service
        .poll_flight_info(Request::new(descriptor_for_ticket(&ticket)))
        .await
        .expect("poll_flight_info must succeed for a known view")
        .into_inner();

    // flight_descriptor = None means "complete, no further polling needed"
    assert!(
        poll_info.flight_descriptor.is_none(),
        "flight_descriptor must be None to signal completion"
    );
    // progress = 1.0 confirms 100 % complete
    assert_eq!(
        poll_info.progress,
        Some(1.0),
        "progress must be 1.0 for a synchronous response"
    );
    // info must be populated with the FlightInfo
    assert!(poll_info.info.is_some(), "PollInfo.info must be populated");
    let info = poll_info.info.unwrap();
    assert!(!info.schema.is_empty(), "FlightInfo schema must be non-empty");
}

/// `poll_flight_info` for an unknown view must propagate `NotFound`.
#[tokio::test]
async fn test_poll_flight_info_unknown_view_returns_not_found() {
    let service = FraiseQLFlightService::new();
    let ticket = FlightTicket::OptimizedView {
        view:     "no_such_view".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };

    let err = service
        .poll_flight_info(Request::new(descriptor_for_ticket(&ticket)))
        .await
        .expect_err("poll_flight_info must fail for unknown view");

    assert_eq!(err.code(), tonic::Code::NotFound);
}

// ---------------------------------------------------------------------------
// Tests: list_actions
// ---------------------------------------------------------------------------

/// `list_actions` advertises all four documented admin actions.
#[tokio::test]
async fn test_list_actions_includes_all_admin_actions() {
    let service = FraiseQLFlightService::new();
    let mut stream =
        service.list_actions(Request::new(Empty {})).await.unwrap().into_inner();

    let mut types: Vec<String> = Vec::new();
    while let Some(Ok(action_type)) = stream.next().await {
        types.push(action_type.r#type.clone());
    }

    assert!(types.contains(&"HealthCheck".to_string()), "HealthCheck must be listed");
    assert!(types.contains(&"ClearCache".to_string()), "ClearCache must be listed");
    assert!(
        types.contains(&"RefreshSchemaRegistry".to_string()),
        "RefreshSchemaRegistry must be listed"
    );
    assert!(
        types.contains(&"GetSchemaVersions".to_string()),
        "GetSchemaVersions must be listed"
    );
}

// ---------------------------------------------------------------------------
// Tests: do_get — placeholder path (no database)
// ---------------------------------------------------------------------------

/// `do_get` with an `OptimizedView` ticket must stream Arrow data when a
/// database adapter is configured.  Uses `MockAdapter` so no live DB is needed.
/// Targets `ta_users` (all-Utf8 schema) to avoid unsupported timestamp conversion.
#[tokio::test]
async fn test_do_get_optimized_view_streams_arrow_data() {
    let service = FraiseQLFlightService::new_with_db(Arc::new(MockAdapter))
        .with_session_secret(TEST_SECRET);
    let ticket = FlightTicket::OptimizedView {
        view:     "ta_users".to_string(),
        filter:   None,
        order_by: None,
        limit:    Some(5),
        offset:   None,
    };

    let result = service.do_get(authenticated_ticket_request(&ticket)).await;
    assert!(
        result.is_ok(),
        "do_get for OptimizedView must succeed with a DB adapter; err: {:?}",
        result.as_ref().err()
    );

    let mut stream = result.unwrap().into_inner();
    // First message must be the Arrow schema
    let first = stream.next().await;
    assert!(first.is_some(), "Stream must produce at least the schema message");
    assert!(first.unwrap().is_ok(), "Schema message must be Ok");
}

/// `do_get` with an unknown view name must return `Status::not_found`.
#[tokio::test]
async fn test_do_get_unknown_view_returns_not_found() {
    let service = FraiseQLFlightService::new().with_session_secret(TEST_SECRET);
    let ticket = FlightTicket::OptimizedView {
        view:     "does_not_exist".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };

    let result = service.do_get(authenticated_ticket_request(&ticket)).await;
    let Err(err) = result else {
        panic!("Unknown view must fail");
    };

    assert_eq!(
        err.code(),
        tonic::Code::NotFound,
        "Unknown view must return NotFound; got: {:?}",
        err
    );
}

/// `do_get` without an Authorization header must return an authentication error.
#[tokio::test]
async fn test_do_get_without_auth_returns_error() {
    let service = FraiseQLFlightService::new().with_session_secret(TEST_SECRET);
    let ticket = FlightTicket::OptimizedView {
        view:     "va_orders".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };

    let bytes = ticket.encode().expect("Failed to encode ticket");
    // Request with no Authorization header
    let req = Request::new(Ticket { ticket: bytes.into() });

    let Err(err) = service.do_get(req).await else {
        panic!("Missing auth must fail");
    };
    assert!(
        matches!(err.code(), tonic::Code::Unauthenticated | tonic::Code::InvalidArgument),
        "Missing auth must fail with Unauthenticated or InvalidArgument; got {:?}: {}",
        err.code(),
        err.message()
    );
}
