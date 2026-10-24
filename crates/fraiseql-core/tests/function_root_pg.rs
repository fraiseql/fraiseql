#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — panics are acceptable

//! A query read from a `STABLE` set-returning function, against PostgreSQL.
//!
//! Two reads a view cannot express well, each answered by a function bound to the query's
//! arguments:
//!
//! - **Computed per argument row.** `lineItems(assetId:)` prices one asset from the rules its kind
//!   and tenant select. The same rows come from a `LATERAL` view filtered on the asset, and the two
//!   must agree — before and after a `where`, an `orderBy` and a `limit` applied on top of the
//!   call. The tenant reaches the function as a parameter, from the token.
//! - **Computed from free input.** `checkIdentifier(value:, country:)` normalises an identifier or
//!   names why it cannot, as an error identifier and its details. No row holds the input, so no
//!   view can answer it.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** recreates its own `fn_root` schema once per run, then only reads it.

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{
        ArgumentDefinition, AutoParams, CompiledSchema, FieldDefinition, FieldType,
        InjectedParamSource, QueryDefinition, TypeDefinition,
    },
    security::SecurityContext,
};
use serde_json::{Value, json};
use tokio::sync::OnceCell;

const ASSET: &str = "5f0c6a52-0d6e-4c3a-9a3e-6a1b2c3d4e01";
const OTHER_ASSET: &str = "5f0c6a52-0d6e-4c3a-9a3e-6a1b2c3d4e02";

const DDL: &[&str] = &[
    "DROP SCHEMA IF EXISTS fn_root CASCADE",
    "CREATE SCHEMA fn_root",
    "CREATE TABLE fn_root.tb_asset (id uuid PRIMARY KEY, tenant_id text NOT NULL, kind text NOT \
     NULL)",
    "CREATE TABLE fn_root.tb_rule (tenant_id text NOT NULL, kind text NOT NULL, label text NOT \
     NULL, amount numeric NOT NULL)",
    "INSERT INTO fn_root.tb_asset VALUES ('5f0c6a52-0d6e-4c3a-9a3e-6a1b2c3d4e01', 't1', \
     'printer'), ('5f0c6a52-0d6e-4c3a-9a3e-6a1b2c3d4e02', 't1', 'scanner')",
    "INSERT INTO fn_root.tb_rule VALUES ('t1', 'printer', 'Rental', 40), ('t1', 'printer', \
     'Service', 12.5), ('t1', 'printer', 'Toner', 7), ('t1', 'scanner', 'Rental', 25), ('t2', \
     'printer', 'Rental', 99)",
    // The view an author writes today: one line per rule the asset's kind and tenant select,
    // driven by LATERAL from the asset row.
    "CREATE VIEW fn_root.v_line_item AS SELECT jsonb_build_object('asset_id', a.id, 'label', \
     r.label, 'amount', r.amount) AS data FROM fn_root.tb_asset a CROSS JOIN LATERAL (SELECT \
     label, amount FROM fn_root.tb_rule r WHERE r.kind = a.kind AND r.tenant_id = a.tenant_id) r",
    // The same lines from the function the issue asks for, bound to the asset and the tenant.
    "CREATE FUNCTION fn_root.fn_line_items(p_asset_id uuid, p_tenant_id text) RETURNS \
     TABLE(data jsonb) LANGUAGE sql STABLE AS $$ SELECT jsonb_build_object('asset_id', a.id, \
     'label', r.label, 'amount', r.amount) FROM fn_root.tb_asset a JOIN fn_root.tb_rule r ON \
     r.kind = a.kind AND r.tenant_id = a.tenant_id WHERE a.id = p_asset_id AND a.tenant_id = \
     p_tenant_id $$",
    // A free-input check: the identifier's digits when they are fourteen, else why not.
    "CREATE FUNCTION fn_root.fn_check_identifier(p_value text, p_country text) RETURNS \
     TABLE(data jsonb) LANGUAGE sql STABLE AS $$ WITH n AS (SELECT regexp_replace(p_value, \
     '\\s', '', 'g') AS v) SELECT CASE WHEN p_country <> 'FR' THEN jsonb_build_object('normalised', \
     NULL, 'errors', jsonb_build_array(jsonb_build_object('identifier', 'identifier.country', \
     'details', jsonb_build_object('country', p_country)))) WHEN n.v ~ '^[0-9]{14}$' THEN \
     jsonb_build_object('normalised', n.v, 'errors', '[]'::jsonb) ELSE \
     jsonb_build_object('normalised', NULL, 'errors', jsonb_build_array(jsonb_build_object( \
     'identifier', 'identifier.length', 'details', jsonb_build_object('expected', 14, 'actual', \
     length(n.v))))) END FROM n $$",
];

