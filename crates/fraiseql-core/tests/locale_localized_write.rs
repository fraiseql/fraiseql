#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1513: a localized field's mutation input is coerced to a locale map before the
//! SQL function runs, and the payload's entity reads back as the request locale's label.
//!
//! Two mutations: `updateLocaleProduct(id, name)` takes the localized value as a top-level
//! argument; `createLocaleProduct(input: {id, name})` as a field of an input object (the
//! flattened Insert path). Each SQL function records the `name` it received and the
//! `fraiseql.locale` it saw, then merges into the stored map with `jsonb_strip_nulls(name ||
//! p_name)` (a `null` label removes its key).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `locale_lw` schema.

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, with_request_locale},
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InputFieldDefinition,
        InputObjectDefinition, LocaleConfig, LocaleSource, MutationDefinition, MutationOperation,
        TypeDefinition,
    },
};
use serde_json::{Value, json};

const SCHEMA: &str = "locale_lw";
const APPLE: &str = "00000000-0000-0000-0000-000000000001";
const CHERRY: &str = "00000000-0000-0000-0000-000000000002";

fn schema() -> CompiledSchema {
    let mut product = TypeDefinition::new("LocaleProduct", format!("{SCHEMA}.v_product"));
    let mut name = FieldDefinition::nullable("name", FieldType::String);
    name.localized = true;
    product.fields = vec![FieldDefinition::new("id", FieldType::Id), name];

    let mut update = MutationDefinition::new("updateLocaleProduct", "LocaleProduct");
    update.sql_source = Some(format!("{SCHEMA}.fn_update_product"));
    update.operation = MutationOperation::Update {
        table: "tb_product".to_string(),
    };
    let mut name_arg = ArgumentDefinition::optional("name", FieldType::String);
    name_arg.localized = true;
    update.arguments = vec![ArgumentDefinition::new("id", FieldType::Id), name_arg];

    let mut input = InputObjectDefinition::new("CreateLocaleProductInput");
    let mut name_field = InputFieldDefinition::new("name", "String");
    name_field.localized = true;
    input.fields = vec![InputFieldDefinition::new("id", "ID!"), name_field];
    let mut create = MutationDefinition::new("createLocaleProduct", "LocaleProduct");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_product"));
    create.operation = MutationOperation::Insert {
        table: "tb_product".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new(
        "input",
        FieldType::Object("CreateLocaleProductInput".to_string()),
    )];

    let mut schema = CompiledSchema::new();
    schema.types.push(product);
    schema.input_types.push(input);
    schema.mutations.push(update);
    schema.mutations.push(create);
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            ["en-US", "fr-FR", "de-DE"].map(String::from).to_vec(),
            BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema.build_indexes();
    schema
}

async fn provision() -> Option<PostgresAdapter> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    let body = |verb: &str, write: &str| {
        format!(
            "RETURNS app.mutation_response LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; \
             BEGIN INSERT INTO {SCHEMA}.tb_audit VALUES ('{verb}', p_name, \
             current_setting('fraiseql.locale', true)); {write}; v.succeeded := true; \
             v.state_changed := true; v.message := '{verb}'; v.entity_type := 'LocaleProduct'; \
             v.entity_id := p_id; v.entity := (SELECT data FROM {SCHEMA}.v_product WHERE id = \
             p_id); RETURN v; END $$"
        )
    };
    let statements = [
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_audit (verb text, name jsonb, seen text)"),
        format!("CREATE TABLE {SCHEMA}.tb_product (id uuid PRIMARY KEY, name jsonb NOT NULL)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_product AS SELECT id, jsonb_build_object('id', id, 'name', \
             name) AS data FROM {SCHEMA}.tb_product"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_product VALUES ('{APPLE}', \
             '{{\"en-US\": \"Apple\", \"de-DE\": \"Apfel\"}}')"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_update_product(p_id uuid, p_name jsonb) {}",
            body(
                "update",
                &format!(
                    "UPDATE {SCHEMA}.tb_product SET name = jsonb_strip_nulls(name || coalesce(p_name, '{{}}')) \
                     WHERE id = p_id"
                )
            )
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_product(p_id uuid, p_name jsonb) {}",
            body(
                "create",
                &format!(
                    "INSERT INTO {SCHEMA}.tb_product VALUES (p_id, jsonb_strip_nulls(p_name))"
                )
            )
        ),
    ];
    for statement in statements {
        adapter
            .execute_raw_query(&statement)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
    Some(adapter)
}

/// One column of a query over the fixture, as JSON.
async fn select(adapter: &PostgresAdapter, sql: &str) -> Vec<Value> {
    adapter
        .execute_raw_query(&format!("SELECT to_jsonb(x) AS data FROM ({sql}) x"))
        .await
        .unwrap()
        .into_iter()
        .map(|r| r["data"].clone())
        .collect()
}

async fn run(executor: &Executor, locale: &str, query: &str) -> Value {
    with_request_locale(locale, executor.execute(query, None))
        .await
        .unwrap_or_else(|e| panic!("{locale} {query}: {e}"))
}

