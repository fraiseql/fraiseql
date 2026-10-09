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

/// `(pk, name map)`: French, English and German labels; English, with a label outside
/// `allowed` and a non-string one; bare French only; nothing at all.
const ROWS: [(i64, &str); 4] = [
    (1, r#"{"fr-FR": "Pomme", "en-US": "Apple", "de-DE": "Apfel"}"#),
    (2, r#"{"en-US": "Pear", "it-IT": "Pera", "fr": 5}"#),
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
    let mut motto = FieldDefinition::nullable("motto", FieldType::String);
    motto.localized = true;
    motto.requires_scope = Some("read:motto".to_string());
    motto.on_deny = fraiseql_core::schema::FieldDenyPolicy::Mask;
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
        .with_field(motto)
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

/// The `name` of each `products` row (ordered by id) under `key`, read in `fr-CA` with
/// `variables`.
async fn labels(executor: &Executor, query: &str, variables: Value, key: &str) -> Vec<Value> {
    let response = with_request_locale("fr-CA", executor.execute(query, Some(&variables)))
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let mut rows: Vec<(String, Value)> = response["data"]["products"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|p| (p["id"].as_str().unwrap().to_string(), p[key].clone()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, v)| v).collect()
}

/// `de-DE` → `en-US`, row by row.
fn de_de() -> Vec<Value> {
    vec![json!("Apfel"), json!("Pear"), Value::Null, Value::Null]
}

/// Cycle 3: `locale:` reads one field selection in another allowed locale, whatever the
/// request locale; two aliases of one field read two locales.
#[tokio::test]
async fn a_locale_argument_picks_the_fields_locale() {
    let Some(executor) = executor().await else {
        return;
    };
    let query = r#"{ products { id name(locale: "de-DE") } }"#;
    assert_eq!(labels(&executor, query, json!({}), "name").await, de_de());

    let query = r#"{ products { id de: name(locale: "de-DE") name } }"#;
    assert_eq!(labels(&executor, query, json!({}), "de").await, de_de(), "aliased");
    assert_eq!(labels(&executor, query, json!({}), "name").await, fr_ca(), "unaliased sibling");
}

/// Cycle 3: the value may come from a variable; an omitted or null one reads the request
/// locale.
#[tokio::test]
async fn a_locale_argument_may_be_a_variable() {
    let Some(executor) = executor().await else {
        return;
    };
    let query = "query Q($l: String) { products { id name(locale: $l) } }";
    assert_eq!(labels(&executor, query, json!({"l": "de-DE"}), "name").await, de_de());
    assert_eq!(labels(&executor, query, json!({}), "name").await, fr_ca(), "omitted");
    assert_eq!(labels(&executor, query, json!({"l": null}), "name").await, fr_ca(), "null");
}

/// Cycle 3: the argument reaches the relay node projection and the Rust projector (a gated
/// read), not only the plain list's SQL projection.
#[tokio::test]
async fn a_locale_argument_reaches_relay_and_the_rust_projector() {
    let Some(executor) = executor().await else {
        return;
    };
    let relay = Executor::new_with_relay(
        schema(),
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap()),
    );
    assert_eq!(
        read(
            &relay,
            r#"{ productsConnection(first: 10) { edges { node { id name(locale: "de-DE") } } } }"#,
            "productsConnection",
            &["name"]
        )
        .await,
        de_de(),
        "relay"
    );

    let principal = fraiseql_core::security::SecurityContext {
        user_id:          fraiseql_core::prelude::UserId::new("reader"),
        tenant_id:        None,
        roles:            vec![],
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-localized-arg".to_string(),
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
        executor.execute_with_security(
            r#"{ products { id name(locale: "de-DE") secret } }"#,
            None,
            &principal,
        ),
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
    assert_eq!(rows.into_iter().map(|(_, n)| n).collect::<Vec<_>>(), de_de(), "gated");
}

/// Cycle 3: a `locale:` outside `allowed` is a validation error naming the allowed set,
/// literal or variable, and so is `locale:` on a field that is not localized. Each is
/// refused before any statement: the adapter fails every call, and counts none.
#[tokio::test]
async fn a_locale_argument_outside_allowed_is_refused_before_any_sql() {
    let adapter = Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new());
    let executor = Executor::new(schema(), adapter.clone());
    for (query, variables, needle) in [
        (
            r#"{ products { id name(locale: "xx") } }"#,
            json!({}),
            "en-US, fr, fr-CA, fr-FR, de-DE",
        ),
        (
            "query Q($l: String) { products { id name(locale: $l) } }",
            json!({"l": "xx"}),
            "en-US, fr, fr-CA, fr-FR, de-DE",
        ),
        ("{ products { id name(locale: 7) } }", json!({}), "must be a String"),
        (r#"{ products { id(locale: "fr") } }"#, json!({}), "localized"),
        (
            r#"{ productsConnection(first: 1) { edges { node { name(locale: "xx") } } } }"#,
            json!({}),
            "en-US, fr, fr-CA, fr-FR, de-DE",
        ),
    ] {
        let err = with_request_locale("fr-CA", executor.execute(query, Some(&variables)))
            .await
            .expect_err(query);
        assert!(
            matches!(err, fraiseql_core::error::FraiseQLError::Validation { .. })
                && err.to_string().contains(needle),
            "{query} {variables}: {err}"
        );
    }
    assert_eq!(adapter.query_count(), 0, "no statement ran");
}

/// Cycle 3: introspection and the SDL show `locale: String` on a localized field, and only
/// there.
#[tokio::test]
async fn introspection_and_sdl_show_the_locale_argument() {
    let executor = Executor::new(
        schema(),
        Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
    );
    let response = executor
        .execute(
            r#"{ __type(name: "Product") { fields { name args { name type { name } } } } }"#,
            None,
        )
        .await
        .unwrap();
    let fields = response["data"]["__type"]["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"));
    let args = |field: &str| -> Vec<Value> {
        fields.iter().find(|f| f["name"] == field).unwrap()["args"]
            .as_array()
            .unwrap()
            .clone()
    };
    assert_eq!(args("name"), vec![json!({"name": "locale", "type": {"name": "String"}})]);
    assert_eq!(args("id"), Vec::<Value>::new());
    let sdl = schema().raw_schema();
    assert!(sdl.contains("name(locale: String): String"), "{sdl}");
}

/// Cycle 4: every allowed label, in `allowed`'s order, row by row. A stored key outside
/// `allowed` and a label that is not a string are not returned.
fn all_labels() -> Vec<Value> {
    vec![
        json!([
            {"locale": "en-US", "value": "Apple"},
            {"locale": "fr-FR", "value": "Pomme"},
            {"locale": "de-DE", "value": "Apfel"}
        ]),
        json!([{"locale": "en-US", "value": "Pear"}]),
        json!([{"locale": "fr", "value": "Cerise"}]),
        json!([]),
    ]
}

/// Cycle 4: `<field>Translations` lists a localized field's labels, through each projector.
#[tokio::test]
async fn translations_list_every_allowed_label_in_order() {
    let Some(executor) = executor().await else {
        return;
    };
    let q = "{ products { id nameTranslations { locale value } } }";
    assert_eq!(
        read(&executor, q, "products", &["nameTranslations"]).await,
        all_labels(),
        "list"
    );

    let q = "{ products { id category { labelTranslations { locale value } } } }";
    assert_eq!(
        read(&executor, q, "products", &["category", "labelTranslations"]).await,
        all_labels(),
        "nested object"
    );

    let q = "{ products { id tags { labelTranslations { locale value } } } }";
    let tags: Vec<Value> = read(&executor, q, "products", &["tags"])
        .await
        .into_iter()
        .map(|tags| tags[0]["labelTranslations"].clone())
        .collect();
    assert_eq!(tags, all_labels(), "nested list");

    let relay = Executor::new_with_relay(
        schema(),
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap()),
    );
    let q = "{ productsConnection(first: 10) { edges { node { id nameTranslations { locale value } } } } }";
    assert_eq!(
        read(&relay, q, "productsConnection", &["nameTranslations"]).await,
        all_labels(),
        "relay"
    );

    // Aliases and `__typename` inside the sibling's selection.
    let q = "{ products { id nameTranslations { __typename l: locale } } }";
    assert_eq!(
        read(&executor, q, "products", &["nameTranslations"]).await[1],
        json!([{"__typename": "LocalizedString", "l": "en-US"}]),
        "sub-selection"
    );
}

/// Cycle 4: a read projected in Rust (a policy-gated sibling selected) lists the same labels.
#[tokio::test]
async fn translations_through_the_rust_projector() {
    let Some(executor) = executor().await else {
        return;
    };
    let principal = fraiseql_core::security::SecurityContext {
        user_id:          fraiseql_core::prelude::UserId::new("reader"),
        tenant_id:        None,
        roles:            vec![],
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-translations".to_string(),
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
        executor.execute_with_security(
            "{ products { id nameTranslations { locale value } secret } }",
            None,
            &principal,
        ),
    )
    .await
    .unwrap();
    let mut rows: Vec<(String, Value)> = response["data"]["products"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|p| (p["id"].as_str().unwrap().to_string(), p["nameTranslations"].clone()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(rows.into_iter().map(|(_, v)| v).collect::<Vec<_>>(), all_labels(), "{response}");
}

/// Cycle 4: an invalid sibling selection is refused before any statement: no sub-selection,
/// an undeclared sub-field, a field that is not localized, and a field-gated one (#1523).
#[tokio::test]
async fn an_invalid_translations_selection_is_refused_before_any_sql() {
    let adapter = Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new());
    let executor = Executor::new(schema(), adapter.clone());
    for (query, needle) in [
        ("{ products { id nameTranslations } }", "nameTranslations"),
        ("{ products { id nameTranslations { bogus } } }", "bogus"),
        ("{ products { id idTranslations { value } } }", "idTranslations"),
        ("{ products { id mottoTranslations { value } } }", "#1523"),
        (
            "{ productsConnection(first: 1) { edges { node { mottoTranslations { value } } } } }",
            "#1523",
        ),
    ] {
        let err = with_request_locale("fr-CA", executor.execute(query, None))
            .await
            .expect_err(query);
        assert!(
            matches!(err, fraiseql_core::error::FraiseQLError::Validation { .. })
                && err.to_string().contains(needle),
            "{query}: {err}"
        );
    }
    assert_eq!(adapter.query_count(), 0, "no statement ran");
}

/// Cycle 4: introspection and the SDL show `<field>Translations: [LocalizedString!]!` and the
/// `LocalizedString` type.
#[tokio::test]
async fn introspection_and_sdl_show_the_translations_sibling() {
    let executor = Executor::new(
        schema(),
        Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
    );
    let response = executor
        .execute(
            r#"{ product: __type(name: "Product") { fields { name type { kind ofType { kind ofType { kind ofType { name } } } } } }
                 localized: __type(name: "LocalizedString") { kind fields { name type { kind ofType { name } } } } }"#,
            None,
        )
        .await
        .unwrap();
    let fields = response["data"]["product"]["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"));
    let sibling = fields
        .iter()
        .find(|f| f["name"] == "nameTranslations")
        .unwrap_or_else(|| panic!("{response}"));
    assert_eq!(
        sibling["type"],
        json!({"kind": "NON_NULL", "ofType": {"kind": "LIST", "ofType": {"kind": "NON_NULL", "ofType": {"name": "LocalizedString"}}}})
    );
    assert!(!fields.iter().any(|f| f["name"] == "idTranslations"));
    assert_eq!(
        response["data"]["localized"],
        json!({"kind": "OBJECT", "fields": [
            {"name": "locale", "type": {"kind": "NON_NULL", "ofType": {"name": "String"}}},
            {"name": "value", "type": {"kind": "NON_NULL", "ofType": {"name": "String"}}}
        ]})
    );
    let sdl = schema().raw_schema();
    assert!(sdl.contains("  nameTranslations: [LocalizedString!]!\n"), "{sdl}");
    assert!(
        sdl.contains("type LocalizedString {\n  locale: String!\n  value: String!\n}"),
        "{sdl}"
    );
}

