//! Unit tests for native-column detection (pure; no database required).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use fraiseql_core::schema::FieldType;

use super::{arg_type_convertible, detect_query_native_columns};

fn cols(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

#[test]
fn inject_param_matching_a_column_is_native() {
    // The bug: an inject-only list query (no explicit args) whose view has a real
    // `tenant_id` column must filter on the native column, not the JSONB fallback
    // (`data->>'tenant_id'`, which is NULL when `data` lacks the key → 0 rows).
    let column_map = cols(&[("id", "uuid"), ("tenant_id", "uuid"), ("data", "jsonb")]);
    let (native, fallbacks) =
        detect_query_native_columns(&[], ["tenant_id"].into_iter(), &column_map);
    assert_eq!(native.get("tenant_id").map(String::as_str), Some("uuid"));
    assert!(fallbacks.is_empty(), "inject-param matches must not emit a fallback warning");
}

#[test]
fn inject_param_without_a_column_is_silent_jsonb_fallback() {
    // A claim that lives inside `data` (no matching column) stays on the JSONB path
    // with no warning — that is a legitimate shape for inject params.
    let column_map = cols(&[("id", "uuid"), ("data", "jsonb")]);
    let (native, fallbacks) =
        detect_query_native_columns(&[], ["tenant_id"].into_iter(), &column_map);
    assert!(native.is_empty());
    assert!(fallbacks.is_empty());
}

#[test]
fn explicit_arg_matching_a_column_is_native() {
    let column_map = cols(&[("id", "uuid"), ("data", "jsonb")]);
    let (native, fallbacks) =
        detect_query_native_columns(&["id"], std::iter::empty::<&str>(), &column_map);
    assert_eq!(native.get("id").map(String::as_str), Some("uuid"));
    assert!(fallbacks.is_empty());
}

#[test]
fn explicit_arg_without_a_column_warns_fallback() {
    // Explicit args keep the existing fallback warning (the author likely expects an
    // indexed native-column lookup).
    let column_map = cols(&[("data", "jsonb")]);
    let (native, fallbacks) =
        detect_query_native_columns(&["slug"], std::iter::empty::<&str>(), &column_map);
    assert!(native.is_empty());
    assert_eq!(fallbacks, vec!["slug".to_string()]);
}

#[test]
fn args_and_inject_params_combine() {
    let column_map = cols(&[("id", "uuid"), ("tenant_id", "uuid")]);
    let (native, fallbacks) =
        detect_query_native_columns(&["id"], ["tenant_id"].into_iter(), &column_map);
    assert_eq!(native.len(), 2);
    assert_eq!(native.get("id").map(String::as_str), Some("uuid"));
    assert_eq!(native.get("tenant_id").map(String::as_str), Some("uuid"));
    assert!(fallbacks.is_empty());
}

// ─── arg↔column type-convertibility (pure; #384 Gap B) ──────────────────────

#[test]
fn int_arg_against_numeric_column_is_convertible() {
    assert!(arg_type_convertible(&FieldType::Int, "integer"));
    assert!(arg_type_convertible(&FieldType::Int, "bigint"));
    assert!(arg_type_convertible(&FieldType::Float, "numeric"));
    assert!(arg_type_convertible(&FieldType::Decimal, "double precision"));
}

#[test]
fn int_arg_against_non_numeric_column_is_not_convertible() {
    // The canonical bug: an Int filter bound against a uuid/text/bool column.
    assert!(!arg_type_convertible(&FieldType::Int, "uuid"));
    assert!(!arg_type_convertible(&FieldType::Int, "text"));
    assert!(!arg_type_convertible(&FieldType::Int, "boolean"));
}

#[test]
fn boolean_arg_only_matches_boolean_column() {
    assert!(arg_type_convertible(&FieldType::Boolean, "boolean"));
    assert!(!arg_type_convertible(&FieldType::Boolean, "text"));
    assert!(!arg_type_convertible(&FieldType::Boolean, "integer"));
}

#[test]
fn uuid_arg_matches_uuid_or_text_column() {
    assert!(arg_type_convertible(&FieldType::Uuid, "uuid"));
    // uuids are commonly stored as text in SQLite / portable schemas.
    assert!(arg_type_convertible(&FieldType::Uuid, "character varying"));
    assert!(!arg_type_convertible(&FieldType::Uuid, "bigint"));
}

#[test]
fn id_and_string_args_are_permissive() {
    // `ID` intentionally spans uuid / integer / text key columns — never warn.
    assert!(arg_type_convertible(&FieldType::Id, "uuid"));
    assert!(arg_type_convertible(&FieldType::Id, "bigint"));
    assert!(arg_type_convertible(&FieldType::Id, "text"));
    // `String` binds as a text-coercible parameter — never warn.
    assert!(arg_type_convertible(&FieldType::String, "integer"));
}

#[test]
fn unknown_column_family_is_never_flagged() {
    // Custom domains, enums, geometry, arrays … are not second-guessed.
    assert!(arg_type_convertible(&FieldType::Int, "mood_enum"));
    assert!(arg_type_convertible(&FieldType::Boolean, "ltree"));
}

#[test]
fn list_arg_is_checked_by_its_element_type() {
    // `[Int!]` against a uuid column is still a mismatch; `[ID!]` is permissive.
    let int_list = FieldType::List(Box::new(FieldType::Int));
    assert!(!arg_type_convertible(&int_list, "uuid"));
    let id_list = FieldType::List(Box::new(FieldType::Id));
    assert!(arg_type_convertible(&id_list, "uuid"));
}
