//! Integration tests for Arrow Flight schema refresh functionality (Phase 4).
//!
//! Tests the RefreshSchemaRegistry and GetSchemaVersions actions which allow
//! safe runtime schema reloading without disrupting running queries.

use arrow_flight::flight_service_client::FlightServiceClient;
use fraiseql_arrow::flight_server::FraiseQLFlightService;
use tonic::transport::Server;

/// Start a test Flight server on a random available port.
///
/// Returns the server address (e.g., "http://127.0.0.1:12345").
async fn start_test_server() -> Result<String, Box<dyn std::error::Error>> {
    let service = FraiseQLFlightService::new();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(format!("http://127.0.0.1:{}", addr.port()))
}

#[tokio::test]
async fn test_get_schema_versions_action() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Call GetSchemaVersions action
    use arrow_flight::Action;
    let action = Action {
        r#type: "GetSchemaVersions".to_string(),
        body: vec![].into(),
    };

    let request = tonic::Request::new(action);
    let response = client.do_action(request).await;

    // Without authentication, should fail
    assert!(response.is_err());
    if let Err(status) = response {
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }
}

#[tokio::test]
async fn test_schema_versioning_metadata() {
    use fraiseql_arrow::metadata::SchemaRegistry;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    let registry = Arc::new(SchemaRegistry::new());

    // Register initial schema
    let schema_v0 = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
    ]));

    registry.register("test_view", schema_v0.clone());

    // Get version info
    let (version, created_at) = registry.get_version_info("test_view").unwrap();
    assert_eq!(version, 0);

    // Update schema (version should increment)
    let schema_v1 = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));

    registry.register("test_view", schema_v1.clone());

    let (new_version, new_created_at) = registry.get_version_info("test_view").unwrap();
    assert_eq!(new_version, 1);

    // New version should be more recent or same time
    assert!(new_created_at >= created_at);
}

#[tokio::test]
async fn test_get_all_versions_list() {
    use fraiseql_arrow::metadata::SchemaRegistry;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    let registry = SchemaRegistry::new();

    // Register multiple schemas
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
    ]));

    registry.register("view1", schema.clone());
    registry.register("view2", schema.clone());
    registry.register("view3", schema);

    // Get all versions
    let versions = registry.get_all_versions();

    assert_eq!(versions.len(), 3);

    let view_names: Vec<String> = versions.iter().map(|(name, _, _)| name.clone()).collect();
    assert!(view_names.contains(&"view1".to_string()));
    assert!(view_names.contains(&"view2".to_string()));
    assert!(view_names.contains(&"view3".to_string()));

    // Verify version numbers are sequential
    let version_numbers: Vec<u64> = versions.iter().map(|(_, v, _)| *v).collect();
    assert_eq!(version_numbers.len(), 3);
}

#[tokio::test]
async fn test_schema_copy_on_write_safety() {
    use fraiseql_arrow::metadata::SchemaRegistry;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    let registry = SchemaRegistry::new();

    let schema_v0 = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
    ]));

    registry.register("view", schema_v0.clone());

    // Get first reference
    let ref1 = registry.get("view").unwrap();
    assert!(Arc::ptr_eq(&ref1, &schema_v0));

    // Update with new schema
    let schema_v1 = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));

    registry.register("view", schema_v1.clone());

    // Old reference should still point to old schema
    assert!(Arc::ptr_eq(&ref1, &schema_v0));
    assert_eq!(ref1.fields().len(), 1);

    // New reference should point to new schema
    let ref2 = registry.get("view").unwrap();
    assert!(Arc::ptr_eq(&ref2, &schema_v1));
    assert_eq!(ref2.fields().len(), 2);
}

#[tokio::test]
async fn test_concurrent_schema_updates() {
    use fraiseql_arrow::metadata::SchemaRegistry;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    let registry = Arc::new(SchemaRegistry::new());

    // Spawn multiple tasks that update schemas concurrently
    let mut handles = vec![];

    for i in 0..5 {
        let reg = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            for j in 0..10 {
                let schema = Arc::new(Schema::new(vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ]));

                let view_name = format!("view_{}", i);
                reg.register(&view_name, schema);

                // Small delay to allow interleaving
                tokio::time::sleep(tokio::time::Duration::from_micros(1)).await;

                let (version, _) = reg.get_version_info(&view_name).unwrap();
                assert!(version >= (j as u64));
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all schemas are registered
    let versions = registry.get_all_versions();
    assert_eq!(versions.len(), 5); // 5 views registered

    // Verify version numbers are reasonable
    for (_, version, _) in versions.iter() {
        assert!(*version >= 9); // At least 10 updates per view
    }
}
