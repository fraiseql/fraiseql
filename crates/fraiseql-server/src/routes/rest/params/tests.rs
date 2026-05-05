//! Tests for the `params` module.

#![allow(clippy::unwrap_used)] // Reason: test assertions use unwrap/unwrap_err intentionally

use fraiseql_core::schema::{
    ArgumentDefinition, AutoParams, Cardinality, FieldDefinition, FieldType, QueryDefinition,
    Relationship, RestConfig, TypeDefinition,
};

use super::*;

// -----------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------

fn test_config() -> RestConfig {
    RestConfig {
        max_page_size: 100,
        default_page_size: 20,
        max_filter_bytes: 4096,
        ..RestConfig::default()
    }
}

fn user_type_def() -> TypeDefinition {
    TypeDefinition::new("User", "v_user")
        .with_field(FieldDefinition::new("id", FieldType::Uuid))
        .with_field(FieldDefinition::new("name", FieldType::String))
        .with_field(FieldDefinition::new("email", FieldType::String))
        .with_field(FieldDefinition::new("age", FieldType::Int))
        .with_field(FieldDefinition::new("active", FieldType::Boolean))
}

fn list_query_def() -> QueryDefinition {
    QueryDefinition {
        name: "users".to_string(),
        return_type: "User".to_string(),
        returns_list: true,
        auto_params: AutoParams::all(),
        arguments: vec![
            ArgumentDefinition::optional("where", FieldType::Json),
            ArgumentDefinition::optional("orderBy", FieldType::Json),
            ArgumentDefinition::optional("limit", FieldType::Int),
            ArgumentDefinition::optional("offset", FieldType::Int),
        ],
        ..default_query_def()
    }
}

fn single_query_def() -> QueryDefinition {
    QueryDefinition {
        name: "user".to_string(),
        return_type: "User".to_string(),
        returns_list: false,
        arguments: vec![ArgumentDefinition::new("id", FieldType::Uuid)],
        ..default_query_def()
    }
}

fn relay_query_def() -> QueryDefinition {
    QueryDefinition {
        name: "users".to_string(),
        return_type: "User".to_string(),
        returns_list: true,
        relay: true,
        relay_cursor_column: Some("pk_user".to_string()),
        auto_params: AutoParams::all(),
        arguments: vec![
            ArgumentDefinition::optional("first", FieldType::Int),
            ArgumentDefinition::optional("after", FieldType::String),
            ArgumentDefinition::optional("last", FieldType::Int),
            ArgumentDefinition::optional("before", FieldType::String),
        ],
        ..default_query_def()
    }
}

fn default_query_def() -> QueryDefinition {
    QueryDefinition::new("test", "Test")
}

fn extractor_list<'a>(
    config: &'a RestConfig,
    query_def: &'a QueryDefinition,
    type_def: &'a TypeDefinition,
) -> RestParamExtractor<'a> {
    RestParamExtractor::new(config, query_def, Some(type_def))
}

// -----------------------------------------------------------------------
// Path param extraction
// -----------------------------------------------------------------------

#[test]
fn path_param_int_coercion() {
    let config = test_config();
    let qd = QueryDefinition {
        arguments: vec![ArgumentDefinition::new("id", FieldType::Int)],
        ..single_query_def()
    };
    let td = user_type_def();
    let ext = RestParamExtractor::new(&config, &qd, Some(&td));

    let result = ext.extract(&[("id", "123")], &[]).unwrap();
    assert_eq!(result.path_params, vec![("id".to_string(), serde_json::json!(123))]);
}

#[test]
fn path_param_uuid_passthrough() {
    let config = test_config();
    let qd = single_query_def();
    let td = user_type_def();
    let ext = RestParamExtractor::new(&config, &qd, Some(&td));

    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    let result = ext.extract(&[("id", uuid)], &[]).unwrap();
    assert_eq!(result.path_params, vec![("id".to_string(), serde_json::json!(uuid))]);
}

