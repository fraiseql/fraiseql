//! A `Decimal` is the string of its exact digits in the generated Python client.
//!
//! The server sends a `Decimal` as a JSON string (`"1.50"`) and takes an argument the
//! same way. The generated types describe the raw JSON, so a `Decimal` is `str`, never
//! `float`: a float annotation would invite a caller to pass one, which the server
//! refuses, and would describe a value the client never holds.

use fraiseql_core::schema::FieldType;

use super::render::{field_type_py, parse_input_type};

#[test]
fn a_decimal_field_is_str() {
    assert_eq!(field_type_py(&FieldType::Decimal), "str");
    assert_eq!(field_type_py(&FieldType::list(FieldType::Decimal)), "list[str]");
}

#[test]
fn a_decimal_argument_is_str() {
    for (graphql, python, required) in [
        ("Decimal", "str", false),
        ("Decimal!", "str", true),
        ("[Decimal!]!", "list[str]", true),
    ] {
        let parsed = parse_input_type(graphql);
        assert_eq!((parsed.py.as_str(), parsed.required), (python, required), "{graphql}");
    }
}