/// Cycle 4: a declared field or type that would collide with the sibling or its type is
/// refused at load.
#[test]
fn a_name_the_sibling_needs_is_refused_at_load() {
    let schema = |fields: &str, extra_type: &str| {
        format!(
            r#"{{"types": [{{"name": "Product", "sql_source": "tv_product", "fields": [
                {{"name": "id", "field_type": "ID", "nullable": false}},
                {{"name": "name", "field_type": "String", "nullable": true, "localized": true}}{fields}
            ]}}{extra_type}],
              "queries": [], "mutations": [], "subscriptions": [],
              "locale": {{"default": "en-US", "allowed": ["en-US", "fr"]}}}}"#
        )
    };
    let err = CompiledSchema::from_json(
        &schema(
            r#", {"name": "nameTranslations", "field_type": "String", "nullable": true}"#,
            "",
        ),
        false,
    )
    .expect_err("a declared `nameTranslations` collides with the sibling");
    assert!(err.to_string().contains("`Product.nameTranslations`"), "{err}");
    let err = CompiledSchema::from_json(
        &schema(
            "",
            r#", {"name": "LocalizedString", "sql_source": "v_x", "fields": [
                {"name": "id", "field_type": "ID", "nullable": false}]}"#,
        ),
        false,
    )
    .expect_err("a declared `LocalizedString` collides with the sibling's type");
    assert!(err.to_string().contains("`LocalizedString`"), "{err}");
    CompiledSchema::from_json(&schema("", ""), false).expect("control: the plain schema loads");
}

