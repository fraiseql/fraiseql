//! Tests for compiled schema types.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use field_type::{DistanceMetric, VectorConfig, VectorIndexType};

use super::*;
use crate::validation::CustomTypeRegistry;

#[test]
fn test_empty_schema_creation() {
    let schema = CompiledSchema::new();
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
    assert!(schema.mutations.is_empty());
    assert!(schema.subscriptions.is_empty());
    assert_eq!(schema.operation_count(), 0);
}

#[test]
fn test_schema_from_json_empty() {
    let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
    let schema = CompiledSchema::from_json(json, false).unwrap();
    assert!(schema.types.is_empty());
}

#[test]
fn test_schema_from_json_with_defaults() {
    // Minimal JSON - all fields should default
    let json = r"{}";
    let schema = CompiledSchema::from_json(json, false).unwrap();
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
}

#[test]
#[allow(clippy::cognitive_complexity)] // Reason: comprehensive schema deserialization test with many field assertions
fn test_schema_from_json_full() {
    let json = r#"{
        "types": [{
            "name": "User",
            "sql_source": "v_user",
            "jsonb_column": "data",
            "fields": [
                {"name": "id", "field_type": "ID", "nullable": false},
                {"name": "email", "field_type": "String", "nullable": false},
                {"name": "name", "field_type": "String", "nullable": true}
            ],
            "description": "A user in the system"
        }],
        "queries": [{
            "name": "users",
            "return_type": "User",
            "returns_list": true,
            "nullable": false,
            "sql_source": "v_user",
            "auto_params": {
                "has_where": true,
                "has_order_by": true,
                "has_limit": true,
                "has_offset": true
            }
        }, {
            "name": "user",
            "return_type": "User",
            "returns_list": false,
            "nullable": true,
            "arguments": [
                {"name": "id", "arg_type": "ID", "nullable": false}
            ]
        }],
        "mutations": [{
            "name": "createUser",
            "return_type": "User",
            "sql_source": "fn_create_user",
            "arguments": [
                {"name": "email", "arg_type": "String", "nullable": false}
            ],
            "operation": {"Insert": {"table": "users"}}
        }],
        "subscriptions": [{
            "name": "userCreated",
            "return_type": "User",
            "topic": "user_created"
        }]
    }"#;

    let schema = CompiledSchema::from_json(json, false).unwrap();

    // Check types
    assert_eq!(schema.types.len(), 1);
    let user_type = &schema.types[0];
    assert_eq!(user_type.name, "User");
    assert_eq!(user_type.sql_source, "v_user");
    assert_eq!(user_type.jsonb_column, "data");
    assert_eq!(user_type.fields.len(), 3);
    assert_eq!(user_type.description, Some("A user in the system".to_string()));
    assert!(!user_type.is_error);
    assert!(!user_type.relay);
    assert!(user_type.requires_role.is_none());
    assert!(user_type.implements.is_empty());

    // Check queries
    assert_eq!(schema.queries.len(), 2);
    let users_query = schema.find_query("users").unwrap();
    assert!(users_query.returns_list);
    assert_eq!(users_query.sql_source.as_deref(), Some("v_user"));
    assert!(users_query.auto_params.has_where);
    assert!(users_query.auto_params.has_order_by);
    assert!(users_query.auto_params.has_limit);
    assert!(users_query.auto_params.has_offset);
    assert!(!users_query.relay);
    assert!(users_query.relay_cursor_column.is_none());
    assert_eq!(users_query.relay_cursor_type, CursorType::Int64);
    assert!(users_query.inject_params.is_empty());
    assert!(users_query.cache_ttl_seconds.is_none());
    assert!(users_query.additional_views.is_empty());
    assert!(users_query.requires_role.is_none());
    assert!(users_query.deprecation.is_none());

    let user_query = schema.find_query("user").unwrap();
    assert!(!user_query.returns_list);
    assert!(user_query.nullable);
    assert_eq!(user_query.arguments.len(), 1);

    // Check mutations
    assert_eq!(schema.mutations.len(), 1);
    let create_user = schema.find_mutation("createUser").unwrap();
    // sql_source regression-proof against issue #53
    assert_eq!(create_user.sql_source.as_deref(), Some("fn_create_user"));
    assert_eq!(create_user.arguments.len(), 1);
    assert!(matches!(
        &create_user.operation,
        MutationOperation::Insert { table } if table == "users"
    ));
    assert!(create_user.inject_params.is_empty());
    assert!(create_user.invalidates_fact_tables.is_empty());
    assert!(create_user.invalidates_views.is_empty());
    assert!(create_user.deprecation.is_none());

    // Check subscriptions
    assert_eq!(schema.subscriptions.len(), 1);
    let sub = schema.find_subscription("userCreated").unwrap();
    assert_eq!(sub.topic, Some("user_created".to_string()));
}

/// Assert `inject_params`, `cache_ttl_seconds`, `additional_views`, `requires_role`
#[test]
fn test_query_full_fields() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/golden/05-security-inject-cache.json");
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Cannot read fixture 05: {e}"));
    let schema = CompiledSchema::from_json(&json, false).unwrap();

    let q = schema.find_query("orders").unwrap();
    assert_eq!(q.inject_params.len(), 1);
    let src = q.inject_params.get("tenant_id").unwrap();
    assert_eq!(*src, InjectedParamSource::Jwt("tenant_id".to_string()));
    assert_eq!(q.cache_ttl_seconds, Some(300));
    assert_eq!(q.additional_views, vec!["v_order_summary", "v_order_items"]);
    assert_eq!(q.requires_role.as_deref(), Some("admin"));
}

/// Assert `sql_source`, `inject_params`, invalidates_* on a mutation
#[test]
fn test_mutation_full_fields() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/golden/05-security-inject-cache.json");
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Cannot read fixture 05: {e}"));
    let schema = CompiledSchema::from_json(&json, false).unwrap();

    let m = schema.find_mutation("createOrder").unwrap();
    // sql_source — regression-proof against issue #53
    assert_eq!(m.sql_source.as_deref(), Some("fn_create_order"));
    assert_eq!(m.inject_params.len(), 2);
    assert!(m.inject_params.contains_key("user_id"));
    assert!(m.inject_params.contains_key("tenant_id"));
    assert_eq!(m.invalidates_fact_tables, vec!["tf_sales", "tf_order_count"]);
    assert_eq!(m.invalidates_views, vec!["v_order_summary", "v_order_items"]);
}

