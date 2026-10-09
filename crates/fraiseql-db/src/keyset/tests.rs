#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

fn key(expr: &str, direction: OrderDirection) -> KeysetKey {
    KeysetKey {
        expr: expr.to_string(),
        direction,
        cast: Cast::Text,
    }
}

fn predicate(keys: &[KeysetKey], values: &[Option<&str>], forward: bool) -> String {
    let values: Vec<Option<String>> = values.iter().map(|v| v.map(str::to_string)).collect();
    keyset_predicate(keys, &values, "pk", |p| p.to_string(), forward, 1).unwrap().0
}

#[test]
fn a_single_ascending_key_forward_places_nulls_last() {
    let keys = [key("k", OrderDirection::Asc)];
    assert_eq!(
        predicate(&keys, &[Some("a")], true),
        "((k > $1::text OR k IS NULL) OR (k = $1::text AND pk > $2))"
    );
}

#[test]
fn from_a_null_a_descending_key_forward_reaches_every_value() {
    let keys = [key("k", OrderDirection::Desc)];
    assert_eq!(predicate(&keys, &[None], true), "(k IS NOT NULL OR (k IS NULL AND pk > $1))");
}

#[test]
fn from_a_null_an_ascending_key_forward_reaches_only_the_position() {
    let keys = [key("k", OrderDirection::Asc)];
    assert_eq!(predicate(&keys, &[None], true), "((k IS NULL AND pk > $1))");
}

#[test]
fn a_backward_page_reverses_every_comparison() {
    let keys = [key("k", OrderDirection::Asc)];
    assert_eq!(
        predicate(&keys, &[Some("a")], false),
        "(k < $1::text OR (k = $1::text AND pk < $2))"
    );
}

#[test]
fn a_cursor_with_the_wrong_number_of_keys_is_refused() {
    let keys = [key("k", OrderDirection::Asc)];
    assert!(keyset_predicate(&keys, &[], "pk", |p| p.to_string(), true, 1).is_err());
}

#[test]
fn a_backward_order_reverses_every_key_and_the_position() {
    let keys = [
        key("a", OrderDirection::Asc),
        key("b", OrderDirection::Desc),
    ];
    assert_eq!(keyset_order(&keys, "pk", true), "a ASC, b DESC, pk ASC");
    assert_eq!(keyset_order(&keys, "pk", false), "a DESC, b ASC, pk DESC");
}

/// Every value is bound as text, so each cast resolves the placeholder as `text` first: a
/// boolean or a native column's type included, whose filter-path casts leave it bare.
#[test]
fn every_bound_value_is_resolved_as_text_before_its_cast() {
    let cast = |cast| KeysetKey {
        expr: "k".to_string(),
        direction: OrderDirection::Asc,
        cast,
    };
    assert_eq!(cast(Cast::Scalar(ScalarFieldType::Boolean)).param("$1"), "($1::text)::boolean");
    assert_eq!(cast(Cast::Scalar(ScalarFieldType::Uuid)).param("$1"), "$1::text");
    assert_eq!(cast(Cast::Native("bool".to_string())).param("$1"), "($1::text)::boolean");
    assert_eq!(cast(Cast::Native("varchar".to_string())).param("$1"), "$1::text");
    assert_eq!(cast(Cast::Native("integer".to_string())).param("$1"), "$1::text::integer");
    assert_eq!(cast(Cast::Float).param("$1"), "($1::text)::float8");
}

/// The signature tells orderings apart by key and by direction.
#[test]
fn the_signature_names_each_key_and_its_direction() {
    let keys = [
        key("a", OrderDirection::Asc),
        key("b", OrderDirection::Desc),
    ];
    assert_eq!(keyset_signature(&keys), "a ASC, b DESC");
    assert_ne!(
        keyset_signature(&[key("a", OrderDirection::Asc)]),
        keyset_signature(&[key("a", OrderDirection::Desc)])
    );
}
