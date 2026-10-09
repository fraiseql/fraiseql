#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1513 Phase 05: `where` and `orderBy` on a localized field read the label the client sees,
//! in the request locale, sorted under the locale's collation.
//!
//! `allowed = [en-US, fr-FR, sv-SE]`, `default = en-US`. The labels are chosen so the two
//! collations disagree: Swedish sorts `Ä` after `Z`, French (and the database default) with
//! `A`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_locale_filter_product` table.

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, with_request_locale},
    schema::{CompiledSchema, FieldDefinition, FieldType, LocaleConfig, LocaleSource},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::json;

const VIEW: &str = "tv_locale_filter_product";

/// `(pk, name map)`. In `fr-FR`: Pomme, Pear (English fallback), Zèbre, none. In `sv-SE`:
/// Äpple, Päron, Zebra, none.
const ROWS: [(i64, &str); 4] = [
    (1, r#"{"fr-FR": "Pomme", "en-US": "Apple", "sv-SE": "Äpple"}"#),
    (2, r#"{"en-US": "Pear", "sv-SE": "Päron"}"#),
    (3, r#"{"fr-FR": "Zèbre", "sv-SE": "Zebra", "en-US": "Zebra"}"#),
    (4, r"{}"),
];

fn schema() -> CompiledSchema {
    let mut name = FieldDefinition::nullable("name", FieldType::String);
    name.localized = true;
    let product = TestTypeBuilder::new("Product", VIEW)
        .with_simple_field("id", FieldType::Id)
        .with_field(name)
        .build();
    let mut products = TestQueryBuilder::new("products", "Product")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    products.auto_params.has_where = true;
    products.auto_params.has_order_by = true;
    let mut schema = TestSchemaBuilder::new().with_type(product).with_query(products).build();
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            ["en-US", "fr-FR", "sv-SE"].map(String::from).to_vec(),
            BTreeMap::new(),
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

/// The ids `products(<arguments>)` returns in `locale`, in response order.
async fn ids(executor: &Executor, locale: &str, arguments: &str) -> Vec<String> {
    let query = format!("{{ products({arguments}) {{ id }} }}");
    let response = with_request_locale(locale, executor.execute(&query, None))
        .await
        .unwrap_or_else(|e| panic!("{locale} {arguments}: {e}"));
    response["data"]["products"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

/// `ids`, sorted: for filters, where the order is not the question.
async fn matched(executor: &Executor, locale: &str, filter: &str) -> Vec<String> {
    let mut found = ids(executor, locale, &format!("where: {{ name: {filter} }}")).await;
    found.sort();
    found
}

fn rows(ids: &[u8]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

/// Cycle 1: each operator compares the label the request locale reads, fallback included.
#[tokio::test]
async fn a_filter_compares_the_request_locales_label() {
    let Some(executor) = executor().await else {
        return;
    };
    let e = &executor;
    assert_eq!(matched(e, "fr-FR", r#"{ eq: "Pomme" }"#).await, rows(&[1]), "eq");
    assert_eq!(matched(e, "en-US", r#"{ eq: "Pomme" }"#).await, rows(&[]), "eq, other locale");
    assert_eq!(matched(e, "fr-FR", r#"{ eq: "Pear" }"#).await, rows(&[2]), "eq, fallback");
    assert_eq!(matched(e, "sv-SE", r#"{ eq: "Äpple" }"#).await, rows(&[1]), "eq, sv-SE");
    assert_eq!(matched(e, "fr-FR", r#"{ neq: "Pomme" }"#).await, rows(&[2, 3]), "neq");
    assert_eq!(matched(e, "fr-FR", r#"{ in: ["Pomme", "Pear"] }"#).await, rows(&[1, 2]), "in");
    assert_eq!(matched(e, "fr-FR", r#"{ nin: ["Pomme"] }"#).await, rows(&[2, 3]), "nin");
    assert_eq!(matched(e, "fr-FR", r#"{ contains: "omm" }"#).await, rows(&[1]), "contains");
    assert_eq!(matched(e, "fr-FR", r#"{ icontains: "POMM" }"#).await, rows(&[1]), "icontains");
    assert_eq!(matched(e, "fr-FR", r#"{ startswith: "Zè" }"#).await, rows(&[3]), "startswith");
    assert_eq!(matched(e, "fr-FR", "{ isNull: true }").await, rows(&[4]), "isNull");
    assert_eq!(matched(e, "fr-FR", "{ isNull: false }").await, rows(&[1, 2, 3]), "not isNull");
}

/// A response row's `name`, as a sanity check that the read and the filter agree.
#[tokio::test]
async fn the_filtered_row_reads_the_label_it_matched() {
    let Some(executor) = executor().await else {
        return;
    };
    let response = with_request_locale(
        "fr-FR",
        executor.execute(r#"{ products(where: { name: { eq: "Pear" } }) { id name } }"#, None),
    )
    .await
    .unwrap();
    assert_eq!(response["data"]["products"], json!([{"id": "2", "name": "Pear"}]), "{response}");
}