#[test]
fn test_schema_to_json_roundtrip() {
    let schema = CompiledSchema {
        types: vec![
            TypeDefinition::new("User", "v_user")
                .with_field(FieldDefinition::new("id", FieldType::Id))
                .with_field(FieldDefinition::new("email", FieldType::String)),
        ],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![QueryDefinition::new("users", "User").returning_list()],
        mutations: vec![],
        subscriptions: vec![],
        directives: vec![],
        fact_tables: std::collections::HashMap::new(),
        observers: Vec::new(),
        federation: None,
        security: None,
        observers_config: None,
        subscriptions_config: None,
        validation_config: None,
        debug_config: None,
        mcp_config: None,
        schema_format_version: None,
        schema_sdl: None,
        custom_scalars: CustomTypeRegistry::default(),
        ..CompiledSchema::default()
    };

    let json = schema.to_json().unwrap();
    let parsed = CompiledSchema::from_json(&json, false).unwrap();

    assert_eq!(schema, parsed);
}

#[test]
fn test_schema_validation_duplicate_types() {
    let schema = CompiledSchema {
        types: vec![
            TypeDefinition::new("User", "v_user"),
            TypeDefinition::new("User", "v_user2"), // Duplicate!
        ],
        ..Default::default()
    };

    let result = schema.validate();
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Duplicate type name: User")));
}

#[test]
fn test_schema_validation_undefined_type_reference() {
    let schema = CompiledSchema {
        types: vec![TypeDefinition::new("User", "v_user")],
        queries: vec![QueryDefinition::new("posts", "Post")], // Post not defined!
        ..Default::default()
    };

    let result = schema.validate();
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("undefined type 'Post'")));
}

#[test]
fn test_schema_validation_success() {
    let schema = CompiledSchema {
        types: vec![TypeDefinition::new("User", "v_user")],
        queries: vec![QueryDefinition::new("users", "User")],
        ..Default::default()
    };

    schema.validate().unwrap_or_else(|e| panic!("expected valid schema: {e:?}"));
}

#[test]
fn test_schema_validation_builtin_types_ok() {
    // Queries returning built-in types should pass validation
    let schema = CompiledSchema {
        types: vec![],
        queries: vec![
            QueryDefinition::new("version", "String"),
            QueryDefinition::new("count", "Int"),
            QueryDefinition::new("active", "Boolean"),
        ],
        ..Default::default()
    };

    schema
        .validate()
        .unwrap_or_else(|e| panic!("expected built-in types to pass validation: {e:?}"));
}

