//! Tests for the `[rest]` TOML configuration section.

use fraiseql_cli::config::toml_schema::TomlSchema;

#[test]
fn toml_with_rest_section_parses() {
    let toml_str = r#"
[schema]
name = "test"

[rest]
enabled = true
path = "/rest/v1"
"#;
    let schema: TomlSchema = toml::from_str(toml_str).expect("should parse [rest] section");
    let rest = schema.rest.expect("rest should be Some");
    assert!(rest.enabled);
    assert_eq!(rest.path, "/rest/v1");
}

#[test]
fn toml_without_rest_section_defaults_to_none() {
    let toml_str = r#"
[schema]
name = "test"
"#;
    let schema: TomlSchema = toml::from_str(toml_str).expect("should parse without [rest]");
    assert!(schema.rest.is_none());
}
