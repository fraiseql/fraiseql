#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1513: a localized field is stored as a locale map and returned as the request locale's
//! label, through the `[locale]` fallback chain.
//!
//! `allowed = [en-US, fr, fr-CA, fr-FR]`, `fallback = {fr-CA = fr-FR}`, `default = en-US`: a
//! request in `fr-CA` reads `fr-CA`, then `fr-FR`, then `fr`, then `en-US`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_locale_product` table.

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, with_request_locale},
    schema::{CompiledSchema, FieldDefinition, FieldType, LocaleConfig, LocaleSource},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const VIEW: &str = "tv_locale_product";

/// `(pk, name map)`: a French label with an English one, English only, bare French only,
/// nothing at all.
const ROWS: [(i64, &str); 4] = [
    (1, r#"{"fr-FR": "Pomme", "en-US": "Apple"}"#),
    (2, r#"{"en-US": "Pear"}"#),
    (3, r#"{"fr": "Cerise"}"#),
    (4, r"{}"),
];

fn schema() -> CompiledSchema {
    schema_with(true)
}

/// The schema, with `name` nullable or not.
fn schema_with(nullable: bool) -> CompiledSchema {
    let mut name = if nullable {
        FieldDefinition::nullable("name", FieldType::String)
    } else {
        FieldDefinition::new("name", FieldType::String)
    };
    name.localized = true;
    let product = TestTypeBuilder::new("Product", VIEW)
        .with_simple_field("id", FieldType::Id)
        .with_field(name)
        .build();
    let products = TestQueryBuilder::new("products", "Product")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    let mut schema = TestSchemaBuilder::new().with_type(product).with_query(products).build();
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            ["en-US", "fr", "fr-CA", "fr-FR", "de-DE"].map(String::from).to_vec(),
            BTreeMap::from([("fr-CA".to_string(), "fr-FR".to_string())]),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema
}

async fn executor() -> Option<Executor> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    let values: Vec<String> = ROWS
        .iter()
        .map(|(pk, name)| {
            format!("({pk}, jsonb_build_object('id', '{pk}', 'name', '{name}'::jsonb))")
        })
        .collect();
    for ddl in [
        format!("DROP TABLE IF EXISTS {VIEW}"),
        format!("CREATE TABLE {VIEW} (pk bigint, data jsonb)"),
        format!("INSERT INTO {VIEW} VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Executor::new(schema(), Arc::new(adapter)))
}

/// An executor over the same rows with `name` declared non-null.
async fn non_null_executor() -> Option<Executor> {
    executor().await?;
    let adapter = PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap();
    Some(Executor::new(schema_with(false), Arc::new(adapter)))
}

async fn names(executor: &Executor, locale: &str, query: &str) -> Vec<Value> {
    let response = with_request_locale(locale, executor.execute(query, None))
        .await
        .unwrap_or_else(|e| panic!("{locale}: {e}"));
    let mut rows: Vec<(String, Value)> = response["data"]["products"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|p| (p["id"].as_str().unwrap().to_string(), p["name"].clone()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, name)| name).collect()
}

/// Cycle 1: returned in the request locale, through the chain.
#[tokio::test]
async fn a_localized_field_is_the_request_locales_label() {
    let Some(executor) = executor().await else {
        return;
    };
    assert_eq!(
        names(&executor, "fr-CA", "{ products { id name } }").await,
        vec![json!("Pomme"), json!("Pear"), json!("Cerise"), Value::Null],
        "fr-CA → fr-FR → fr → en-US"
    );
    assert_eq!(
        names(&executor, "en-US", "{ products { id name } }").await,
        vec![json!("Apple"), json!("Pear"), Value::Null, Value::Null],
        "en-US has no fallback beyond itself"
    );
}

/// Cycle 1: a non-null localized field with no label in the chain is a GraphQL non-null error,
/// not a `null` in a non-null position. The engine completes no output field against its
/// declared nullability today, localized or not (#1522).
#[tokio::test]
#[ignore = "#1522: non-null output completion is not enforced on any field"]
async fn a_non_null_localized_field_with_no_label_is_an_error() {
    let Some(executor) = non_null_executor().await else {
        return;
    };
    let response =
        with_request_locale("en-US", executor.execute("{ products { id name } }", None)).await;
    let errored = match &response {
        Err(_) => true,
        Ok(body) => body.get("errors").is_some(),
    };
    assert!(errored, "row 3 and 4 have no en-US label: {response:?}");
}
