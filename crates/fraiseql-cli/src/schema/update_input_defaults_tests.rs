//! Which defaulted input fields an update receives.

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldType, InputFieldDefinition, InputObjectDefinition,
    MutationDefinition, MutationOperation,
};

use super::{UpdateInputDefault, update_input_defaults};

fn mutation(name: &str, operation: MutationOperation, input: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "Order");
    m.operation = operation;
    m.arguments = vec![ArgumentDefinition::new(
        "input",
        FieldType::Object(input.to_string()),
    )];
    m
}

fn update(name: &str, input: &str) -> MutationDefinition {
    mutation(
        name,
        MutationOperation::Update {
            table: "tb_order".to_string(),
        },
        input,
    )
}

fn insert(name: &str, input: &str) -> MutationDefinition {
    mutation(
        name,
        MutationOperation::Insert {
            table: "tb_order".to_string(),
        },
        input,
    )
}

/// `OrderInput { id: ID!, currency: String = "EUR", lines: [LineInput!] }`,
/// `LineInput { qty: Int! = 1 }` and `PatchInput { note: String }`.
fn schema(mutations: Vec<MutationDefinition>) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut line = InputObjectDefinition::new("LineInput");
    line.fields = vec![
        InputFieldDefinition::new("qty", "Int!")
            .with_nullable(false)
            .with_default_value("1"),
    ];
    let mut order = InputObjectDefinition::new("OrderInput");
    order.fields = vec![
        InputFieldDefinition::new("id", "ID!").with_nullable(false),
        InputFieldDefinition::new("currency", "String").with_default_value("\"EUR\""),
        InputFieldDefinition::new("lines", "[LineInput!]"),
    ];
    let mut patch = InputObjectDefinition::new("PatchInput");
    patch.fields = vec![InputFieldDefinition::new("note", "String")];
    schema.input_types = vec![line, order, patch];
    schema.mutations = mutations;
    schema.build_indexes();
    schema
}

fn found(mutation: &str, input_type: &str, field: &str, default: &str) -> UpdateInputDefault {
    UpdateInputDefault {
        mutation:   mutation.to_string(),
        input_type: input_type.to_string(),
        field:      field.to_string(),
        default:    default.to_string(),
    }
}

#[test]
fn an_updates_input_and_the_inputs_nested_in_it_are_reported() {
    assert_eq!(
        update_input_defaults(&schema(vec![update("updateOrder", "OrderInput")])),
        vec![
            found("updateOrder", "OrderInput", "currency", "\"EUR\""),
            found("updateOrder", "LineInput", "qty", "1"),
        ]
    );
}

#[test]
fn a_type_shared_with_a_create_is_reported_under_the_update_only() {
    let reported = update_input_defaults(&schema(vec![
        insert("createOrder", "OrderInput"),
        update("updateOrder", "OrderInput"),
    ]));
    assert!(reported.iter().all(|f| f.mutation == "updateOrder"), "{reported:?}");
    assert_eq!(reported.len(), 2);
}

#[test]
fn a_create_with_defaults_and_an_update_without_are_accepted() {
    assert_eq!(
        update_input_defaults(&schema(vec![
            insert("createOrder", "OrderInput"),
            update("updateOrder", "PatchInput"),
        ])),
        vec![]
    );
}

#[test]
fn an_input_reached_through_a_list_argument_is_reported() {
    let mut m = update("updateLines", "LineInput");
    m.arguments = vec![ArgumentDefinition::new(
        "lines",
        FieldType::List {
            item:           Box::new(FieldType::Object("LineInput".to_string())),
            nullable_items: false,
        },
    )];
    assert_eq!(
        update_input_defaults(&schema(vec![m])),
        vec![found("updateLines", "LineInput", "qty", "1")]
    );
}

#[test]
fn a_self_referencing_input_is_walked_once() {
    let mut schema = schema(vec![update("updateOrder", "PatchInput")]);
    schema.input_types[2]
        .fields
        .push(InputFieldDefinition::new("parent", "PatchInput"));
    schema.input_types[2]
        .fields
        .push(InputFieldDefinition::new("kind", "String").with_default_value("\"a\""));
    schema.build_indexes();
    assert_eq!(
        update_input_defaults(&schema),
        vec![found("updateOrder", "PatchInput", "kind", "\"a\"")]
    );
}
