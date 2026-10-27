//! `RuntimeConfig::suggestions` withholds every "Did you mean" hint a request's spelling
//! can trigger (#1717).
//!
//! Each test sends one misspelt request to two executors over the same schema, one with
//! hints and one without, and reads both refusals. The one with hints must carry the
//! hint, or the test proves nothing about the other; the one without must still refuse,
//! naming what was misspelt, and carry no hint. One test per site that builds a hint:
//! a switch honoured at one of them and not the next still maps the schema.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
    compiler::fact_table::{DimensionColumn, FactTableMetadata, MeasureColumn, SqlType},
    runtime::{
        Executor, RuntimeConfig,
        executor::test_support::{MockAdapter, mock_user_results, test_schema},
    },
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        SubscriptionDefinition, TypeDefinition,
    },
};

const HINT: &str = "Did you mean";

/// `users` over a typed `User`, a `createUser` mutation, a `tf_sale` fact table and a
/// `userCreated(status)` subscription: one declared name near each misspelling below.
fn schema() -> CompiledSchema {
    let mut schema = test_schema();
    let mut user = TypeDefinition::new("User", "v_user");
    user.fields.push(FieldDefinition::new("id", FieldType::Id));
    user.fields.push(FieldDefinition::new("name", FieldType::String));
    schema.types.push(user);
    schema.mutations.push(MutationDefinition {
        arguments: vec![ArgumentDefinition::new("name", FieldType::String)],
        sql_source: Some("fn_create_user".to_string()),
        ..MutationDefinition::new("createUser", "User")
    });
    schema.add_fact_table(
        "tf_sale".to_string(),
        FactTableMetadata {
            table_name:               "tf_sale".to_string(),
            type_name:                None,
            measures:                 vec![MeasureColumn {
                name:       "revenue".to_string(),
                sql_type:   SqlType::Decimal,
                nullable:   false,
                additivity: crate::compiler::fact_table::Additivity::Additive,
            }],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        },
    );
    schema.subscriptions.push(
        SubscriptionDefinition::new("userCreated", "User")
            .with_argument(ArgumentDefinition::new("status", FieldType::String)),
    );
    schema.build_indexes();
    schema
}

/// The same schema with `users` a relay connection.
fn relay_schema() -> CompiledSchema {
    let mut schema = schema();
    schema.queries[0].relay = true;
    schema.queries[0].relay_cursor_column = Some("id".to_string());
    schema.build_indexes();
    schema
}

fn executor(schema: CompiledSchema, suggestions: bool) -> Executor {
    let config = RuntimeConfig {
        suggestions,
        ..RuntimeConfig::default()
    };
    // Relay-capable, so a connection reaches its argument parsing.
    Executor::with_config_and_relay(schema, Arc::new(MockAdapter::new(mock_user_results())), config)
}

/// What a refusal reads as: an `Err`'s text, or an `Ok` response carrying `errors`.
fn text(outcome: crate::error::Result<impl std::fmt::Debug>) -> String {
    match outcome {
        Err(err) => err.to_string(),
        Ok(answer) => format!("{answer:?}"),
    }
}

/// Assert the hint follows the switch: present with it on, absent with it off, and the
/// refusal (naming `misspelt`) the same either way.
fn assert_hint_follows_switch(with: &str, without: &str, misspelt: &str) {
    assert!(with.contains(HINT), "the control carries no hint, so it proves nothing: {with}");
    assert!(with.contains(misspelt), "the control refused something else: {with}");
    assert!(without.contains(misspelt), "refused something else, or not at all: {without}");
    assert!(!without.contains(HINT), "a hint with suggestions off: {without}");
}

async fn refusals(schema: CompiledSchema, query: &str, variables: Option<&Value>) -> [String; 2] {
    let [with, without] = [true, false].map(|on| executor(schema.clone(), on));
    [
        text(with.execute(query, variables).await),
        text(without.execute(query, variables).await),
    ]
}

#[tokio::test]
async fn an_unknown_query() {
    let [with, without] = refusals(schema(), "{ userr { id } }", None).await;
    assert_hint_follows_switch(&with, &without, "userr");
}

#[tokio::test]
async fn an_unknown_argument() {
    let [with, without] = refusals(schema(), "{ users(limitt: 1) { id } }", None).await;
    assert_hint_follows_switch(&with, &without, "limitt");
}

