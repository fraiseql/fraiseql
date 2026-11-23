#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! A mutation document's variables are held to their declarations, as a query's are.
//!
//! A variable is declared at a type its argument can take (GraphQL § 5.8.5), and the value
//! a request supplies for it has that declared type (§ 6.1.2). Both are refused before any
//! root of the operation writes, so a document with a second root at fault writes nothing.
//! Each case is asked through every entry a document reaches the executor by, and the rows
//! each function writes are counted.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test creates and drops its own `mutation_variable_types_*` schema;
//! the shared `app` response contract is created by whichever test gets there first.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    chrono::Utc,
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
    security::SecurityContext,
};
use serde_json::{Value, json};

/// The response contract, the schema, its table and functions; `None` without a database.
async fn provisioned(schema: &str) -> Option<Arc<PostgresAdapter>> {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return None;
    };
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let response_columns = "succeeded BOOLEAN, state_changed BOOLEAN, \
         error_class app.mutation_error_class, status_detail TEXT, http_status SMALLINT, \
         message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, updated_fields TEXT[], \
         cascade JSONB, error_detail JSONB, metadata JSONB";
    let write = |name: &str, param: &str, column: &str| {
        format!(
            "CREATE FUNCTION {schema}.{name}({param}) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE r app.mutation_response; BEGIN \
             INSERT INTO {schema}.tb_write ({column}) VALUES (p_value); \
             r.succeeded := true; r.state_changed := true; r.message := ''; \
             r.entity_type := 'Write'; \
             r.entity := jsonb_build_object('id', (SELECT count(*) FROM {schema}.tb_write)::text); \
             RETURN r; END; $$"
        )
    };
    for ddl in [
        "DO $$ BEGIN CREATE SCHEMA IF NOT EXISTS app; \
         EXCEPTION WHEN duplicate_schema OR unique_violation THEN NULL; END $$"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); \
         EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$"
            .to_string(),
        format!(
            "DO $$ BEGIN CREATE TYPE app.mutation_response AS ({response_columns}); \
             EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$"
        ),
        format!("DROP SCHEMA IF EXISTS {schema} CASCADE"),
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE TABLE {schema}.tb_write (label text, page int)"),
        write("fn_record_receipt", "p_value text", "label"),
        write("fn_record_page", "p_value int", "page"),
        write("fn_record_kind", "p_value text", "label"),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Arc::new(adapter))
}

