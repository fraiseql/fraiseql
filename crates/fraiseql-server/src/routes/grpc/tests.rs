//! Tests for `routes/grpc/` modules.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::{
    db::{
        dialect::RowViewColumnType,
        types::{ColumnSpec, ColumnValue},
    },
    schema::FieldType,
};
use prost_reflect::Value;

use super::handler::{
    column_specs_from_type, column_value_to_proto, encode_response, encode_row,
    field_type_to_column_type, grpc_method_to_mutation_name, grpc_method_to_query_name,
    proto_value_to_json, recase_keys_to_snake,
};

// ── field_type_to_column_type ────────────────────────────────────────

#[test]
fn scalar_types_map_correctly() {
    assert_eq!(field_type_to_column_type(&FieldType::String), Some(RowViewColumnType::Text));
    assert_eq!(field_type_to_column_type(&FieldType::Int), Some(RowViewColumnType::Int32));
    assert_eq!(field_type_to_column_type(&FieldType::Float), Some(RowViewColumnType::Float64));
    assert_eq!(field_type_to_column_type(&FieldType::Boolean), Some(RowViewColumnType::Boolean));
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
    let v =
        column_value_to_proto(&ColumnValue::Uuid("00000000-0000-0000-0000-000000000000".into()));
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
        name: Some("User".into()),
        field: vec![
            FieldDescriptorProto {
                name: Some("id".into()),
                number: Some(1),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label: Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name: Some("name".into()),
                number: Some(2),
                r#type: Some(field_descriptor_proto::Type::String.into()),
                label: Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name: Some("age".into()),
                number: Some(3),
                r#type: Some(field_descriptor_proto::Type::Int32.into()),
                label: Some(field_descriptor_proto::Label::Optional.into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("test.proto".into()),
        package: Some("test".into()),
        syntax: Some("proto3".into()),
        message_type: vec![user_msg],
        ..Default::default()
    };

    let fds = FileDescriptorSet { file: vec![file] };
    let bytes = fds.encode_to_vec();
    prost_reflect::DescriptorPool::decode(bytes.as_slice()).unwrap()
}

/// Helper: a `DescriptorPool` carrying a service with one `ListUsers` method.
///
/// The message-only pool above cannot exercise `build_dispatch_table`, which walks a
/// **service**'s methods.
fn test_service_pool() -> prost_reflect::DescriptorPool {
    use prost::Message;
    use prost_reflect::prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
        MethodDescriptorProto, ServiceDescriptorProto, field_descriptor_proto,
    };

    let user_msg = DescriptorProto {
        name: Some("User".into()),
        field: vec![FieldDescriptorProto {
            name: Some("id".into()),
            number: Some(1),
            r#type: Some(field_descriptor_proto::Type::String.into()),
            label: Some(field_descriptor_proto::Label::Optional.into()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let service = ServiceDescriptorProto {
        name: Some("TestService".into()),
        method: vec![MethodDescriptorProto {
            name: Some("ListUsers".into()),
            input_type: Some(".test.User".into()),
            output_type: Some(".test.User".into()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("service.proto".into()),
        package: Some("test".into()),
        syntax: Some("proto3".into()),
        message_type: vec![user_msg],
        service: vec![service],
        ..Default::default()
    };

    let fds = FileDescriptorSet { file: vec![file] };
    prost_reflect::DescriptorPool::decode(fds.encode_to_vec().as_slice()).unwrap()
}

/// A schema with one `User` type and the named list query.
fn grpc_schema(
    query: fraiseql_core::schema::QueryDefinition,
) -> fraiseql_core::schema::CompiledSchema {
    use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};

    let mut schema = CompiledSchema::new();
    schema.types.push(TypeDefinition {
        fields: vec![FieldDefinition::new("id", FieldType::Id)],
        ..TypeDefinition::new("User", "v_user")
    });
    schema.queries.push(query);
    schema.build_indexes();
    schema
}

/// #1329: a function-backed query registers **no** gRPC method.
///
/// This table answers a method by reading `vr_<type.sql_source>` directly — the
/// resolver is never consulted. Registering a function-backed query would therefore
/// serve the type's rows in place of the function's computed answer: a wrong result
/// that looks like a right one, which is the worst shape a transport gap can take.
/// An absent method is the honest alternative.
#[test]
fn a_function_backed_query_registers_no_grpc_method() {
    use fraiseql_core::schema::QueryDefinition;

    let schema = grpc_schema(
        QueryDefinition::new("users", "User")
            .returning_list()
            .with_function("preview_users"),
    );
    let table =
        super::handler::build_dispatch_table(&schema, "test.TestService", &test_service_pool())
            .expect("the table builds");

    assert!(
        table.is_empty(),
        "a function-backed query must not become a gRPC method: {:?}",
        table.keys().collect::<Vec<_>>()
    );
}

/// The counterweight: the same shape, SQL-backed, **does** register.
///
/// Without it the test above would pass for a descriptor pool whose method never
/// matched a query at all — which is the way a skip-test most easily becomes
/// decorative.
#[test]
fn a_sql_backed_query_registers_a_grpc_method() {
    use fraiseql_core::schema::QueryDefinition;

    let schema = grpc_schema(
        QueryDefinition::new("users", "User").returning_list().with_sql_source("v_user"),
    );
    let table =
        super::handler::build_dispatch_table(&schema, "test.TestService", &test_service_pool())
            .expect("the table builds");

    assert_eq!(
        table.len(),
        1,
        "the SQL-backed sibling must still register: {:?}",
        table.keys().collect::<Vec<_>>()
    );
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

// ── recase_keys_to_snake (#456: gRPC single-JSONB arg recasing) ──────────

#[test]
fn recase_keys_to_snake_recases_nested_object_keys() {
    use serde_json::json;
    // A nested `input` message serializes with camelCase JSON names; its keys
    // must reach the SQL function as snake_case.
    let input = json!({ "shippingAddress": "1 Main St", "customerNote": "gift" });
    let recased = recase_keys_to_snake(input);
    assert_eq!(recased, json!({ "shipping_address": "1 Main St", "customer_note": "gift" }));
}

#[test]
fn recase_keys_to_snake_recurses_into_nested_objects_and_arrays() {
    use serde_json::json;
    let input = json!({
        "billingAddress": { "postalCode": "75001", "lineItems": [ { "skuId": "x" } ] }
    });
    let recased = recase_keys_to_snake(input);
    assert_eq!(
        recased,
        json!({
            "billing_address": { "postal_code": "75001", "line_items": [ { "sku_id": "x" } ] }
        })
    );
}

#[test]
fn recase_keys_to_snake_is_acronym_and_digit_aware() {
    use serde_json::json;
    // Same `to_snake_case` the read path uses → writes round-trip as reads.
    let input = json!({ "s3Key": "k", "dns1Id": "d" });
    let recased = recase_keys_to_snake(input);
    assert_eq!(recased, json!({ "s3_key": "k", "dns_1_id": "d" }));
}

#[test]
fn recase_keys_to_snake_leaves_scalars_and_snake_keys_untouched() {
    use serde_json::json;
    // Scalars carry no keys; already-snake keys are idempotent — so a flattened
    // positional scalar arg or a Preserve-authored object is unchanged.
    assert_eq!(recase_keys_to_snake(json!("u1")), json!("u1"));
    assert_eq!(recase_keys_to_snake(json!(42)), json!(42));
    assert_eq!(
        recase_keys_to_snake(json!({ "already_snake": 1 })),
        json!({ "already_snake": 1 })
    );
}

// ── #1330: the gRPC mutation path and the universal chokepoint ────────────
//
// `execute_grpc_mutation` calls the database function directly, so every gate
// enforced at `execute_mutation_impl` is skipped. Each case below is paired with
// its **positive twin**: a refusal on its own cannot be told apart from a gRPC
// path that refuses everything, and "the gate fires" is only meaningful beside
// "the same call succeeds when it should".
//
// The discriminator in both directions is whether the SQL function was reached:
// a gate that refuses must reach it **zero** times, and a gate that passes must
// reach it **once**. That is a property of the gate rather than of the response
// shape, so it holds without a faithful `app.mutation_response` fixture.
mod chokepoint {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use async_trait::async_trait;
    use chrono::Utc;
    use fraiseql_core::{
        db::{
            DatabaseAdapter, DatabaseType, SupportsMutations, WhereClause,
            types::{JsonbValue, OrderByClause, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::{
            CompiledSchema, FieldDefinition, FieldType, MutationDefinition, SqlProjectionHint,
            TypeDefinition,
        },
        security::{ActorType, SecurityContext},
    };
    use prost_reflect::DynamicMessage;
    use serde_json::Value as JsonValue;

    use super::{super::handler, test_descriptor_pool};

    /// The entity id the canned success row reports. UUID-shaped, because the
    /// chokepoint parses the column as one.
    const ENTITY_ID: &str = "3f2504e0-4f89-11d3-9a0c-0305e82c3301";

    /// A mutation-capable adapter that counts how often the SQL function ran.
    #[derive(Debug, Clone, Default)]
    struct RecordingAdapter {
        calls:   Arc<AtomicUsize>,
        /// When set, the function reports a refusal instead of a success.
        refusal: Option<(String, String)>,
    }

    impl RecordingAdapter {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        /// The function runs and declines the write: `succeeded = false` with an
        /// `error_class` and a message, which is a business outcome rather than a
        /// transport failure.
        fn refusing(error_class: &str, message: &str) -> Self {
            Self {
                refusal: Some((error_class.to_string(), message.to_string())),
                ..Self::default()
            }
        }
    }

    // Reason: DatabaseAdapter is defined with #[async_trait]; an implementation must
    // match its transformed signatures.
    #[async_trait]
    impl DatabaseAdapter for RecordingAdapter {
        // Writes: opted in, because both capability gates default to refusing.
        fn supports_mutations(&self) -> bool {
            true
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<HashMap<String, JsonValue>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[JsonValue],
        ) -> FraiseQLResult<Vec<HashMap<String, JsonValue>>> {
            Ok(vec![])
        }

        /// The one method that matters here: reaching it means every gate passed.
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[JsonValue],
        ) -> FraiseQLResult<Vec<HashMap<String, JsonValue>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // The canonical `app.mutation_response` shape the chokepoint parses.
            // The old direct-to-adapter path read `succeeded` out of whatever the
            // function returned; `execute_mutation_impl` deserializes the whole
            // row, so `state_changed` is required and `entity_id` must be a UUID.
            let mut row = HashMap::new();
            if let Some((error_class, message)) = &self.refusal {
                row.insert("succeeded".to_string(), JsonValue::Bool(false));
                row.insert("state_changed".to_string(), JsonValue::Bool(false));
                row.insert("error_class".to_string(), JsonValue::String(error_class.clone()));
                row.insert("message".to_string(), JsonValue::String(message.clone()));
                return Ok(vec![row]);
            }
            row.insert("succeeded".to_string(), JsonValue::Bool(true));
            row.insert("state_changed".to_string(), JsonValue::Bool(true));
            row.insert("entity_id".to_string(), JsonValue::String(ENTITY_ID.to_string()));
            Ok(vec![row])
        }
    }

    impl SupportsMutations for RecordingAdapter {}

    /// A schema with one `createUser` mutation, optionally gated.
    fn gated_schema(requires_role: Option<&str>, requires_actor: Vec<ActorType>) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition {
            fields: vec![FieldDefinition::new("id", FieldType::Id)],
            ..TypeDefinition::new("User", "v_user")
        });
        let mut m = MutationDefinition::new("createUser", "User");
        m.sql_source = Some("fn_create_user".to_string());
        m.requires_role = requires_role.map(ToString::to_string);
        m.requires_actor = requires_actor;
        schema.mutations.push(m);
        schema.build_indexes();
        schema
    }

    fn principal(roles: &[&str]) -> SecurityContext {
        SecurityContext {
            user_id:          "grpc-caller".into(),
            roles:            roles.iter().map(ToString::to_string).collect(),
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-grpc".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    /// A request message carrying one field, as a gRPC mutation call would.
    fn request() -> DynamicMessage {
        let pool = test_descriptor_pool();
        let desc = pool.get_message_by_name("test.User").unwrap();
        let field = desc.get_field_by_name("name").unwrap();
        let mut msg = DynamicMessage::new(desc);
        msg.set_field(&field, prost_reflect::Value::String("Alice".to_string()));
        msg
    }

    #[tokio::test]
    async fn a_grpc_mutation_without_the_required_role_is_refused() {
        let adapter = RecordingAdapter::default();
        let executor =
            Executor::new(gated_schema(Some("writer"), vec![]), Arc::new(adapter.clone()));
        let caller = principal(&["viewer"]);

        let result = handler::execute_grpc_mutation(
            &executor,
            "createUser",
            &request(),
            false,
            Some(&caller),
        )
        .await;

        assert!(result.is_err(), "a caller without `writer` must be refused");
        assert_eq!(
            adapter.calls(),
            0,
            "and refused BEFORE the write — a gate that runs after the function has already \
             written is not a gate"
        );
    }

    #[tokio::test]
    async fn a_grpc_mutation_with_the_required_role_reaches_the_write() {
        let adapter = RecordingAdapter::default();
        let executor =
            Executor::new(gated_schema(Some("writer"), vec![]), Arc::new(adapter.clone()));
        let caller = principal(&["writer"]);

        let outcome = handler::execute_grpc_mutation(
            &executor,
            "createUser",
            &request(),
            false,
            Some(&caller),
        )
        .await;

        assert!(
            outcome.is_ok(),
            "the positive twin: without it, `refused` cannot be told from `gRPC is broken` — \
             got {outcome:?}"
        );
        assert_eq!(adapter.calls(), 1);
    }

    #[tokio::test]
    async fn a_grpc_mutation_requiring_an_actor_is_refused_without_one() {
        let adapter = RecordingAdapter::default();
        let executor = Executor::new(
            gated_schema(None, vec![ActorType::HumanUser]),
            Arc::new(adapter.clone()),
        );

        let result =
            handler::execute_grpc_mutation(&executor, "createUser", &request(), false, None).await;

        assert!(result.is_err(), "#966: an unauthenticated caller cannot satisfy requires_actor");
        assert_eq!(adapter.calls(), 0);
    }

    #[tokio::test]
    async fn a_grpc_mutation_requiring_an_actor_proceeds_with_one() {
        let adapter = RecordingAdapter::default();
        let executor = Executor::new(
            gated_schema(None, vec![ActorType::HumanUser]),
            Arc::new(adapter.clone()),
        );
        let caller = principal(&[]);

        let _ = handler::execute_grpc_mutation(
            &executor,
            "createUser",
            &request(),
            false,
            Some(&caller),
        )
        .await;

        assert_eq!(adapter.calls(), 1);
    }

    #[tokio::test]
    async fn an_ungated_grpc_mutation_still_reaches_the_write() {
        // The control: routing through the chokepoint must not refuse a mutation
        // that declares no gate at all.
        let adapter = RecordingAdapter::default();
        let executor = Executor::new(gated_schema(None, vec![]), Arc::new(adapter.clone()));

        let _ =
            handler::execute_grpc_mutation(&executor, "createUser", &request(), false, None).await;

        assert_eq!(adapter.calls(), 1);
    }

    // ── #1330 cycle 2: the response contract ──────────────────────────────

    #[tokio::test]
    async fn a_successful_mutation_reports_the_entity_id() {
        let adapter = RecordingAdapter::default();
        let executor = Executor::new(gated_schema(None, vec![]), Arc::new(adapter.clone()));

        let result =
            handler::execute_grpc_mutation(&executor, "createUser", &request(), false, None)
                .await
                .expect("an ungated mutation succeeds");

        assert!(result.success);
        assert_eq!(
            result.id.as_deref(),
            Some(ENTITY_ID),
            "the caller has to learn the id of the row it just created"
        );
    }

    #[tokio::test]
    async fn a_declined_mutation_is_a_response_not_a_transport_error() {
        // `succeeded = false` is the write refusing on a business rule. It is an
        // answer, so it travels as a populated MutationResponse — a gRPC error
        // status would tell the client the call failed, which is a different and
        // wrong thing to retry.
        let adapter = RecordingAdapter::refusing("conflict", "email already exists");
        let executor = Executor::new(gated_schema(None, vec![]), Arc::new(adapter.clone()));

        let result =
            handler::execute_grpc_mutation(&executor, "createUser", &request(), false, None)
                .await
                .expect("a declined write is still a completed call");

        assert!(!result.success, "the envelope carries the refusal");
        assert_eq!(result.error.as_deref(), Some("email already exists"));
        assert_eq!(adapter.calls(), 1, "and the function did run");
    }
}

// ── #1336 / #858: the principal this transport dispatches with ──────────────
//
// gRPC has been wrong about its own principal twice, in the same place and for the
// same reason: it built one itself instead of using the shared producer. #858 fixed
// `tenant_id` and `attributes` for MCP and left gRPC on `SecurityContext::from_user`;
// #1336 added enrichment to every transport and gRPC was again the one that had
// nothing to add it to. These drive `principal_from_user` — the whole producer below
// token validation — with no descriptor pool, dispatch table or adapter, because it
// reads none of them.
#[cfg(feature = "auth")]
mod principal_production {
    use fraiseql_core::{security::AuthenticatedUser, types::UserId};
    use serde_json::json;

    use crate::{identity::tests as identity_fixtures, routes::grpc::DynamicGrpcService};

    /// The service type is only a carrier here: `principal_from_user` is an associated
    /// function that takes the resolver explicitly, so `A` is never touched.
    type Svc = DynamicGrpcService;

    /// A validated token's user, carrying the `org_id` claim a multi-tenant
    /// deployment scopes on.
    fn user() -> AuthenticatedUser {
        let mut extra = std::collections::HashMap::new();
        extra.insert("org_id".to_string(), json!("tenant-a"));
        extra.insert("department".to_string(), json!("ops"));
        AuthenticatedUser {
            user_id:      UserId("u1".to_string()),
            email:        Some("u1@example.test".to_string()),
            display_name: None,
            scopes:       Vec::new(),
            expires_at:   chrono::Utc::now() + chrono::Duration::hours(1),
            extra_claims: extra,
        }
    }

    #[tokio::test]
    async fn the_principal_carries_the_tokens_tenant_and_claims() {
        // #858's fix, which never reached this transport: `SecurityContext::from_user`
        // leaves `tenant_id` unset and `attributes` empty, so `org_id` never became a
        // tenant and every `SessionVariableSource::Jwt` mapping resolved to nothing on
        // gRPC. Asserted without a resolver, because it is true with enrichment off.
        let ctx = Svc::principal_from_user(None, &user(), "req-1".to_string())
            .await
            .expect("no resolver configured — nothing to refuse");

        assert_eq!(
            ctx.tenant_id.as_ref().map(|t| t.0.as_str()),
            Some("tenant-a"),
            "the JWT's org_id must become the tenant, as it does on /graphql and MCP"
        );
        assert_eq!(
            ctx.attributes.get("department"),
            Some(&json!("ops")),
            "extra claims must reach `attributes`, or every jwt: session-variable \
             mapping resolves to nothing on this transport"
        );
        assert_eq!(ctx.transport(), Some("grpc"), "and the ingress door is recorded (#376)");
    }

    #[tokio::test]
    async fn a_resolved_subject_proceeds_and_carries_its_enriched_fields() {
        // The positive twin. Without it, a producer that refused unconditionally would
        // satisfy both refusal cases below.
        let resolver = identity_fixtures::resolver_returning(&[
            ("actor_id", json!("a-1")),
            ("actor_role", json!("admin")),
        ]);

        let ctx = Svc::principal_from_user(Some(&resolver), &user(), "req-2".to_string())
            .await
            .expect("a subject the actor table knows must proceed");

        assert_eq!(
            ctx.attributes.get("fraiseql.enriched.actor_id"),
            Some(&json!("a-1")),
            "the resolved identity must be merged under the forge-proof namespace"
        );
        assert_eq!(
            ctx.enrichment_mark(),
            Some(fraiseql_core::security::EnrichmentMark::Resolved),
            "and the principal must record that it passed the seam, or the engine \
             refuses it downstream"
        );
    }

    #[tokio::test]
    async fn an_unknown_subject_is_permission_denied() {
        // Zero rows is a denial. Before #1336 this transport built the context and
        // dispatched it, so an unprovisioned subject reached the data.
        let resolver = identity_fixtures::resolver_returning(&[]);

        let response = Svc::principal_from_user(Some(&resolver), &user(), "req-3".to_string())
            .await
            .expect_err("a subject the actor table does not know must be refused");

        assert_eq!(
            response.headers().get("grpc-status").map(|v| v.to_str().unwrap()),
            Some("7"),
            "PERMISSION_DENIED — the gRPC spelling of the 403 /graphql answers"
        );
    }

    #[tokio::test]
    async fn a_resolver_outage_is_unavailable_not_denied() {
        // The two must not collapse: a denial is final and a client must not retry it,
        // an outage is transient and a client should.
        let resolver = identity_fixtures::resolver_unavailable();

        let response = Svc::principal_from_user(Some(&resolver), &user(), "req-4".to_string())
            .await
            .expect_err("a resolver outage must never fall through to an unscoped query");

        assert_eq!(
            response.headers().get("grpc-status").map(|v| v.to_str().unwrap()),
            Some("14"),
            "UNAVAILABLE — distinct from PERMISSION_DENIED (7), which is what a \
             collapsed mapping would send"
        );
    }
}
