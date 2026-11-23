#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use super::*;
use crate::{
    graphql::parse_query,
    schema::{InputFieldDefinition, InputObjectDefinition},
};

/// The document's one root's arguments and its variable definitions.
fn usage(document: &str) -> (Vec<GraphQLArgument>, Vec<VariableDefinition>) {
    let parsed = parse_query(document).unwrap();
    (parsed.selections[0].arguments.clone(), parsed.variables)
}

fn check(declared: &[ArgumentDefinition], document: &str) -> Result<()> {
    check_in(&CompiledSchema::default(), declared, document)
}

fn check_in(
    schema: &CompiledSchema,
    declared: &[ArgumentDefinition],
    document: &str,
) -> Result<()> {
    let (arguments, variables) = usage(document);
    validate_variable_positions(schema, "Query.items", declared, &arguments, &variables)
}

fn refusal(result: Result<()>) -> String {
    match result {
        Err(FraiseQLError::Validation { message, .. }) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_parser_keeps_a_list_items_nullability() {
    let (_, variables) =
        usage("query Q($a: [ID!], $b: [ID], $c: [ID!]!, $d: [[Int!]]) { items { id } }");
    let declared: Vec<(String, bool, bool)> = variables
        .iter()
        .map(|v| (v.var_type.name.clone(), v.var_type.nullable, v.var_type.list_nullable))
        .collect();
    assert_eq!(
        declared,
        [
            ("[ID!]".to_string(), true, false),
            ("[ID]".to_string(), true, true),
            ("[ID!]".to_string(), false, false),
            ("[[Int!]]".to_string(), true, true),
        ]
    );
}

/// A position's own default stands in for a null, as a variable's non-null default does.
#[test]
fn a_position_default_admits_a_nullable_variable() {
    let mut with_default = ArgumentDefinition::new("n", FieldType::Int);
    with_default.default_value = Some(crate::schema::GraphQLValue::Int(3));
    let without_default = ArgumentDefinition::new("n", FieldType::Int);
    let document = "query Q($v: Int) { items(n: $v) { id } }";

    assert!(check(&[with_default], document).is_ok());
    assert_eq!(
        refusal(check(&[without_default], document)),
        "Variable `$v` is declared as `Int` but is used for argument `n` on Query.items, which \
         has type `Int!`"
    );
}

/// The position's default lifts the outer requirement only: a list's items have no default.
#[test]
fn a_position_default_does_not_reach_a_lists_items() {
    let mut ids = ArgumentDefinition::new("ids", FieldType::list(FieldType::Id));
    ids.default_value = Some(crate::schema::GraphQLValue::List(Vec::new()));

    assert_eq!(
        refusal(check(&[ids], "query Q($v: [ID]) { items(ids: $v) { id } }")),
        "Variable `$v` is declared as `[ID]` but is used for argument `ids` on Query.items, \
         which has type `[ID!]!`"
    );
}

/// An input field's default exempts a nullable variable written at that field.
#[test]
fn an_input_field_default_admits_a_nullable_variable() {
    let mut schema = CompiledSchema::default();
    let mut defaulted = InputFieldDefinition::new("size", "Int!").with_nullable(false);
    defaulted.default_value = Some("10".to_string());
    let required = InputFieldDefinition::new("page", "Int!").with_nullable(false);
    schema
        .input_types
        .push(InputObjectDefinition::new("Page").with_fields(vec![defaulted, required]));
    let declared = [ArgumentDefinition::optional(
        "page",
        FieldType::Input("Page".to_string()),
    )];

    assert!(
        check_in(
            &schema,
            &declared,
            "query Q($v: Int) { items(page: {size: $v, page: 1}) { id } }"
        )
        .is_ok()
    );
    assert_eq!(
        refusal(check_in(
            &schema,
            &declared,
            "query Q($v: Int) { items(page: {size: 1, page: $v}) { id } }"
        )),
        "Variable `$v` is declared as `Int` but is used at `page.page` on Query.items, which has \
         type `Int!`"
    );
}

/// Nested lists are compared item by item, at every depth.
#[test]
fn a_nested_lists_inner_items_are_compared() {
    let matrix = ArgumentDefinition::optional(
        "m",
        FieldType::list_of_nullable(FieldType::list(FieldType::Int)),
    );

    assert!(
        check(std::slice::from_ref(&matrix), "query Q($v: [[Int!]]) { items(m: $v) { id } }")
            .is_ok()
    );
    assert_eq!(
        refusal(check(&[matrix], "query Q($v: [[Int]]) { items(m: $v) { id } }")),
        "Variable `$v` is declared as `[[Int]]` but is used for argument `m` on Query.items, \
         which has type `[[Int!]]`"
    );
}

/// The `__type` meta-field's `name` is `String!`.
#[test]
fn the_type_meta_fields_name_is_non_null() {
    assert_eq!(published_argument_type(&type_name_argument()), "String!");
}
