//! What a template's author is told of a path a captured row cannot fill.

use super::{CompiledSchema, SubscribableEntity};
use crate::schema::{
    CaptureTableConfig, FieldDefinition, FieldType, SubscriptionsConfig, TypeDefinition,
};

/// `Order { id, reviewStatus, tags: [String], meta: JSON, customer: Customer,
/// lines: [Line] }`, captured from `tables`, of which `documents` keep the document in
/// `data`.
fn schema(tables: &[&str], documents: &[&str]) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("reviewStatus", FieldType::String),
        FieldDefinition::new("tags", FieldType::list(FieldType::String)),
        FieldDefinition::new("meta", FieldType::Json),
        FieldDefinition::new("customer", FieldType::Object("Customer".to_string())),
        FieldDefinition::new("lines", FieldType::list(FieldType::Object("Line".to_string()))),
    ];
    let mut customer = TypeDefinition::new("Customer", "v_customer");
    customer.fields = vec![FieldDefinition::new("name", FieldType::String)];
    schema.types = vec![order, customer];
    if !tables.is_empty() {
        schema.subscribable.push(SubscribableEntity {
            entity_type: "Order".to_string(),
            tables:      tables.iter().map(ToString::to_string).collect(),
            pre_image:   false,
        });
    }
    let mut subscriptions = SubscriptionsConfig::default();
    for table in documents {
        subscriptions.capture.insert(
            (*table).to_string(),
            CaptureTableConfig {
                document_column: "data".to_string(),
            },
        );
    }
    schema.subscriptions_config = Some(subscriptions);
    schema.build_indexes();
    schema
}

fn warning(schema: &CompiledSchema, path: &str) -> Option<String> {
    schema.captured_row_warning(schema.find_type("Order").expect("the type"), path)
}

#[test]
fn a_path_into_an_embedded_field_of_a_row_captured_table_is_warned_of() {
    let schema = schema(&["tb_order"], &[]);

    let said = warning(&schema, "customer.name").expect("a captured row holds no `customer`");

    for part in [
        "`{{ customer.name }}`",
        "`customer`",
        "`Order`",
        "`tb_order`",
    ] {
        assert!(said.contains(part), "names {part}: {said}");
    }
    assert!(said.contains("[subscriptions.capture.tb_order]"), "names the entry: {said}");
}

#[test]
fn an_embedded_field_named_whole_is_warned_of_as_a_path_into_it_is() {
    let schema = schema(&["tb_order"], &[]);

    assert!(warning(&schema, "customer").is_some());
    assert!(warning(&schema, "lines").is_some(), "a list of objects is embedded too");
}

#[test]
fn a_field_a_column_can_carry_is_not_warned_of() {
    let schema = schema(&["tb_order"], &[]);

    for path in ["id", "review_status", "tags", "meta", "meta.key"] {
        assert_eq!(warning(&schema, path), None, "{path}");
    }
}

#[test]
fn a_type_no_table_is_captured_for_is_not_warned_of() {
    assert_eq!(warning(&schema(&[], &[]), "customer.name"), None);
}

#[test]
fn another_types_row_captured_table_is_not_this_types() {
    let mut schema = schema(&[], &[]);
    schema.subscribable.push(SubscribableEntity {
        entity_type: "Customer".to_string(),
        tables:      vec!["tb_customer".to_string()],
        pre_image:   false,
    });

    assert_eq!(warning(&schema, "customer.name"), None);
}

#[test]
fn a_table_that_keeps_its_document_is_not_warned_of() {
    assert_eq!(warning(&schema(&["tb_order"], &["tb_order"]), "customer.name"), None);
}

#[test]
fn one_row_captured_table_among_several_is_the_one_named() {
    let schema = schema(&["tb_order", "tb_order_archive"], &["tb_order"]);

    let said = warning(&schema, "customer.name").expect("the archive is captured as its row");

    assert!(said.contains("`tb_order_archive`"), "{said}");
    assert!(!said.contains("`tb_order`"), "the table that keeps its document is not: {said}");
}

#[test]
fn a_path_the_document_does_not_hold_either_is_left_to_the_refusal() {
    let schema = schema(&["tb_order"], &[]);

    assert_eq!(warning(&schema, "fk_customer"), None);
    assert_eq!(warning(&schema, "reviewStatus"), None);
}
