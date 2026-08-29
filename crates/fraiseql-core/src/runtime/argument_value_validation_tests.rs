//! Unit tests for argument-value validation (§ 5.6.1, § 5.8.5, § 6.1.2).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use serde_json::json;

use super::*;
use crate::graphql::types::GraphQLType;

fn named(name: &str, nullable: bool) -> GraphQLType {
    GraphQLType {
        name: name.to_string(),
        nullable,
        list: false,
        list_nullable: true,
    }
}

fn var(name: &str, type_name: &str, nullable: bool) -> VariableDefinition {
    VariableDefinition {
        name:          name.to_string(),
        var_type:      named(type_name, nullable),
        default_value: None,
    }
}

fn literal(name: &str, value_type: &str, value_json: &str) -> GraphQLArgument {
    GraphQLArgument {
        name:       name.to_string(),
        value_type: value_type.to_string(),
        value_json: value_json.to_string(),
    }
}

fn var_use(name: &str, var_name: &str) -> GraphQLArgument {
    GraphQLArgument {
        name:       name.to_string(),
        value_type: "variable".to_string(),
        value_json: serde_json::to_string(&value_json::variable_ref(var_name)).unwrap(),
    }
}

fn limit_arg() -> Vec<ArgumentDefinition> {
    vec![ArgumentDefinition::optional("limit", FieldType::Int)]
}

fn message(result: Result<()>) -> String {
    match result {
        Err(FraiseQLError::Validation { message, .. }) => message,
        other => panic!("expected a Validation error, got {other:?}"),
    }
}

// ---------------------------------------------------------------- § 5.6.1

#[test]
fn an_int_literal_is_accepted_at_an_int_argument() {
    let args = [literal("limit", "int", "2")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &[]).is_ok());
}

#[test]
fn a_string_literal_is_refused_at_an_int_argument() {
    let args = [literal("limit", "string", "\"2\"")];
    let msg = message(validate_argument_values("Query.things", &limit_arg(), &args, &[]));
    assert!(msg.contains("limit"), "message was: {msg}");
    assert!(msg.contains("Int"), "message was: {msg}");
    assert!(msg.contains("String"), "message was: {msg}");
}

#[test]
fn a_float_literal_is_refused_at_an_int_argument() {
    let args = [literal("limit", "float", "2.5")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &[]).is_err());
}

#[test]
fn a_boolean_literal_is_refused_at_an_int_argument() {
    let args = [literal("limit", "boolean", "true")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &[]).is_err());
}

#[test]
fn an_int_literal_past_the_32_bit_range_is_refused() {
    let args = [literal("limit", "int", "99999999999999")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &[]).is_err());
}

#[test]
fn an_explicit_null_is_accepted_because_nullability_is_not_adjudicated_here() {
    let args = [literal("limit", "null", "null")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &[]).is_ok());
}

#[test]
fn an_argument_the_field_does_not_declare_is_left_to_the_name_rule() {
    // `first` is accepted by name on a relay query but carries no published
    // type, so it is not adjudicated here.
    let args = [literal("first", "string", "\"2\"")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &[]).is_ok());
}

#[test]
fn a_json_argument_accepts_every_shape() {
    let declared = vec![ArgumentDefinition::optional("where", FieldType::Json)];
    for (kind, body) in [("string", "\"x\""), ("int", "1"), ("object", "{\"a\":1}")] {
        let args = [literal("where", kind, body)];
        assert!(
            validate_argument_values("Query.things", &declared, &args, &[]).is_ok(),
            "JSON argument refused a {kind} value"
        );
    }
}

#[test]
fn an_enum_literal_is_not_adjudicated_as_a_scalar() {
    let declared = vec![ArgumentDefinition::optional("status", FieldType::String)];
    let args = [literal("status", "enum", "\"PENDING\"")];
    assert!(validate_argument_values("Query.things", &declared, &args, &[]).is_ok());
}

#[test]
fn a_string_literal_is_accepted_at_a_uuid_argument() {
    let declared = vec![ArgumentDefinition::new("id", FieldType::Uuid)];
    let args = [literal(
        "id",
        "string",
        "\"11110000-0000-0000-0000-000000000001\"",
    )];
    assert!(validate_argument_values("Query.thing", &declared, &args, &[]).is_ok());
}

#[test]
fn an_int_literal_is_refused_at_a_uuid_argument() {
    let declared = vec![ArgumentDefinition::new("id", FieldType::Uuid)];
    let args = [literal("id", "int", "1")];
    assert!(validate_argument_values("Query.thing", &declared, &args, &[]).is_err());
}

