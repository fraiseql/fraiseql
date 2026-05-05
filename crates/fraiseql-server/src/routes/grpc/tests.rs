//! Tests for `routes/grpc/` modules.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::handler::{
    column_specs_from_type, column_value_to_proto, encode_response, encode_row,
    field_type_to_column_type, grpc_method_to_mutation_name, grpc_method_to_query_name,
    proto_value_to_json,
};
use fraiseql_core::{
    db::{dialect::RowViewColumnType, types::{ColumnSpec, ColumnValue}},
    schema::FieldType,
};
use prost_reflect::Value;

// ── field_type_to_column_type ────────────────────────────────────────

#[test]
fn scalar_types_map_correctly() {
    assert_eq!(field_type_to_column_type(&FieldType::String), Some(RowViewColumnType::Text));
    assert_eq!(field_type_to_column_type(&FieldType::Int), Some(RowViewColumnType::Int32));
    assert_eq!(field_type_to_column_type(&FieldType::Float), Some(RowViewColumnType::Float64));
    assert_eq!(
        field_type_to_column_type(&FieldType::Boolean),
        Some(RowViewColumnType::Boolean)
    );
    assert_eq!(field_type_to_column_type(&FieldType::Id), Some(RowViewColumnType::Uuid));
    assert_eq!(
        field_type_to_column_type(&FieldType::DateTime),
        Some(RowViewColumnType::Timestamptz)
    );
    assert_eq!(field_type_to_column_type(&FieldType::Date), Some(RowViewColumnType::Date));
    assert_eq!(field_type_to_column_type(&FieldType::Json), Some(RowViewColumnType::Json));
    assert_eq!(field_type_to_column_type(&FieldType::Uuid), Some(RowViewColumnType::Uuid));
}

#[test]
fn non_scalar_types_return_none() {
    assert_eq!(field_type_to_column_type(&FieldType::Object("User".to_string())), None);
    assert_eq!(field_type_to_column_type(&FieldType::List(Box::new(FieldType::String))), None);
    assert_eq!(field_type_to_column_type(&FieldType::Vector), None);
}

#[test]
fn rich_scalars_map_to_text() {
    assert_eq!(
        field_type_to_column_type(&FieldType::Scalar("Email".to_string())),
        Some(RowViewColumnType::Text)
    );
}

#[test]
fn enums_map_to_text() {
    assert_eq!(
        field_type_to_column_type(&FieldType::Enum("Status".to_string())),
        Some(RowViewColumnType::Text)
    );
}

// ── grpc_method_to_query_name ───────────────────────────────────────

#[test]
fn get_prefix_stripped() {
    assert_eq!(grpc_method_to_query_name("GetUser"), "user");
}

#[test]
fn list_prefix_stripped() {
    assert_eq!(grpc_method_to_query_name("ListUsers"), "users");
}

#[test]
fn pascal_case_to_snake() {
    assert_eq!(grpc_method_to_query_name("GetUserProfile"), "user_profile");
}

#[test]
fn no_prefix_passthrough() {
    assert_eq!(grpc_method_to_query_name("SearchUsers"), "search_users");
}

// ── grpc_method_to_mutation_name ──────────────────────────────────

#[test]
fn mutation_name_pascal_to_camel() {
    assert_eq!(grpc_method_to_mutation_name("CreateUser"), "createUser");
}

#[test]
fn mutation_name_single_word() {
    assert_eq!(grpc_method_to_mutation_name("Delete"), "delete");
}

#[test]
fn mutation_name_empty() {
    assert_eq!(grpc_method_to_mutation_name(""), "");
}

// ── column_value_to_proto ───────────────────────────────────────────

#[test]
fn null_returns_none() {
    assert!(column_value_to_proto(&ColumnValue::Null).is_none());
}

#[test]
fn text_encodes_as_string() {
    let v = column_value_to_proto(&ColumnValue::Text("hello".into()));
    assert_eq!(v, Some(Value::String("hello".into())));
}

#[test]
fn int32_encodes() {
    let v = column_value_to_proto(&ColumnValue::Int32(42));
    assert_eq!(v, Some(Value::I32(42)));
}

#[test]
fn int64_encodes() {
    let v = column_value_to_proto(&ColumnValue::Int64(123_456_789_012));
    assert_eq!(v, Some(Value::I64(123_456_789_012)));
}

#[test]
fn float64_encodes() {
    let v = column_value_to_proto(&ColumnValue::Float64(1.23));
    assert_eq!(v, Some(Value::F64(1.23)));
}

#[test]
fn bool_encodes() {
    let v = column_value_to_proto(&ColumnValue::Boolean(true));
    assert_eq!(v, Some(Value::Bool(true)));
}

#[test]
fn uuid_encodes_as_string() {
    let v = column_value_to_proto(&ColumnValue::Uuid(
        "00000000-0000-0000-0000-000000000000".into(),
    ));
    assert_eq!(v, Some(Value::String("00000000-0000-0000-0000-000000000000".into())));
}

#[test]
fn date_encodes_as_string() {
    let v = column_value_to_proto(&ColumnValue::Date("2025-01-15".into()));
    assert_eq!(v, Some(Value::String("2025-01-15".into())));
}

