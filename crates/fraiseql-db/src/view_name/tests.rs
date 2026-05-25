#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use super::*;

#[test]
fn from_str_and_from_string_are_equal() {
    let a: ViewName = "v_user".into();
    let b: ViewName = String::from("v_user").into();
    assert_eq!(a, b);
}

#[test]
fn display_matches_raw_string() {
    let name: ViewName = "benchmark.v_user".into();
    assert_eq!(name.to_string(), "benchmark.v_user");
}

#[test]
fn deref_to_str() {
    let name: ViewName = "v_user".into();
    let s: &str = &name;
    assert_eq!(s, "v_user");
    assert!(name.starts_with("v_"));
}

#[test]
fn cloning_is_cheap_arc_bump() {
    let original: ViewName = "v_user".into();
    let cloned = original.clone();
    // Same Arc allocation
    assert!(Arc::ptr_eq(&original.0, &cloned.0));
}

#[test]
fn hashmap_lookup_by_str_via_borrow() {
    let mut m: HashMap<ViewName, u32> = HashMap::new();
    m.insert("v_user".into(), 1);
    m.insert("v_post".into(), 2);

    // Look up by &str, not ViewName — proves Borrow<str> works.
    assert_eq!(m.get("v_user"), Some(&1));
    assert_eq!(m.get("v_post"), Some(&2));
    assert_eq!(m.get("v_missing"), None);
}

#[test]
fn serde_round_trip_is_transparent_string() {
    let name: ViewName = "v_user".into();
    let json = serde_json::to_string(&name).expect("serialize ViewName");
    assert_eq!(json, "\"v_user\"");

    let back: ViewName = serde_json::from_str(&json).expect("deserialize ViewName");
    assert_eq!(back, name);
}

#[test]
fn partial_eq_str_and_string() {
    let name: ViewName = "v_user".into();
    assert_eq!(name, "v_user");
    assert_eq!(name, String::from("v_user"));
    assert_ne!(name, "v_other");
}

#[test]
fn as_arc_shares_allocation() {
    let name: ViewName = "v_user".into();
    let arc1 = name.as_arc();
    let arc2 = name.as_arc();
    assert!(Arc::ptr_eq(&arc1, &arc2));
    assert!(Arc::ptr_eq(&arc1, &name.0));
}