fn line_item_type() -> TypeDefinition {
    let mut item = TypeDefinition::new("LineItem", "fn_root.v_line_item");
    item.fields = vec![
        FieldDefinition::new("assetId", FieldType::Uuid),
        FieldDefinition::new("label", FieldType::String),
        FieldDefinition::new("amount", FieldType::Decimal),
    ];
    item
}

fn check_types() -> [TypeDefinition; 2] {
    let mut check = TypeDefinition::new("IdentifierCheck", "");
    check.fields = vec![
        FieldDefinition::nullable("normalised", FieldType::String),
        FieldDefinition::new(
            "errors",
            FieldType::list(FieldType::Object("IdentifierError".to_string())),
        ),
    ];
    let mut error = TypeDefinition::new("IdentifierError", "");
    error.fields = vec![
        FieldDefinition::new("identifier", FieldType::String),
        FieldDefinition::new("details", FieldType::Json),
    ];
    [check, error]
}

/// `own` followed by the auto-param arguments the compiler declares on a list query, so
/// the binding has to tell them apart from the function's own parameter.
fn with_auto_arguments(own: ArgumentDefinition) -> Vec<ArgumentDefinition> {
    vec![
        own,
        ArgumentDefinition::optional("where", FieldType::Json),
        ArgumentDefinition::optional("orderBy", FieldType::Json),
        ArgumentDefinition::optional("limit", FieldType::Int),
        ArgumentDefinition::optional("offset", FieldType::Int),
    ]
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.types.push(line_item_type());
    schema.types.extend(check_types());

    let mut view = QueryDefinition::new("lineItemRows", "LineItem")
        .returning_list()
        .with_sql_source("fn_root.v_line_item");
    view.arguments = with_auto_arguments(ArgumentDefinition::new("assetId", FieldType::Uuid));
    view.auto_params = AutoParams::all();
    schema.queries.push(view);

    let mut function = QueryDefinition::new("lineItems", "LineItem")
        .returning_list()
        .with_sql_source_function("fn_root.fn_line_items");
    function.arguments = with_auto_arguments(ArgumentDefinition::new("assetId", FieldType::Uuid));
    function
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
    function.auto_params = AutoParams::all();
    schema.queries.push(function);

    let mut check = QueryDefinition::new("checkIdentifier", "IdentifierCheck")
        .with_sql_source_function("fn_root.fn_check_identifier");
    check.arguments = vec![
        ArgumentDefinition::new("value", FieldType::String),
        ArgumentDefinition::new("country", FieldType::String),
    ];
    schema.queries.push(check);

    schema.build_indexes();
    schema
}

fn principal(tenant: &str) -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        Some(tenant.into()),
        scopes:           vec![],
        attributes:       HashMap::from([("tenant_id".to_string(), json!(tenant))]),
        request_id:       "req-fn-root".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The fixture, created once per run: the tests only read it.
static FIXTURE: OnceCell<()> = OnceCell::const_new();

async fn executor() -> Executor {
    let url = fraiseql_test_support::database_url();
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    FIXTURE
        .get_or_init(|| async {
            for ddl in DDL {
                adapter.execute_raw_query(ddl).await.unwrap();
            }
        })
        .await;
    Executor::new(schema(), Arc::new(adapter))
}

async fn run(executor: &Executor, document: &str, tenant: &str) -> Value {
    executor
        .execute_with_security(document, None, &principal(tenant))
        .await
        .unwrap_or_else(|e| panic!("{document}: {e}"))
}

