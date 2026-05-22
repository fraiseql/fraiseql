//! Tests for the `embedding` module.

#![allow(clippy::unwrap_used)]

use fraiseql_core::schema::{Cardinality, Relationship};

use super::executor::{
    extract_join_key, extract_query_data, find_list_query_for_type, set_empty_embedding,
};

#[test]
fn extract_join_key_one_to_many() {
    let rel = Relationship {
        name:           "posts".to_string(),
        target_type:    "Post".to_string(),
        foreign_key:    "fk_user".to_string(),
        referenced_key: "pk_user".to_string(),
        cardinality:    Cardinality::OneToMany,
    };
    let row = serde_json::json!({"pk_user": 42, "name": "Alice"});
    let key = extract_join_key(&row, &rel);
    assert_eq!(key, Some(serde_json::json!(42)));
}

#[test]
fn extract_join_key_many_to_one() {
    let rel = Relationship {
        name:           "author".to_string(),
        target_type:    "User".to_string(),
        foreign_key:    "fk_user".to_string(),
        referenced_key: "pk_user".to_string(),
        cardinality:    Cardinality::ManyToOne,
    };
    let row = serde_json::json!({"fk_user": 7, "title": "Hello"});
    let key = extract_join_key(&row, &rel);
    assert_eq!(key, Some(serde_json::json!(7)));
}

#[test]
fn extract_join_key_null_returns_none() {
    let rel = Relationship {
        name:           "author".to_string(),
        target_type:    "User".to_string(),
        foreign_key:    "fk_user".to_string(),
        referenced_key: "pk_user".to_string(),
        cardinality:    Cardinality::ManyToOne,
    };
    let row = serde_json::json!({"fk_user": null, "title": "Hello"});
    assert!(extract_join_key(&row, &rel).is_none());
}

#[test]
fn extract_join_key_missing_field_returns_none() {
    let rel = Relationship {
        name:           "posts".to_string(),
        target_type:    "Post".to_string(),
        foreign_key:    "fk_user".to_string(),
        referenced_key: "pk_user".to_string(),
        cardinality:    Cardinality::OneToMany,
    };
    let row = serde_json::json!({"name": "Alice"});
    assert!(extract_join_key(&row, &rel).is_none());
}

#[test]
fn set_empty_embedding_one_to_many() {
    let mut row = serde_json::json!({"id": 1});
    set_empty_embedding(&mut row, "posts", Cardinality::OneToMany);
    assert_eq!(row["posts"], serde_json::json!([]));
}

#[test]
fn set_empty_embedding_many_to_one() {
    let mut row = serde_json::json!({"id": 1});
    set_empty_embedding(&mut row, "author", Cardinality::ManyToOne);
    assert!(row["author"].is_null());
}

#[test]
fn set_empty_embedding_one_to_one() {
    let mut row = serde_json::json!({"id": 1});
    set_empty_embedding(&mut row, "profile", Cardinality::OneToOne);
    assert!(row["profile"].is_null());
}

#[test]
fn extract_query_data_standard_envelope() {
    let parsed = serde_json::json!({
        "data": {
            "posts": [
                {"id": 1, "title": "Hello"},
                {"id": 2, "title": "World"},
            ]
        }
    });
    let data = extract_query_data(&parsed, "posts").unwrap();
    assert!(data.is_array());
    assert_eq!(data.as_array().unwrap().len(), 2);
}

#[test]
fn extract_query_data_missing_query_returns_none() {
    let parsed = serde_json::json!({"data": {}});
    assert!(extract_query_data(&parsed, "posts").is_none());
}

#[test]
fn find_list_query_for_type_returns_list_query() {
    use fraiseql_core::schema::{CompiledSchema, QueryDefinition};

    let mut schema = CompiledSchema::default();
    schema.queries.push(QueryDefinition {
        name: "post".to_string(),
        return_type: "Post".to_string(),
        returns_list: false,
        ..QueryDefinition::new("post", "Post")
    });
    schema.queries.push(QueryDefinition {
        name: "posts".to_string(),
        return_type: "Post".to_string(),
        returns_list: true,
        ..QueryDefinition::new("posts", "Post")
    });

    let found = find_list_query_for_type(&schema, "Post");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "posts");
}

#[test]
fn find_list_query_for_type_no_match() {
    let schema = fraiseql_core::schema::CompiledSchema::default();
    assert!(find_list_query_for_type(&schema, "Post").is_none());
}