// -----------------------------------------------------------------------
// Offset pagination
// -----------------------------------------------------------------------

#[test]
fn offset_pagination_explicit() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("limit", "10"), ("offset", "5")]).unwrap();
    assert_eq!(
        result.pagination,
        PaginationParams::Offset {
            limit:  10,
            offset: 5,
        }
    );
}

#[test]
fn offset_pagination_defaults() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[]).unwrap();
    assert_eq!(
        result.pagination,
        PaginationParams::Offset {
            limit:  20, // default_page_size
            offset: 0,
        }
    );
}

#[test]
fn limit_clamped_to_max_page_size() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("limit", "500")]).unwrap();
    assert_eq!(
        result.pagination,
        PaginationParams::Offset {
            limit:  100,
            offset: 0,
        }
    );
}

// -----------------------------------------------------------------------
// Cursor (Relay) pagination
// -----------------------------------------------------------------------

#[test]
fn cursor_pagination_explicit() {
    let config = test_config();
    let qd = relay_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("first", "10"), ("after", "abc")]).unwrap();
    assert_eq!(
        result.pagination,
        PaginationParams::Cursor {
            first:  Some(10),
            after:  Some("abc".to_string()),
            last:   None,
            before: None,
        }
    );
}

#[test]
fn cursor_pagination_defaults() {
    let config = test_config();
    let qd = relay_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[]).unwrap();
    assert_eq!(
        result.pagination,
        PaginationParams::Cursor {
            first:  Some(20), // default_page_size
            after:  None,
            last:   None,
            before: None,
        }
    );
}

#[test]
fn first_clamped_to_max_page_size() {
    let config = test_config();
    let qd = relay_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("first", "500")]).unwrap();
    match result.pagination {
        PaginationParams::Cursor { first, .. } => assert_eq!(first, Some(100)),
        other => panic!("expected Cursor, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// Cross-pagination guards
// -----------------------------------------------------------------------

#[test]
fn relay_rejects_limit_offset() {
    let config = test_config();
    let qd = relay_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("limit", "10")]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("cursor-based pagination"), "got: {msg}");
    assert!(msg.contains("first"), "got: {msg}");
}

#[test]
fn offset_rejects_first_after() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("first", "10")]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("offset-based pagination"), "got: {msg}");
    assert!(msg.contains("limit"), "got: {msg}");
}

// -----------------------------------------------------------------------
// Simple equality filters
// -----------------------------------------------------------------------

#[test]
fn simple_equality_filter() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("name", "Alice")]).unwrap();
    assert_eq!(result.where_clause, Some(serde_json::json!({ "name": { "eq": "Alice" } })));
}

// -----------------------------------------------------------------------
// Bracket operator filters
// -----------------------------------------------------------------------

#[test]
fn bracket_operator_filter() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("name[icontains]", "Ali")]).unwrap();
    assert_eq!(
        result.where_clause,
        Some(serde_json::json!({ "name": { "icontains": "Ali" } }))
    );
}

#[test]
fn bracket_operator_invalid() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("name[beginsWith]", "A")]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown bracket operator"), "got: {msg}");
    assert!(msg.contains("Available bracket operators"), "got: {msg}");
}

// -----------------------------------------------------------------------
// JSON filter
// -----------------------------------------------------------------------

#[test]
fn json_filter_passthrough() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let filter = r#"{"name":{"startswith":"A"}}"#;
    let result = ext.extract(&[], &[("filter", filter)]).unwrap();
    assert_eq!(result.where_clause, Some(serde_json::json!({ "name": { "startswith": "A" } })));
}

#[test]
fn filter_exceeding_max_bytes() {
    let config = RestConfig {
        max_filter_bytes: 10,
        ..test_config()
    };
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let filter = r#"{"name":{"eq":"very long value here"}}"#;
    let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exceeds maximum size"), "got: {msg}");
}

