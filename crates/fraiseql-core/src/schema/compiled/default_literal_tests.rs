//! Default values publish as GraphQL literals.
//!
//! The compiled schema stores a default as JSON text; the SDL and introspection's
//! `__InputValue.defaultValue` must print it as a GraphQL literal of the declared type:
//! an enum value bare, an input object with bare keys, a string with GraphQL escapes.
//! A default that has no such literal is refused when the schema loads, which is both
//! the server's boot and the compiler's own load check.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use apollo_parser::{Parser, cst};

use super::CompiledSchema;
use crate::schema::{IntrospectionBuilder, IntrospectionInputValue};

/// `label`'s default as JSON text: `a "quoted"` + newline + `line`.
const LABEL_DEFAULT: &str = r#""a \"quoted\"\nline""#;

/// A compiled schema whose input fields and arguments carry the given defaults.
fn schema_json(
    status_default: &str,
    filter_default: &str,
    meta_default: &str,
    arg_default: &str,
) -> String {
    let label = serde_json::to_string(LABEL_DEFAULT).unwrap();
    let status = serde_json::to_string(status_default).unwrap();
    let filter = serde_json::to_string(filter_default).unwrap();
    let meta = serde_json::to_string(meta_default).unwrap();
    format!(
        r#"{{
        "types": [{{"name":"Item","sql_source":"v_item","fields":[{{"name":"id","field_type":"ID"}}]}}],
        "enums": [{{"name":"Status","values":[{{"name":"ACTIVE"}},{{"name":"ARCHIVED"}}]}}],
        "input_types": [
            {{"name":"ItemFilter","fields":[
                {{"name":"status","field_type":"Status","default_value":"\"ACTIVE\""}},
                {{"name":"tags","field_type":"[Status!]","default_value":"[\"ACTIVE\"]"}}
            ]}},
            {{"name":"CreateItemInput","fields":[
                {{"name":"name","field_type":"String","nullable":false}},
                {{"name":"isActive","field_type":"Boolean","nullable":false,"default_value":"false"}},
                {{"name":"count","field_type":"Int","default_value":"0"}},
                {{"name":"ratio","field_type":"Float","default_value":"1.5"}},
                {{"name":"label","field_type":"String","default_value":{label}}},
                {{"name":"status","field_type":"Status","nullable":false,"default_value":{status}}},
                {{"name":"filter","field_type":"ItemFilter","default_value":{filter}}},
                {{"name":"meta","field_type":"JSON","default_value":{meta}}}
            ]}}
        ],
        "queries": [{{"name":"items","return_type":"Item","returns_list":true,"sql_source":"v_item","arguments":[
            {{"name":"orderStatus","arg_type":{{"Enum":"Status"}},"nullable":true,"default_value":{arg_default}}},
            {{"name":"limit","arg_type":"Int","nullable":true,"default_value":10}}
        ]}}],
        "mutations": [{{"name":"createItem","return_type":"Item","sql_source":"fn_create_item","arguments":[
            {{"name":"input","arg_type":{{"Input":"CreateItemInput"}},"nullable":false}}
        ]}}],
        "subscriptions": []
    }}"#
    )
}

fn valid_json() -> String {
    schema_json(
        r#""ARCHIVED""#,
        r#"{"status":"ARCHIVED","tags":["ACTIVE"]}"#,
        r#"{"k":1}"#,
        r#""ACTIVE""#,
    )
}

fn valid() -> CompiledSchema {
    CompiledSchema::from_json(&valid_json(), false).unwrap()
}

/// The input-field lines of `input <name> { … }` in the generated SDL.
fn sdl_input_lines(sdl: &str, name: &str) -> Vec<String> {
    let head = format!("input {name} {{");
    let body = sdl.split(&head).nth(1).unwrap_or_else(|| panic!("no `{head}` in:\n{sdl}"));
    // The type ends at its own `}` line: an object default holds braces of its own.
    body.lines()
        .map(str::trim)
        .take_while(|l| *l != "}")
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

fn input_value<'a>(
    values: &'a [IntrospectionInputValue],
    name: &str,
) -> &'a IntrospectionInputValue {
    values
        .iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("no input value `{name}`"))
}

