//! Tests for compiled schema types.

use field_type::{DistanceMetric, VectorConfig, VectorIndexType};

use super::*;

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
    let schema = CompiledSchema::from_json(json).unwrap();
    assert!(schema.types.is_empty());
}

#[test]
fn test_schema_from_json_with_defaults() {
    // Minimal JSON - all fields should default
    let json = r"{}";
    let schema = CompiledSchema::from_json(json).unwrap();
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
}

#[test]
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

    let schema = CompiledSchema::from_json(json).unwrap();

    // Check types
    assert_eq!(schema.types.len(), 1);
    let user_type = &schema.types[0];
    assert_eq!(user_type.name, "User");
    assert_eq!(user_type.sql_source, "v_user");
    assert_eq!(user_type.fields.len(), 3);
    assert_eq!(user_type.description, Some("A user in the system".to_string()));

    // Check queries
    assert_eq!(schema.queries.len(), 2);
    let users_query = schema.find_query("users").unwrap();
    assert!(users_query.returns_list);
    assert!(users_query.auto_params.has_where);

    let user_query = schema.find_query("user").unwrap();
    assert!(!user_query.returns_list);
    assert!(user_query.nullable);
    assert_eq!(user_query.arguments.len(), 1);

    // Check mutations
    assert_eq!(schema.mutations.len(), 1);
    let create_user = schema.find_mutation("createUser").unwrap();
    assert_eq!(create_user.arguments.len(), 1);
    assert!(matches!(
        &create_user.operation,
        MutationOperation::Insert { table } if table == "users"
    ));

    // Check subscriptions
    assert_eq!(schema.subscriptions.len(), 1);
    let sub = schema.find_subscription("userCreated").unwrap();
    assert_eq!(sub.topic, Some("user_created".to_string()));
}

#[test]
fn test_schema_to_json_roundtrip() {
    let schema = CompiledSchema {
        types:         vec![
            TypeDefinition::new("User", "v_user")
                .with_field(FieldDefinition::new("id", FieldType::Id))
                .with_field(FieldDefinition::new("email", FieldType::String)),
        ],
        enums:         vec![],
        input_types:   vec![],
        interfaces:    vec![],
        unions:        vec![],
        queries:       vec![QueryDefinition::new("users", "User").returning_list()],
        mutations:     vec![],
        subscriptions: vec![],
        directives:    vec![],
        fact_tables:   std::collections::HashMap::new(),
    };

    let json = schema.to_json().unwrap();
    let parsed = CompiledSchema::from_json(&json).unwrap();

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
    assert!(result.is_err());
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
    assert!(result.is_err());
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

    assert!(schema.validate().is_ok());
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

    assert!(schema.validate().is_ok());
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

    let function = MutationOperation::Function {
        name: "create_user".to_string(),
    };
    let json = serde_json::to_string(&function).unwrap();
    assert_eq!(json, r#"{"Function":{"name":"create_user"}}"#);

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
        types:         vec![TypeDefinition::new("User", "v_user")],
        enums:         vec![],
        input_types:   vec![],
        interfaces:    vec![],
        unions:        vec![],
        queries:       vec![
            QueryDefinition::new("users", "User"),
            QueryDefinition::new("user", "User"),
        ],
        mutations:     vec![MutationDefinition::new("createUser", "User")],
        subscriptions: vec![SubscriptionDefinition::new("userCreated", "User")],
        directives:    vec![],
        fact_tables:   std::collections::HashMap::new(),
    };

    assert_eq!(schema.operation_count(), 4); // 2 queries + 1 mutation + 1 subscription
}

/// Test that JSON generated by Python `SchemaCompiler` can be parsed.
///
/// This is a critical cross-language compatibility test for the Schema Freeze
/// architecture. The JSON format below is exactly what Python's `SchemaCompiler`
/// produces - if this test fails, Python/Rust interop is broken.
#[test]
fn test_python_generated_json_compat() {
    // This JSON is exactly what Python's SchemaCompiler.compile() produces
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
    let schema = CompiledSchema::from_json(python_json)
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
    assert!(schema.validate().is_ok());
}

// ============================================================================
// Vector Types Tests (Phase 11.1)
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

    let schema = CompiledSchema::from_json(json).unwrap();

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
        types:         vec![
            TypeDefinition::new("Document", "documents")
                .with_field(FieldDefinition::new("id", FieldType::Id))
                .with_field(
                    FieldDefinition::vector("embedding", VectorConfig::openai())
                        .with_description("OpenAI embedding"),
                ),
        ],
        enums:         vec![],
        input_types:   vec![],
        interfaces:    vec![],
        unions:        vec![],
        queries:       vec![],
        mutations:     vec![],
        subscriptions: vec![],
        directives:    vec![],
        fact_tables:   std::collections::HashMap::new(),
    };

    let json = schema.to_json().unwrap();
    let parsed = CompiledSchema::from_json(&json).unwrap();

    assert_eq!(schema, parsed);

    let doc_type = &parsed.types[0];
    let embedding = doc_type.find_field("embedding").unwrap();
    assert!(embedding.is_vector());
    assert_eq!(embedding.vector_config.as_ref().unwrap().dimensions, 1536);
}

/// Test that Python-generated vector schema JSON parses correctly.
///
/// This JSON is exactly what Python's SchemaCompiler produces for vector fields.
/// If this test fails, Python/Rust interop for vectors is broken.
#[test]
fn test_python_generated_vector_schema_compat() {
    // This JSON matches what Python's SchemaCompiler produces
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
    let schema = CompiledSchema::from_json(python_json)
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
    assert!(schema.validate().is_ok());
}