#[test]
fn filter_unknown_field() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let filter = r#"{"bogus":{"eq":"x"}}"#;
    let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown field 'bogus'"), "got: {msg}");
    assert!(msg.contains("Available fields"), "got: {msg}");
}

#[test]
fn filter_unknown_operator() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let filter = r#"{"name":{"bogusOp":"x"}}"#;
    let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown filter operator"), "got: {msg}");
    assert!(msg.contains("Available operators"), "got: {msg}");
}

#[test]
fn filter_nesting_depth_exceeded() {
    let config = test_config();
    let qd = list_query_def();
    // No type_def — skip field validation so deeply nested JSON passes field check.
    let ext = RestParamExtractor::new(&config, &qd, None);

    // Build JSON with depth > 64.
    let mut json = r#""leaf""#.to_string();
    for i in 0..65 {
        json = format!(r#"{{"k{i}":{json}}}"#);
    }
    let filter = &json;
    let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nesting depth"), "got: {msg}");
}

// -----------------------------------------------------------------------
// Sort
// -----------------------------------------------------------------------

#[test]
fn sort_ascending_descending() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("sort", "name,-age")]).unwrap();
    assert_eq!(
        result.order_by,
        Some(serde_json::json!([
            { "field": "name", "direction": "ASC" },
            { "field": "age", "direction": "DESC" },
        ]))
    );
}

#[test]
fn sort_invalid_field() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("sort", "bogus")]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown field 'bogus'"), "got: {msg}");
}

// -----------------------------------------------------------------------
// Select
// -----------------------------------------------------------------------

#[test]
fn select_fields() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("select", "id,name")]).unwrap();
    assert_eq!(
        result.field_selection,
        RestFieldSpec::Fields(vec!["id".to_string(), "name".to_string()])
    );
}

#[test]
fn select_dot_notation_rejects_non_count_suffix() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("select", "address.city")]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unsupported dot-suffix"), "got: {msg}");
}

// -----------------------------------------------------------------------
// Unknown param
// -----------------------------------------------------------------------

#[test]
fn unknown_param_rejected() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("unknown", "x")]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown query parameter"), "got: {msg}");
    assert!(msg.contains("Available parameters"), "got: {msg}");
}

// -----------------------------------------------------------------------
// Type coercion
// -----------------------------------------------------------------------

#[test]
fn coerce_int() {
    let result = coerce_to_type("42", &FieldType::Int).unwrap();
    assert_eq!(result, serde_json::json!(42));
}

#[test]
fn coerce_float() {
    let result = coerce_to_type("2.78", &FieldType::Float).unwrap();
    assert_eq!(result, serde_json::json!(2.78));
}

#[test]
fn coerce_boolean_true() {
    assert_eq!(coerce_to_type("true", &FieldType::Boolean).unwrap(), serde_json::json!(true));
    assert_eq!(coerce_to_type("1", &FieldType::Boolean).unwrap(), serde_json::json!(true));
    assert_eq!(coerce_to_type("yes", &FieldType::Boolean).unwrap(), serde_json::json!(true));
}

#[test]
fn coerce_boolean_false() {
    assert_eq!(coerce_to_type("false", &FieldType::Boolean).unwrap(), serde_json::json!(false));
    assert_eq!(coerce_to_type("0", &FieldType::Boolean).unwrap(), serde_json::json!(false));
}

#[test]
fn coerce_boolean_invalid() {
    let err = coerce_to_type("maybe", &FieldType::Boolean).unwrap_err();
    assert!(err.to_string().contains("Expected boolean"), "{err}");
}

#[test]
fn coerce_string_passthrough() {
    let result = coerce_to_type("hello", &FieldType::String).unwrap();
    assert_eq!(result, serde_json::json!("hello"));
}