#[test]
fn test_field_type_serialization() {
    // Test that field types serialize correctly for cross-language compat

    let field = FieldDefinition::new("id", FieldType::Id);
    let json = serde_json::to_string(&field).unwrap();
    assert!(json.contains(r#""field_type":"ID""#));

    let list_field = FieldDefinition::new("tags", FieldType::List(Box::new(FieldType::String)));
    let list_json = serde_json::to_string(&list_field).unwrap();
    assert!(list_json.contains(r#""field_type":{"List":"String"}"#));

    let obj_field = FieldDefinition::new("author", FieldType::Object("User".to_string()));
    let obj_json = serde_json::to_string(&obj_field).unwrap();
    assert!(obj_json.contains(r#""field_type":{"Object":"User"}"#));
}

#[test]
fn test_field_type_is_scalar() {
    assert!(FieldType::String.is_scalar());
    assert!(FieldType::Int.is_scalar());
    assert!(FieldType::Boolean.is_scalar());
    assert!(FieldType::Id.is_scalar());
    assert!(FieldType::DateTime.is_scalar());
    assert!(FieldType::Json.is_scalar());

    assert!(!FieldType::List(Box::new(FieldType::String)).is_scalar());
    assert!(!FieldType::Object("User".to_string()).is_scalar());
}

#[test]
fn test_field_type_to_graphql_string() {
    assert_eq!(FieldType::String.to_graphql_string(), "String");
    assert_eq!(FieldType::Int.to_graphql_string(), "Int");
    assert_eq!(FieldType::Id.to_graphql_string(), "ID");

    assert_eq!(FieldType::List(Box::new(FieldType::String)).to_graphql_string(), "[String]");

    assert_eq!(
        FieldType::List(Box::new(FieldType::List(Box::new(FieldType::Int)))).to_graphql_string(),
        "[[Int]]"
    );

    assert_eq!(FieldType::Object("User".to_string()).to_graphql_string(), "User");
}

#[test]
fn test_type_definition_builder() {
    let user = TypeDefinition::new("User", "v_user")
        .with_jsonb_column("payload")
        .with_description("A user entity")
        .with_field(FieldDefinition::new("id", FieldType::Id))
        .with_field(
            FieldDefinition::nullable("email", FieldType::String).with_description("Email"),
        );

    assert_eq!(user.name, "User");
    assert_eq!(user.sql_source, "v_user");
    assert_eq!(user.jsonb_column, "payload");
    assert_eq!(user.description, Some("A user entity".to_string()));
    assert_eq!(user.fields.len(), 2);

    let email_field = user.find_field("email").unwrap();
    assert!(email_field.nullable);
    assert_eq!(email_field.description, Some("Email".to_string()));
}

#[test]
fn test_mutation_operation_serialization() {
    let insert = MutationOperation::Insert {
        table: "users".to_string(),
    };
    let json = serde_json::to_string(&insert).unwrap();
    assert_eq!(json, r#"{"Insert":{"table":"users"}}"#);

    let custom = MutationOperation::Custom;
    let json = serde_json::to_string(&custom).unwrap();
    assert_eq!(json, r#""Custom""#);
}

#[test]
fn test_auto_params_presets() {
    let all = AutoParams::all();
    assert!(all.has_where);
    assert!(all.has_order_by);
    assert!(all.has_limit);
    assert!(all.has_offset);

    let none = AutoParams::none();
    assert!(!none.has_where);
    assert!(!none.has_order_by);
    assert!(!none.has_limit);
    assert!(!none.has_offset);
}

#[test]
fn test_argument_definition() {
    let required = ArgumentDefinition::new("id", FieldType::Id);
    assert!(!required.nullable);

    let optional = ArgumentDefinition::optional("limit", FieldType::Int);
    assert!(optional.nullable);
}

#[test]
fn test_operation_count() {
    let schema = CompiledSchema {
        types: vec![TypeDefinition::new("User", "v_user")],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![
            QueryDefinition::new("users", "User"),
            QueryDefinition::new("user", "User"),
        ],
        mutations: vec![MutationDefinition::new("createUser", "User")],
        subscriptions: vec![SubscriptionDefinition::new("userCreated", "User")],
        directives: vec![],
        fact_tables: std::collections::HashMap::new(),
        observers: Vec::new(),
        federation: None,
        security: None,
        observers_config: None,
        subscriptions_config: None,
        validation_config: None,
        debug_config: None,
        mcp_config: None,
        schema_format_version: None,
        schema_sdl: None,
        custom_scalars: CustomTypeRegistry::default(),
        ..CompiledSchema::default()
    };

    assert_eq!(schema.operation_count(), 4); // 2 queries + 1 mutation + 1 subscription
}

/// Test that JSON emitted by the Python authoring library can be parsed.
///
/// This is a critical cross-language compatibility test for the Schema Freeze
/// architecture. The JSON format below is exactly what the Python decorator library
/// emits into `schema.json` — if this test fails, Python→CLI→Rust interop is broken.
#[test]
fn test_python_generated_json_compat() {
    // This JSON matches what the Python authoring decorators emit into schema.json
    let python_json = r#"{
  "types": [
    {
      "name": "User",
      "sql_source": "v_user",
      "jsonb_column": "data",
      "fields": [
        {
          "name": "id",
          "field_type": "ID",
          "nullable": false
        },
        {
          "name": "name",
          "field_type": "String",
          "nullable": false
        },
        {
          "name": "email",
          "field_type": "String",
          "nullable": true
        }
      ],
      "description": "A user in the system."
    }
  ],
  "queries": [
    {
      "name": "get_users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "description": "Get all users."
    },
    {
      "name": "get_user",
      "return_type": "User",
      "returns_list": false,
      "nullable": true,
      "arguments": [
        {
          "name": "id",
          "arg_type": "ID",
          "nullable": false
        }
      ],
      "description": "Get a single user by ID."
    }
  ],
  "mutations": [
    {
      "name": "create_user",
      "return_type": "User",
      "arguments": [
        {
          "name": "name",
          "arg_type": "String",
          "nullable": false
        },
        {
          "name": "email",
          "arg_type": "String",
          "nullable": false
        }
      ],
      "description": "Create a new user.",
      "operation": "Custom"
    }
  ],
  "subscriptions": []
}"#;

    // Parse the JSON - this must succeed for Python/Rust interop to work
    let schema = CompiledSchema::from_json(python_json, false)
        .expect("Python-generated JSON should parse successfully");

    // Verify types
    assert_eq!(schema.types.len(), 1);
    let user_type = &schema.types[0];
    assert_eq!(user_type.name, "User");
    assert_eq!(user_type.sql_source, "v_user");
    assert_eq!(user_type.jsonb_column, "data");
    assert_eq!(user_type.fields.len(), 3);
    assert_eq!(user_type.description, Some("A user in the system.".to_string()));

    // Verify fields
    let id_field = user_type.find_field("id").expect("id field");
    assert_eq!(id_field.field_type, super::field_type::FieldType::Id);
    assert!(!id_field.nullable);

    let email_field = user_type.find_field("email").expect("email field");
    assert!(email_field.nullable);

    // Verify queries
    assert_eq!(schema.queries.len(), 2);

    let get_users = schema.find_query("get_users").expect("get_users query");
    assert_eq!(get_users.return_type, "User");
    assert!(get_users.returns_list);
    assert!(!get_users.nullable);

    let get_user = schema.find_query("get_user").expect("get_user query");
    assert!(!get_user.returns_list);
    assert!(get_user.nullable);
    assert_eq!(get_user.arguments.len(), 1);
    assert_eq!(get_user.arguments[0].name, "id");

    // Verify mutations
    assert_eq!(schema.mutations.len(), 1);
    let create_user = schema.find_mutation("create_user").expect("create_user mutation");
    assert_eq!(create_user.return_type, "User");
    assert_eq!(create_user.arguments.len(), 2);
    assert!(matches!(create_user.operation, MutationOperation::Custom));

    // Verify subscriptions (empty)
    assert!(schema.subscriptions.is_empty());

    // Verify validation passes
    schema
        .validate()
        .unwrap_or_else(|e| panic!("expected Python-generated schema to pass validation: {e:?}"));
}

// ============================================================================
// Vector Types Tests
// ============================================================================

#[test]
fn test_vector_config_creation() {
    let config = VectorConfig::new(1536);
    assert_eq!(config.dimensions, 1536);
    assert_eq!(config.index_type, VectorIndexType::Hnsw);
    assert_eq!(config.distance_metric, DistanceMetric::Cosine);
}

#[test]
fn test_vector_config_openai() {
    let config = VectorConfig::openai();
    assert_eq!(config.dimensions, 1536);
    assert_eq!(config.index_type, VectorIndexType::Hnsw);
    assert_eq!(config.distance_metric, DistanceMetric::Cosine);
}

#[test]
fn test_vector_config_openai_small() {
    let config = VectorConfig::openai_small();
    assert_eq!(config.dimensions, 512);
    assert_eq!(config.index_type, VectorIndexType::Hnsw);
    assert_eq!(config.distance_metric, DistanceMetric::Cosine);
}

#[test]
fn test_vector_config_builder() {
    let config = VectorConfig::new(768)
        .with_index(VectorIndexType::IvfFlat)
        .with_distance(DistanceMetric::L2);

    assert_eq!(config.dimensions, 768);
    assert_eq!(config.index_type, VectorIndexType::IvfFlat);
    assert_eq!(config.distance_metric, DistanceMetric::L2);
}

#[test]
fn test_distance_metric_operators() {
    assert_eq!(DistanceMetric::Cosine.operator(), "<=>");
    assert_eq!(DistanceMetric::L2.operator(), "<->");
    assert_eq!(DistanceMetric::InnerProduct.operator(), "<#>");
}

#[test]
fn test_distance_metric_ops_classes() {
    assert_eq!(DistanceMetric::Cosine.hnsw_ops_class(), "vector_cosine_ops");
    assert_eq!(DistanceMetric::L2.hnsw_ops_class(), "vector_l2_ops");
    assert_eq!(DistanceMetric::InnerProduct.hnsw_ops_class(), "vector_ip_ops");
}

#[test]
fn test_vector_index_sql() {
    let hnsw_sql =
        VectorIndexType::Hnsw.index_sql("documents", "embedding", DistanceMetric::Cosine);
    assert_eq!(
        hnsw_sql,
        Some("CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops)".to_string())
    );

    let ivf_sql = VectorIndexType::IvfFlat.index_sql("docs", "vec", DistanceMetric::L2);
    assert_eq!(
        ivf_sql,
        Some("CREATE INDEX ON docs USING ivfflat (vec vector_l2_ops)".to_string())
    );

    let none_sql = VectorIndexType::None.index_sql("t", "c", DistanceMetric::Cosine);
    assert_eq!(none_sql, None);
}

#[test]
fn test_field_definition_vector() {
    let embedding = FieldDefinition::vector("embedding", VectorConfig::openai());

    assert_eq!(embedding.name, "embedding");
    assert!(embedding.is_vector());
    assert!(matches!(embedding.field_type, FieldType::Vector));
    assert!(!embedding.nullable);

    let config = embedding.vector_config.expect("should have vector config");
    assert_eq!(config.dimensions, 1536);
}

#[test]
fn test_field_type_vector_is_scalar() {
    assert!(FieldType::Vector.is_scalar());
    assert!(FieldType::Vector.is_vector());
    assert!(!FieldType::String.is_vector());
}

#[test]
fn test_field_type_vector_graphql_string() {
    assert_eq!(FieldType::Vector.to_graphql_string(), "[Float!]!");
}

#[test]
fn test_field_type_vector_sql_type() {
    let config = VectorConfig::new(1536);
    assert_eq!(FieldType::Vector.to_sql_type(Some(&config)), "vector(1536)");
    assert_eq!(FieldType::Vector.to_sql_type(None), "vector");
}

#[test]
fn test_vector_config_serialization() {
    let config = VectorConfig {
        dimensions:      1536,
        index_type:      VectorIndexType::Hnsw,
        distance_metric: DistanceMetric::Cosine,
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains(r#""dimensions":1536"#));
    assert!(json.contains(r#""index_type":"hnsw""#));
    assert!(json.contains(r#""distance_metric":"cosine""#));

    let parsed: VectorConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, config);
}

#[test]
fn test_vector_config_all_distance_metrics() {
    let metrics = [
        (DistanceMetric::Cosine, "cosine"),
        (DistanceMetric::L2, "l2"),
        (DistanceMetric::InnerProduct, "inner_product"),
    ];

    for (metric, expected_name) in metrics {
        let json = serde_json::to_string(&metric).unwrap();
        assert_eq!(json, format!(r#""{}""#, expected_name));

        let parsed: DistanceMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, metric);
    }
}

#[test]
fn test_vector_config_all_index_types() {
    let types = [
        (VectorIndexType::Hnsw, "hnsw"),
        (VectorIndexType::IvfFlat, "ivf_flat"),
        (VectorIndexType::None, "none"),
    ];

    for (index_type, expected_name) in types {
        let json = serde_json::to_string(&index_type).unwrap();
        assert_eq!(json, format!(r#""{}""#, expected_name));

        let parsed: VectorIndexType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, index_type);
    }
}

#[test]
fn test_field_definition_with_vector_config() {
    let field = FieldDefinition::new("data", FieldType::Vector)
        .with_vector_config(VectorConfig::openai())
        .with_description("Document embedding");

    assert_eq!(field.name, "data");
    assert!(field.is_vector());
    assert_eq!(field.description, Some("Document embedding".to_string()));

    let config = field.vector_config.expect("should have vector config");
    assert_eq!(config.dimensions, 1536);
}

#[test]
fn test_schema_with_vector_field_json() {
    let json = r#"{
        "types": [{
            "name": "Document",
            "sql_source": "documents",
            "fields": [
                {"name": "id", "field_type": "ID", "nullable": false},
                {"name": "content", "field_type": "String", "nullable": false},
                {
                    "name": "embedding",
                    "field_type": "Vector",
                    "nullable": false,
                    "vector_config": {
                        "dimensions": 1536,
                        "index_type": "hnsw",
                        "distance_metric": "cosine"
                    }
                }
            ]
        }],
        "queries": [],
        "mutations": [],
        "subscriptions": []
    }"#;

    let schema = CompiledSchema::from_json(json, false).unwrap();

    assert_eq!(schema.types.len(), 1);
    let doc_type = &schema.types[0];
    assert_eq!(doc_type.name, "Document");
    assert_eq!(doc_type.fields.len(), 3);

    let embedding_field = doc_type.find_field("embedding").expect("embedding field");
    assert!(embedding_field.is_vector());
    assert!(matches!(embedding_field.field_type, FieldType::Vector));

    let config = embedding_field.vector_config.as_ref().expect("should have vector config");
    assert_eq!(config.dimensions, 1536);
    assert_eq!(config.index_type, VectorIndexType::Hnsw);
    assert_eq!(config.distance_metric, DistanceMetric::Cosine);
}

#[test]
fn test_vector_field_roundtrip() {
    let schema = CompiledSchema {
        types: vec![
            TypeDefinition::new("Document", "documents")
                .with_field(FieldDefinition::new("id", FieldType::Id))
                .with_field(
                    FieldDefinition::vector("embedding", VectorConfig::openai())
                        .with_description("OpenAI embedding"),
                ),
        ],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![],
        mutations: vec![],
        subscriptions: vec![],
        directives: vec![],
        fact_tables: std::collections::HashMap::new(),
        observers: Vec::new(),
        federation: None,
        security: None,
        observers_config: None,
        subscriptions_config: None,
        validation_config: None,
        debug_config: None,
        mcp_config: None,
        schema_format_version: None,
        schema_sdl: None,
        custom_scalars: CustomTypeRegistry::default(),
        ..CompiledSchema::default()
    };

    let json = schema.to_json().unwrap();
    let parsed = CompiledSchema::from_json(&json, false).unwrap();

    assert_eq!(schema, parsed);

    let doc_type = &parsed.types[0];
    let embedding = doc_type.find_field("embedding").unwrap();
    assert!(embedding.is_vector());
    assert_eq!(embedding.vector_config.as_ref().unwrap().dimensions, 1536);
}

/// Test that vector schema JSON emitted by the Python authoring library parses correctly.
///
/// This JSON is exactly what the Python decorator library emits for vector fields.
/// If this test fails, Python→CLI→Rust interop for vectors is broken.
#[test]
fn test_python_generated_vector_schema_compat() {
    // This JSON matches what the Python authoring decorators emit for vector fields
    let python_json = r#"{
        "types": [{
            "name": "Document",
            "sql_source": "documents",
            "jsonb_column": "data",
            "fields": [
                {"name": "id", "field_type": "String", "nullable": false},
                {"name": "content", "field_type": "String", "nullable": false},
                {
                    "name": "embedding",
                    "field_type": "Vector",
                    "nullable": false,
                    "vector_config": {
                        "dimensions": 1536,
                        "index_type": "hnsw",
                        "distance_metric": "cosine"
                    },
                    "description": "OpenAI embedding"
                }
            ],
            "description": "A document with vector embedding."
        }],
        "queries": [],
        "mutations": [],
        "subscriptions": []
    }"#;

    // Parse should succeed
    let schema = CompiledSchema::from_json(python_json, false)
        .expect("Python-generated vector schema should parse");

    // Verify type
    assert_eq!(schema.types.len(), 1);
    let doc_type = &schema.types[0];
    assert_eq!(doc_type.name, "Document");
    assert_eq!(doc_type.sql_source, "documents");

    // Verify vector field
    let embedding = doc_type.find_field("embedding").expect("embedding field");
    assert!(embedding.is_vector());
    assert!(matches!(embedding.field_type, FieldType::Vector));
    assert!(!embedding.nullable);
    assert_eq!(embedding.description, Some("OpenAI embedding".to_string()));

    // Verify vector config
    let config = embedding.vector_config.as_ref().expect("vector_config");
    assert_eq!(config.dimensions, 1536);
    assert_eq!(config.index_type, VectorIndexType::Hnsw);
    assert_eq!(config.distance_metric, DistanceMetric::Cosine);

    // Verify validation passes
    schema.validate().unwrap_or_else(|e| {
        panic!("expected Python-generated vector schema to pass validation: {e:?}")
    });
}

#[test]
fn test_compiled_schema_has_version_after_stamp() {
    let schema = CompiledSchema {
        schema_format_version: Some(CURRENT_SCHEMA_FORMAT_VERSION),
        ..Default::default()
    };
    let json = serde_json::to_string(&schema).unwrap();
    let reloaded: CompiledSchema = serde_json::from_str(&json).unwrap();
    assert_eq!(reloaded.schema_format_version, Some(CURRENT_SCHEMA_FORMAT_VERSION));
    reloaded
        .validate_format_version()
        .unwrap_or_else(|e| panic!("expected current version to pass format validation: {e:?}"));
}

#[test]
fn test_future_schema_version_is_rejected() {
    let schema = CompiledSchema {
        schema_format_version: Some(999),
        ..Default::default()
    };
    let result = schema.validate_format_version();
    assert!(result.is_err(), "expected future version 999 to be rejected, got: {result:?}");
}

#[test]
fn test_legacy_schema_without_version_warns_but_loads() {
    // schema_format_version = None simulates a pre-v2.1 compiled schema
    let schema = CompiledSchema::default();
    // Should return Ok (callers log a warning, but do not reject)
    schema
        .validate_format_version()
        .unwrap_or_else(|e| panic!("expected legacy schema (no version) to be accepted: {e:?}"));
}

// ---------------------------------------------------------------------------
// graphql_value.rs tests
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_int() {
    let v = GraphQLValue::Int(42);
    assert_eq!(GraphQLValue::from_json(&v.to_json()).expect("roundtrip"), v);
}

#[test]
fn roundtrip_float() {
    let v = GraphQLValue::Float(1.5);
    let rt = GraphQLValue::from_json(&v.to_json()).expect("roundtrip");
    assert!(matches!(rt, GraphQLValue::Float(_)));
}

#[test]
fn roundtrip_string() {
    let v = GraphQLValue::String("hello".to_string());
    assert_eq!(GraphQLValue::from_json(&v.to_json()).expect("roundtrip"), v);
}

#[test]
fn roundtrip_list() {
    let v = GraphQLValue::List(vec![GraphQLValue::Int(1), GraphQLValue::Null]);
    assert_eq!(GraphQLValue::from_json(&v.to_json()).expect("roundtrip"), v);
}

#[test]
fn roundtrip_null() {
    let v = GraphQLValue::Null;
    assert_eq!(GraphQLValue::from_json(&v.to_json()).expect("roundtrip"), v);
}

#[test]
fn roundtrip_boolean() {
    let v = GraphQLValue::Boolean(true);
    assert_eq!(GraphQLValue::from_json(&v.to_json()).expect("roundtrip"), v);
}

#[test]
fn json_null_parses_as_null() {
    assert_eq!(
        GraphQLValue::from_json(&serde_json::Value::Null).expect("parse"),
        GraphQLValue::Null
    );
}

#[test]
fn serde_roundtrip_via_json_string() {
    let v = GraphQLValue::List(vec![GraphQLValue::Int(1), GraphQLValue::Null]);
    let json_str = serde_json::to_string(&v).expect("serialize");
    let back: GraphQLValue = serde_json::from_str(&json_str).expect("deserialize");
    assert_eq!(back, v);
}

// ---------------------------------------------------------------------------
// scalar_types.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_builtin_scalars_recognized() {
    // Test all builtin scalars are recognized
    for &scalar in BUILTIN_SCALARS {
        assert!(is_known_scalar(scalar), "Builtin scalar '{}' should be recognized", scalar);
    }
}

#[test]
fn test_rich_scalars_recognized() {
    // Test all rich scalars are recognized
    for &scalar in RICH_SCALARS {
        assert!(is_known_scalar(scalar), "Rich scalar '{}' should be recognized", scalar);
    }
}

#[test]
fn test_unknown_types_not_recognized() {
    assert!(!is_known_scalar("User"));
    assert!(!is_known_scalar("Post"));
    assert!(!is_known_scalar("CustomType"));
    assert!(!is_known_scalar(""));
}

#[test]
fn test_builtin_scalar_count() {
    // Verify we have the expected number of builtin scalars
    assert_eq!(BUILTIN_SCALARS.len(), 14);
}

#[test]
fn test_rich_scalar_count() {
    // Verify we have the expected number of rich scalars
    assert_eq!(RICH_SCALARS.len(), 51);
}

#[test]
fn test_no_duplicate_scalars() {
    // Ensure no scalar appears in both lists
    for &builtin in BUILTIN_SCALARS {
        assert!(
            !RICH_SCALARS.contains(&builtin),
            "Scalar '{}' appears in both BUILTIN and RICH lists",
            builtin
        );
    }
}

#[test]
fn test_specific_builtin_scalars() {
    // Verify specific important builtin scalars
    assert!(is_known_scalar("ID"));
    assert!(is_known_scalar("String"));
    assert!(is_known_scalar("Int"));
    assert!(is_known_scalar("Float"));
    assert!(is_known_scalar("Boolean"));
    assert!(is_known_scalar("DateTime"));
}

#[test]
fn test_specific_rich_scalars() {
    // Verify specific important rich scalars
    assert!(is_known_scalar("Email"));
    assert!(is_known_scalar("UUID"));
    assert!(is_known_scalar("URL"));
    assert!(is_known_scalar("IBAN"));
    assert!(is_known_scalar("IPAddress"));
}

#[test]
fn test_case_sensitive_matching() {
    // Scalar matching is case-sensitive (exact match required)
    assert!(is_known_scalar("String"));
    assert!(!is_known_scalar("string"));
    assert!(is_known_scalar("Email"));
    assert!(!is_known_scalar("email"));
}

// ---------------------------------------------------------------------------
// security_config.rs tests
// ---------------------------------------------------------------------------

// ── TenancyMode ─────────────────────────────────────────────────────

#[test]
fn tenancy_mode_default_is_none() {
    assert_eq!(TenancyMode::default(), TenancyMode::None);
}

#[test]
fn tenancy_mode_serde_round_trip() {
    for (mode, expected_str) in [
        (TenancyMode::None, "\"none\""),
        (TenancyMode::Row, "\"row\""),
        (TenancyMode::Schema, "\"schema\""),
    ] {
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, expected_str, "serialization of {mode}");
        let back: TenancyMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode, "deserialization of {expected_str}");
    }
}

#[test]
fn tenancy_mode_invalid_string_rejected() {
    let result: Result<TenancyMode, _> = serde_json::from_str("\"invalid\"");
    assert!(result.is_err(), "unknown variant must fail");
}

#[test]
fn tenancy_mode_display() {
    assert_eq!(TenancyMode::None.to_string(), "none");
    assert_eq!(TenancyMode::Row.to_string(), "row");
    assert_eq!(TenancyMode::Schema.to_string(), "schema");
}

// ── TenancyConfig ───────────────────────────────────────────────────

#[test]
fn tenancy_config_default_values() {
    let config = TenancyConfig::default();
    assert_eq!(config.mode, TenancyMode::None);
    assert_eq!(config.tenant_claim, "tenant_id");
}

#[test]
fn tenancy_config_serde_round_trip() {
    let config = TenancyConfig {
        mode:         TenancyMode::Row,
        tenant_claim: "org_id".to_string(),
    };
    let json = serde_json::to_string(&config).unwrap();
    let back: TenancyConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back, config);
}

#[test]
fn tenancy_config_deserialize_from_compiled_json() {
    let json = r#"{"mode": "schema", "tenant_claim": "tenant_id"}"#;
    let config: TenancyConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.mode, TenancyMode::Schema);
    assert_eq!(config.tenant_claim, "tenant_id");
}

#[test]
fn tenancy_config_defaults_when_empty() {
    let config: TenancyConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(config.mode, TenancyMode::None);
    assert_eq!(config.tenant_claim, "tenant_id");
}

// ── SecurityConfig with tenancy ─────────────────────────────────────

#[test]
fn security_config_tenancy_defaults_to_none() {
    let config = SecurityConfig::default();
    assert_eq!(config.tenancy.mode, TenancyMode::None);
}

#[test]
fn security_config_tenancy_skipped_when_default() {
    let config = SecurityConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    // tenancy field should be absent when it's the default
    assert!(!json.contains("tenancy"), "default tenancy should be skipped in serialization");
}

#[test]
fn security_config_tenancy_present_when_non_default() {
    let config = SecurityConfig {
        tenancy: TenancyConfig {
            mode:         TenancyMode::Row,
            tenant_claim: "tenant_id".to_string(),
        },
        ..SecurityConfig::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("tenancy"), "non-default tenancy should be serialized");
    assert!(json.contains("\"row\""), "mode=row should appear in JSON");
}

#[test]
fn security_config_with_tenancy_round_trip() {
    let config = SecurityConfig {
        tenancy: TenancyConfig {
            mode:         TenancyMode::Schema,
            tenant_claim: "org_id".to_string(),
        },
        ..SecurityConfig::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    let back: SecurityConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.tenancy.mode, TenancyMode::Schema);
    assert_eq!(back.tenancy.tenant_claim, "org_id");
}

// ---------------------------------------------------------------------------
// field_type.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_builtin_scalars() {
    assert_eq!(FieldType::parse("String"), FieldType::String);
    assert_eq!(FieldType::parse("Int"), FieldType::Int);
    assert_eq!(FieldType::parse("Float"), FieldType::Float);
    assert_eq!(FieldType::parse("Boolean"), FieldType::Boolean);
    assert_eq!(FieldType::parse("ID"), FieldType::Id);
    assert_eq!(FieldType::parse("DateTime"), FieldType::DateTime);
    assert_eq!(FieldType::parse("Date"), FieldType::Date);
    assert_eq!(FieldType::parse("Time"), FieldType::Time);
    assert_eq!(FieldType::parse("JSON"), FieldType::Json);
    assert_eq!(FieldType::parse("UUID"), FieldType::Uuid);
}

#[test]
fn test_parse_rich_scalars_exact_case() {
    // Email is in RICH_SCALARS and should be recognized with exact case
    let result = FieldType::parse("Email");
    assert_eq!(result, FieldType::Scalar("Email".to_string()));

    // IBAN is in RICH_SCALARS
    let result = FieldType::parse("IBAN");
    assert_eq!(result, FieldType::Scalar("IBAN".to_string()));

    // URL is in RICH_SCALARS
    let result = FieldType::parse("URL");
    assert_eq!(result, FieldType::Scalar("URL".to_string()));
}

#[test]
fn test_parse_rich_scalars_case_insensitive() {
    // Email is in RICH_SCALARS - should match case-insensitively
    let result = FieldType::parse("email");
    assert_eq!(result, FieldType::Scalar("Email".to_string()));

    // Should also work for mixed case
    let result = FieldType::parse("EMAIL");
    assert_eq!(result, FieldType::Scalar("Email".to_string()));

    // IBAN - case insensitive matching
    let result = FieldType::parse("iban");
    assert_eq!(result, FieldType::Scalar("IBAN".to_string()));

    // PhoneNumber - case insensitive
    let result = FieldType::parse("phonenumber");
    assert_eq!(result, FieldType::Scalar("PhoneNumber".to_string()));
}

#[test]
fn test_parse_all_rich_scalars() {
    // Test a sampling of all rich scalar categories
    let rich_scalars = vec![
        // Contact/Communication
        "Email",
        "PhoneNumber",
        "URL",
        "DomainName",
        "Hostname",
        // Location/Address
        "PostalCode",
        "Latitude",
        "Longitude",
        "Coordinates",
        "Timezone",
        // Financial
        "IBAN",
        "CUSIP",
        "CurrencyCode",
        "Money",
        "StockSymbol",
        // Identifiers
        "Slug",
        "SemanticVersion",
        "APIKey",
        "VIN",
        // Networking
        "IPAddress",
        "IPv4",
        "IPv6",
        "MACAddress",
        "CIDR",
        // Transportation
        "AirportCode",
        "FlightNumber",
        // Content
        "Markdown",
        "HTML",
        "MimeType",
        "Color",
        // Database
        "LTree",
        // Ranges
        "DateRange",
        "Duration",
        "Percentage",
    ];

    for scalar_name in rich_scalars {
        let result = FieldType::parse(scalar_name);
        assert_eq!(
            result,
            FieldType::Scalar(scalar_name.to_string()),
            "Failed to parse rich scalar: {}",
            scalar_name
        );
    }
}

#[test]
fn test_parse_unknown_type_as_object() {
    // Unknown types should become Object types
    let result = FieldType::parse("CustomType");
    assert_eq!(result, FieldType::Object("CustomType".to_string()));

    let result = FieldType::parse("User");
    assert_eq!(result, FieldType::Object("User".to_string()));
}

#[test]
fn test_parse_with_list_syntax() {
    // List of builtin scalar
    let result = FieldType::parse("[String]");
    assert_eq!(result, FieldType::List(Box::new(FieldType::String)));

    // List of rich scalar
    let result = FieldType::parse("[Email]");
    assert_eq!(result, FieldType::List(Box::new(FieldType::Scalar("Email".to_string()))));

    // List of object type
    let result = FieldType::parse("[User]");
    assert_eq!(result, FieldType::List(Box::new(FieldType::Object("User".to_string()))));
}

#[test]
fn test_parse_with_non_null_marker() {
    // Non-null scalar
    let result = FieldType::parse("String!");
    assert_eq!(result, FieldType::String);

    // Non-null rich scalar
    let result = FieldType::parse("Email!");
    assert_eq!(result, FieldType::Scalar("Email".to_string()));

    // Non-null list of non-null items
    let result = FieldType::parse("[String!]!");
    assert_eq!(result, FieldType::List(Box::new(FieldType::String)));
}

#[test]
fn test_parse_nested_lists() {
    // Nested list
    let result = FieldType::parse("[[String]]");
    assert_eq!(result, FieldType::List(Box::new(FieldType::List(Box::new(FieldType::String)))));

    // Nested list with rich scalar
    let result = FieldType::parse("[[Email]]");
    assert_eq!(
        result,
        FieldType::List(Box::new(FieldType::List(Box::new(FieldType::Scalar(
            "Email".to_string()
        )))))
    );
}

#[test]
fn test_parse_as_scalar_if_unknown_converts_objects() {
    let mut known_types = std::collections::HashSet::new();

    // Without known_types, unknown types become objects
    let result = FieldType::parse("CustomType");
    assert_eq!(result, FieldType::Object("CustomType".to_string()));

    // With parse_as_scalar_if_unknown, they become scalars
    let result = FieldType::parse_as_scalar_if_unknown("CustomType", &known_types);
    assert_eq!(result, FieldType::Scalar("CustomType".to_string()));

    // But if it's in known_types, it stays an object
    known_types.insert("CustomType".to_string());
    let result = FieldType::parse_as_scalar_if_unknown("CustomType", &known_types);
    assert_eq!(result, FieldType::Object("CustomType".to_string()));
}

#[test]
fn test_parse_case_variations() {
    // Test various case combinations for builtin types
    assert_eq!(FieldType::parse("string"), FieldType::String);
    assert_eq!(FieldType::parse("STRING"), FieldType::String);
    assert_eq!(FieldType::parse("String"), FieldType::String);

    // Test integer variations
    assert_eq!(FieldType::parse("int"), FieldType::Int);
    assert_eq!(FieldType::parse("INT"), FieldType::Int);
    assert_eq!(FieldType::parse("integer"), FieldType::Int);
    assert_eq!(FieldType::parse("INTEGER"), FieldType::Int);
}

#[test]
fn test_field_encryption_config_deserialization() {
    let json = r#"{
        "name": "email",
        "field_type": "String",
        "encryption": {
            "key_reference": "keys/email",
            "algorithm": "AES-256-GCM"
        }
    }"#;
    let field: FieldDefinition = serde_json::from_str(json).unwrap();
    assert!(field.encryption.is_some());
    let enc = field.encryption.unwrap();
    assert_eq!(enc.key_reference, "keys/email");
    assert_eq!(enc.algorithm, "AES-256-GCM");
}

#[test]
fn test_field_without_encryption() {
    let json = r#"{"name": "id", "field_type": "Int"}"#;
    let field: FieldDefinition = serde_json::from_str(json).unwrap();
    assert!(field.encryption.is_none());
    assert!(!field.is_encrypted());
}

#[test]
fn test_field_encryption_default_algorithm() {
    let json = r#"{"key_reference": "keys/ssn"}"#;
    let config: FieldEncryptionConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.algorithm, "AES-256-GCM");
}

#[test]
fn test_field_with_encryption_builder() {
    let field =
        FieldDefinition::new("email", FieldType::String).with_encryption(FieldEncryptionConfig {
            key_reference: "keys/email".to_string(),
            algorithm:     "AES-256-GCM".to_string(),
        });
    assert!(field.is_encrypted());
    assert_eq!(field.encryption.unwrap().key_reference, "keys/email");
}

#[test]
fn test_field_encryption_roundtrip_serialization() {
    let field =
        FieldDefinition::new("email", FieldType::String).with_encryption(FieldEncryptionConfig {
            key_reference: "keys/email".to_string(),
            algorithm:     "AES-256-GCM".to_string(),
        });
    let json = serde_json::to_string(&field).unwrap();
    let deserialized: FieldDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(field, deserialized);
}

// ---------------------------------------------------------------------------
// config_types.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_federation_config_default() {
    let config = FederationConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.version, None);
    assert!(config.entities.is_empty());
    assert!(config.circuit_breaker.is_none());
}

#[test]
fn test_circuit_breaker_config_default() {
    let config = CircuitBreakerConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.failure_threshold, 5);
    assert_eq!(config.recovery_timeout_secs, 30);
    assert_eq!(config.success_threshold, 2);
    assert!(config.per_entity.is_empty());
}

#[test]
fn test_security_config_default() {
    let config = CompiledSecurityConfig::default();
    assert!(config.default_policy.is_none());
    assert!(config.rules.is_empty());
    assert!(config.policies.is_empty());
    assert!(config.field_auth.is_empty());
    assert!(config.enterprise.rate_limiting_enabled);
}

#[test]
fn test_observers_config_default() {
    let config = ObserversConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.backend, "redis");
    assert!(config.handlers.is_empty());
}