/// `(label, amount)` of each line `field` answered, in the order it answered them.
fn lines(response: &Value, field: &str) -> Vec<(String, f64)> {
    response["data"][field]
        .as_array()
        .unwrap_or_else(|| panic!("{field}: {response}"))
        .iter()
        .map(|line| {
            let amount = &line["amount"];
            let amount = amount
                .as_f64()
                .or_else(|| amount.as_str().and_then(|s| s.parse().ok()))
                .unwrap_or_else(|| panic!("amount of {line}"));
            (line["label"].as_str().unwrap().to_string(), amount)
        })
        .collect()
}

// ── computed per argument row ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_function_root_answers_the_rows_its_lateral_view_answers() {
    let executor = executor().await;
    for (shape, args) in [
        ("every line", r"orderBy: { label: ASC }"),
        ("filtered", r#"where: { label: { neq: "Toner" } }, orderBy: { label: ASC }"#),
        ("ordered and paged", r"orderBy: { amount: DESC }, limit: 2"),
    ] {
        let read = |field: &str| {
            format!(r#"{{ {field}(assetId: "{ASSET}", {args}) {{ label amount }} }}"#)
        };
        let view = run(&executor, &read("lineItemRows"), "t1").await;
        let function = run(&executor, &read("lineItems"), "t1").await;
        let (view, function) = (lines(&view, "lineItemRows"), lines(&function, "lineItems"));
        assert!(!view.is_empty(), "{shape}: the fixture prices the asset");
        assert_eq!(function, view, "{shape}");
    }
}

#[tokio::test]
async fn a_function_roots_where_order_and_page_apply_to_the_rows_of_its_call() {
    let executor = executor().await;
    let response = run(
        &executor,
        &format!(
            r#"{{ lineItems(assetId: "{ASSET}", where: {{ label: {{ neq: "Toner" }} }}, orderBy: {{ amount: DESC }}, limit: 1) {{ label amount }} }}"#
        ),
        "t1",
    )
    .await;
    assert_eq!(lines(&response, "lineItems"), vec![("Rental".to_string(), 40.0)]);
}

#[tokio::test]
async fn a_function_root_binds_the_tenant_from_the_token() {
    let executor = executor().await;
    let other_tenant =
        run(&executor, &format!(r#"{{ lineItems(assetId: "{ASSET}") {{ label }} }}"#), "t2").await;
    assert_eq!(other_tenant["data"]["lineItems"], json!([]), "{other_tenant}");
    let other_asset = run(
        &executor,
        &format!(r#"{{ lineItems(assetId: "{OTHER_ASSET}") {{ label amount }} }}"#),
        "t1",
    )
    .await;
    assert_eq!(lines(&other_asset, "lineItems"), vec![("Rental".to_string(), 25.0)]);
}

#[tokio::test]
async fn an_anonymous_read_of_a_tenant_bound_function_root_is_refused() {
    let executor = executor().await;
    let error = executor
        .execute(&format!(r#"{{ lineItems(assetId: "{ASSET}") {{ label }} }}"#), None)
        .await
        .expect_err("no token, no tenant to bind");
    assert!(error.to_string().contains("security context"), "{error}");
}

// ── computed from free input ─────────────────────────────────────────────────────────

const CHECK: &str = "query($v: String!, $c: String!) { checkIdentifier(value: $v, country: $c) { \
                     normalised errors { identifier details } } }";

async fn check(executor: &Executor, value: &str, country: &str) -> Value {
    let response = executor
        .execute(CHECK, Some(&json!({ "v": value, "c": country })))
        .await
        .unwrap_or_else(|e| panic!("checkIdentifier({value}, {country}): {e}"));
    response["data"]["checkIdentifier"].clone()
}

#[tokio::test]
async fn a_free_input_check_returns_the_normalised_value() {
    let executor = executor().await;
    assert_eq!(
        check(&executor, "123 456 789 00012", "FR").await,
        json!({ "normalised": "12345678900012", "errors": [] })
    );
}

#[tokio::test]
async fn a_free_input_check_returns_an_error_identifier_and_its_details() {
    let executor = executor().await;
    assert_eq!(
        check(&executor, "123 456", "FR").await,
        json!({
            "normalised": null,
            "errors": [{ "identifier": "identifier.length", "details": { "expected": 14, "actual": 6 } }],
        })
    );
    assert_eq!(
        check(&executor, "12345678900012", "BE").await["errors"][0]["identifier"],
        "identifier.country"
    );
}
