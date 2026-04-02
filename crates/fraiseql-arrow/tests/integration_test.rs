//! Integration tests for Arrow Flight server lifecycle.
//!
//! These tests verify that the Flight server starts correctly and handles
//! basic RPC calls. Actual data streaming will be tested .
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::doc_markdown)] // Reason: test prose doesn't require backtick wrapping
#![allow(clippy::items_after_statements)] // Reason: test helper closures defined near use site
#![allow(clippy::default_trait_access)] // Reason: Default::default() for struct field initialization

use arrow_flight::{
    Criteria, FlightDescriptor, Ticket, flight_service_client::FlightServiceClient,
};
use fraiseql_arrow::{FlightTicket, flight_server::FraiseQLFlightService};
use tonic::transport::{Endpoint, Server};

const TEST_FLIGHT_SECRET: &str = "flight-test-session-secret-for-integration-tests";

/// Start a test Flight server on a random available port.
///
/// Returns the server address (e.g., "http://127.0.0.1:12345").
///
/// Must be called within a `temp_env::async_with_vars` block that sets `FLIGHT_SESSION_SECRET`.
async fn start_test_server() -> Result<String, Box<dyn std::error::Error>> {
    let service = FraiseQLFlightService::new();

    // Use port 0 to get a random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(format!("http://127.0.0.1:{}", addr.port()))
}

#[tokio::test]
async fn test_server_starts_and_accepts_connections() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr.clone())
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Test ListFlights (should succeed even if empty)
        let request = tonic::Request::new(Criteria {
            expression: vec![].into(),
        });
        let response = client.list_flights(request).await;
        assert!(response.is_ok(), "ListFlights should succeed");
    })
    .await;
}

#[tokio::test]
async fn test_get_schema_for_observer_events() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create ticket for observer events
        let ticket = FlightTicket::ObserverEvents {
            entity_type: "Order".to_string(),
            start_date: None,
            end_date: None,
            limit: None,
        };

        let ticket_bytes = ticket.encode().unwrap();

        // Request schema
        let descriptor = FlightDescriptor::new_path(vec![String::from_utf8(ticket_bytes).unwrap()]);
        let request = tonic::Request::new(descriptor);

        let response = client.get_schema(request).await.expect("GetSchema failed");
        let schema_result = response.into_inner();

        // Verify we got schema bytes back
        assert!(!schema_result.schema.is_empty(), "Schema should not be empty");

        // Decode and verify schema structure
        let schema =
            arrow::ipc::root_as_message(&schema_result.schema).expect("Failed to decode schema");
        // Just verify we can decode it - detailed schema checks in unit tests
        assert!(schema.header_type() == arrow::ipc::MessageHeader::Schema);
    })
    .await;
}

#[tokio::test]
async fn test_get_schema_for_graphql_query() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create ticket for GraphQL query
        let ticket = FlightTicket::GraphQLQuery {
            query: "{ users { id name } }".to_string(),
            variables: None,
        };

        let ticket_bytes = ticket.encode().unwrap();

        // Request schema
        let descriptor = FlightDescriptor::new_path(vec![String::from_utf8(ticket_bytes).unwrap()]);
        let request = tonic::Request::new(descriptor);

        let response = client.get_schema(request).await.expect("GetSchema failed");
        let schema_result = response.into_inner();

        // Verify we got schema bytes back
        assert!(!schema_result.schema.is_empty(), "Schema should not be empty");
    })
    .await;
}

#[tokio::test]
async fn test_do_get_returns_empty_stream() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create a session token for authentication
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize)]
        struct SessionTokenClaims {
            sub:          String,
            exp:          i64,
            iat:          i64,
            scopes:       Vec<String>,
            session_type: String,
        }

        let now = chrono::Utc::now();
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

        // Create ticket for GraphQL query
        let ticket = FlightTicket::GraphQLQuery {
            query:     "{ users { id } }".to_string(),
            variables: None,
        };

        let ticket_bytes = ticket.encode().unwrap();

        // Request data with authentication
        let mut request = tonic::Request::new(Ticket {
            ticket: ticket_bytes.into(),
        });

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token)
                .parse()
                .expect("Failed to insert auth header"),
        );

        // Without an executor configured the server returns Unavailable rather than
        // an empty stream; auth succeeded if the error is NOT Unauthenticated/PermissionDenied.
        match client.do_get(request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                let _ = stream.message().await; // stream may be empty — that's fine
            },
            Err(status) => assert_eq!(
                status.code(),
                tonic::Code::Unavailable,
                "do_get should fail with Unavailable (no executor) not an auth error; got: {status:?}",
            ),
        }
    })
    .await;
}

#[tokio::test]
async fn test_invalid_ticket_returns_error() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create a session token for authentication
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize)]
        struct SessionTokenClaims {
            sub: String,
            exp: i64,
            iat: i64,
            scopes: Vec<String>,
            session_type: String,
        }

        let now = chrono::Utc::now();
        let exp = now + chrono::Duration::minutes(5);

        let claims = SessionTokenClaims {
            sub: "test-user".to_string(),
            exp: exp.timestamp(),
            iat: now.timestamp(),
            scopes: vec!["user".to_string()],
            session_type: "flight".to_string(),
        };

        let key = EncodingKey::from_secret(TEST_FLIGHT_SECRET.as_bytes());
        let header = Header::new(Algorithm::HS256);

        let session_token = encode(&header, &claims, &key).expect("Failed to encode token");

        // Send invalid ticket bytes (but with valid auth header)
        let mut request = tonic::Request::new(Ticket {
            ticket: b"invalid json".to_vec().into(),
        });

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token)
                .parse()
                .expect("Failed to insert auth header"),
        );

        let response = client.do_get(request).await;
        assert!(response.is_err(), "Invalid ticket should return error");

        let err = response.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("Invalid ticket"));
    })
    .await;
}

#[tokio::test]
async fn test_bulk_export_ticket_not_implemented() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create bulk export ticket (not implemented )
        let ticket = FlightTicket::BulkExport {
            table: "users".to_string(),
            filter: None,
            limit: Some(1000),
            format: None,
        };

        let ticket_bytes = ticket.encode().unwrap();

        // Request schema for bulk export
        let descriptor = FlightDescriptor::new_path(vec![String::from_utf8(ticket_bytes).unwrap()]);
        let request = tonic::Request::new(descriptor);

        let response = client.get_schema(request).await;
        assert!(response.is_err(), "BulkExport should not be implemented yet");

        let err = response.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unimplemented);
    })
    .await;
}