#[test]
fn coerce_json_value() {
    let result = coerce_to_type(r#"{"key":"val"}"#, &FieldType::Json).unwrap();
    assert_eq!(result, serde_json::json!({"key": "val"}));
}

#[test]
fn coerce_list_csv() {
    let result =
        coerce_to_type("a,b,c", &FieldType::List(Box::new(FieldType::String))).unwrap();
    assert_eq!(result, serde_json::json!(["a", "b", "c"]));
}

#[test]
fn coerce_list_json_array() {
    let result =
        coerce_to_type(r#"["a","b"]"#, &FieldType::List(Box::new(FieldType::String))).unwrap();
    assert_eq!(result, serde_json::json!(["a", "b"]));
}

// -----------------------------------------------------------------------
// Single-resource endpoint
// -----------------------------------------------------------------------

#[test]
fn single_resource_no_pagination() {
    let config = test_config();
    let qd = single_query_def();
    let td = user_type_def();
    let ext = RestParamExtractor::new(&config, &qd, Some(&td));

    let result = ext.extract(&[("id", "550e8400-e29b-41d4-a716-446655440000")], &[]).unwrap();
    assert_eq!(result.pagination, PaginationParams::None);
}

// -----------------------------------------------------------------------
// Variables count limit
// -----------------------------------------------------------------------

#[test]
fn total_params_exceeding_max() {
    let config = test_config();
    let qd = list_query_def();
    // No type_def to skip field validation.
    let ext = RestParamExtractor::new(&config, &qd, None);

    // Build > 1000 simple filters.
    let pairs: Vec<(String, String)> =
        (0..1001).map(|i| (format!("f{i}"), format!("v{i}"))).collect();
    let query_pairs: Vec<(&str, &str)> =
        pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let err = ext.extract(&[], &query_pairs).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Too many parameters"), "got: {msg}");
}

// -----------------------------------------------------------------------
// parse_bracket_key helper
// -----------------------------------------------------------------------

#[test]
fn parse_bracket_key_valid() {
    assert_eq!(
        parse_bracket_key("name[icontains]"),
        Some(("name".to_string(), "icontains".to_string()))
    );
}

#[test]
fn parse_bracket_key_no_brackets() {
    assert_eq!(parse_bracket_key("name"), None);
}

#[test]
fn parse_bracket_key_empty_op() {
    assert_eq!(parse_bracket_key("name[]"), None);
}

#[test]
fn parse_bracket_key_empty_field() {
    assert_eq!(parse_bracket_key("[op]"), None);
}

// -----------------------------------------------------------------------
// json_depth helper
// -----------------------------------------------------------------------

#[test]
fn json_depth_flat() {
    assert_eq!(json_depth(&serde_json::json!("hello")), 1);
}

#[test]
fn json_depth_nested_object() {
    assert_eq!(json_depth(&serde_json::json!({"a": {"b": "c"}})), 3);
}

#[test]
fn json_depth_nested_array() {
    assert_eq!(json_depth(&serde_json::json!([[[1]]])), 4);
}

// -----------------------------------------------------------------------
// Parenthetical select parser
// -----------------------------------------------------------------------

#[test]
fn parse_select_entries_flat_fields() {
    let entries = parse_select_entries("id,name,email").unwrap();
    assert_eq!(
        entries,
        vec![
            SelectEntry::Field("id".to_string()),
            SelectEntry::Field("name".to_string()),
            SelectEntry::Field("email".to_string()),
        ]
    );
}

#[test]
fn parse_select_entries_embedded() {
    let entries = parse_select_entries("id,name,posts(id,title)").unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], SelectEntry::Field("id".to_string()));
    assert_eq!(entries[1], SelectEntry::Field("name".to_string()));
    match &entries[2] {
        SelectEntry::Embedded(spec) => {
            assert_eq!(spec.relationship, "posts");
            assert!(spec.rename.is_none());
            assert_eq!(
                spec.fields,
                vec![
                    SelectEntry::Field("id".to_string()),
                    SelectEntry::Field("title".to_string()),
                ]
            );
        },
        _ => panic!("Expected Embedded"),
    }
}