fn introspected_input_fields(schema: &CompiledSchema, input: &str) -> Vec<IntrospectionInputValue> {
    let introspection = IntrospectionBuilder::build(schema);
    let t = introspection
        .types
        .iter()
        .find(|t| t.name.as_deref() == Some(input))
        .unwrap_or_else(|| panic!("introspection has no `{input}`"));
    t.input_fields.clone().unwrap()
}

fn introspected_query_args(schema: &CompiledSchema, query: &str) -> Vec<IntrospectionInputValue> {
    let introspection = IntrospectionBuilder::build(schema);
    let root = introspection.types.iter().find(|t| t.name.as_deref() == Some("Query")).unwrap();
    let field = root.fields.as_ref().unwrap().iter().find(|f| f.name == query).unwrap();
    field.args.clone()
}

/// The CST kind of `literal` parsed as an argument value, or the parser's errors.
fn literal_kind(literal: &str) -> Result<&'static str, String> {
    let source = format!("{{ f(a: {literal}) }}");
    let tree = Parser::new(&source).parse();
    let errors: Vec<String> = tree.errors().map(|e| e.message().to_string()).collect();
    if !errors.is_empty() {
        return Err(format!("`{literal}` is not a GraphQL literal: {errors:?}"));
    }
    let doc = tree.document();
    let value = doc
        .definitions()
        .find_map(|d| match d {
            cst::Definition::OperationDefinition(op) => op.selection_set(),
            _ => None,
        })
        .and_then(|s| s.selections().next())
        .and_then(|s| match s {
            cst::Selection::Field(f) => f.arguments(),
            _ => None,
        })
        .and_then(|a| a.arguments().next())
        .and_then(|a| a.value())
        .unwrap();
    Ok(match value {
        cst::Value::EnumValue(_) => "enum",
        cst::Value::StringValue(_) => "string",
        cst::Value::ObjectValue(_) => "object",
        cst::Value::ListValue(_) => "list",
        cst::Value::BooleanValue(_) => "boolean",
        cst::Value::IntValue(_) => "int",
        cst::Value::FloatValue(_) => "float",
        cst::Value::NullValue(_) => "null",
        cst::Value::Variable(_) => "variable",
    })
}

// ── SDL ─────────────────────────────────────────────────────────────────────

#[test]
fn the_issue_repro_a_non_null_input_field_with_a_default_prints_it() {
    let lines = sdl_input_lines(&valid().raw_schema(), "CreateItemInput");
    assert!(lines.contains(&"isActive: Boolean! = false".to_string()), "{lines:#?}");
}

#[test]
fn a_non_null_input_field_without_a_default_stays_required() {
    let lines = sdl_input_lines(&valid().raw_schema(), "CreateItemInput");
    assert!(lines.contains(&"name: String!".to_string()), "{lines:#?}");
}

#[test]
fn every_input_field_default_prints_as_a_literal_of_its_type() {
    let lines = sdl_input_lines(&valid().raw_schema(), "CreateItemInput");
    for expected in [
        "count: Int = 0",
        "ratio: Float = 1.5",
        r#"label: String = "a \"quoted\"\nline""#,
        "status: Status! = ARCHIVED",
        "filter: ItemFilter = {status: ARCHIVED, tags: [ACTIVE]}",
        "meta: JSON = {k: 1}",
    ] {
        assert!(lines.contains(&expected.to_string()), "missing `{expected}` in {lines:#?}");
    }
    let nested = sdl_input_lines(&valid().raw_schema(), "ItemFilter");
    assert!(nested.contains(&"status: Status = ACTIVE".to_string()), "{nested:#?}");
    assert!(nested.contains(&"tags: [Status!] = [ACTIVE]".to_string()), "{nested:#?}");
}

#[test]
fn an_enum_argument_default_prints_bare() {
    let sdl = valid().raw_schema();
    assert!(sdl.contains("orderStatus: Status = ACTIVE"), "{sdl}");
    assert!(sdl.contains("limit: Int = 10"), "{sdl}");
}

#[test]
fn the_generated_sdl_parses() {
    let sdl = valid().raw_schema();
    let tree = Parser::new(&sdl).parse();
    let errors: Vec<String> = tree.errors().map(|e| e.message().to_string()).collect();
    assert!(errors.is_empty(), "{errors:?}\n{sdl}");
}

// ── Introspection ───────────────────────────────────────────────────────────

