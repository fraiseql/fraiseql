#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! A mutation answers each `Decimal` it returns as the digits its function wrote.
//!
//! The function's response row carries `jsonb` documents: the entity, a failure's
//! `error_detail`, a cascade's entities and a payload's success fields under `result`.
//! Decoded to JSON numbers, a `Decimal` in any of them went through an `f64` and lost every
//! digit past the fifteenth. Each is a string of the digits instead, read at the type the
//! position has: the type the row stamps, the payload's own fields, a cascade entry's
//! `__typename`. A number at a position that is not a `Decimal` stays a number.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test creates and drops its own `decimal_digits_write_*` schema;
//! the shared `app` response contract is created by whichever test gets there first.

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

/// More digits than an `f64` holds.
const WIDE: &str = "12345678901234567.89";

const INVOICE: &str = "jsonb_build_object('id', 'i1', 'total', 12345678901234567.89, \
                       'count', 2, 'lines', jsonb_build_array(jsonb_build_object('sku', 'a', \
                       'amount', 0.010)))";

/// The response contract, the schema and its functions; `None` without a database.
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
        format!(
            "CREATE FUNCTION {schema}.fn_update_invoice(p_fail boolean, p_stamp boolean) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE r app.mutation_response; BEGIN \
             IF p_fail THEN \
               r.succeeded := false; r.state_changed := false; \
               r.error_class := 'validation'; r.message := 'short'; \
               r.entity_type := CASE WHEN p_stamp THEN 'InsufficientFunds' END; \
               r.error_detail := jsonb_build_object('shortfall', 98765432109876543.21, \
                 'attempts', 3); \
             ELSE \
               r.succeeded := true; r.state_changed := true; r.message := ''; \
               r.entity_type := CASE WHEN p_stamp THEN 'Invoice' END; \
               r.entity := {INVOICE}; \
             END IF; \
             RETURN r; END; $$"
        ),
        format!(
            "CREATE FUNCTION {schema}.fn_pay_invoice() \
             RETURNS TABLE ({response_columns}, result JSONB) LANGUAGE sql AS $$ \
             SELECT true, true, NULL::app.mutation_error_class, NULL, NULL::smallint, '', \
               NULL::uuid, 'Invoice', {INVOICE}, ARRAY['total'], \
               jsonb_build_object('updated', jsonb_build_array(jsonb_build_object(\
                 '__typename', 'Line', 'id', 'l1', 'operation', 'UPDATED', \
                 'entity', jsonb_build_object('sku', 'a', 'amount', 0.010))), \
                 'deleted', '[]'::jsonb), \
               NULL::jsonb, NULL::jsonb, \
               jsonb_build_object('balance', 12345678901234567.89, 'paid_lines', 1) $$"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Arc::new(adapter))
}

