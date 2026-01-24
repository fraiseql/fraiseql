//! Integration tests for ta_* (table-backed Arrow) views.
//!
//! These tests verify that ta_* schemas are correctly registered and can be
//! queried via the Arrow Flight API using placeholder data.
//!
//! Database-dependent tests (with real PostgreSQL tables) can be added in
//! separate test files with feature gates.

use arrow::ipc::root_as_message;
use arrow_flight::{
    flight_service_client::FlightServiceClient, FlightDescriptor, Ticket,
};
use fraiseql_arrow::{flight_server::FraiseQLFlightService, FlightTicket};
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
async fn test_get_schema_for_ta_orders() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for ta_orders optimized view
    let ticket = FlightTicket::OptimizedView {
        view: "ta_orders".to_string(),
        filter: None,
        order_by: None,
        limit: None,
        offset: None,
    };

    let ticket_bytes = ticket.encode().unwrap();
    let ticket_path = String::from_utf8(ticket_bytes).unwrap();

    // Request schema
    let descriptor = FlightDescriptor::new_path(vec![ticket_path]);
    let request = tonic::Request::new(descriptor);

    let response = client
        .get_schema(request)
        .await
        .expect("GetSchema failed for ta_orders");
    let schema_result = response.into_inner();

    // Verify we got schema bytes back
    assert!(
        !schema_result.schema.is_empty(),
        "Schema should not be empty for ta_orders"
    );

    // Decode and verify schema structure
    let schema = root_as_message(&schema_result.schema)
        .expect("Failed to decode schema for ta_orders");
    assert!(schema.header_type() == arrow::ipc::MessageHeader::Schema);

    // Verify schema fields match ta_orders definition
    if schema.header_type() == arrow::ipc::MessageHeader::Schema {
        // Schema decoding successful - detailed field verification could be added
        println!("✅ ta_orders schema successfully retrieved and decoded");
    }
}

#[tokio::test]
async fn test_get_schema_for_ta_users() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for ta_users optimized view
    let ticket = FlightTicket::OptimizedView {
        view: "ta_users".to_string(),
        filter: None,
        order_by: None,
        limit: None,
        offset: None,
    };

    let ticket_bytes = ticket.encode().unwrap();
    let ticket_path = String::from_utf8(ticket_bytes).unwrap();

    // Request schema
    let descriptor = FlightDescriptor::new_path(vec![ticket_path]);
    let request = tonic::Request::new(descriptor);

    let response = client
        .get_schema(request)
        .await
        .expect("GetSchema failed for ta_users");
    let schema_result = response.into_inner();

    // Verify we got schema bytes back
    assert!(
        !schema_result.schema.is_empty(),
        "Schema should not be empty for ta_users"
    );

    // Decode and verify schema structure
    let schema = root_as_message(&schema_result.schema)
        .expect("Failed to decode schema for ta_users");
    assert!(schema.header_type() == arrow::ipc::MessageHeader::Schema);

    println!("✅ ta_users schema successfully retrieved and decoded");
}

#[tokio::test]
async fn test_do_get_ta_orders_returns_data() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for ta_orders with limit
    let ticket = FlightTicket::OptimizedView {
        view: "ta_orders".to_string(),
        filter: None,
        order_by: None,
        limit: Some(5),
        offset: None,
    };

    let ticket_bytes = ticket.encode().unwrap();
    let ticket_request = Ticket {
        ticket: ticket_bytes.into(),
    };

    let response = client
        .do_get(tonic::Request::new(ticket_request))
        .await
        .expect("DoGet failed for ta_orders");

    // Collect the stream - should have schema + batches
    let mut stream = response.into_inner();
    let mut message_count = 0;
    let mut batch_count = 0;

    while let Ok(Some(_flight_data)) = stream.message().await {
        message_count += 1;
        // First message is schema, subsequent messages are data batches
        if message_count > 1 {
            batch_count += 1;
        }
    }

    // Should have at least schema + 1 data batch
    assert!(
        message_count > 1,
        "Expected schema + data batches, got {} messages",
        message_count
    );
    println!(
        "✅ ta_orders DoGet returned {} messages ({} batches)",
        message_count, batch_count
    );
}

#[tokio::test]
async fn test_do_get_ta_users_returns_data() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for ta_users with limit
    let ticket = FlightTicket::OptimizedView {
        view: "ta_users".to_string(),
        filter: None,
        order_by: None,
        limit: Some(10),
        offset: None,
    };

    let ticket_bytes = ticket.encode().unwrap();
    let ticket_request = Ticket {
        ticket: ticket_bytes.into(),
    };

    let response = client
        .do_get(tonic::Request::new(ticket_request))
        .await
        .expect("DoGet failed for ta_users");

    // Collect the stream - should have schema + batches
    let mut stream = response.into_inner();
    let mut message_count = 0;

    while let Ok(Some(_flight_data)) = stream.message().await {
        message_count += 1;
    }

    // Should have at least schema + 1 data batch
    assert!(
        message_count > 1,
        "Expected schema + data batches, got {} messages",
        message_count
    );
    println!("✅ ta_users DoGet returned {} messages", message_count);
}

#[tokio::test]
async fn test_ta_orders_schema_has_correct_fields() {
    let service = FraiseQLFlightService::new();

    // Verify ta_orders schema is registered
    let schema = service
        .schema_registry()
        .get("ta_orders")
        .expect("ta_orders schema not found");

    // Verify field names and count
    assert_eq!(
        schema.fields().len(),
        4,
        "ta_orders should have 4 fields"
    );

    let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    assert_eq!(field_names, vec!["id", "total", "created_at", "customer_name"]);

    println!("✅ ta_orders schema has correct fields: {:?}", field_names);
}

#[tokio::test]
async fn test_ta_users_schema_has_correct_fields() {
    let service = FraiseQLFlightService::new();

    // Verify ta_users schema is registered
    let schema = service
        .schema_registry()
        .get("ta_users")
        .expect("ta_users schema not found");

    // Verify field names and count
    assert_eq!(
        schema.fields().len(),
        4,
        "ta_users should have 4 fields"
    );

    let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    assert_eq!(field_names, vec!["id", "email", "name", "created_at"]);

    println!("✅ ta_users schema has correct fields: {:?}", field_names);
}
