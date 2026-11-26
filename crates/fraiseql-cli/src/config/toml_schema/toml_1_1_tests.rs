//! `fraiseql.toml` is parsed as TOML 1.1. These pin the syntax TOML 1.0 refused, so the
//! difference from the Python SDK's `load_config`, which parses TOML 1.0, is deliberate.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::TomlSchema;

const HEAD: &str =
    "[project]\nname = \"myapp\"\nversion = \"1.0.0\"\ndatabase_target = \"postgresql\"\n\n";

#[test]
fn an_inline_table_may_end_with_a_comma() {
    let toml = format!(
        "{HEAD}[types.User]\nsql_source = \"v_user\"\nfields = {{ id = {{ type = \"ID\", nullable = false, }}, }}\n"
    );
    let schema = TomlSchema::parse_toml(&toml).unwrap();
    assert!(schema.types["User"].fields.contains_key("id"));
}

#[test]
fn an_inline_table_may_span_lines() {
    let toml = format!(
        "{HEAD}[types.User]\nsql_source = \"v_user\"\nfields = {{\n  id = {{ type = \"ID\", nullable = false }}\n}}\n"
    );
    let schema = TomlSchema::parse_toml(&toml).unwrap();
    assert!(schema.types["User"].fields.contains_key("id"));
}

#[test]
fn a_basic_string_may_hold_a_hex_escape() {
    let toml =
        "[project]\nname = \"my\\x61pp\"\nversion = \"1.0.0\"\ndatabase_target = \"postgresql\"\n";
    assert_eq!(TomlSchema::parse_toml(toml).unwrap().project.name, "myapp");
}