#[test]
fn introspection_publishes_input_field_defaults_as_literals() {
    let schema = valid();
    let fields = introspected_input_fields(&schema, "CreateItemInput");
    for (name, literal, kind) in [
        ("isActive", "false", "boolean"),
        ("count", "0", "int"),
        ("ratio", "1.5", "float"),
        ("label", r#""a \"quoted\"\nline""#, "string"),
        ("status", "ARCHIVED", "enum"),
        ("filter", "{status: ARCHIVED, tags: [ACTIVE]}", "object"),
        ("meta", "{k: 1}", "object"),
    ] {
        let published = input_value(&fields, name).default_value.as_deref();
        assert_eq!(published, Some(literal), "`{name}`");
        assert_eq!(literal_kind(literal), Ok(kind), "`{name}`");
    }
    assert_eq!(input_value(&fields, "name").default_value, None);
}

#[test]
fn introspection_publishes_argument_defaults_as_literals() {
    let args = introspected_query_args(&valid(), "items");
    assert_eq!(input_value(&args, "orderStatus").default_value.as_deref(), Some("ACTIVE"));
    assert_eq!(literal_kind("ACTIVE"), Ok("enum"));
    assert_eq!(input_value(&args, "limit").default_value.as_deref(), Some("10"));
}

// ── Refused at load ─────────────────────────────────────────────────────────

fn load_error(json: &str) -> String {
    CompiledSchema::from_json(json, false)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

#[test]
fn the_valid_fixture_loads() {
    assert_eq!(load_error(&valid_json()), "");
}

#[test]
fn an_enum_default_naming_no_value_is_refused_at_load() {
    let error = load_error(&schema_json(
        r#""GONE""#,
        r#"{"status":"ARCHIVED"}"#,
        r#"{"k":1}"#,
        r#""ACTIVE""#,
    ));
    for part in ["CreateItemInput.status", "GONE", "Status", "recompile"] {
        assert!(error.contains(part), "missing `{part}` in: {error}");
    }
}

#[test]
fn an_input_object_default_with_an_unknown_key_is_refused_at_load() {
    let error = load_error(&schema_json(
        r#""ARCHIVED""#,
        r#"{"colour":"red"}"#,
        r#"{"k":1}"#,
        r#""ACTIVE""#,
    ));
    for part in [
        "CreateItemInput.filter",
        "colour",
        "ItemFilter",
        "recompile",
    ] {
        assert!(error.contains(part), "missing `{part}` in: {error}");
    }
}

#[test]
fn a_custom_scalar_default_whose_key_is_not_a_name_is_refused_at_load() {
    let error = load_error(&schema_json(
        r#""ARCHIVED""#,
        r#"{"status":"ARCHIVED"}"#,
        r#"{"not-a-name":1}"#,
        r#""ACTIVE""#,
    ));
    for part in ["CreateItemInput.meta", "not-a-name", "recompile"] {
        assert!(error.contains(part), "missing `{part}` in: {error}");
    }
}

#[test]
fn an_argument_enum_default_naming_no_value_is_refused_at_load() {
    let error = load_error(&schema_json(
        r#""ARCHIVED""#,
        r#"{"status":"ARCHIVED"}"#,
        r#"{"k":1}"#,
        r#""GONE""#,
    ));
    for part in ["items(orderStatus:)", "GONE", "Status", "recompile"] {
        assert!(error.contains(part), "missing `{part}` in: {error}");
    }
}

#[test]
fn a_null_default_for_a_non_null_field_is_refused_at_load() {
    let json = valid_json().replace(r#""default_value":"false""#, r#""default_value":"null""#);
    assert_ne!(json, valid_json(), "the fixture must change");
    let error = load_error(&json);
    for part in [
        "CreateItemInput.isActive",
        "`null` is not a value of a non-null type",
        "recompile",
    ] {
        assert!(error.contains(part), "missing `{part}` in: {error}");
    }
}

#[test]
fn an_input_field_default_that_is_not_json_is_refused_at_load() {
    let json = valid_json().replace(r#""default_value":"0""#, r#""default_value":"zero""#);
    assert_ne!(json, valid_json(), "the fixture must change");
    let error = load_error(&json);
    for part in ["CreateItemInput.count", "recompile"] {
        assert!(error.contains(part), "missing `{part}` in: {error}");
    }
}
