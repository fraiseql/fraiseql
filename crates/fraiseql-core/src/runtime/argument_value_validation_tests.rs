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

// ── Enum membership — § 5.6.1 / § 6.1.2 for enums (#1362) ────────────────────

use crate::schema::{
    CompiledSchema, EnumDefinition, EnumValueDefinition, InputFieldDefinition,
    InputObjectDefinition,
};

/// `OrderStatus` with three members, `CreateOrderInput { reference, status }`, and a
/// `where`-shaped nesting so the walk into input objects is exercised by the same
/// fixture the defect was reported against.
fn enum_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.enums.push(
        EnumDefinition::new("OrderStatus")
            .with_value(EnumValueDefinition::new("PENDING"))
            .with_value(EnumValueDefinition::new("SHIPPED"))
            .with_value(EnumValueDefinition::new("CANCELLED")),
    );
    schema.input_types.push(
        InputObjectDefinition::new("CreateOrderInput")
            .with_field(InputFieldDefinition::new("reference", "String"))
            .with_field(InputFieldDefinition::new("status", "OrderStatus"))
            .with_field(InputFieldDefinition::new("history", "[OrderStatus!]"))
            .with_field(InputFieldDefinition::new("nested", "CreateOrderInput")),
    );
    schema
}

fn input_arg() -> Vec<ArgumentDefinition> {
    vec![ArgumentDefinition::optional(
        "input",
        // The compiler emits an input-type reference as `Object`, never `Input`.
        FieldType::Object("CreateOrderInput".to_string()),
    )]
}

fn status_arg() -> Vec<ArgumentDefinition> {
    vec![ArgumentDefinition::optional(
        "status",
        FieldType::Enum("OrderStatus".to_string()),
    )]
}

/// The whole finding in one case: the shape the issue reported, through the entry
/// every transport that writes uses.
#[test]
fn a_non_member_at_an_enum_input_field_is_refused() {
    let schema = enum_schema();
    let values = json!({"input": {"reference": "r-1", "status": "BANANA"}});
    let err = message(validate_enum_argument_values(
        &schema,
        "Mutation.createOrder",
        &input_arg(),
        Some(&values),
    ));
    assert!(err.contains("OrderStatus"), "the refusal must name the enum: {err}");
    assert!(err.contains("input.status"), "the refusal must name the path: {err}");
    assert!(
        err.contains("PENDING, SHIPPED, CANCELLED"),
        "the refusal must name the members introspection already publishes: {err}"
    );
    assert!(
        !err.contains("BANANA"),
        "the offending value is described, never quoted back (#1197): {err}"
    );
}

#[test]
fn a_member_at_an_enum_input_field_is_accepted() {
    let schema = enum_schema();
    let values = json!({"input": {"reference": "r-1", "status": "SHIPPED"}});
    assert!(
        validate_enum_argument_values(&schema, "M.m", &input_arg(), Some(&values)).is_ok(),
        "a declared member must pass"
    );
}

/// Every row of the issue's table, in one place. The value spelling matters: a
/// member's *value* rather than its name, and a JSON number, were both forwarded.
#[test]
fn the_reported_non_member_spellings_are_each_refused() {
    let schema = enum_schema();
    for wrote in [
        json!("BANANA"),
        json!("pending"),
        json!("anything at all"),
        json!(42),
    ] {
        let values = json!({"input": {"status": wrote}});
        assert!(
            validate_enum_argument_values(&schema, "M.m", &input_arg(), Some(&values)).is_err(),
            "{wrote} is not a member of OrderStatus and must be refused"
        );
    }
}

#[test]
fn a_non_member_inside_a_list_of_enums_is_refused() {
    let schema = enum_schema();
    let values = json!({"input": {"history": ["PENDING", "BANANA"]}});
    let err = message(validate_enum_argument_values(&schema, "M.m", &input_arg(), Some(&values)));
    assert!(err.contains("input.history[1]"), "the refusal must name the element: {err}");
}

/// § 3.11 — a bare value stands for a one-element list, so a list declaration must
/// not become a hole an unchecked enum fits through.
#[test]
fn a_bare_non_member_written_at_a_list_of_enums_is_refused() {
    let schema = enum_schema();
    let values = json!({"input": {"history": "BANANA"}});
    assert!(validate_enum_argument_values(&schema, "M.m", &input_arg(), Some(&values)).is_err());
}

