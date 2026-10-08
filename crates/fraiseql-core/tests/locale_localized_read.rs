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
    let mut label = FieldDefinition::nullable("label", FieldType::String);
    label.localized = true;
    let mut secret = FieldDefinition::nullable("secret", FieldType::String);
    secret.requires_scope = Some("read:secret".to_string());
    // Masked rather than refused, so a principal without the scope still reads the row.
    secret.on_deny = fraiseql_core::schema::FieldDenyPolicy::Mask;
    let product = TestTypeBuilder::new("Product", VIEW)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_field(name)
        .with_field(FieldDefinition::nullable("category", FieldType::Object("Category".into())))
        .with_field(FieldDefinition::nullable(
            "tags",
            FieldType::List(Box::new(FieldType::Object("Tag".into()))),
        ))
        .with_field(secret)
        .build();
    let category = TestTypeBuilder::new("Category", "v_unused_category")
        .with_simple_field("id", FieldType::Id)
        .with_field(label.clone())
        .build();
    let tag = TestTypeBuilder::new("Tag", "v_unused_tag")
        .with_simple_field("id", FieldType::Id)
        .with_field(label)
        .build();
    let products = TestQueryBuilder::new("products", "Product")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    let mut connection = TestQueryBuilder::new("productsConnection", "Product")
        .returns_list(true)
        .with_sql_source(VIEW)
        .relay_cursor_column("pk")
        .build();
    connection.auto_params.has_order_by = true;
    let mut schema = TestSchemaBuilder::new()
        .with_type(product)
        .with_type(category)
        .with_type(tag)
        .with_query(products)
        .with_query(connection)
        .build();
    schema.interfaces.push(
        fraiseql_core::schema::InterfaceDefinition::new("Node")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );
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
            format!(
                "({pk}, jsonb_build_object('id', '{pk}', 'pk', {pk}, 'name', '{name}'::jsonb, \
                 'secret', 's', 'category', jsonb_build_object('id', 'c{pk}', 'label', \
                 '{name}'::jsonb), 'tags', jsonb_build_array(jsonb_build_object('id', 't{pk}', \
                 'label', '{name}'::jsonb))))"
            )
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

/// The expected fr-CA labels, row by row.
fn fr_ca() -> Vec<Value> {
    vec![json!("Pomme"), json!("Pear"), json!("Cerise"), Value::Null]
}

/// `value` read at `path` from each `products` row, rows ordered by id.
async fn read(executor: &Executor, query: &str, root: &str, path: &[&str]) -> Vec<Value> {
    let response = with_request_locale("fr-CA", executor.execute(query, None))
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let rows = response["data"][root].clone();
    let rows = rows.get("edges").map_or(rows.clone(), |edges| {
        Value::Array(edges.as_array().unwrap().iter().map(|e| e["node"].clone()).collect())
    });
    let mut out: Vec<(String, Value)> = rows
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|row| {
            let mut v = row;
            for key in path {
                v = &v[*key];
            }
            (row["id"].as_str().unwrap().to_string(), v.clone())
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, v)| v).collect()
}

/// Cycle 2: a relay connection's nodes.
#[tokio::test]
async fn a_relay_node_is_localized() {
    // Seeds the rows; the relay runner needs an executor built with it.
    if executor().await.is_none() {
        return;
    }
    let executor = Executor::new_with_relay(
        schema(),
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap()),
    );
    assert_eq!(
        read(
            &executor,
            "{ productsConnection(first: 10) { edges { node { id name } } } }",
            "productsConnection",
            &["name"]
        )
        .await,
        fr_ca()
    );
}

/// Cycle 2: a localized field of a nested object, at any depth of the stored document.
#[tokio::test]
async fn a_nested_objects_localized_field_is_localized() {
    let Some(executor) = executor().await else {
        return;
    };
    assert_eq!(
        read(
            &executor,
            "{ products { id category { label } } }",
            "products",
            &["category", "label"]
        )
        .await,
        fr_ca()
    );
}

/// Cycle 2: a localized field of an element of a nested list.
#[tokio::test]
async fn a_nested_list_elements_localized_field_is_localized() {
    let Some(executor) = executor().await else {
        return;
    };
    let labels: Vec<Value> =
        read(&executor, "{ products { id tags { label } } }", "products", &["tags"])
            .await
            .into_iter()
            .map(|tags| tags[0]["label"].clone())
            .collect();
    assert_eq!(labels, fr_ca());
}

/// Cycle 2: a read whose selection includes a policy-gated field returns the stored document
/// whole and is projected in Rust, not by the SQL projection.
#[tokio::test]
async fn a_gated_read_projected_in_rust_is_localized() {
    let Some(executor) = executor().await else {
        return;
    };
    let principal = fraiseql_core::security::SecurityContext {
        user_id:          fraiseql_core::prelude::UserId::new("reader"),
        tenant_id:        None,
        roles:            vec![],
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-localized".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    };
    let response = with_request_locale(
        "fr-CA",
        executor.execute_with_security("{ products { id name secret } }", None, &principal),
    )
    .await
    .unwrap();
    let mut rows: Vec<(String, Value)> = response["data"]["products"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|p| (p["id"].as_str().unwrap().to_string(), p["name"].clone()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(rows.into_iter().map(|(_, n)| n).collect::<Vec<_>>(), fr_ca(), "{response}");
}
