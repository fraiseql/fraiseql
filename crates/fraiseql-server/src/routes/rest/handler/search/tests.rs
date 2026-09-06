//! Tests for the `?search=` plan.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::schema::{FieldType, TypeDefinition};
use fraiseql_test_utils::schema_builder::{TestFieldBuilder, TestTypeBuilder};

use super::plan_search;

fn type_with(fields: &[(&str, FieldType)]) -> TypeDefinition {
    let mut builder = TestTypeBuilder::new("Doc", "v_doc");
    for (name, ty) in fields {
        builder = builder.with_field(TestFieldBuilder::new(name, ty.clone()).build());
    }
    builder.build()
}

/// The predicate and the ranking cover the same fields, because they are read
/// from the same `searchable_fields()` call.
///
/// They used to be two decisions in two places — the predicate here, the order
/// as a `{"_relevance": "desc"}` string in the handler — and the second one
/// described a ranking that did not exist (#1284).
#[test]
fn the_predicate_and_the_ranking_cover_the_same_fields() {
    let td = type_with(&[
        ("id", FieldType::Int),
        ("title", FieldType::String),
        ("bodyText", FieldType::String),
    ]);
    let plan = plan_search("ada lovelace", Some(&td)).unwrap();

    let or = plan.where_clause["_or"].as_array().unwrap();
    let searched: Vec<&str> = or
        .iter()
        .map(|c| c.as_object().unwrap().keys().next().unwrap().as_str())
        .collect();
    assert_eq!(searched, vec!["title", "bodyText"], "the non-String field is not searched");

    // …and the rank ranks over exactly those, as the storage keys the predicate
    // is lowered to: `bodyText` filters on `data->>'body_text'`, so it must rank
    // on `data->>'body_text'` too.
    assert_eq!(plan.relevance.fields, vec!["title".to_string(), "body_text".to_string()]);
    assert_eq!(plan.relevance.query, "ada lovelace");
}

/// One searchable field needs no `_or` wrapper — and still ranks.
#[test]
fn a_single_searchable_field_yields_a_bare_clause_and_a_rank() {
    let td = type_with(&[("id", FieldType::Int), ("label", FieldType::String)]);
    let plan = plan_search("row-42", Some(&td)).unwrap();

    assert!(plan.where_clause.get("_or").is_none(), "no wrapper: {}", plan.where_clause);
    assert_eq!(plan.where_clause["label"]["websearch_query"], "row-42");
    assert_eq!(plan.relevance.fields, vec!["label".to_string()]);
}

/// A type with nothing to search has no plan — neither a predicate that matches
/// everything nor a rank over an empty document.
#[test]
fn a_type_with_no_searchable_field_has_no_plan() {
    let td = type_with(&[("id", FieldType::Int), ("count", FieldType::Int)]);
    assert!(plan_search("x", Some(&td)).is_none());
    assert!(plan_search("x", None).is_none());
}
