//! What a template's author is told of a path a captured row cannot fill.

use super::{CompiledSchema, ConditionRead, SubscribableEntity};
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

// What an observer's condition is told of each key it reads.

fn condition(
    schema: &CompiledSchema,
    key: &str,
    read: ConditionRead,
) -> Result<Option<String>, String> {
    schema.condition_key(schema.find_type("Order").expect("the type"), key, read)
}

const READS: [ConditionRead; 3] = [
    ConditionRead::Value,
    ConditionRead::Presence,
    ConditionRead::Change,
];

#[test]
fn a_stored_key_of_a_scalar_field_is_read_in_silence() {
    for tables in [&[][..], &["tb_order"][..]] {
        let schema = schema(tables, &[]);
        for key in ["id", "review_status", "tags", "meta"] {
            for read in READS {
                assert_eq!(condition(&schema, key, read), Ok(None), "{key} {read:?} {tables:?}");
            }
        }
    }
}

/// Only the document reaches an observer of a type with no row-captured table, and it
/// holds the type's fields: a key no field holds is absent from every event.
#[test]
fn an_unknown_key_of_a_document_only_type_is_refused() {
    let refused =
        condition(&schema(&[], &[]), "totl", ConditionRead::Value).expect_err("no event holds it");
    for part in ["`totl`", "`Order`", "every event", "fails to evaluate"] {
        assert!(refused.contains(part), "names {part}: {refused}");
    }
}

/// The schema holds no table's columns: on a type with a row-captured table, a key no
/// field holds may be one, so it is warned of where the same key is refused without one.
#[test]
fn the_same_unknown_key_is_refused_without_a_row_captured_table_and_warned_of_with_one() {
    assert!(condition(&schema(&[], &[]), "fk_customer", ConditionRead::Value).is_err());

    let said = condition(&schema(&["tb_order"], &[]), "fk_customer", ConditionRead::Value)
        .expect("a column may carry it")
        .expect("and the author is told");
    for part in ["`fk_customer`", "`Order`", "`tb_order`", "mutation's event"] {
        assert!(said.contains(part), "names {part}: {said}");
    }

    let kept = schema(&["tb_order"], &["tb_order"]);
    assert!(
        condition(&kept, "fk_customer", ConditionRead::Value).is_err(),
        "a table that keeps its document is captured as the document"
    );
}

#[test]
fn a_published_name_is_refused_naming_the_stored_key() {
    let refused = condition(&schema(&[], &[]), "reviewStatus", ConditionRead::Presence)
        .expect_err("the document holds the stored key");
    assert!(refused.contains("`review_status`"), "{refused}");
}

#[test]
fn a_dotted_key_is_refused_whatever_the_tables() {
    for tables in [&[][..], &["tb_order"][..]] {
        for key in ["customer.name", "meta.key"] {
            let refused = condition(&schema(tables, &[]), key, ConditionRead::Presence)
                .expect_err("no event holds a dotted key");
            assert!(refused.contains(&format!("`{key}`")), "{refused}");
        }
    }
}

#[test]
fn an_embedded_field_of_a_row_captured_type_is_warned_of_per_read() {
    let schema = schema(&["tb_order"], &[]);
    let said = |read| {
        condition(&schema, "customer", read)
            .expect("a mutation's event holds it")
            .expect("a captured row lacks it")
    };

    assert!(
        said(ConditionRead::Value).contains("fails to evaluate"),
        "{}",
        said(ConditionRead::Value)
    );
    assert!(said(ConditionRead::Presence).contains("`has_field` is false"));
    assert!(said(ConditionRead::Change).contains("no change"));
    for part in [
        "`customer`",
        "`Order`",
        "`tb_order`",
        "[subscriptions.capture.tb_order]",
    ] {
        assert!(said(ConditionRead::Value).contains(part), "names {part}");
    }
    assert!(
        condition(&schema, "lines", ConditionRead::Value).expect("accepted").is_some(),
        "a list of objects"
    );
}

#[test]
fn an_embedded_field_is_read_in_silence_where_only_the_document_is_captured() {
    for (tables, documents) in [(&[][..], &[][..]), (&["tb_order"][..], &["tb_order"][..])] {
        assert_eq!(
            condition(&schema(tables, documents), "customer", ConditionRead::Value),
            Ok(None)
        );
    }
}
