//! A `Decimal` is the string of its exact digits in the generated `TypeScript` client.
//!
//! The server sends a `Decimal` as a JSON string (`"1.50"`) and takes an argument the
//! same way. A JS `number` is a binary float, so a `Decimal` is `string`, never `number`:
//! a `number` annotation would invite a caller to pass one, which the server refuses.

use super::{FieldType, field_type_ts, parse_input_type};

#[test]
fn a_decimal_field_is_string() {
    assert_eq!(field_type_ts(&FieldType::Decimal), "string");
    assert_eq!(field_type_ts(&FieldType::list(FieldType::Decimal)), "string[]");
}

#[test]
fn a_decimal_argument_is_string() {
    for (graphql, ts, required) in [
        ("Decimal", "string", false),
        ("Decimal!", "string", true),
        ("[Decimal!]!", "string[]", true),
    ] {
        let parsed = parse_input_type(graphql);
        assert_eq!((parsed.ts.as_str(), parsed.required), (ts, required), "{graphql}");
    }
}
