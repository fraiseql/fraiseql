//! The `value_json` round trip, driven by #719's own inputs.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::collections::BTreeMap;

use graphql_parser::query::Value as GqlValue;
use serde_json::json;

use super::*;

fn gql_string(s: &str) -> GqlValue<'static, String> {
    GqlValue::String(s.to_string())
}

/// Every character class that broke the hand-rolled `s.replace('"', "\\\"")`
/// must survive the round trip byte for byte.
///
/// Before the fix each of these produced invalid JSON, and the reader's
/// `.ok()?` then *dropped* the argument — which widens a `where:` filter rather
/// than narrowing it.
#[test]
fn strings_survive_the_round_trip_intact() {
    let hostile = [
        r"C:\Users\alice\notes.txt", // backslashes — a Windows path
        "line one\nline two",        // newline
        "tab\there",                 // tab
        "quote\"inside",             // the only character the old escaper handled
        "carriage\rreturn",
        "null\u{0}byte",
        "bell\u{7}",
        "unicode ☃ snowman",
        "emoji 🍓",
        r#"{"looks":"like json"}"#,
        r"\\",
        "\\\"",
    ];

    for original in hostile {
        let encoded = encode(&gql_string(original)).expect("encodes");
        let decoded = decode(&encoded)
            .unwrap_or_else(|e| panic!("{original:?} encoded to invalid JSON {encoded:?}: {e}"));
        assert_eq!(
            decoded.as_str(),
            Some(original),
            "{original:?} did not survive the round trip (encoded as {encoded})"
        );
    }
}

/// A literal string starting with `$` is a value, not a variable reference.
///
/// The in-band `"$name"` convention made `"$100"` indistinguishable from a
/// reference to a variable called `100`, which resolved to `null`.
#[test]
fn dollar_prefixed_literals_are_not_variable_references() {
    let mut variables = HashMap::new();
    variables.insert("100".to_string(), json!("LEAKED"));
    variables.insert("price".to_string(), json!(42));

    for literal in ["$100", "$price", "$", "$$", "$var"] {
        let encoded = encode(&gql_string(literal)).expect("encodes");
        let decoded = decode(&encoded).expect("decodes");
        assert_eq!(variable_name(&decoded), None, "{literal:?} must not read as a variable");
        let resolved = resolve_variables(decoded, &variables);
        assert_eq!(
            resolved,
            json!(literal),
            "{literal:?} must resolve to itself, not to a variable's value"
        );
    }
}

/// A real variable reference still resolves, at the top level and nested.
#[test]
fn variable_references_resolve_at_every_depth() {
    let mut variables = HashMap::new();
    variables.insert("limit".to_string(), json!(25));
    variables.insert("tenant".to_string(), json!("acme"));

    let top = decode(&encode(&GqlValue::Variable("limit".to_string())).unwrap()).unwrap();
    assert_eq!(variable_name(&top), Some("limit"));
    assert_eq!(resolve_variables(top, &variables), json!(25));

    let inner: BTreeMap<String, GqlValue<'static, String>> =
        std::iter::once(("eq".to_string(), GqlValue::Variable("tenant".to_string()))).collect();
    let nested = GqlValue::Object(
        std::iter::once(("tenantId".to_string(), GqlValue::Object(inner))).collect(),
    );
    let decoded = decode(&encode(&nested).unwrap()).unwrap();
    assert_eq!(
        resolve_variables(decoded, &variables),
        json!({"tenantId": {"eq": "acme"}}),
        "a variable nested inside a where: argument must still resolve"
    );

    let in_list = GqlValue::List(vec![
        GqlValue::Variable("tenant".to_string()),
        gql_string("literal"),
    ]);
    let decoded = decode(&encode(&in_list).unwrap()).unwrap();
    assert_eq!(resolve_variables(decoded, &variables), json!(["acme", "literal"]));
}

/// An undefined variable resolves to null — GraphQL's treatment of an omitted
/// nullable — rather than being silently dropped.
#[test]
fn an_undefined_variable_resolves_to_null() {
    let decoded = decode(&encode(&GqlValue::Variable("missing".to_string())).unwrap()).unwrap();
    assert_eq!(resolve_variables(decoded, &HashMap::new()), json!(null));
}

/// The GraphQL re-serialization used by the multi-root pipeline escapes strings
/// and emits bare variable references.
#[test]
fn to_graphql_escapes_strings_and_emits_variable_references() {
    assert_eq!(to_graphql(&json!("plain")), r#""plain""#);
    assert_eq!(to_graphql(&json!("has \"quote\"")), r#""has \"quote\"""#);
    assert_eq!(to_graphql(&json!(r"back\slash")), r#""back\\slash""#);
    assert_eq!(to_graphql(&json!("new\nline")), r#""new\nline""#);
    assert_eq!(to_graphql(&variable_ref("limit")), "$limit");
    assert_eq!(to_graphql(&json!({"field": "id"})), r#"{field: "id"}"#);

    // Key *order* depends on whether `serde_json/preserve_order` is unified into
    // the build (#899), so assert the shape rather than a byte string.
    let rendered = to_graphql(&json!([{"field": "id", "direction": "ASC"}]));
    assert!(
        rendered.starts_with("[{") && rendered.ends_with("}]"),
        "a list of objects must render as a GraphQL list of objects: {rendered}"
    );
    assert!(
        rendered.contains(r#"field: "id""#) && rendered.contains(r#"direction: "ASC""#),
        "object keys must be bare GraphQL names, not quoted JSON keys: {rendered}"
    );
}

/// Decoding a malformed `value_json` errors rather than dropping the argument.
#[test]
fn a_malformed_stored_argument_errors_instead_of_being_dropped() {
    let err = decode(r#"{"unterminated": "#).expect_err("malformed JSON must not decode");
    assert!(
        format!("{err}").contains("widen"),
        "the error must say why dropping is unacceptable, got: {err}"
    );
}

/// Serialization refuses a value nested past the shared depth cap rather than
/// exhausting the stack.
#[test]
fn nesting_past_the_cap_is_refused() {
    let mut value = gql_string("leaf");
    for _ in 0..=MAX_DEPTH {
        value = GqlValue::List(vec![value]);
    }
    assert!(encode(&value).is_err(), "a value past the depth cap must be refused");
}