#[test]
fn json_encodes_as_string() {
    let v = column_value_to_proto(&ColumnValue::Json(r#"{"key":"value"}"#.into()));
    assert_eq!(v, Some(Value::String(r#"{"key":"value"}"#.into())));
}

// ── proto_value_to_json ─────────────────────────────────────────────

#[test]
fn proto_bool_to_json() {
    let v = proto_value_to_json(&Value::Bool(true));
    assert_eq!(v, serde_json::Value::Bool(true));
}

#[test]
fn proto_string_to_json() {
    let v = proto_value_to_json(&Value::String("hello".into()));
    assert_eq!(v, serde_json::Value::String("hello".into()));
}

#[test]
fn proto_i32_to_json() {
    let v = proto_value_to_json(&Value::I32(42));
    assert_eq!(v, serde_json::json!(42));
}

#[test]
fn proto_f64_to_json() {
    let v = proto_value_to_json(&Value::F64(1.23));
    assert_eq!(v, serde_json::json!(1.23));
}

// ── encode_row / encode_response ────────────────────────────────────

/// Helper: build a minimal `DescriptorPool` with a User message.
fn test_descriptor_pool() -> prost_reflect::DescriptorPool {
    // Minimal FileDescriptorProto for a User message with id (string) and name (string).
    use prost::Message;
    use prost_reflect::prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
        field_descriptor_proto,
    };

    let user_msg = DescriptorProto {
        name:  Some("User".into()),
        field: vec![
            FieldDescriptorProto {
                name:   Some("id".into()),
                number: Some(1),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("name".into()),
                number: Some(2),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name:   Some("age".into()),
                number: Some(3),
                r#type: Some(field_descriptor_proto::Type::Int32.into()),
                label:  Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name:         Some("test.proto".into()),
        package:      Some("test".into()),
        syntax:       Some("proto3".into()),
        message_type: vec![user_msg],
        ..Default::default()
    };

    let fds = FileDescriptorSet { file: vec![file] };
    let bytes = fds.encode_to_vec();
    prost_reflect::DescriptorPool::decode(bytes.as_slice()).unwrap()
}

#[test]
fn encode_row_sets_fields() {
    let pool = test_descriptor_pool();
    let user_desc = pool.get_message_by_name("test.User").unwrap();

    let columns = vec![
        ColumnSpec {
            name:        "id".into(),
            column_type: RowViewColumnType::Uuid,
        },
        ColumnSpec {
            name:        "name".into(),
            column_type: RowViewColumnType::Text,
        },
        ColumnSpec {
            name:        "age".into(),
            column_type: RowViewColumnType::Int32,
        },
    ];

    let row = vec![
        ColumnValue::Text("abc-123".into()),
        ColumnValue::Text("Alice".into()),
        ColumnValue::Int32(30),
    ];

    let msg = encode_row(&row, &columns, &user_desc);

    let id_field = user_desc.get_field_by_name("id").unwrap();
    let name_field = user_desc.get_field_by_name("name").unwrap();
    let age_field = user_desc.get_field_by_name("age").unwrap();

    assert_eq!(msg.get_field(&id_field).into_owned(), Value::String("abc-123".into()));
    assert_eq!(msg.get_field(&name_field).into_owned(), Value::String("Alice".into()));
    assert_eq!(msg.get_field(&age_field).into_owned(), Value::I32(30));
}

#[test]
fn encode_row_null_leaves_field_unset() {
    let pool = test_descriptor_pool();
    let user_desc = pool.get_message_by_name("test.User").unwrap();

    let columns = vec![
        ColumnSpec {
            name:        "id".into(),
            column_type: RowViewColumnType::Uuid,
        },
        ColumnSpec {
            name:        "name".into(),
            column_type: RowViewColumnType::Text,
        },
        ColumnSpec {
            name:        "age".into(),
            column_type: RowViewColumnType::Int32,
        },
    ];

    let row = vec![
        ColumnValue::Text("abc".into()),
        ColumnValue::Null,
        ColumnValue::Int32(0),
    ];

    let msg = encode_row(&row, &columns, &user_desc);

    let name_field = user_desc.get_field_by_name("name").unwrap();
    // Null leaves the field at its default (empty string for proto3 string).
    assert!(!msg.has_field(&name_field));
}

#[test]
fn encode_response_get_single_row() {
    let pool = test_descriptor_pool();
    let user_desc = pool.get_message_by_name("test.User").unwrap();

    let columns = vec![
        ColumnSpec {
            name:        "id".into(),
            column_type: RowViewColumnType::Uuid,
        },
        ColumnSpec {
            name:        "name".into(),
            column_type: RowViewColumnType::Text,
        },
    ];

    let rows = vec![vec![
        ColumnValue::Text("u-1".into()),
        ColumnValue::Text("Bob".into()),
    ]];

    let response = encode_response(rows, &columns, false, &user_desc, &user_desc);

    let id_field = user_desc.get_field_by_name("id").unwrap();
    assert_eq!(response.get_field(&id_field).into_owned(), Value::String("u-1".into()));
}

#[test]
fn encode_response_empty_rows() {
    let pool = test_descriptor_pool();
    let user_desc = pool.get_message_by_name("test.User").unwrap();

    let columns = vec![ColumnSpec {
        name:        "id".into(),
        column_type: RowViewColumnType::Uuid,
    }];

    // No rows — response should have default values.
    let response = encode_response(vec![], &columns, false, &user_desc, &user_desc);
    let id_field = user_desc.get_field_by_name("id").unwrap();
    assert!(!response.has_field(&id_field));
}

// ── column_specs_from_type ──────────────────────────────────────────

#[test]
fn column_specs_from_type_filters_non_scalars() {
    use fraiseql_core::schema::{FieldDefinition, TypeDefinition};

    let type_def = TypeDefinition::new("User", "tb_users")
        .with_field(FieldDefinition::new("id", FieldType::Id))
        .with_field(FieldDefinition::new("name", FieldType::String))
        .with_field(FieldDefinition::new(
            "posts",
            FieldType::List(Box::new(FieldType::Object("Post".into()))),
        ))
        .with_field(FieldDefinition::new("age", FieldType::Int));

    let specs = column_specs_from_type(&type_def);
    let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["id", "name", "age"]);
}