#[test]
fn test_federation_config_serde() {
    let json = r#"{
        "enabled": true,
        "version": "v2",
        "entities": [{"name": "User", "key_fields": ["id"]}],
        "circuit_breaker": {
            "enabled": true,
            "failure_threshold": 3,
            "recovery_timeout_secs": 15,
            "success_threshold": 1
        }
    }"#;

    let config: FederationConfig = serde_json::from_str(json).unwrap();
    assert!(config.enabled);
    assert_eq!(config.version, Some("v2".to_string()));
    assert_eq!(config.entities.len(), 1);
    assert_eq!(config.entities[0].name, "User");

    let cb = config.circuit_breaker.unwrap();
    assert!(cb.enabled);
    assert_eq!(cb.failure_threshold, 3);
}

#[test]
fn test_entity_override() {
    let config = CircuitBreakerConfig {
        per_entity: vec![EntityCircuitBreakerOverride {
            entity:            "Product".to_string(),
            failure_threshold: Some(2),
            recovery_timeout:  None,
            success_threshold: None,
        }],
        ..Default::default()
    };

    assert_eq!(config.per_entity[0].entity, "Product");
    assert_eq!(config.per_entity[0].failure_threshold, Some(2));
}

#[test]
fn test_roundtrip_serialization() {
    let config = FederationConfig {
        enabled:         true,
        version:         Some("v2".to_string()),
        service_name:    Some("my-service".to_string()),
        schema_url:      None,
        shareable_types: Vec::new(),
        entities:        vec![FederationEntity {
            name:       "User".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        circuit_breaker: Some(CircuitBreakerConfig::default()),
    };

    let json = serde_json::to_string(&config).unwrap();
    let restored: FederationConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config, restored);
}

// ── CrudNamingConfig ─────────────────────────────────────────────────────

#[test]
fn crud_trinity_resolves_create() {
    let cfg = CrudNamingConfig {
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("CREATE", "user"), Some("create_user".to_string()));
}

#[test]
fn crud_trinity_resolves_update() {
    let cfg = CrudNamingConfig {
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("UPDATE", "user"), Some("update_user".to_string()));
}

#[test]
fn crud_trinity_resolves_delete() {
    let cfg = CrudNamingConfig {
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("DELETE", "user"), Some("delete_user".to_string()));
}

#[test]
fn crud_function_schema_prefix_applied() {
    let cfg = CrudNamingConfig {
        function_schema: Some("app".to_string()),
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("CREATE", "user"), Some("app.create_user".to_string()));
}

#[test]
fn crud_function_schema_prefix_applied_to_custom_template() {
    let cfg = CrudNamingConfig {
        function_schema: Some("app".to_string()),
        create_template: Some("insert_{entity}".to_string()),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("CREATE", "order"), Some("app.insert_order".to_string()));
}

#[test]
fn crud_custom_template_overrides_preset() {
    let cfg = CrudNamingConfig {
        function_naming: Some(CrudNamingPreset::Trinity),
        create_template: Some("insert_{entity}".to_string()),
        ..Default::default()
    };
    // Custom template wins over trinity
    assert_eq!(cfg.resolve("CREATE", "user"), Some("insert_user".to_string()));
    // Other operations fall back to trinity
    assert_eq!(cfg.resolve("UPDATE", "user"), Some("update_user".to_string()));
}

#[test]
fn crud_no_config_returns_none() {
    let cfg = CrudNamingConfig::default();
    assert_eq!(cfg.resolve("CREATE", "user"), None);
    assert_eq!(cfg.resolve("UPDATE", "user"), None);
    assert_eq!(cfg.resolve("DELETE", "user"), None);
}

#[test]
fn crud_unknown_operation_returns_none() {
    let cfg = CrudNamingConfig {
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("UPSERT", "user"), None);
}

#[test]
fn crud_operation_case_insensitive() {
    let cfg = CrudNamingConfig {
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("create", "user"), Some("create_user".to_string()));
    assert_eq!(cfg.resolve("Create", "user"), Some("create_user".to_string()));
}

#[test]
fn crud_entity_with_underscores() {
    let cfg = CrudNamingConfig {
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("CREATE", "user_profile"), Some("create_user_profile".to_string()));
}

#[test]
fn crud_serde_roundtrip_trinity() {
    let cfg = CrudNamingConfig {
        function_schema: Some("app".to_string()),
        function_naming: Some(CrudNamingPreset::Trinity),
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: CrudNamingConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, restored);
}

#[test]
fn crud_serde_roundtrip_custom_templates() {
    let cfg = CrudNamingConfig {
        function_schema: Some("app".to_string()),
        create_template: Some("insert_{entity}".to_string()),
        update_template: Some("upsert_{entity}".to_string()),
        delete_template: Some("remove_{entity}".to_string()),
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: CrudNamingConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, restored);
}

#[test]
fn test_naming_convention_default_is_preserve() {
    assert_eq!(NamingConvention::default(), NamingConvention::Preserve);
}

#[test]
fn test_naming_convention_serde_roundtrip() {
    let camel = NamingConvention::CamelCase;
    let json = serde_json::to_string(&camel).unwrap();
    assert_eq!(json, r#""camelCase""#);
    let restored: NamingConvention = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, NamingConvention::CamelCase);

    let preserve = NamingConvention::Preserve;
    let json = serde_json::to_string(&preserve).unwrap();
    assert_eq!(json, r#""preserve""#);
    let restored: NamingConvention = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, NamingConvention::Preserve);
}

// --- Issue #250: FieldDefinition.hierarchy ---

#[test]
fn test_field_definition_hierarchy_serde_roundtrip() {
    let mut field = FieldDefinition::new("category_path", FieldType::String);
    field.hierarchy = Some("category".to_string());

    let json = serde_json::to_string(&field).unwrap();
    assert!(json.contains(r#""hierarchy":"category""#));

    let restored: FieldDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.hierarchy, Some("category".to_string()));
}

#[test]
fn test_field_definition_hierarchy_absent_defaults_to_none() {
    let json = r#"{"name":"id","field_type":"ID","nullable":false}"#;
    let field: FieldDefinition = serde_json::from_str(json).unwrap();
    assert!(field.hierarchy.is_none());
}

#[test]
fn test_field_definition_hierarchy_skipped_when_none() {
    let field = FieldDefinition::new("id", FieldType::Id);
    let json = serde_json::to_string(&field).unwrap();
    assert!(!json.contains("hierarchy"));
}