#[test]
fn parse_select_entries_nested_depth_2() {
    let entries = parse_select_entries("id,posts(id,title,comments(id,body))").unwrap();
    assert_eq!(entries.len(), 2);
    match &entries[1] {
        SelectEntry::Embedded(spec) => {
            assert_eq!(spec.relationship, "posts");
            assert_eq!(spec.fields.len(), 3);
            match &spec.fields[2] {
                SelectEntry::Embedded(inner) => {
                    assert_eq!(inner.relationship, "comments");
                    assert_eq!(
                        inner.fields,
                        vec![
                            SelectEntry::Field("id".to_string()),
                            SelectEntry::Field("body".to_string()),
                        ]
                    );
                },
                _ => panic!("Expected nested Embedded"),
            }
        },
        _ => panic!("Expected Embedded"),
    }
}

#[test]
fn parse_select_entries_rename_syntax() {
    let entries = parse_select_entries("id,author:fk_user(id,name)").unwrap();
    assert_eq!(entries.len(), 2);
    match &entries[1] {
        SelectEntry::Embedded(spec) => {
            assert_eq!(spec.relationship, "fk_user");
            assert_eq!(spec.rename, Some("author".to_string()));
            assert_eq!(
                spec.fields,
                vec![
                    SelectEntry::Field("id".to_string()),
                    SelectEntry::Field("name".to_string()),
                ]
            );
        },
        _ => panic!("Expected Embedded"),
    }
}

#[test]
fn parse_select_entries_count_only() {
    let entries = parse_select_entries("id,posts.count").unwrap();
    assert_eq!(
        entries,
        vec![
            SelectEntry::Field("id".to_string()),
            SelectEntry::Count("posts".to_string()),
        ]
    );
}

#[test]
fn parse_select_entries_unbalanced_parens() {
    let err = parse_select_entries("id,posts(id,title").unwrap_err();
    assert!(err.to_string().contains("Unbalanced parentheses"));
}

#[test]
fn parse_select_entries_invalid_dot_suffix() {
    let err = parse_select_entries("id,posts.foo").unwrap_err();
    assert!(err.to_string().contains("Unsupported dot-suffix"));
}

// -----------------------------------------------------------------------
// Embedding depth validation
// -----------------------------------------------------------------------

#[test]
fn embedding_depth_within_limit() {
    let spec = EmbeddedSpec {
        relationship: "posts".to_string(),
        rename:       None,
        fields:       vec![SelectEntry::Field("id".to_string())],
    };
    assert!(validate_embedding_depth(&spec, 1, 3).is_ok());
}

#[test]
fn embedding_depth_exceeds_limit() {
    let inner = EmbeddedSpec {
        relationship: "comments".to_string(),
        rename:       None,
        fields:       vec![SelectEntry::Field("id".to_string())],
    };
    let outer = EmbeddedSpec {
        relationship: "posts".to_string(),
        rename:       None,
        fields:       vec![SelectEntry::Embedded(inner)],
    };
    // depth=1, max=1 -> inner at depth=2 should fail
    let err = validate_embedding_depth(&outer, 1, 1).unwrap_err();
    assert!(err.to_string().contains("exceeds maximum"));
}

// -----------------------------------------------------------------------
// Embedding relationship validation via extractor
// -----------------------------------------------------------------------

fn user_type_with_relationships() -> TypeDefinition {
    let mut td = user_type_def();
    td.relationships = vec![Relationship {
        name:           "posts".to_string(),
        target_type:    "Post".to_string(),
        foreign_key:    "fk_user".to_string(),
        referenced_key: "pk_user".to_string(),
        cardinality:    Cardinality::OneToMany,
    }];
    td
}

#[test]
fn extract_with_valid_embedding() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_with_relationships();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("select", "id,name,posts(id,title)")]);
    let params = result.unwrap();
    assert_eq!(params.embeddings.len(), 1);
    assert_eq!(params.embeddings[0].relationship, "posts");
}

