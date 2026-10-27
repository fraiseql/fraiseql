#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::{Value, json};

use super::DecimalIndex;
use crate::{
    db::traits::{DecimalShape, quote_decimals},
    schema::{
        CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition, TypeDefinition,
        UnionDefinition,
    },
};

fn field(name: &str, field_type: FieldType) -> FieldDefinition {
    FieldDefinition::new(name, field_type)
}

fn list(item: FieldType) -> FieldType {
    FieldType::List {
        item:           Box::new(item),
        nullable_items: false,
    }
}

fn object(name: &str) -> FieldType {
    FieldType::Object(name.to_string())
}

fn schema(types: Vec<TypeDefinition>) -> CompiledSchema {
    CompiledSchema {
        types,
        ..CompiledSchema::default()
    }
}

fn type_def(name: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
    fields
        .into_iter()
        .fold(TypeDefinition::new(name, "v_x"), TypeDefinition::with_field)
}

fn implementing(mut t: TypeDefinition, interface: &str) -> TypeDefinition {
    t.implements.push(interface.to_string());
    t
}

fn read(shape: &DecimalShape, src: &str) -> Value {
    let mut out = Vec::new();
    quote_decimals(src.as_bytes(), shape, &mut out);
    serde_json::from_slice(&out).unwrap()
}

fn shape_of(schema: &CompiledSchema, type_name: &str) -> Option<DecimalShape> {
    DecimalIndex::new(schema).shape(schema, type_name)
}

#[test]
fn a_decimal_field_is_found_under_its_name_its_stored_key_and_its_alias() {
    let mut aliased = field("listPrice", FieldType::Decimal);
    aliased.alias = Some("price".to_string());
    let schema = schema(vec![type_def(
        "Product",
        vec![
            field("unitPrice", FieldType::Decimal),
            aliased,
            field("rating", FieldType::Float),
        ],
    )]);
    let shape = shape_of(&schema, "Product").unwrap();
    assert_eq!(
        read(
            &shape,
            r#"{"unitPrice": 1.50, "unit_price": 2.50, "price": 3.50, "list_price": 4.50, "rating": 4.50}"#
        ),
        json!({"unitPrice": "1.50", "unit_price": "2.50", "price": "3.50", "list_price": "4.50", "rating": 4.5})
    );
}

#[test]
fn a_type_that_reaches_no_decimal_has_no_shape() {
    let schema = schema(vec![
        type_def(
            "User",
            vec![
                field("score", FieldType::Float),
                field("friends", list(object("User"))),
            ],
        ),
        type_def("Product", vec![field("price", FieldType::Decimal)]),
    ]);
    assert!(shape_of(&schema, "User").is_none(), "a recursive type with no Decimal");
    assert!(shape_of(&schema, "Missing").is_none());
}

#[test]
fn a_decimal_below_nested_objects_and_lists_is_reached() {
    let schema = schema(vec![
        type_def(
            "Customer",
            vec![
                field("name", FieldType::String),
                field("lastOrder", object("Order")),
            ],
        ),
        type_def("Order", vec![field("lines", list(object("Line")))]),
        type_def(
            "Line",
            vec![
                field("amount", FieldType::Decimal),
                field("rates", list(FieldType::Decimal)),
            ],
        ),
    ]);
    let shape = shape_of(&schema, "Customer").unwrap();
    assert_eq!(
        read(
            &shape,
            r#"{"last_order": {"lines": [{"amount": 1.50, "rates": [0.10, 0.20]}]}}"#
        ),
        json!({"last_order": {"lines": [{"amount": "1.50", "rates": ["0.10", "0.20"]}]}})
    );
}

#[test]
fn a_recursive_type_holding_a_decimal_is_read_at_every_depth() {
    let schema = schema(vec![type_def(
        "User",
        vec![
            field("balance", FieldType::Decimal),
            field("friends", list(object("User"))),
        ],
    )]);
    let shape = shape_of(&schema, "User").unwrap();
    assert_eq!(
        read(
            &shape,
            r#"{"balance": 1.10, "friends": [{"balance": 2.20, "friends": [{"balance": 3.30}]}]}"#
        ),
        json!({"balance": "1.10", "friends": [{"balance": "2.20", "friends": [{"balance": "3.30"}]}]})
    );
}

/// `Invoice.amount` is a `Decimal`; `Rating.amount` is a `Float`.
fn union_schema() -> CompiledSchema {
    let mut schema = schema(vec![
        implementing(type_def("Invoice", vec![field("amount", FieldType::Decimal)]), "Entry"),
        implementing(type_def("Rating", vec![field("amount", FieldType::Float)]), "Entry"),
        type_def(
            "Feed",
            vec![
                field("items", list(FieldType::Union("Item".to_string()))),
                field("entries", list(FieldType::Interface("Entry".to_string()))),
            ],
        ),
    ]);
    schema.unions.push(
        UnionDefinition::new("Item")
            .with_members(vec!["Invoice".to_string(), "Rating".to_string()]),
    );
    schema.interfaces.push(InterfaceDefinition::new("Entry"));
    schema
}

#[test]
fn a_union_or_interface_member_is_read_as_the_type_it_names() {
    let schema = union_schema();
    let doc =
        r#"[{"__typename": "Invoice", "amount": 1.50}, {"__typename": "Rating", "amount": 1.50}]"#;
    let quoted = json!([{"__typename": "Invoice", "amount": "1.50"}, {"__typename": "Rating", "amount": 1.5}]);
    assert_eq!(read(&shape_of(&schema, "Item").unwrap(), doc), quoted, "a union");
    assert_eq!(read(&shape_of(&schema, "Entry").unwrap(), doc), quoted, "an interface");
    let feed = shape_of(&schema, "Feed").unwrap();
    assert_eq!(
        read(&feed, &format!(r#"{{"items": {doc}, "entries": {doc}}}"#)),
        json!({"items": quoted, "entries": quoted}),
        "a union and an interface below an object"
    );
}

#[test]
fn an_abstract_document_naming_no_type_is_read_as_the_fallback() {
    let schema = union_schema();
    let index = DecimalIndex::new(&schema);
    let doc = r#"{"amount": 1.50}"#;
    let with = index.shape_with_fallback(&schema, "Item", Some("Invoice")).unwrap();
    assert_eq!(read(&with, doc), json!({"amount": "1.50"}));
    assert_eq!(read(&index.shape(&schema, "Item").unwrap(), doc), json!({"amount": 1.5}));
}

#[test]
fn a_union_of_types_that_reach_no_decimal_has_no_shape() {
    let mut schema = schema(vec![type_def("Rating", vec![field("amount", FieldType::Float)])]);
    schema
        .unions
        .push(UnionDefinition::new("Only").with_members(vec!["Rating".to_string()]));
    assert!(shape_of(&schema, "Only").is_none());
}
