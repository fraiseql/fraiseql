//! Whether a read's rows are parsed with its `Decimal` shape.
//!
//! A projected read prints each `Decimal` leaf it selects with `->>`, so the digits arrive
//! as a JSON string and quoting the row's numbers would only copy it. A read is parsed
//! without its shape only when every position its projection can hold a `Decimal` at is
//! such a leaf. Anything else keeps the shape: a list or a subtree taken whole, a field of
//! a union or an interface, a root that is one, and a read of the stored row.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use crate::{
    backend::types::JsonbValue,
    runtime::{
        RuntimeConfig,
        executor::{Executor, test_support::CapturingMockAdapter},
    },
    schema::{
        CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition, QueryDefinition,
        TypeDefinition, UnionDefinition,
    },
};

fn object(name: &str, view: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
    let mut type_def = TypeDefinition::new(name, view);
    type_def.fields = fields;
    type_def
}

fn list(item: FieldType) -> FieldType {
    FieldType::List {
        item:           Box::new(item),
        nullable_items: true,
    }
}

/// `ledgers` over `v_ledger`, whose `Ledger` reaches a `Decimal` at every position a
/// projection can meet one, and `parties`, a root read as a union.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.queries.push(
        QueryDefinition::new("ledgers", "Ledger")
            .returning_list()
            .with_sql_source("v_ledger"),
    );
    schema.queries.push(
        QueryDefinition::new("parties", "Party")
            .returning_list()
            .with_sql_source("v_party"),
    );
    schema.types.push(object(
        "Ledger",
        "v_ledger",
        vec![
            FieldDefinition::new("total", FieldType::Decimal),
            FieldDefinition::new("count", FieldType::Int),
            FieldDefinition::nullable("entry", FieldType::Object("Entry".to_string())),
            FieldDefinition::new("lines", list(FieldType::Object("Entry".to_string()))),
            FieldDefinition::new("rates", list(FieldType::Decimal)),
            FieldDefinition::nullable("tag", FieldType::Object("Tag".to_string())),
            FieldDefinition::nullable("party", FieldType::Union("Party".to_string())),
            FieldDefinition::nullable("holder", FieldType::Interface("Holder".to_string())),
        ],
    ));
    schema.types.push(object(
        "Entry",
        "v_entry",
        vec![
            FieldDefinition::new("amount", FieldType::Decimal),
            FieldDefinition::new("memo", FieldType::String),
            FieldDefinition::new("splits", list(FieldType::Decimal)),
        ],
    ));
    schema.types.push(object(
        "Tag",
        "v_tag",
        vec![FieldDefinition::new("name", FieldType::String)],
    ));
    schema.types.push(object(
        "Org",
        "v_party",
        vec![FieldDefinition::new("credit", FieldType::Decimal)],
    ));
    let mut bank =
        object("Bank", "v_bank", vec![FieldDefinition::new("balance", FieldType::Decimal)]);
    bank.implements = vec!["Holder".to_string()];
    schema.types.push(bank);
    let mut party = UnionDefinition::new("Party");
    party.member_types = vec!["Org".to_string()];
    schema.unions.push(party);
    schema.interfaces.push(
        InterfaceDefinition::new("Holder")
            .with_field(FieldDefinition::new("balance", FieldType::Decimal)),
    );
    schema
}

fn executor() -> (Executor, Arc<CapturingMockAdapter>) {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![JsonbValue::new(
        serde_json::json!({"total": "1.50", "count": 1}),
    )]));
    (
        Executor::with_config(schema(), adapter.clone(), RuntimeConfig::default()),
        adapter,
    )
}

/// Whether `document`'s rows were handed to the adapter with a `Decimal` shape.
async fn parsed_with_shape(document: &str) -> Option<bool> {
    let (exec, adapter) = executor();
    // Only what the read asked the adapter for is under test, not the shaped answer.
    let _ = exec.execute(document, None).await;
    adapter.captured_decimals()
}

#[tokio::test]
async fn a_read_whose_decimals_are_all_projected_leaves_is_parsed_without_its_shape() {
    for document in [
        "{ ledgers { total count } }",
        "{ ledgers { t: total } }",
        "{ ledgers { entry { amount memo } } }",
        "{ ledgers { count tag { name } } }",
        // Each element of a list of objects is built from its sub-fields.
        "{ ledgers { total lines { amount } } }",
        "{ ledgers { lines { memo } } }",
    ] {
        assert_eq!(parsed_with_shape(document).await, Some(false), "{document}");
    }
}

#[tokio::test]
async fn a_read_that_can_meet_a_decimal_as_a_number_keeps_its_shape() {
    for document in [
        // A list of scalars is projected as its stored array, so its numbers arrive as
        // written, in an element of a list of objects too.
        "{ ledgers { rates } }",
        "{ ledgers { lines { splits } } }",
        // An object built from its sub-fields is only as covered as they are.
        "{ ledgers { entry { amount splits } } }",
        // A member's fields are read without their type, as numbers.
        "{ ledgers { party { ... on Org { credit } } } }",
        "{ ledgers { holder { balance } } }",
        // A root of a union reads the stored row.
        "{ parties { ... on Org { credit } } }",
    ] {
        assert_eq!(parsed_with_shape(document).await, Some(true), "{document}");
    }
}

#[tokio::test]
async fn a_direct_read_keeps_its_shape() {
    let (exec, adapter) = executor();
    let matcher = crate::runtime::QueryMatcher::new(schema());
    let query_match = matcher.match_query("{ ledgers { total } }", None).unwrap();
    let _ = exec.execute_query_direct(&query_match, None, None, None).await;
    assert_eq!(
        adapter.captured_decimals(),
        Some(true),
        "a direct read takes the stored row whole"
    );
}