#[test]
fn extract_with_invalid_relationship() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def(); // No relationships
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("select", "id,comments(id,body)")]).unwrap_err();
    assert!(err.to_string().contains("has no relationship 'comments'"));
    assert!(err.to_string().contains("Available: none"));
}

#[test]
fn extract_with_embedding_filter() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_with_relationships();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(
        &[],
        &[
            ("select", "id,posts(id,title)"),
            ("posts.status", "published"),
        ],
    );
    let params = result.unwrap();
    assert_eq!(params.embedding_filters.len(), 1);
    let posts_filter = params.embedding_filters.get("posts").unwrap();
    assert_eq!(posts_filter, &serde_json::json!({"status": {"eq": "published"}}),);
}

#[test]
fn extract_count_only_embedding() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_with_relationships();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("select", "id,posts.count")]);
    let params = result.unwrap();
    assert_eq!(params.embedding_counts, vec!["posts"]);
}

#[test]
fn extract_embedding_depth_exceeded() {
    let mut config = test_config();
    config.max_embedding_depth = 1;
    let qd = list_query_def();
    let td = user_type_with_relationships();
    let ext = extractor_list(&config, &qd, &td);

    // Depth 2: posts -> comments (but max is 1)
    let err = ext.extract(&[], &[("select", "id,posts(id,comments(id,body))")]).unwrap_err();
    assert!(err.to_string().contains("exceeds maximum"));
}

// -----------------------------------------------------------------------
// Full-text search
// -----------------------------------------------------------------------

fn article_type_def() -> TypeDefinition {
    TypeDefinition::new("Article", "v_article")
        .with_field(FieldDefinition::new("id", FieldType::Uuid))
        .with_field(FieldDefinition::new("title", FieldType::String))
        .with_field(FieldDefinition::new("body", FieldType::String))
        .with_field(FieldDefinition::new("status", FieldType::String))
}

fn article_list_query_def() -> QueryDefinition {
    QueryDefinition {
        name: "articles".to_string(),
        return_type: "Article".to_string(),
        returns_list: true,
        auto_params: AutoParams::all(),
        arguments: vec![
            ArgumentDefinition::optional("where", FieldType::Json),
            ArgumentDefinition::optional("orderBy", FieldType::Json),
            ArgumentDefinition::optional("limit", FieldType::Int),
            ArgumentDefinition::optional("offset", FieldType::Int),
        ],
        ..default_query_def()
    }
}

#[test]
fn search_param_parsed() {
    let config = test_config();
    let qd = article_list_query_def();
    let td = article_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("search", "rust async")]).unwrap();
    assert_eq!(result.search_query, Some("rust async".to_string()));
}

#[test]
fn search_combined_with_filters() {
    let config = test_config();
    let qd = article_list_query_def();
    let td = article_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("search", "rust"), ("status[eq]", "published")]).unwrap();
    assert_eq!(result.search_query, Some("rust".to_string()));
    assert_eq!(
        result.where_clause,
        Some(serde_json::json!({ "status": { "eq": "published" } }))
    );
}

#[test]
fn search_on_resource_without_searchable_fields_fails() {
    let config = test_config();
    let qd = list_query_def();
    // Use a type with no String fields so searchable_fields() returns empty.
    let td = TypeDefinition::new("Counter", "v_counter")
        .with_field(FieldDefinition::new("id", FieldType::Uuid))
        .with_field(FieldDefinition::new("value", FieldType::Int));
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("search", "hello")]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Full-text search not available"), "got: {msg}");
    assert!(msg.contains("No searchable fields"), "got: {msg}");
}

#[test]
fn search_with_explicit_sort_preserves_sort() {
    let config = test_config();
    let qd = article_list_query_def();
    let td = article_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("search", "rust"), ("sort", "title")]).unwrap();
    assert_eq!(result.search_query, Some("rust".to_string()));
    assert!(result.order_by.is_some());
}

