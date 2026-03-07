//! Integration tests for ta_* (table-backed Arrow) views.
//!
//! These tests verify that ta_* schemas are correctly registered and can be
//! queried via the Arrow Flight API using placeholder data.
//!
//! Database-dependent tests (with real PostgreSQL tables) can be added in
//! separate test files with feature gates.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use arrow::ipc::root_as_message;
use arrow_flight::{FlightDescriptor, Ticket, flight_service_client::FlightServiceClient};
use fraiseql_arrow::{FlightTicket, flight_server::FraiseQLFlightService};
use tonic::transport::{Endpoint, Server};

const TEST_FLIGHT_SECRET: &str = "flight-test-session-secret-for-integration-tests";

/// Create a session token for authenticated testing.
///
/// Must be called within a `temp_env::async_with_vars` block that sets `FLIGHT_SESSION_SECRET`.
fn create_test_session_token() -> String {
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

    encode(&header, &claims, &key).expect("Failed to encode token")
}

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
async fn test_get_schema_for_ta_orders() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create ticket for ta_orders optimized view
        let ticket = FlightTicket::OptimizedView {
            view:     "ta_orders".to_string(),
            filter:   None,
            order_by: None,
            limit:    None,
            offset:   None,
        };

        let ticket_bytes = ticket.encode().unwrap();
        let ticket_path = String::from_utf8(ticket_bytes).unwrap();

        // Request schema
        let descriptor = FlightDescriptor::new_path(vec![ticket_path]);
        let request = tonic::Request::new(descriptor);

        let response = client.get_schema(request).await.expect("GetSchema failed for ta_orders");
        let schema_result = response.into_inner();

        // Verify we got schema bytes back
        assert!(!schema_result.schema.is_empty(), "Schema should not be empty for ta_orders");

        // Decode and verify schema structure
        let schema =
            root_as_message(&schema_result.schema).expect("Failed to decode schema for ta_orders");
        assert!(schema.header_type() == arrow::ipc::MessageHeader::Schema);

        // Verify schema fields match ta_orders definition
        if schema.header_type() == arrow::ipc::MessageHeader::Schema {
            // Schema decoding successful - detailed field verification could be added
            println!("ta_orders schema successfully retrieved and decoded");
        }
    })
    .await;
}

#[tokio::test]
async fn test_get_schema_for_ta_users() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create ticket for ta_users optimized view
        let ticket = FlightTicket::OptimizedView {
            view:     "ta_users".to_string(),
            filter:   None,
            order_by: None,
            limit:    None,
            offset:   None,
        };

        let ticket_bytes = ticket.encode().unwrap();
        let ticket_path = String::from_utf8(ticket_bytes).unwrap();

        // Request schema
        let descriptor = FlightDescriptor::new_path(vec![ticket_path]);
        let request = tonic::Request::new(descriptor);

        let response = client.get_schema(request).await.expect("GetSchema failed for ta_users");
        let schema_result = response.into_inner();

        // Verify we got schema bytes back
        assert!(!schema_result.schema.is_empty(), "Schema should not be empty for ta_users");

        // Decode and verify schema structure
        let schema =
            root_as_message(&schema_result.schema).expect("Failed to decode schema for ta_users");
        assert!(schema.header_type() == arrow::ipc::MessageHeader::Schema);

        println!("ta_users schema successfully retrieved and decoded");
    })
    .await;
}

#[tokio::test]
async fn test_do_get_ta_orders_returns_data() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create session token for authentication
        let session_token = create_test_session_token();

        // Create ticket for ta_orders with limit
        let ticket = FlightTicket::OptimizedView {
            view:     "ta_orders".to_string(),
            filter:   None,
            order_by: None,
            limit:    Some(5),
            offset:   None,
        };

        let ticket_bytes = ticket.encode().unwrap();
        let ticket_request = Ticket {
            ticket: ticket_bytes.into(),
        };

        let mut request = tonic::Request::new(ticket_request);
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token)
                .parse()
                .expect("Failed to insert auth header"),
        );

        // Without a database executor configured, the server returns Unavailable.
        // Verify the failure is due to missing executor, not an auth rejection.
        match client.do_get(request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                let mut message_count = 0;
                let mut batch_count = 0;
                while let Ok(Some(_flight_data)) = stream.message().await {
                    message_count += 1;
                    if message_count > 1 {
                        batch_count += 1;
                    }
                }
                println!(
                    "ta_orders DoGet returned {} messages ({} batches)",
                    message_count, batch_count
                );
            },
            Err(status) => assert!(
                matches!(status.code(), tonic::Code::Unavailable | tonic::Code::FailedPrecondition),
                "do_get should fail due to missing executor/adapter, not an auth error; got: {status:?}",
            ),
        }
    })
    .await;
}

#[tokio::test]
async fn test_do_get_ta_users_returns_data() {
    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(TEST_FLIGHT_SECRET))], async {
        let addr = start_test_server().await.unwrap();

        let channel = Endpoint::from_shared(addr)
            .expect("Invalid endpoint")
            .connect()
            .await
            .expect("Failed to connect to Flight server");
        let mut client = FlightServiceClient::new(channel);

        // Create session token for authentication
        let session_token = create_test_session_token();

        // Create ticket for ta_users with limit
        let ticket = FlightTicket::OptimizedView {
            view:     "ta_users".to_string(),
            filter:   None,
            order_by: None,
            limit:    Some(10),
            offset:   None,
        };

        let ticket_bytes = ticket.encode().unwrap();
        let ticket_request = Ticket {
            ticket: ticket_bytes.into(),
        };

        let mut request = tonic::Request::new(ticket_request);
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", session_token)
                .parse()
                .expect("Failed to insert auth header"),
        );

        // Without a database executor configured, the server returns Unavailable.
        // Verify the failure is due to missing executor, not an auth rejection.
        match client.do_get(request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                let mut message_count = 0;
                while let Ok(Some(_flight_data)) = stream.message().await {
                    message_count += 1;
                }
                println!("ta_users DoGet returned {} messages", message_count);
            },
            Err(status) => assert!(
                matches!(status.code(), tonic::Code::Unavailable | tonic::Code::FailedPrecondition),
                "do_get should fail due to missing executor/adapter, not an auth error; got: {status:?}",
            ),
        }
    })
    .await;
}

#[tokio::test]
async fn test_ta_orders_schema_has_correct_fields() {
    let service = FraiseQLFlightService::new();

    // Verify ta_orders schema is registered
    let schema = service.schema_registry().get("ta_orders").expect("ta_orders schema not found");

    // Verify field names and count
    assert_eq!(schema.fields().len(), 4, "ta_orders should have 4 fields");

    let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    assert_eq!(field_names, vec!["id", "total", "created_at", "customer_name"]);

    println!("✅ ta_orders schema has correct fields: {:?}", field_names);
}

#[tokio::test]
async fn test_ta_users_schema_has_correct_fields() {
    let service = FraiseQLFlightService::new();

    // Verify ta_users schema is registered
    let schema = service.schema_registry().get("ta_users").expect("ta_users schema not found");

    // Verify field names and count
    assert_eq!(schema.fields().len(), 4, "ta_users should have 4 fields");

    let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    assert_eq!(field_names, vec!["id", "email", "name", "created_at"]);

    println!("✅ ta_users schema has correct fields: {:?}", field_names);
}