fn compiled(schema: &str) -> CompiledSchema {
    let mutation = |name: &str, function: &str, argument: &str, arg_type: Value| {
        json!({
            "name": name,
            "return_type": "Write",
            "sql_source": format!("{schema}.{function}"),
            "operation": { "Insert": { "table": "tb_write" } },
            "changelog": false,
            "arguments": [{ "name": argument, "arg_type": arg_type, "nullable": false }]
        })
    };
    serde_json::from_value(json!({
        "types": [{
            "name": "Write",
            "sql_source": "",
            "fields": [{ "name": "id", "field_type": "ID" }]
        }],
        "mutations": [
            mutation("recordReceipt", "fn_record_receipt", "orderLabel", json!("String")),
            mutation("recordPage", "fn_record_page", "page", json!("Int")),
            mutation("recordKind", "fn_record_kind", "kind", json!({ "Enum": "Kind" })),
        ],
        "enums": [{ "name": "Kind", "values": [{ "name": "A" }, { "name": "B" }] }]
    }))
    .expect("schema")
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + fraiseql_core::chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The rows the functions have written.
async fn written(adapter: &PostgresAdapter, schema: &str) -> i64 {
    let rows: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(&format!("SELECT count(*)::bigint AS n FROM {schema}.tb_write"))
        .await
        .unwrap();
    rows[0]["n"].as_i64().unwrap()
}

/// `document`, under `variables`, through each entry a document reaches the executor by:
/// the refusal's text, or the response when it was answered.
async fn answers(
    executor: &Executor,
    document: &str,
    variables: &Value,
) -> Vec<Result<Value, String>> {
    let variables = Some(variables);
    let body = executor
        .execute_operation_body(document, variables, None)
        .await
        .and_then(fraiseql_core::runtime::ResponseBody::into_value);
    [
        executor.execute(document, variables).await,
        body,
        executor.execute_with_security(document, variables, &principal()).await,
    ]
    .into_iter()
    .map(|answer| answer.map_err(|e| e.to_string()))
    .collect()
}

/// Asserts `document` is refused through every entry with a message starting `refusal`,
/// and wrote nothing.
async fn assert_refused(schema: &str, document: &str, variables: Value, refusal: &str) {
    let Some(adapter) = provisioned(schema).await else {
        return;
    };
    let executor = Executor::new(compiled(schema), Arc::clone(&adapter));
    for answer in answers(&executor, document, &variables).await {
        match answer {
            Err(message) => assert!(
                message.contains(refusal),
                "{document} {variables}\n  refused with: {message}\n  expected: {refusal}"
            ),
            Ok(response) => panic!("{document} {variables}\n  answered: {response}"),
        }
    }
    assert_eq!(written(&adapter, schema).await, 0, "{document} {variables} wrote");
}

/// A variable declared at a type its argument cannot take (§ 5.8.5) is refused for its
/// declaration, whatever value it carries.
#[tokio::test]
async fn a_variable_declared_at_a_type_its_argument_cannot_take_is_refused() {
    assert_refused(
        "mutation_variable_types_declared_int",
        "mutation ($orderLabel: Int!) { recordReceipt(orderLabel: $orderLabel) { id } }",
        json!({ "orderLabel": "A-1" }),
        "Variable `$orderLabel` is declared as `Int` but is used for argument `orderLabel` on \
         Mutation.recordReceipt, which has type `String`",
    )
    .await;
    assert_refused(
        "mutation_variable_types_declared_string",
        "mutation ($page: String!) { recordPage(page: $page) { id } }",
        json!({ "page": 1 }),
        "Variable `$page` is declared as `String` but is used for argument `page` on \
         Mutation.recordPage, which has type `Int`",
    )
    .await;
}

/// A value not of its variable's declared type, or none for a non-null variable (§ 6.1.2),
/// is refused for the variable.
#[tokio::test]
async fn a_value_not_of_its_variables_declared_type_is_refused() {
    let document = "mutation ($page: Int!) { recordPage(page: $page) { id } }";
    assert_refused(
        "mutation_variable_types_value",
        document,
        json!({ "page": "1" }),
        "Variable `$page` is declared `Int` but was supplied",
    )
    .await;
    assert_refused(
        "mutation_variable_types_enum",
        "mutation ($k: Kind!) { recordKind(kind: $k) { id } }",
        json!({ "k": "Z" }),
        "Variable `$k`",
    )
    .await;
    assert_refused(
        "mutation_variable_types_missing",
        document,
        json!({}),
        "Variable `$page` is declared `Int!` but no value was supplied",
    )
    .await;
}

/// A document whose second root is at fault writes nothing: every variable is held to its
/// declaration before the first root runs.
#[tokio::test]
async fn a_mutation_whose_second_root_is_at_fault_writes_nothing() {
    assert_refused(
        "mutation_variable_types_second_value",
        "mutation ($l: String!, $p: Int!) { a: recordReceipt(orderLabel: $l) { id } \
         b: recordPage(page: $p) { id } }",
        json!({ "l": "A-1", "p": "x" }),
        "Variable `$p` is declared `Int` but was supplied",
    )
    .await;
    assert_refused(
        "mutation_variable_types_second_declared",
        "mutation ($l: String!, $p: String!) { a: recordReceipt(orderLabel: $l) { id } \
         b: recordPage(page: $p) { id } }",
        json!({ "l": "A-1", "p": "1" }),
        "Variable `$p` is declared as `String` but is used for argument `page` on \
         Mutation.recordPage, which has type `Int`",
    )
    .await;
}

/// Variables declared and supplied at their arguments' types are written, each root once.
#[tokio::test]
async fn variables_of_their_declared_types_are_written() {
    let schema = "mutation_variable_types_served";
    let Some(adapter) = provisioned(schema).await else {
        return;
    };
    let executor = Executor::new(compiled(schema), Arc::clone(&adapter));
    let document = "mutation ($l: String!, $p: Int!) { a: recordReceipt(orderLabel: $l) { id } \
                    b: recordPage(page: $p) { id } }";
    for answer in answers(&executor, document, &json!({ "l": "A-1", "p": 2 })).await {
        let response = answer.unwrap();
        assert!(response.get("errors").is_none(), "{response}");
        assert!(response["data"]["a"]["id"].is_string(), "{response}");
        assert!(response["data"]["b"]["id"].is_string(), "{response}");
    }
    assert_eq!(written(&adapter, schema).await, 6);
}