fn compiled(schema: &str) -> CompiledSchema {
    let field = |name: &str, field_type: Value| json!({ "name": name, "field_type": field_type });
    serde_json::from_value(json!({
        "types": [
            {
                "name": "Invoice",
                "sql_source": format!("{schema}.v_invoice"),
                "implements": ["CascadeNode"],
                "fields": [
                    field("id", json!("ID")),
                    field("total", json!("Decimal")),
                    field("count", json!("Int")),
                    field("lines", json!({ "List": { "item": { "Object": "Line" } } })),
                ]
            },
            {
                "name": "Line",
                "sql_source": format!("{schema}.v_line"),
                "implements": ["CascadeNode"],
                "fields": [field("sku", json!("String")), field("amount", json!("Decimal"))]
            },
            {
                "name": "InsufficientFunds",
                "sql_source": "",
                "is_error": true,
                "fields": [
                    field("message", json!("String")),
                    field("shortfall", json!("Decimal")),
                    field("attempts", json!("Int")),
                ]
            },
            {
                "name": "PayInvoicePayload",
                "sql_source": "",
                "fields": [
                    field("entity", json!({ "Object": "Invoice" })),
                    field("cascade", json!({ "Object": "CascadeUpdates" })),
                    field("updatedFields", json!({ "List": { "item": "String" } })),
                    field("balance", json!("Decimal")),
                    field("paidLines", json!("Int")),
                ]
            },
            {
                "name": "CascadeUpdates",
                "sql_source": "",
                "fields": [field("updated", json!({ "List": { "item": { "Object": "UpdatedEntity" } } }))]
            },
            {
                "name": "UpdatedEntity",
                "sql_source": "",
                "fields": [
                    field("id", json!("ID")),
                    field("operation", json!("String")),
                    field("entity", json!({ "Interface": "CascadeNode" })),
                ]
            }
        ],
        "interfaces": [{ "name": "CascadeNode", "fields": [] }],
        "unions": [
            { "name": "InvoiceResult", "member_types": ["Invoice", "InsufficientFunds"] }
        ],
        "mutations": [
            {
                "name": "updateInvoice",
                "return_type": "InvoiceResult",
                "sql_source": format!("{schema}.fn_update_invoice"),
                "operation": { "Update": { "table": "tb_invoice" } },
                "changelog": false,
                "arguments": [
                    { "name": "fail", "arg_type": "Boolean", "nullable": false },
                    { "name": "stamp", "arg_type": "Boolean", "nullable": false }
                ]
            },
            {
                "name": "payInvoice",
                "return_type": "PayInvoicePayload",
                "sql_source": format!("{schema}.fn_pay_invoice"),
                "cascade": true,
                "changelog": false,
                "operation": { "Update": { "table": "tb_invoice" } }
            }
        ]
    }))
    .expect("schema")
}

async fn answer(adapter: Arc<PostgresAdapter>, schema: &str, document: &str) -> Value {
    let response = Executor::new(compiled(schema), adapter).execute(document, None).await.unwrap();
    assert!(response.get("errors").is_none(), "{response}");
    response["data"].clone()
}

#[tokio::test]
async fn a_mutation_entity_keeps_its_digits_at_the_type_the_row_stamps() {
    let schema = "decimal_digits_write_entity";
    let Some(adapter) = provisioned(schema).await else {
        return;
    };
    // Unstamped, the entity is the one type a success of the union can be.
    for stamp in [true, false] {
        let data = answer(
            Arc::clone(&adapter),
            schema,
            &format!(
                "mutation {{ updateInvoice(fail: false, stamp: {stamp}) \
                 {{ ... on Invoice {{ total count lines {{ amount }} }} }} }}"
            ),
        )
        .await;
        assert_eq!(
            data["updateInvoice"],
            json!({ "total": WIDE, "count": 2, "lines": [{ "amount": "0.010" }] }),
            "stamped: {stamp}"
        );
    }
}

#[tokio::test]
async fn a_failure_keeps_its_digits_at_the_error_type_it_stamps() {
    let schema = "decimal_digits_write_failure";
    let Some(adapter) = provisioned(schema).await else {
        return;
    };
    // Unstamped, the failure is the union's one error member.
    for stamp in [true, false] {
        let data = answer(
            Arc::clone(&adapter),
            schema,
            &format!(
                "mutation {{ updateInvoice(fail: true, stamp: {stamp}) \
                 {{ ... on InsufficientFunds {{ shortfall attempts }} }} }}"
            ),
        )
        .await;
        assert_eq!(
            data["updateInvoice"],
            json!({ "shortfall": "98765432109876543.21", "attempts": 3, "status": "validation" }),
            "stamped: {stamp}"
        );
    }
}

#[tokio::test]
async fn a_payload_keeps_its_digits_in_its_entity_its_cascade_and_its_success_fields() {
    let schema = "decimal_digits_write_payload";
    let Some(adapter) = provisioned(schema).await else {
        return;
    };
    let data = answer(
        adapter,
        schema,
        "mutation { payInvoice { entity { total } balance paidLines \
         cascade { updated { entity { ... on Line { amount } } } } } }",
    )
    .await;
    assert_eq!(
        data["payInvoice"],
        json!({
            "entity": { "total": WIDE },
            "balance": WIDE,
            "paidLines": 1,
            "cascade": { "updated": [{ "entity": { "amount": "0.010" } }] }
        })
    );
}
