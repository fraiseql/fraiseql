//! Unit tests for native-column detection (pure; no database required).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use super::detect_query_native_columns;

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
