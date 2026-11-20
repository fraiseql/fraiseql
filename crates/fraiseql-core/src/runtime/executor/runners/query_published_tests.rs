//! A REST read holds every stored value to the OpenAPI type it publishes the field as, on
//! each of its entry points (a page, a bounded read for a bulk write, a composed read) and
//! at every embedded level of a composed one: a value of another kind is served in that
//! type when nothing is lost, and the read is refused otherwise, naming the field, its
//! published type and the stored kind, never the value.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
    backend::JsonbValue,
    error::{FraiseQLError, Result},
    runtime::{
        EmbedSelection, Executor, QueryMatch, RuntimeConfig,
        executor::test_support::CapturingMockAdapter,
    },
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldType, QueryDefinition, Relationship,
        TypeDefinition,
    },
    security::SecurityContext,
};

fn list_query(name: &str, return_type: &str, view: &str) -> QueryDefinition {
    let mut query = QueryDefinition::new(name, return_type);
    query.returns_list = true;
    query.sql_source = Some(view.to_string());
    query
}

fn relationship(name: &str, target: &str, cardinality: Cardinality, fk: &str) -> Relationship {
    Relationship {
        name: name.to_string(),
        target_type: target.to_string(),
        cardinality,
        foreign_key: fk.to_string(),
        referenced_key: "id".to_string(),
        native_join: None,
    }
}

/// `Reading` has many `Part`s (`parts`), each made by one `Maker` (`maker`).
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("count", FieldType::Int),
            FieldDefinition::nullable("on", FieldType::Boolean),
            FieldDefinition::nullable("price", FieldType::Decimal),
        ],
        relationships: vec![relationship(
            "parts",
            "Part",
            Cardinality::OneToMany,
            "fk_reading",
        )],
        ..TypeDefinition::new("Reading", "v_reading")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("fk_reading", FieldType::Id),
            FieldDefinition::new("fk_maker", FieldType::Id),
            FieldDefinition::new("qty", FieldType::Int),
        ],
        relationships: vec![relationship(
            "maker",
            "Maker",
            Cardinality::ManyToOne,
            "fk_maker",
        )],
        ..TypeDefinition::new("Part", "v_part")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("rank", FieldType::Int),
        ],
        ..TypeDefinition::new("Maker", "v_maker")
    });
    schema.queries.push(list_query("readings", "Reading", "v_reading"));
    schema.queries.push(list_query("parts", "Part", "v_part"));
    schema.queries.push(list_query("makers", "Maker", "v_maker"));
    schema.build_indexes();
    schema
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          crate::types::UserId::new("u-1"),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-published".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn readings(schema: &CompiledSchema, fields: &[&str]) -> QueryMatch {
    let query = schema.queries.iter().find(|q| q.name == "readings").unwrap().clone();
    QueryMatch::from_operation(
        query,
        fields.iter().map(ToString::to_string).collect(),
        std::collections::HashMap::new(),
        schema.find_type("Reading"),
    )
    .unwrap()
}

/// The three ways REST reads rows.
#[derive(Debug, Clone, Copy)]
enum Entry {
    Page,
    Bounded,
    Composed,
}

const ENTRIES: [Entry; 3] = [Entry::Page, Entry::Bounded, Entry::Composed];

/// A read of `fields` of every `Reading` through `entry`, the adapter answering `row`.
async fn read(entry: Entry, fields: &[&str], row: Value) -> Result<Value> {
    let schema = schema();
    let qm = readings(&schema, fields);
    let row = match entry {
        Entry::Composed => json!({"d": row, "e": {}}),
        Entry::Page | Entry::Bounded => row,
    };
    let adapter = Arc::new(CapturingMockAdapter::new(vec![JsonbValue::new(row)]));
    let executor = Executor::read_only_with_config(schema, adapter, RuntimeConfig::default());
    let ctx = principal();
    match entry {
        Entry::Page => executor.execute_query_direct(&qm, None, Some(&ctx), None).await,
        Entry::Bounded => executor.execute_query_direct_bounded(&qm, Some(&ctx), 100).await,
        Entry::Composed => {
            executor.execute_query_composed(&qm, &[], &[], None, Some(&ctx), None).await
        },
    }
}

/// A composed read of every `Reading`'s `id` with `embeds`, the adapter answering `row`.
async fn composed(embeds: &[EmbedSelection], row: Value) -> Result<Value> {
    let schema = schema();
    let qm = readings(&schema, &["id"]);
    let adapter = Arc::new(CapturingMockAdapter::new(vec![JsonbValue::new(row)]));
    let executor = Executor::read_only_with_config(schema, adapter, RuntimeConfig::default());
    executor
        .execute_query_composed(&qm, embeds, &[], None, Some(&principal()), None)
        .await
}