/// Cycle 5: the stored values the two evaluators must agree on. Maps with every allowed
/// label, a partial one, keys outside `allowed`, `null` and non-string labels, an empty one,
/// and stored values that are not maps.
const CORPUS: &[&str] = &[
    r#"{"en-US": "Apple", "fr": "Pomme", "fr-CA": "Pomme QC", "fr-FR": "Pomme FR", "de-DE": "Apfel"}"#,
    r#"{"fr-FR": "Pomme", "en-US": "Apple"}"#,
    r#"{"it-IT": "Mela", "fr-BE": "Pomme BE", "EN-US": "upper"}"#,
    r#"{"fr-CA": null, "fr-FR": 7, "fr": true, "en-US": "Apple"}"#,
    r#"{"fr-CA": {"x": 1}, "fr-FR": ["a"], "fr": "", "en-US": "Apple"}"#,
    r"{}",
    r#""a plain string""#,
    r"5",
    r"null",
    r#"["fr", "Pomme"]"#,
];

/// Cycle 5: for each stored value × each allowed locale, the label SQL reads and the one the
/// in-process evaluator reads are the same; so are the translations each lists.
#[tokio::test]
async fn the_sql_and_rust_evaluators_agree() {
    use fraiseql_core::db::projection_generator::{
        LocalizedRead, TranslationPart, localized_text_expr, localized_translations_expr,
    };
    let Some(pg) = fraiseql_test_support::postgres().await else {
        return;
    };
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    let schema = schema();
    let config = schema.locale.as_ref().unwrap();
    let rows: Vec<String> = CORPUS
        .iter()
        .enumerate()
        .map(|(i, v)| format!("({i}, '{}'::jsonb)", v.replace('\'', "''")))
        .collect();
    let allowed = &config.allowed;
    let keys = vec![
        ("locale".to_string(), TranslationPart::Locale),
        ("value".to_string(), TranslationPart::Value),
    ];
    let read = LocalizedRead::Translations {
        allowed: allowed.clone(),
        keys:    keys.clone(),
    };
    let mut compared = 0;
    for tag in &config.allowed {
        let chain = config.chain(tag).unwrap().to_vec();
        let sql = format!(
            "SELECT jsonb_build_object('i', m.i, 'label', {}, 'all', {}) AS data FROM (VALUES {}) \
             AS m(i, v)",
            localized_text_expr("m.v", &chain).unwrap(),
            localized_translations_expr("m.v", allowed, &keys).unwrap(),
            rows.join(", ")
        );
        for row in adapter.execute_raw_query(&sql).await.unwrap() {
            let row = &row["data"];
            let i = usize::try_from(row["i"].as_u64().unwrap()).unwrap();
            let stored: Value = serde_json::from_str(CORPUS[i]).unwrap();
            assert_eq!(
                row["label"],
                fraiseql_core::runtime::localize(&stored, &chain),
                "label of {stored} in {tag}"
            );
            assert_eq!(
                row["all"],
                fraiseql_core::runtime::translations(&stored, &read),
                "translations of {stored}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, CORPUS.len() * config.allowed.len(), "every pair was compared");
}
