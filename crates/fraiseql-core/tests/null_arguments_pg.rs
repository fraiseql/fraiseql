#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — panics are acceptable

//! #1745: an explicit `null` on a list query's own argument is the argument's absence.
//!
//! GraphQL reads `null` on a nullable argument as no value, so `docs(where: null)` is the
//! request `docs` is, whether the `null` is written inline or passed as a variable. A client
//! that builds an optional filter and sends `null` when it is empty was refused with `where
//! clause must be a JSON object`. `orderBy`, `limit` and `offset` read the same way.
//!
//! The list is scoped by a tenant taken from the token, so each case also shows what a `null`
//! does not do: it drops the client's filter, never the server's.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** recreates its own `tv_p1745_doc` table once per run, then only reads it.

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{
        ArgumentDefinition, AutoParams, CompiledSchema, FieldDefinition, FieldType,
        InjectedParamSource, NativeColumn, QueryDefinition, TypeDefinition,
    },
    security::SecurityContext,
};
use serde_json::{Value, json};
use tokio::sync::OnceCell;

const TABLE: &str = "tv_p1745_doc";

/// Tenant `t1` holds `a`, `b` and `c`, stored out of `rank` order; `z` is another tenant's.
const DDL: &[&str] = &[
    "DROP TABLE IF EXISTS tv_p1745_doc",
    "CREATE TABLE tv_p1745_doc (tenant_id text NOT NULL, data jsonb NOT NULL)",
    "INSERT INTO tv_p1745_doc VALUES ('t1', '{\"id\": \"a\", \"rank\": 3}'), ('t1', '{\"id\": \
     \"b\", \"rank\": 1}'), ('t2', '{\"id\": \"z\", \"rank\": 0}'), ('t1', '{\"id\": \"c\", \
     \"rank\": 2}')",
];

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut doc = TypeDefinition::new("Doc", TABLE);
    doc.fields = vec![
        FieldDefinition::new("id", FieldType::String),
        FieldDefinition::new("rank", FieldType::Int),
    ];
    schema.types.push(doc);

    let mut docs = QueryDefinition::new("docs", "Doc").returning_list().with_sql_source(TABLE);
    docs.arguments = vec![
        ArgumentDefinition::optional("where", FieldType::Json),
        ArgumentDefinition::optional("orderBy", FieldType::Json),
        ArgumentDefinition::optional("limit", FieldType::Int),
        ArgumentDefinition::optional("offset", FieldType::Int),
    ];
    docs.auto_params = AutoParams::all();
    docs.inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
    // The tenant is a column of the table, beside the document.
    docs.native_columns
        .insert("tenant_id".to_string(), NativeColumn::nullable("text"));
    schema.queries.push(docs);

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
        request_id:       "req-null-arguments".to_string(),
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

/// The ids `document` answers for tenant `t1`, in the order it answers them.
async fn ids(executor: &Executor, document: &str, variables: Option<&Value>) -> Vec<String> {
    let response = executor
        .execute_with_security(document, variables, &principal("t1"))
        .await
        .unwrap_or_else(|e| panic!("{document}: {e}"));
    response["data"]["docs"]
        .as_array()
        .unwrap_or_else(|| panic!("{document}: {response}"))
        .iter()
        .map(|doc| doc["id"].as_str().unwrap().to_string())
        .collect()
}

/// The same, sorted: the rows a document answers, whatever their order.
async fn id_set(executor: &Executor, document: &str, variables: Option<&Value>) -> Vec<String> {
    let mut ids = ids(executor, document, variables).await;
    ids.sort();
    ids
}

#[tokio::test]
async fn a_null_where_written_inline_reads_as_no_where() {
    let executor = executor().await;

    let rows = id_set(&executor, "{ docs(where: null) { id } }", None).await;

    // The tenant's three rows: every one of them, and not the other tenant's `z`.
    assert_eq!(rows, ["a", "b", "c"]);
    assert_eq!(rows, id_set(&executor, "{ docs { id } }", None).await);
}

#[tokio::test]
async fn a_null_where_passed_as_a_variable_reads_as_no_where() {
    let executor = executor().await;

    let rows = id_set(
        &executor,
        "query($w: JSON) { docs(where: $w) { id } }",
        Some(&json!({ "w": null })),
    )
    .await;

    assert_eq!(rows, ["a", "b", "c"]);
}

/// The `null` is the argument's, not a predicate's: a filter is still applied, and a filter
/// holding a `null` where a predicate belongs is still refused.
#[tokio::test]
async fn a_where_that_is_not_null_still_filters_or_is_refused() {
    let executor = executor().await;

    let filtered = id_set(&executor, r"{ docs(where: {rank: {gt: 1}}) { id } }", None).await;
    assert_eq!(filtered, ["a", "c"]);

    for refused in [
        r"{ docs(where: {_and: [null]}) { id } }",
        r#"{ docs(where: "rank") { id } }"#,
    ] {
        let error = executor
            .execute_with_security(refused, None, &principal("t1"))
            .await
            .expect_err("a malformed filter is refused, never read as no filter");
        assert!(error.to_string().contains("where"), "{refused}: {error}");
    }
}

#[tokio::test]
async fn a_null_order_by_reads_as_no_order_by() {
    let executor = executor().await;

    let rows = id_set(&executor, "{ docs(orderBy: null) { id } }", None).await;
    let by_variable = id_set(
        &executor,
        "query($o: JSON) { docs(orderBy: $o) { id } }",
        Some(&json!({ "o": null })),
    )
    .await;

    assert_eq!(rows, ["a", "b", "c"]);
    assert_eq!(by_variable, ["a", "b", "c"]);
    // And an `orderBy` that is not `null` still orders.
    assert_eq!(
        ids(&executor, "{ docs(orderBy: {rank: ASC}) { id } }", None).await,
        ["b", "c", "a"]
    );
}

#[tokio::test]
async fn a_null_limit_and_a_null_offset_read_as_no_page() {
    let executor = executor().await;

    let rows = id_set(&executor, "{ docs(limit: null, offset: null) { id } }", None).await;

    assert_eq!(rows, ["a", "b", "c"]);
}

/// Every argument at once, as a client that fills none of its optional inputs sends them.
#[tokio::test]
async fn every_null_argument_together_reads_as_the_bare_query() {
    let executor = executor().await;

    let rows = id_set(
        &executor,
        "query($w: JSON, $o: JSON, $l: Int, $f: Int) { docs(where: $w, orderBy: $o, limit: $l, \
         offset: $f) { id } }",
        Some(&json!({ "w": null, "o": null, "l": null, "f": null })),
    )
    .await;

    assert_eq!(rows, ["a", "b", "c"]);
}