/// A single value is the request locale's label; the function receives a one-key
/// map and merges it, so the stored map keeps its other labels. Writes see no locale, and the
/// stored value stays a map (the projection proof).
#[tokio::test]
async fn a_single_value_becomes_the_request_locales_label() {
    let Some(adapter) = provision().await else {
        return;
    };
    let executor = Executor::new(
        schema(),
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap()),
    );

    let response = run(
        &executor,
        "fr-FR",
        &format!(r#"mutation {{ updateLocaleProduct(id: "{APPLE}", name: {{ value: "Pomme" }}) {{ id }} }}"#),
    )
    .await;
    assert!(response.get("errors").is_none(), "{response}");
    let response = run(
        &executor,
        "fr-FR",
        &format!(
            r#"mutation {{ createLocaleProduct(input: {{ id: "{CHERRY}", name: {{ value: "Cerise" }} }}) {{ id }} }}"#
        ),
    )
    .await;
    assert!(response.get("errors").is_none(), "{response}");

    assert_eq!(
        select(
            &adapter,
            &format!("SELECT verb, name, seen FROM {SCHEMA}.tb_audit ORDER BY verb")
        )
        .await,
        vec![
            json!({"verb": "create", "name": {"fr-FR": "Cerise"}, "seen": null}),
            json!({"verb": "update", "name": {"fr-FR": "Pomme"}, "seen": null}),
        ]
    );
    assert_eq!(
        select(&adapter, &format!("SELECT name FROM {SCHEMA}.tb_product ORDER BY id")).await,
        vec![
            json!({"name": {"en-US": "Apple", "de-DE": "Apfel", "fr-FR": "Pomme"}}),
            json!({"name": {"fr-FR": "Cerise"}}),
        ]
    );
}

/// A full list of translations reaches the function as the map; a `null` label
/// removes its key.
#[tokio::test]
async fn translations_reach_the_function_as_the_map() {
    let Some(adapter) = provision().await else {
        return;
    };
    let executor = Executor::new(
        schema(),
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap()),
    );
    let response = run(
        &executor,
        "en-US",
        &format!(
            r#"mutation {{ updateLocaleProduct(id: "{APPLE}", name: {{ translations: [
                {{ locale: "fr-FR", value: "Pomme" }}, {{ locale: "de-DE", value: null }}] }}) {{ id }} }}"#
        ),
    )
    .await;
    assert!(response.get("errors").is_none(), "{response}");
    assert_eq!(
        select(&adapter, &format!("SELECT name FROM {SCHEMA}.tb_audit")).await,
        vec![json!({"name": {"fr-FR": "Pomme", "de-DE": null}})]
    );
    assert_eq!(
        select(&adapter, &format!("SELECT name FROM {SCHEMA}.tb_product WHERE id = '{APPLE}'"))
            .await,
        vec![json!({"name": {"en-US": "Apple", "fr-FR": "Pomme"}})]
    );
}