// ---------------------------------------------------------------- § 5.8.5

#[test]
fn an_int_declared_variable_is_usable_at_an_int_argument() {
    let vars = [var("n", "Int", false)];
    let args = [var_use("limit", "n")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &vars).is_ok());
}

#[test]
fn a_string_declared_variable_is_refused_at_an_int_argument() {
    let vars = [var("n", "String", false)];
    let args = [var_use("limit", "n")];
    let msg = message(validate_argument_values("Query.things", &limit_arg(), &args, &vars));
    assert!(msg.contains("$n"), "message was: {msg}");
    assert!(msg.contains("String"), "message was: {msg}");
    assert!(msg.contains("Int"), "message was: {msg}");
}

#[test]
fn a_string_declared_variable_is_allowed_at_a_uuid_argument() {
    // A code generator that maps a custom scalar to String is not the mistake
    // this rule exists to catch.
    let declared = vec![ArgumentDefinition::new("id", FieldType::Uuid)];
    let vars = [var("id", "String", false)];
    let args = [var_use("id", "id")];
    assert!(validate_argument_values("Query.thing", &declared, &args, &vars).is_ok());
}

#[test]
fn a_variable_declared_at_a_type_the_schema_does_not_publish_is_not_adjudicated() {
    let vars = [var("n", "SomeProjectScalar", false)];
    let args = [var_use("limit", "n")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &vars).is_ok());
}

#[test]
fn an_undeclared_variable_reference_is_left_to_the_5_8_3_rule() {
    let args = [var_use("limit", "nowhere")];
    assert!(validate_argument_values("Query.things", &limit_arg(), &args, &[]).is_ok());
}

// ---------------------------------------------------------------- § 6.1.2

#[test]
fn a_supplied_value_of_the_declared_type_is_accepted() {
    let vars = [var("n", "Int", false)];
    let values = json!({"n": 2});
    assert!(validate_variable_values(Some("Q"), &vars, Some(&values)).is_ok());
}

#[test]
fn a_supplied_value_contradicting_its_declaration_is_refused() {
    let vars = [var("n", "Int", false)];
    let values = json!({"n": "2"});
    let msg = message(validate_variable_values(Some("Q"), &vars, Some(&values)));
    assert!(msg.contains("$n"), "message was: {msg}");
    assert!(msg.contains("Int"), "message was: {msg}");
    assert!(msg.contains('Q'), "message was: {msg}");
}

#[test]
fn a_non_null_variable_with_no_value_is_refused() {
    let vars = [var("n", "Int", false)];
    let msg = message(validate_variable_values(None, &vars, None));
    assert!(msg.contains("$n"), "message was: {msg}");
    assert!(msg.contains("no value was supplied"), "message was: {msg}");
}

#[test]
fn a_non_null_variable_supplied_as_null_is_refused() {
    let vars = [var("n", "Int", false)];
    let values = json!({"n": null});
    assert!(validate_variable_values(None, &vars, Some(&values)).is_err());
}

#[test]
fn a_nullable_variable_with_no_value_is_accepted() {
    // Load-bearing: this is what lets `limit: $limit` fall back to the query's
    // compiled default instead of forcing `LIMIT NULL`.
    let vars = [var("n", "Int", true)];
    assert!(validate_variable_values(None, &vars, None).is_ok());
}

#[test]
fn a_non_null_variable_with_a_default_and_no_value_is_accepted() {
    let mut v = var("n", "Int", false);
    v.default_value = Some("10".to_string());
    assert!(validate_variable_values(None, &[v], None).is_ok());
}

#[test]
fn a_list_declared_variable_is_not_adjudicated() {
    let mut v = var("ids", "Int", true);
    v.var_type.list = true;
    let values = json!({"ids": [1, 2, 3]});
    assert!(validate_variable_values(None, &[v], Some(&values)).is_ok());
}

#[test]
fn a_variable_of_a_project_scalar_is_not_adjudicated() {
    let vars = [var("payload", "SomeProjectScalar", false)];
    let values = json!({"payload": {"anything": true}});
    assert!(validate_variable_values(None, &vars, Some(&values)).is_ok());
}

#[test]
fn an_out_of_range_int_says_so_rather_than_int_where_int_was_expected() {
    let args = [literal("limit", "int", "99999999999999")];
    let msg = message(validate_argument_values("Query.things", &limit_arg(), &args, &[]));
    assert!(msg.contains("32-bit"), "message was: {msg}");
    assert!(!msg.contains("a Int value"), "message was: {msg}");
}
