//! End-to-end integration tests for the gRPC transport (Cycle 7).
//!
//! Tests the full stack: compile schema → build descriptor → build
//! `DynamicGrpcService` → exercise gRPC requests via `tower::ServiceExt::oneshot`
//! against a `FailingAdapter` with canned row-shaped responses.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test code
#![allow(clippy::missing_errors_doc)] // Reason: test code

use std::sync::Arc;

use fraiseql_core::db::types::ColumnValue;
use fraiseql_core::schema::{CompiledSchema, GrpcConfig};
use fraiseql_server::routes::grpc::{self, DynamicGrpcService};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestMutationBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use http_body_util::BodyExt as _;
use prost::Message as _;
use prost_reflect::prost_types::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    MethodDescriptorProto, ServiceDescriptorProto, field_descriptor_proto,
};
use tower::ServiceExt as _;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PACKAGE: &str = "fraiseql.v1";
const SERVICE_NAME: &str = "fraiseql.v1.FraiseqlService";

// ---------------------------------------------------------------------------
// Helper: build a CompiledSchema with gRPC enabled and a single User type
// ---------------------------------------------------------------------------

fn build_grpc_schema(descriptor_path: &str) -> CompiledSchema {
    let mut schema = TestSchemaBuilder::new()
        .with_query(
            TestQueryBuilder::new("user", "User")
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("users", "User")
                .returns_list(true)
                .build(),
        )
        .with_mutation(
            TestMutationBuilder::new("createUser", "User")
                .with_sql_source("fn_create_user")
                .build(),
        )
        .with_type(
            TestTypeBuilder::new("User", "tb_users")
                .with_field(TestFieldBuilder::new("id", fraiseql_core::schema::FieldType::Id).build())
                .with_field(TestFieldBuilder::new("name", fraiseql_core::schema::FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("email", fraiseql_core::schema::FieldType::String).build())
                .with_field(TestFieldBuilder::new("age", fraiseql_core::schema::FieldType::Int).build())
                .build(),
        )
        .build();

    schema.grpc_config = Some(GrpcConfig {
        enabled: true,
        descriptor_path: descriptor_path.to_string(),
        ..GrpcConfig::default()
    });

    schema.build_indexes();
    schema
}

// ---------------------------------------------------------------------------
// Helper: build a FileDescriptorSet with User message, request/response
// messages, and FraiseqlService with GetUser and ListUsers RPCs
// ---------------------------------------------------------------------------

fn build_descriptor_set() -> FileDescriptorSet {
    // User message: fields sorted alphabetically (age=1, email=2, id=3, name=4)
    let user_msg = DescriptorProto {
        name: Some("User".into()),
        field: vec![
            FieldDescriptorProto {
                name:   Some("age".into()),
                number: Some(1),
                r#type: Some(field_descriptor_proto::Type::Int32.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("email".into()),
                number: Some(2),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("id".into()),
                number: Some(3),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("name".into()),
                number: Some(4),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // GetUserRequest (for single-item query "user" → GetUser)
    let get_user_request = DescriptorProto {
        name: Some("GetUserRequest".into()),
        field: vec![
            FieldDescriptorProto {
                name:   Some("id".into()),
                number: Some(1),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // ListUsersRequest (for list query "users" → ListUsers)
    let list_users_request = DescriptorProto {
        name: Some("ListUsersRequest".into()),
        field: vec![
            FieldDescriptorProto {
                name:   Some("limit".into()),
                number: Some(1),
                r#type: Some(field_descriptor_proto::Type::Int32.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("offset".into()),
                number: Some(2),
                r#type: Some(field_descriptor_proto::Type::Int32.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // ListUsersResponse: repeated User items = 1; int32 total_count = 2;
    let list_users_response = DescriptorProto {
        name: Some("ListUsersResponse".into()),
        field: vec![
            FieldDescriptorProto {
                name:      Some("items".into()),
                number:    Some(1),
                r#type:    Some(field_descriptor_proto::Type::Message.into()),
                label:     Some(field_descriptor_proto::Label::Repeated.into()),
                type_name: Some(".fraiseql.v1.User".into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("total_count".into()),
                number: Some(2),
                r#type: Some(field_descriptor_proto::Type::Int32.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // CreateUserRequest (for mutation "createUser" → CreateUser)
    let create_user_request = DescriptorProto {
        name: Some("CreateUserRequest".into()),
        field: vec![
            FieldDescriptorProto {
                name:   Some("name".into()),
                number: Some(1),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("email".into()),
                number: Some(2),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // MutationResponse: bool success = 1; optional string id = 2; optional string error = 3;
    let mutation_response = DescriptorProto {
        name: Some("MutationResponse".into()),
        field: vec![
            FieldDescriptorProto {
                name:   Some("success".into()),
                number: Some(1),
                r#type: Some(field_descriptor_proto::Type::Bool.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("id".into()),
                number: Some(2),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("error".into()),
                number: Some(3),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // Service definition
    let service = ServiceDescriptorProto {
        name: Some("FraiseqlService".into()),
        method: vec![
            MethodDescriptorProto {
                name:       Some("GetUser".into()),
                input_type: Some(".fraiseql.v1.GetUserRequest".into()),
                output_type: Some(".fraiseql.v1.User".into()),
                ..Default::default()
            },
            MethodDescriptorProto {
                name:       Some("ListUsers".into()),
                input_type: Some(".fraiseql.v1.ListUsersRequest".into()),
                output_type: Some(".fraiseql.v1.ListUsersResponse".into()),
                ..Default::default()
            },
            MethodDescriptorProto {
                name:        Some("CreateUser".into()),
                input_type:  Some(".fraiseql.v1.CreateUserRequest".into()),
                output_type: Some(".fraiseql.v1.MutationResponse".into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name:         Some("service.proto".into()),
        package:      Some(PACKAGE.into()),
        syntax:       Some("proto3".into()),
        message_type: vec![
            user_msg,
            get_user_request,
            list_users_request,
            list_users_response,
            create_user_request,
            mutation_response,
        ],
        service: vec![service],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

/// Write the descriptor to a temp file and return the path.
fn write_descriptor(dir: &std::path::Path) -> String {
    let fds = build_descriptor_set();
    let bytes = fds.encode_to_vec();
    let path = dir.join("descriptor.binpb");
    std::fs::write(&path, &bytes).expect("write descriptor");
    path.to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------
// Helper: build service + adapter with canned row data
// ---------------------------------------------------------------------------

fn build_service(
    adapter: FailingAdapter,
    schema: CompiledSchema,
) -> DynamicGrpcService<FailingAdapter> {
    let schema = Arc::new(schema);
    let adapter = Arc::new(adapter);

    let (svc, name) = grpc::build_grpc_service(schema, adapter)
        .expect("build_grpc_service should succeed")
        .expect("gRPC should be enabled");

    assert_eq!(name, SERVICE_NAME);
    svc
}

// ---------------------------------------------------------------------------
// Helper: build a gRPC request with framed body
// ---------------------------------------------------------------------------

fn grpc_request(method: &str, msg_bytes: &[u8]) -> http::Request<tonic::body::Body> {
    // gRPC frame: 1 byte compression flag + 4 bytes big-endian length + payload
    let mut framed = Vec::with_capacity(5 + msg_bytes.len());
    framed.push(0); // no compression
    let len = u32::try_from(msg_bytes.len()).unwrap();
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(msg_bytes);

    let uri = format!("/{SERVICE_NAME}/{method}");

    http::Request::builder()
        .method("POST")
        .uri(&uri)
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .body(tonic::body::Body::new(axum::body::Body::from(framed)))
        .unwrap()
}

/// Send a gRPC request through the service and return (`status_code`, `grpc_status`, `body_bytes`).
async fn send_grpc(
    svc: &DynamicGrpcService<FailingAdapter>,
    method: &str,
    msg_bytes: &[u8],
) -> (http::StatusCode, Option<String>, Vec<u8>) {
    let req = grpc_request(method, msg_bytes);
    let response = svc.clone().oneshot(req).await.expect("service call should not error");

    let status = response.status();
    let grpc_status = response
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let body_bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes()
        .to_vec();

    (status, grpc_status, body_bytes)
}

/// Decode a gRPC response body (skip 5-byte frame header) into a `DynamicMessage`.
fn decode_response(
    body: &[u8],
    message_name: &str,
) -> prost_reflect::DynamicMessage {
    assert!(
        body.len() >= 5,
        "gRPC response body too short: {} bytes",
        body.len()
    );
    let msg_bytes = &body[5..];

    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let desc = pool
        .get_message_by_name(&format!("{PACKAGE}.{message_name}"))
        .unwrap_or_else(|| panic!("Message {message_name} not found in pool"));

    prost_reflect::DynamicMessage::decode(desc, msg_bytes)
        .expect("decode response message")
}

// ---------------------------------------------------------------------------
// Canned row data
// ---------------------------------------------------------------------------

fn alice_row() -> Vec<ColumnValue> {
    vec![
        ColumnValue::Text("a1b2c3".into()),   // id (Uuid rendered as text by handler)
        ColumnValue::Text("Alice".into()),     // name
        ColumnValue::Text("alice@example.com".into()), // email
        ColumnValue::Int32(30),                // age
    ]
}

fn bob_row() -> Vec<ColumnValue> {
    vec![
        ColumnValue::Text("d4e5f6".into()),
        ColumnValue::Text("Bob".into()),
        ColumnValue::Null,                     // email is null
        ColumnValue::Int32(25),
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_user_returns_single_row() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    let adapter = FailingAdapter::new()
        .with_row_response("vr_tb_users", vec![alice_row()]);

    let svc = build_service(adapter, schema);

    // Build an empty GetUserRequest (no filter fields set).
    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.GetUserRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, body) = send_grpc(&svc, "GetUser", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(grpc_status.as_deref(), Some("0"), "gRPC status should be OK");

    // Decode response as a User message (GetUser returns User directly).
    let response = decode_response(&body, "User");

    let user_desc = pool.get_message_by_name("fraiseql.v1.User").unwrap();
    let id_field = user_desc.get_field_by_name("id").unwrap();
    let name_field = user_desc.get_field_by_name("name").unwrap();
    let email_field = user_desc.get_field_by_name("email").unwrap();
    let age_field = user_desc.get_field_by_name("age").unwrap();

    assert_eq!(
        response.get_field(&id_field).into_owned(),
        prost_reflect::Value::String("a1b2c3".into())
    );
    assert_eq!(
        response.get_field(&name_field).into_owned(),
        prost_reflect::Value::String("Alice".into())
    );
    assert_eq!(
        response.get_field(&email_field).into_owned(),
        prost_reflect::Value::String("alice@example.com".into())
    );
    assert_eq!(
        response.get_field(&age_field).into_owned(),
        prost_reflect::Value::I32(30)
    );
}

#[tokio::test]
async fn list_users_returns_repeated_items() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    let adapter = FailingAdapter::new()
        .with_row_response("vr_tb_users", vec![alice_row(), bob_row()]);

    let svc = build_service(adapter, schema);

    // Build an empty ListUsersRequest.
    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.ListUsersRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, body) = send_grpc(&svc, "ListUsers", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(grpc_status.as_deref(), Some("0"));

    // Decode response as ListUsersResponse.
    let response = decode_response(&body, "ListUsersResponse");

    let resp_desc = pool
        .get_message_by_name("fraiseql.v1.ListUsersResponse")
        .unwrap();
    let items_field = resp_desc.get_field_by_name("items").unwrap();
    let items = response.get_field(&items_field);

    if let prost_reflect::Value::List(items) = items.as_ref() {
        assert_eq!(items.len(), 2, "Expected 2 items");

        // First item: Alice
        if let prost_reflect::Value::Message(alice) = &items[0] {
            let user_desc = pool.get_message_by_name("fraiseql.v1.User").unwrap();
            let name_field = user_desc.get_field_by_name("name").unwrap();
            assert_eq!(
                alice.get_field(&name_field).into_owned(),
                prost_reflect::Value::String("Alice".into())
            );
        } else {
            panic!("Expected Message value for items[0]");
        }

        // Second item: Bob
        if let prost_reflect::Value::Message(bob) = &items[1] {
            let user_desc = pool.get_message_by_name("fraiseql.v1.User").unwrap();
            let name_field = user_desc.get_field_by_name("name").unwrap();
            assert_eq!(
                bob.get_field(&name_field).into_owned(),
                prost_reflect::Value::String("Bob".into())
            );
            // Bob's email is null — field should be unset (default empty string in proto3).
            let email_field = user_desc.get_field_by_name("email").unwrap();
            assert!(!bob.has_field(&email_field), "Bob's email should be unset (null)");
        } else {
            panic!("Expected Message value for items[1]");
        }
    } else {
        panic!("Expected List value for items field");
    }
}

#[tokio::test]
async fn get_user_empty_result_returns_default_message() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    // No canned rows — adapter returns empty.
    let adapter = FailingAdapter::new();
    let svc = build_service(adapter, schema);

    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.GetUserRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, body) = send_grpc(&svc, "GetUser", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(grpc_status.as_deref(), Some("0"));

    // Response should be a User with all default values (no fields set).
    let response = decode_response(&body, "User");
    let user_desc = pool.get_message_by_name("fraiseql.v1.User").unwrap();
    let id_field = user_desc.get_field_by_name("id").unwrap();
    assert!(!response.has_field(&id_field), "Empty result should leave fields unset");
}

#[tokio::test]
async fn list_users_empty_result_returns_empty_items() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    let adapter = FailingAdapter::new();
    let svc = build_service(adapter, schema);

    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.ListUsersRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, body) = send_grpc(&svc, "ListUsers", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(grpc_status.as_deref(), Some("0"));

    let response = decode_response(&body, "ListUsersResponse");
    let resp_desc = pool
        .get_message_by_name("fraiseql.v1.ListUsersResponse")
        .unwrap();
    let items_field = resp_desc.get_field_by_name("items").unwrap();

    // Items should be empty (default for repeated field).
    assert!(!response.has_field(&items_field), "Empty list should have no items");
}

#[tokio::test]
async fn unknown_method_returns_unimplemented() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    let adapter = FailingAdapter::new();
    let svc = build_service(adapter, schema);

    let (status, grpc_status, _body) = send_grpc(&svc, "NonExistentMethod", &[]).await;

    assert_eq!(status, http::StatusCode::OK); // gRPC always returns 200
    // gRPC status 12 = UNIMPLEMENTED
    assert_eq!(grpc_status.as_deref(), Some("12"));
}

#[tokio::test]
async fn short_body_returns_invalid_argument() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    let adapter = FailingAdapter::new();
    let svc = build_service(adapter, schema);

    // Send a request with only 3 bytes (less than the 5-byte gRPC frame header).
    let short_body = vec![0u8, 0, 0];
    let uri = format!("/{SERVICE_NAME}/GetUser");
    let req = http::Request::builder()
        .method("POST")
        .uri(&uri)
        .header("content-type", "application/grpc")
        .body(tonic::body::Body::new(axum::body::Body::from(short_body)))
        .unwrap();

    let response = svc.clone().oneshot(req).await.unwrap();
    let grpc_status = response
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // gRPC status 3 = INVALID_ARGUMENT
    assert_eq!(grpc_status.as_deref(), Some("3"));
}

#[tokio::test]
async fn grpc_disabled_returns_none() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let mut schema = build_grpc_schema(&desc_path);

    // Disable gRPC
    schema.grpc_config.as_mut().unwrap().enabled = false;

    let adapter = FailingAdapter::new();
    let result = grpc::build_grpc_service(Arc::new(schema), Arc::new(adapter))
        .expect("should not error");
    assert!(result.is_none(), "Disabled gRPC should return None");
}

#[tokio::test]
async fn no_grpc_config_returns_none() {
    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "tb_users")
                .with_field(TestFieldBuilder::new("id", fraiseql_core::schema::FieldType::Id).build())
                .build(),
        )
        .build();
    schema.grpc_config = None;

    let adapter = FailingAdapter::new();
    let result = grpc::build_grpc_service(Arc::new(schema), Arc::new(adapter))
        .expect("should not error");
    assert!(result.is_none(), "No gRPC config should return None");
}

#[tokio::test]
async fn adapter_failure_propagates_as_grpc_error() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    // Configure adapter to fail on the first query.
    let adapter = FailingAdapter::new().fail_on_query(0);
    let svc = build_service(adapter, schema);

    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.GetUserRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, _body) = send_grpc(&svc, "GetUser", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK); // gRPC always returns 200
    // gRPC status 13 = INTERNAL
    assert_eq!(grpc_status.as_deref(), Some("13"));
}

#[tokio::test]
async fn dispatch_table_has_correct_view_names() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    let adapter = FailingAdapter::new()
        .with_row_response("vr_tb_users", vec![alice_row()]);

    let svc = build_service(adapter.clone(), schema);

    // Send a GetUser request — the adapter should receive a query to "vr_tb_users".
    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.GetUserRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (_status, grpc_status, _body) = send_grpc(&svc, "GetUser", &req_bytes).await;
    assert_eq!(grpc_status.as_deref(), Some("0"));

    // Verify the adapter was queried with the correct view name.
    let queries = adapter.recorded_queries();
    assert_eq!(queries, vec!["vr_tb_users"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// Cycle 8: Mutation tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_user_mutation_returns_mutation_response() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    // Canned function response: Trinity pattern returns status + entity_id.
    let mut function_row = std::collections::HashMap::new();
    function_row.insert("status".to_string(), serde_json::json!("success"));
    function_row.insert("entity_id".to_string(), serde_json::json!("new-user-123"));

    let adapter = FailingAdapter::new()
        .with_function_response("fn_create_user", vec![function_row]);

    let svc = build_service(adapter, schema);

    // Build a CreateUserRequest with name and email fields.
    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.CreateUserRequest")
        .unwrap();
    let mut req_msg = prost_reflect::DynamicMessage::new(req_desc.clone());

    let name_field = req_desc.get_field_by_name("name").unwrap();
    req_msg.set_field(&name_field, prost_reflect::Value::String("Charlie".into()));

    let email_field = req_desc.get_field_by_name("email").unwrap();
    req_msg.set_field(&email_field, prost_reflect::Value::String("charlie@example.com".into()));

    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, body) = send_grpc(&svc, "CreateUser", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(grpc_status.as_deref(), Some("0"), "gRPC status should be OK");

    // Decode response as MutationResponse.
    let response = decode_response(&body, "MutationResponse");
    let resp_desc = pool
        .get_message_by_name("fraiseql.v1.MutationResponse")
        .unwrap();

    let success_field = resp_desc.get_field_by_name("success").unwrap();
    assert_eq!(
        response.get_field(&success_field).into_owned(),
        prost_reflect::Value::Bool(true)
    );

    let id_field = resp_desc.get_field_by_name("id").unwrap();
    assert_eq!(
        response.get_field(&id_field).into_owned(),
        prost_reflect::Value::String("new-user-123".into())
    );
}

#[tokio::test]
async fn create_user_mutation_failure_returns_error_in_response() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    // Canned function response: failure case.
    let mut function_row = std::collections::HashMap::new();
    function_row.insert("status".to_string(), serde_json::json!("error"));
    function_row.insert("message".to_string(), serde_json::json!("email already exists"));

    let adapter = FailingAdapter::new()
        .with_function_response("fn_create_user", vec![function_row]);

    let svc = build_service(adapter, schema);

    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.CreateUserRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, body) = send_grpc(&svc, "CreateUser", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(grpc_status.as_deref(), Some("0"));

    let response = decode_response(&body, "MutationResponse");
    let resp_desc = pool
        .get_message_by_name("fraiseql.v1.MutationResponse")
        .unwrap();

    let success_field = resp_desc.get_field_by_name("success").unwrap();
    assert_eq!(
        response.get_field(&success_field).into_owned(),
        prost_reflect::Value::Bool(false)
    );

    let error_field = resp_desc.get_field_by_name("error").unwrap();
    assert_eq!(
        response.get_field(&error_field).into_owned(),
        prost_reflect::Value::String("email already exists".into())
    );
}

#[tokio::test]
async fn mutation_adapter_failure_propagates_as_grpc_error() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    // Configure adapter to fail on function call.
    let adapter = FailingAdapter::new().fail_on_query(0);
    let svc = build_service(adapter, schema);

    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();
    let req_desc = pool
        .get_message_by_name("fraiseql.v1.CreateUserRequest")
        .unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let req_bytes = req_msg.encode_to_vec();

    let (status, grpc_status, _body) = send_grpc(&svc, "CreateUser", &req_bytes).await;

    assert_eq!(status, http::StatusCode::OK);
    // gRPC status 13 = INTERNAL
    assert_eq!(grpc_status.as_deref(), Some("13"));
}

#[tokio::test]
async fn all_three_rpcs_are_callable() {
    let tmp = tempfile::tempdir().unwrap();
    let desc_path = write_descriptor(tmp.path());
    let schema = build_grpc_schema(&desc_path);

    let mut function_row = std::collections::HashMap::new();
    function_row.insert("status".to_string(), serde_json::json!("success"));
    function_row.insert("entity_id".to_string(), serde_json::json!("u-99"));

    let adapter = FailingAdapter::new()
        .with_row_response("vr_tb_users", vec![alice_row()])
        .with_function_response("fn_create_user", vec![function_row]);

    let svc = build_service(adapter, schema);

    let fds = build_descriptor_set();
    let pool = prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap();

    // 1. GetUser — query RPC
    let req_desc = pool.get_message_by_name("fraiseql.v1.GetUserRequest").unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let (_, grpc_status, _) = send_grpc(&svc, "GetUser", &req_msg.encode_to_vec()).await;
    assert_eq!(grpc_status.as_deref(), Some("0"), "GetUser should succeed");

    // 2. ListUsers — query RPC
    let req_desc = pool.get_message_by_name("fraiseql.v1.ListUsersRequest").unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let (_, grpc_status, _) = send_grpc(&svc, "ListUsers", &req_msg.encode_to_vec()).await;
    assert_eq!(grpc_status.as_deref(), Some("0"), "ListUsers should succeed");

    // 3. CreateUser — mutation RPC
    let req_desc = pool.get_message_by_name("fraiseql.v1.CreateUserRequest").unwrap();
    let req_msg = prost_reflect::DynamicMessage::new(req_desc);
    let (_, grpc_status, _) = send_grpc(&svc, "CreateUser", &req_msg.encode_to_vec()).await;
    assert_eq!(grpc_status.as_deref(), Some("0"), "CreateUser should succeed");

    // 4. Unknown method — still returns UNIMPLEMENTED
    let (_, grpc_status, _) = send_grpc(&svc, "DeleteUser", &[]).await;
    assert_eq!(grpc_status.as_deref(), Some("12"), "Unknown method should return UNIMPLEMENTED");
}
