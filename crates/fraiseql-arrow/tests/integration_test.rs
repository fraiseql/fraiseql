//! Integration tests for Arrow Flight server lifecycle.
//!
//! These tests verify that the Flight server starts correctly and handles
//! basic RPC calls. Actual data streaming will be tested .

use arrow_flight::{
    Criteria, FlightDescriptor, Ticket, flight_service_client::FlightServiceClient,
};
use fraiseql_arrow::{FlightTicket, flight_server::FraiseQLFlightService};
use tonic::transport::Server;

/// Start a test Flight server on a random available port.
///
/// Returns the server address (e.g., "http://127.0.0.1:12345").
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
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr.clone())
        .await
        .expect("Failed to connect to Flight server");

    // Test ListFlights (should succeed even if empty)
    let request = tonic::Request::new(Criteria {
        expression: vec![].into(),
    });
    let response = client.list_flights(request).await;
    assert!(response.is_ok(), "ListFlights should succeed");
}

#[tokio::test]
async fn test_get_schema_for_observer_events() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for observer events
    let ticket = FlightTicket::ObserverEvents {
        entity_type: "Order".to_string(),
        start_date:  None,
        end_date:    None,
        limit:       None,
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
}

#[tokio::test]
async fn test_get_schema_for_graphql_query() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for GraphQL query
    let ticket = FlightTicket::GraphQLQuery {
        query:     "{ users { id name } }".to_string(),
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
}

#[tokio::test]
async fn test_do_get_returns_empty_stream() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Phase 2.2b: Create a session token for authentication
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

    let secret = std::env::var("FLIGHT_SESSION_SECRET")
        .unwrap_or_else(|_| "flight-session-default-secret".to_string());

    let key = EncodingKey::from_secret(secret.as_bytes());
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

    let response = client.do_get(request).await.expect("DoGet failed");
    let mut stream = response.into_inner();

    // Should return at least the schema message
    let first_item = stream.message().await.expect("Stream error");
    assert!(first_item.is_some(), "Stream should return schema and data messages");
}

#[tokio::test]
async fn test_invalid_ticket_returns_error() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Phase 2.2b: Create a session token for authentication
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

    let secret = std::env::var("FLIGHT_SESSION_SECRET")
        .unwrap_or_else(|_| "flight-session-default-secret".to_string());

    let key = EncodingKey::from_secret(secret.as_bytes());
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
}

#[tokio::test]
async fn test_bulk_export_ticket_not_implemented() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create bulk export ticket (not implemented )
    let ticket = FlightTicket::BulkExport {
        table:  "users".to_string(),
        filter: None,
        limit:  Some(1000),
    };

    let ticket_bytes = ticket.encode().unwrap();

    // Request schema for bulk export
    let descriptor = FlightDescriptor::new_path(vec![String::from_utf8(ticket_bytes).unwrap()]);
    let request = tonic::Request::new(descriptor);

    let response = client.get_schema(request).await;
    assert!(response.is_err(), "BulkExport should not be implemented yet");

    let err = response.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unimplemented);
}