#[tokio::test]
async fn an_undefined_variable() {
    let query = "query Q($limit: Int) { users(limit: $limt) { id } }";
    let [with, without] = refusals(schema(), query, Some(&json!({"limit": 1}))).await;
    assert_hint_follows_switch(&with, &without, "limt");
}

#[tokio::test]
async fn an_unknown_variable_type() {
    let query = "query Q($limit: Intt) { users(limit: $limit) { id } }";
    let [with, without] = refusals(schema(), query, Some(&json!({"limit": 1}))).await;
    assert_hint_follows_switch(&with, &without, "Intt");
}

#[tokio::test]
async fn an_unknown_where_field() {
    let query = r#"{ users(where: {nmae: {eq: "x"}}) { id } }"#;
    let [with, without] = refusals(schema(), query, None).await;
    assert_hint_follows_switch(&with, &without, "nmae");
}

#[tokio::test]
async fn an_unknown_sort_field() {
    let [with, without] = refusals(schema(), "{ users(orderBy: {nmae: ASC}) { id } }", None).await;
    assert_hint_follows_switch(&with, &without, "nmae");
}

#[tokio::test]
async fn an_unknown_where_field_on_a_connection() {
    let query = r#"{ users(first: 2, where: {nmae: {eq: "x"}}) { edges { node { id } } } }"#;
    let [with, without] = refusals(relay_schema(), query, None).await;
    assert_hint_follows_switch(&with, &without, "nmae");
}

#[tokio::test]
async fn an_unknown_sort_field_on_a_connection() {
    let query = "{ users(first: 2, orderBy: {nmae: ASC}) { edges { node { id } } } }";
    let [with, without] = refusals(relay_schema(), query, None).await;
    assert_hint_follows_switch(&with, &without, "nmae");
}

#[tokio::test]
async fn an_unknown_mutation() {
    let query = r#"mutation { createUserr(name: "a") { id } }"#;
    let [with, without] = refusals(schema(), query, None).await;
    assert_hint_follows_switch(&with, &without, "createUserr");
}

#[tokio::test]
async fn an_unknown_mutation_argument() {
    let query = r#"mutation { createUser(nmae: "a") { id } }"#;
    let [with, without] = refusals(schema(), query, None).await;
    assert_hint_follows_switch(&with, &without, "nmae");
}

#[tokio::test]
async fn an_unknown_fact_table_for_an_aggregate() {
    let [with, without] = refusals(schema(), "{ sales_aggregate }", None).await;
    assert_hint_follows_switch(&with, &without, "tf_sales");
}

#[tokio::test]
async fn an_unknown_fact_table_for_a_window() {
    let [with, without] = refusals(schema(), "{ sales_window }", None).await;
    assert_hint_follows_switch(&with, &without, "tf_sales");
}

#[tokio::test]
async fn an_unknown_query_to_explain() {
    let [with, without] = [true, false].map(|on| executor(schema(), on));
    let with = text(with.explain("userr", None, None, None).await);
    let without = text(without.explain("userr", None, None, None).await);
    assert_hint_follows_switch(&with, &without, "userr");
}

#[test]
fn an_unknown_mutation_to_plan() {
    let [with, without] = [true, false].map(|on| executor(schema(), on));
    let query = r#"mutation { createUserr(name: "a") { id } }"#;
    let with = text(with.plan_query(query, None));
    let without = text(without.plan_query(query, None));
    assert_hint_follows_switch(&with, &without, "createUserr");
}

#[test]
fn an_unknown_subscription_argument_to_plan() {
    let document =
        crate::graphql::parse_query(r#"subscription { userCreated(statsu: "a") { id } }"#).unwrap();
    let [with, without] = [true, false].map(|on| executor(schema(), on));
    let with = text(with.plan_subscription(&document, None, None));
    let without = text(without.plan_subscription(&document, None, None));
    assert_hint_follows_switch(&with, &without, "statsu");
}

#[test]
fn an_unknown_subscription_argument_to_the_manager() {
    let document =
        crate::graphql::parse_query(r#"subscription { userCreated(statsu: "a") { id } }"#).unwrap();
    let manager = |on: bool| {
        crate::runtime::subscription::SubscriptionManager::new(Arc::new(schema()))
            .with_suggestions(on)
    };
    let with = text(manager(true).subscription_arguments(&document, None));
    let without = text(manager(false).subscription_arguments(&document, None));
    assert_hint_follows_switch(&with, &without, "statsu");
}