/// An unknown locale, a duplicate one, a value of the wrong type and a shape that is
/// none of the accepted ones are validation errors, with no statement run.
#[tokio::test]
async fn an_invalid_localized_input_is_refused_before_any_sql() {
    let adapter = Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new());
    let executor = Executor::new(schema(), adapter.clone());
    let update = |name: &str| {
        format!(r#"mutation {{ updateLocaleProduct(id: "{APPLE}", name: {name}) {{ id }} }}"#)
    };
    for (query, needles) in [
        (
            update(
                r#"{ translations: [{ locale: "fr-FR", value: "Pomme" }, { locale: "xx", value: "?" }] }"#,
            ),
            vec!["xx", "en-US, fr-FR, de-DE"],
        ),
        (
            update(
                r#"{ translations: [{ locale: "fr-FR", value: "a" }, { locale: "fr-FR", value: "b" }] }"#,
            ),
            vec!["fr-FR", "more than once"],
        ),
        (update("5"), vec!["name"]),
        (update(r"{ value: 5 }"), vec!["name"]),
        (update(r#"{ value: "a", translations: [] }"#), vec!["name"]),
        (
            format!(
                r#"mutation {{ createLocaleProduct(input: {{ id: "{CHERRY}", name: {{ translations: [{{ locale: "it-IT", value: "x" }}] }} }}) {{ id }} }}"#
            ),
            vec!["it-IT"],
        ),
    ] {
        let err = with_request_locale("fr-FR", executor.execute(&query, None))
            .await
            .expect_err(&query);
        let message = err.to_string();
        assert!(
            matches!(err, fraiseql_core::error::FraiseQLError::Validation { .. })
                && needles.iter().all(|n| message.contains(n)),
            "{query}: {message}"
        );
    }
    assert_eq!(adapter.query_count(), 0, "no statement ran");
}

/// The payload's entity reads its localized field as the request locale's label.
#[tokio::test]
async fn the_payload_entity_is_localized() {
    let Some(_adapter) = provision().await else {
        return;
    };
    let executor = Executor::new(
        schema(),
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap()),
    );
    let mutation = format!(
        r#"mutation {{ updateLocaleProduct(id: "{APPLE}", name: {{ value: "Pomme" }}) {{ id name }} }}"#
    );
    let fr = run(&executor, "fr-FR", &mutation).await;
    assert_eq!(fr["data"]["updateLocaleProduct"]["name"], json!("Pomme"), "{fr}");
    let en = run(
        &executor,
        "en-US",
        &format!(r#"mutation {{ updateLocaleProduct(id: "{APPLE}") {{ id name }} }}"#),
    )
    .await;
    assert_eq!(en["data"]["updateLocaleProduct"]["name"], json!("Apple"), "{en}");
}

/// Introspection and the SDL give a localized argument and input field `LocalizedInput`, and
/// publish it and `LocalizedStringInput`.
#[tokio::test]
async fn introspection_and_sdl_show_localized_inputs() {
    let executor = Executor::new(
        schema(),
        Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
    );
    let response = executor
        .execute(
            r#"{ mutation: __type(name: "Mutation") { fields { name args { name type { kind name } } } }
                 input: __type(name: "CreateLocaleProductInput") { inputFields { name type { kind name } } }
                 localized: __type(name: "LocalizedInput") { kind inputFields { name } }
                 translation: __type(name: "LocalizedStringInput") { kind inputFields { name } } }"#,
            None,
        )
        .await
        .unwrap();
    let data = &response["data"];
    let update = data["mutation"]["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .find(|f| f["name"] == "updateLocaleProduct")
        .unwrap()
        .clone();
    let name_arg = update["args"].as_array().unwrap().iter().find(|a| a["name"] == "name").unwrap();
    assert_eq!(name_arg["type"], json!({"kind": "INPUT_OBJECT", "name": "LocalizedInput"}));
    let name_field = data["input"]["inputFields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "name")
        .unwrap();
    assert_eq!(name_field["type"], json!({"kind": "INPUT_OBJECT", "name": "LocalizedInput"}));
    assert_eq!(
        data["localized"],
        json!({"kind": "INPUT_OBJECT", "inputFields": [{"name": "value"}, {"name": "translations"}]})
    );
    assert_eq!(
        data["translation"],
        json!({"kind": "INPUT_OBJECT", "inputFields": [{"name": "locale"}, {"name": "value"}]})
    );

    let sdl = schema().raw_schema();
    assert!(sdl.contains("updateLocaleProduct(id: ID!, name: LocalizedInput)"), "{sdl}");
    assert!(sdl.contains("  name: LocalizedInput\n"), "{sdl}");
    assert!(sdl.contains("input LocalizedInput {"), "{sdl}");
    assert!(sdl.contains("input LocalizedStringInput {"), "{sdl}");
}

/// A localized argument or input field that is not a String, or one in a schema with no
/// `[locale]`, is refused at load; so is a declared type taking `LocalizedInput`'s name.
#[test]
fn an_unservable_localized_input_is_refused_at_load() {
    let base = |argument: &str, input_field: &str, locale: bool, extra_input: &str| {
        let locale = if locale {
            r#", "locale": {"default": "en-US", "allowed": ["en-US", "fr-FR"]}"#
        } else {
            ""
        };
        format!(
            r#"{{"types": [{{"name": "P", "sql_source": "v_p", "fields": [
                {{"name": "id", "field_type": "ID", "nullable": false}}]}}],
              "input_types": [{{"name": "PInput", "fields": [{input_field}]}}{extra_input}],
              "queries": [], "subscriptions": [],
              "mutations": [{{"name": "m", "return_type": "P", "sql_source": "fn_m",
                "arguments": [{argument}]}}]{locale}}}"#
        )
    };
    let string_arg =
        r#"{"name": "name", "arg_type": "String", "nullable": true, "localized": true}"#;
    let int_arg = r#"{"name": "name", "arg_type": "Int", "nullable": true, "localized": true}"#;
    let string_field = r#"{"name": "name", "field_type": "String", "localized": true}"#;
    let int_field = r#"{"name": "name", "field_type": "Int", "localized": true}"#;
    let plain_field = r#"{"name": "name", "field_type": "String"}"#;
    for (json, needle) in [
        (
            base(int_arg, plain_field, true, ""),
            "`m(name)` is localized but is not a String",
        ),
        (
            base(string_arg, int_field, true, ""),
            "`PInput.name` is localized but is not a String",
        ),
        (base(string_arg, plain_field, false, ""), "declares no [locale]"),
        (
            base(string_arg, plain_field, true, r#", {"name": "LocalizedInput", "fields": []}"#),
            "`LocalizedInput` is declared",
        ),
    ] {
        let err = CompiledSchema::from_json(&json, false).expect_err(needle);
        assert!(err.to_string().contains(needle), "{needle}: {err}");
    }
    CompiledSchema::from_json(&base(string_arg, string_field, true, ""), false)
        .expect("control: a localized String argument and field load");
}