fn parts(fields: &[&str], embeds: Vec<EmbedSelection>) -> EmbedSelection {
    EmbedSelection {
        relationship: "parts".to_string(),
        output_key: "parts".to_string(),
        fields: fields.iter().map(ToString::to_string).collect(),
        limit: Some(100),
        embeds,
        ..EmbedSelection::default()
    }
}

fn maker(fields: &[&str]) -> EmbedSelection {
    EmbedSelection {
        relationship: "maker".to_string(),
        output_key: "maker".to_string(),
        fields: fields.iter().map(ToString::to_string).collect(),
        ..EmbedSelection::default()
    }
}

/// The message a refused read carries.
fn refusal(result: Result<Value>) -> String {
    match result {
        Err(FraiseQLError::Internal { message, .. }) => message,
        other => panic!("a refused read, not {other:?}"),
    }
}

#[tokio::test]
async fn every_entry_serves_a_stored_value_of_another_kind_as_its_published_type() {
    for entry in ENTRIES {
        let row = json!({"id": "r1", "count": "7", "on": "true", "price": 12});
        let data = read(entry, &["id", "count", "on", "price"], row).await.unwrap();
        assert_eq!(
            data["data"]["readings"],
            json!([{"id": "r1", "count": 7, "on": true, "price": "12"}]),
            "{entry:?}"
        );
    }
}

#[tokio::test]
async fn every_entry_refuses_a_value_its_published_type_cannot_hold_without_quoting_it() {
    for entry in ENTRIES {
        let row = json!({"id": "r1", "count": {"s3cr3t": "v4lue"}});
        let message = refusal(read(entry, &["id", "count"], row).await);
        assert_eq!(
            message,
            r#"`Reading.count` is published as an OpenAPI `integer`, and the stored value is an object that is not one. At ["readings",0,"count"]."#,
            "{entry:?}"
        );
    }
}

/// Every element of a to-many embed, and a to-one embed two levels down, is held to its
/// own type's published fields.
#[tokio::test]
async fn an_embedded_level_serves_its_values_as_their_published_types() {
    let row = json!({
        "d": {"id": "r1"},
        "e": {"parts": [
            {"d": {"id": 1, "qty": "3"}, "e": {"maker": {"d": {"id": "m1", "rank": "5"}, "e": {}}}},
            {"d": {"id": "2", "qty": 4}, "e": {"maker": null}},
        ]},
    });
    let data = composed(&[parts(&["id", "qty"], vec![maker(&["id", "rank"])])], row)
        .await
        .unwrap();
    assert_eq!(
        data["data"]["readings"],
        json!([{"id": "r1", "parts": [
            {"id": 1, "qty": 3, "maker": {"id": "m1", "rank": 5}},
            {"id": 2, "qty": 4, "maker": null},
        ]}])
    );
}

#[tokio::test]
async fn a_value_an_embedded_level_cannot_serve_refuses_the_read() {
    let row = json!({
        "d": {"id": "r1"},
        "e": {"parts": [
            {"d": {"id": 1, "qty": 3}, "e": {}},
            {"d": {"id": 2, "qty": [3]}, "e": {}},
        ]},
    });
    let message = refusal(composed(&[parts(&["id", "qty"], vec![])], row).await);
    assert_eq!(
        message,
        r#"`Part.qty` is published as an OpenAPI `integer`, and the stored value is a list that is not one. At ["readings",0,"parts",1,"qty"]."#
    );
}

#[tokio::test]
async fn a_value_two_embedded_levels_down_is_held_to_its_type() {
    let row = json!({
        "d": {"id": "r1"},
        "e": {"parts": [
            {"d": {"id": 1, "qty": 3}, "e": {"maker": {"d": {"id": "m1", "rank": true}, "e": {}}}},
        ]},
    });
    let embeds = [parts(&["id", "qty"], vec![maker(&["id", "rank"])])];
    let message = refusal(composed(&embeds, row).await);
    assert_eq!(
        message,
        r#"`Maker.rank` is published as an OpenAPI `integer`, and the stored value is a Boolean that is not one. At ["readings",0,"parts",0,"maker","rank"]."#
    );
}

/// An embedded row missing a non-null field has no partial form either: the read is
/// refused, as a root row's is.
#[tokio::test]
async fn an_embedded_row_missing_a_non_null_field_refuses_the_read() {
    let row = json!({
        "d": {"id": "r1"},
        "e": {"parts": [{"d": {"id": 1}, "e": {}}]},
    });
    let message = refusal(composed(&[parts(&["id", "qty"], vec![])], row).await);
    assert!(
        message.ends_with(r#"At ["readings",0,"parts",0,"qty"]: the stored value is missing."#),
        "{message}"
    );
}