#[test]
fn search_on_single_resource_fails() {
    // `?search=x` on a non-searchable single-resource endpoint fails with
    // "not available" (search is a reserved param, not treated as a filter).
    let config = test_config();
    let qd = single_query_def();
    let td = user_type_def();
    let ext = RestParamExtractor::new(&config, &qd, Some(&td));

    let err = ext
        .extract(&[("id", "550e8400-e29b-41d4-a716-446655440000")], &[("search", "x")])
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Full-text search not available"), "got: {msg}");
}

// -----------------------------------------------------------------------
// Logical operators
// -----------------------------------------------------------------------

#[test]
fn logical_or_two_conditions() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("or", "(name[eq]=Alice,name[eq]=Bob)")]).unwrap();
    assert_eq!(
        result.where_clause,
        Some(serde_json::json!({
            "_or": [
                { "name": { "eq": "Alice" } },
                { "name": { "eq": "Bob" } }
            ]
        }))
    );
}

#[test]
fn logical_and_explicit() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("and", "(age[gte]=18,age[lte]=65)")]).unwrap();
    assert_eq!(
        result.where_clause,
        Some(serde_json::json!({
            "_and": [
                { "age": { "gte": 18 } },
                { "age": { "lte": 65 } }
            ]
        }))
    );
}

#[test]
fn logical_not() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext.extract(&[], &[("not", "(active[eq]=false)")]).unwrap();
    assert_eq!(
        result.where_clause,
        Some(serde_json::json!({
            "_not": [
                { "active": { "eq": false } }
            ]
        }))
    );
}

#[test]
fn logical_nested_or_and() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext
        .extract(&[], &[("or", "(and=(age[gte]=18,active[eq]=true),name[eq]=admin)")])
        .unwrap();
    let wc = result.where_clause.unwrap();
    assert!(wc.get("_or").is_some(), "expected _or in {wc}");
    let or_arr = wc["_or"].as_array().unwrap();
    assert_eq!(or_arr.len(), 2);
    assert!(or_arr[0].get("_and").is_some(), "expected _and in {}", or_arr[0]);
}

#[test]
fn logical_combined_with_regular_filters() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let result = ext
        .extract(
            &[],
            &[
                ("active[eq]", "true"),
                ("or", "(name[eq]=Alice,name[eq]=Bob)"),
            ],
        )
        .unwrap();

    let wc = result.where_clause.unwrap();
    // Should have _and wrapping the regular filter + the or group.
    assert!(wc.get("_and").is_some(), "expected _and wrapper in {wc}");
}

#[test]
fn logical_depth_exceeded() {
    let config = test_config();
    let qd = list_query_def();
    // No type_def to skip field validation.
    let ext = RestParamExtractor::new(&config, &qd, None);

    // Build deeply nested: or=(and=(or=(and=(...))))
    let mut inner = "name[eq]=x".to_string();
    for _ in 0..65 {
        inner = format!("or=({inner})");
    }
    let input = format!("({inner})");
    let err = ext.extract(&[], &[("or", &input)]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nesting depth") || msg.contains("depth"), "got: {msg}");
}

#[test]
fn filter_json_with_logical_operators() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let filter = r#"{"_or":[{"name":{"eq":"Alice"}},{"name":{"eq":"Bob"}}]}"#;
    let result = ext.extract(&[], &[("filter", filter)]).unwrap();
    let wc = result.where_clause.unwrap();
    assert!(wc.get("_or").is_some(), "expected _or in {wc}");
}

#[test]
fn filter_json_with_nested_logical_validates_fields() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let filter = r#"{"_or":[{"bogus":{"eq":"x"}}]}"#;
    let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
    assert!(err.to_string().contains("Unknown field 'bogus'"));
}

#[test]
fn logical_invalid_syntax() {
    let config = test_config();
    let qd = list_query_def();
    let td = user_type_def();
    let ext = extractor_list(&config, &qd, &td);

    let err = ext.extract(&[], &[("or", "not-parenthetical")]).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("must be enclosed in parentheses") || msg.contains("syntax"),
        "got: {msg}"
    );
}