#[test]
fn a_non_member_nested_one_input_object_deeper_is_refused() {
    let schema = enum_schema();
    let values = json!({"input": {"nested": {"status": "BANANA"}}});
    let err = message(validate_enum_argument_values(&schema, "M.m", &input_arg(), Some(&values)));
    assert!(err.contains("input.nested.status"), "the refusal must name the path: {err}");
}

#[test]
fn a_non_member_at_a_bare_enum_argument_is_refused() {
    let schema = enum_schema();
    let values = json!({"status": "BANANA"});
    assert!(validate_enum_argument_values(&schema, "Q.q", &status_arg(), Some(&values)).is_err());
}

#[test]
fn an_absent_or_null_enum_field_is_left_to_the_required_field_rule() {
    let schema = enum_schema();
    for values in [
        json!({"input": {"reference": "r"}}),
        json!({"input": {"status": null}}),
    ] {
        assert!(
            validate_enum_argument_values(&schema, "M.m", &input_arg(), Some(&values)).is_ok(),
            "nullability is #414's question, not this one"
        );
    }
}

/// The leniency policy still holds for everything the schema does not enumerate.
#[test]
fn a_field_whose_type_the_schema_does_not_declare_is_not_adjudicated() {
    let mut schema = enum_schema();
    schema.input_types.push(
        InputObjectDefinition::new("Loose")
            .with_field(InputFieldDefinition::new("whatever", "SomeProjectScalar")),
    );
    let declared = vec![ArgumentDefinition::optional(
        "input",
        FieldType::Object("Loose".to_string()),
    )];
    let values = json!({"input": {"whatever": "anything at all"}});
    assert!(validate_enum_argument_values(&schema, "M.m", &declared, Some(&values)).is_ok());
}

// ── the literal path ─────────────────────────────────────────────────────────

#[test]
fn a_non_member_written_as_an_inline_enum_literal_is_refused() {
    let schema = enum_schema();
    // `value_json` encodes a bare GraphQL enum name as a JSON string — the same
    // shape a variable supplies, which is what lets one walk cover both.
    let args = [literal("status", "enum", "\"BANANA\"")];
    assert!(
        validate_enum_argument_literals(&schema, "Query.orders", &status_arg(), &args).is_err(),
        "an inline literal must be adjudicated, not only a variable"
    );
}

#[test]
fn a_member_written_as_an_inline_enum_literal_is_accepted() {
    let schema = enum_schema();
    let args = [literal("status", "enum", "\"SHIPPED\"")];
    assert!(validate_enum_argument_literals(&schema, "Query.orders", &status_arg(), &args).is_ok());
}

/// A literal that is a *reference* is the variable check's half. Adjudicating the
/// marker object here would refuse every `status: $s`.
#[test]
fn a_variable_reference_is_left_to_the_variable_check() {
    let schema = enum_schema();
    let args = [var_use("status", "s")];
    assert!(validate_enum_argument_literals(&schema, "Query.orders", &status_arg(), &args).is_ok());
}

// ── the variable path (§ 6.1.2) ──────────────────────────────────────────────

#[test]
fn a_non_member_supplied_for_an_enum_variable_is_refused() {
    let schema = enum_schema();
    let defs = [var("s", "OrderStatus", true)];
    let values = json!({"s": "BANANA"});
    let err = message(validate_enum_variable_values(&schema, Some("Op"), &defs, Some(&values)));
    assert!(err.contains("$s"), "the refusal must name the variable: {err}");
    assert!(err.contains("Op"), "the refusal must name the operation: {err}");
}

#[test]
fn a_non_member_supplied_inside_an_input_object_variable_is_refused() {
    let schema = enum_schema();
    let defs = [var("input", "CreateOrderInput", true)];
    let values = json!({"input": {"status": "BANANA"}});
    let err = message(validate_enum_variable_values(&schema, None, &defs, Some(&values)));
    assert!(err.contains("status"), "the refusal must name the field: {err}");
}

#[test]
fn a_member_supplied_for_an_enum_variable_is_accepted() {
    let schema = enum_schema();
    let defs = [var("s", "OrderStatus", true)];
    assert!(
        validate_enum_variable_values(&schema, None, &defs, Some(&json!({"s": "PENDING"}))).is_ok()
    );
}

/// A schema that declares no enums cannot refuse anything — the #939 principle:
/// reject what the schema positively contradicts, not an absence of evidence.
#[test]
fn a_schema_declaring_no_enums_adjudicates_nothing() {
    let schema = CompiledSchema::new();
    let defs = [var("s", "OrderStatus", true)];
    assert!(
        validate_enum_variable_values(&schema, None, &defs, Some(&json!({"s": "BANANA"}))).is_ok()
    );
}
