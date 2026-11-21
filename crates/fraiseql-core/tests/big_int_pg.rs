//! A `BigInt` field reads and compares a `bigint` column exactly.
//!
//! GraphQL's `Int` is a signed 32-bit integer, so a `bigint` past that range was a field
//! error under one. A `BigInt` serves the column as the string of its digits: a JSON number
//! past 2^53 is not read exactly by a `JavaScript` client. The fixture's rows sit on either
//! side of 2^53, where an `f64` stops holding every integer, and at the ends of the 64-bit
//! range, so a read or a filter that went through an `f64` anywhere would answer with a
//! neighbour's digits or match a neighbour's row.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test owns a uniquely named table and view.

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::Executor,
    schema::{CompiledSchema, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

/// The fixture's `bigint`s by row id: 2^53, 2^53 + 1, the first integer an `f64` cannot
/// hold, and the two ends of the signed 64-bit range.
const ROWS: [(i64, &str); 4] = [
    (1, "9007199254740992"),
    (2, "9007199254740993"),
    (3, "-9223372036854775808"),
    (4, "9223372036854775807"),
];

/// `Ledger.balance` is a `BigInt` its view writes from a `bigint` column, and
/// `Ledger.inner.balance` embeds the same column.
fn schema(view: &str) -> CompiledSchema {
    let ledger = TestTypeBuilder::new("Ledger", view)
        .with_simple_field("id", FieldType::Int)
        .with_simple_field("balance", FieldType::parse("BigInt"))
        .with_simple_field("inner", FieldType::Object("Inner".to_string()))
        .build();
    let inner = TestTypeBuilder::new("Inner", view)
        .with_simple_field("balance", FieldType::parse("BigInt"))
        .build();
    let mut ledgers = TestQueryBuilder::new("ledgers", "Ledger")
        .returns_list(true)
        .with_sql_source(view)
        .build();
    ledgers.auto_params.has_where = true;
    TestSchemaBuilder::new()
        .with_type(ledger)
        .with_type(inner)
        .with_query(ledgers)
        .build()
}

async fn executor(test: &str) -> Option<Executor> {
    let table = format!("tb_big_int_{test}");
    let view = format!("v_big_int_{test}");
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    let values: Vec<String> = ROWS.iter().map(|(id, n)| format!("({id}, {n})")).collect();
    for stmt in [
        format!("DROP VIEW IF EXISTS {view}"),
        format!("DROP TABLE IF EXISTS {table}"),
        format!("CREATE TABLE {table} (id int, balance bigint)"),
        format!("INSERT INTO {table} VALUES {}", values.join(", ")),
        format!(
            "CREATE VIEW {view} AS SELECT id, jsonb_build_object('id', id, 'balance', balance, \
             'inner', jsonb_build_object('balance', balance)) AS data FROM {table}"
        ),
    ] {
        adapter.execute_raw_query(&stmt).await.expect("provision the fixture");
    }
    Some(Executor::new(schema(&view), Arc::new(adapter)))
}

/// The `data.ledgers` rows of a response that carries no errors, sorted by `id`.
fn rows(response: &Value) -> Vec<Value> {
    assert_eq!(response.get("errors"), None, "{response}");
    let mut rows = response["data"]["ledgers"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .clone();
    rows.sort_by_key(|row| row["id"].as_i64());
    rows
}

/// The ids of the rows a response holds, sorted.
fn ids(response: &Value) -> Vec<i64> {
    rows(response).iter().map(|row| row["id"].as_i64().unwrap()).collect()
}

/// Each `bigint` is served as the string of its digits, at the root and embedded, every
/// digit the column holds.
#[tokio::test]
async fn a_big_int_is_served_as_the_exact_digits_of_its_column() {
    let Some(exec) = executor("served").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let response = exec
        .execute("{ ledgers { id balance inner { balance } } }", None)
        .await
        .unwrap();
    let expected: Vec<Value> = ROWS
        .iter()
        .map(|(id, n)| json!({"id": id, "balance": n, "inner": {"balance": n}}))
        .collect();
    assert_eq!(rows(&response), expected);
}

/// A selection of the `BigInt` alone, through the body a transport sends: the raw copy
/// answers a read whose rows need no shaping in Rust, and would serve the number the
/// column holds. The same fixture's `id` alone is answered raw, so the copy is reachable.
#[tokio::test]
async fn a_selection_of_the_big_int_alone_is_served_as_its_digits() {
    let Some(exec) = executor("alone").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let body = |query: &'static str| {
        let exec = &exec;
        async move {
            let body = exec.execute_operation_body(query, None, None).await.unwrap();
            let raw = body.is_raw();
            (serde_json::from_slice::<Value>(&body.into_bytes().unwrap()).unwrap(), raw)
        }
    };
    let (_, ids_raw) = body("{ ledgers { id } }").await;
    assert!(ids_raw, "an `id` read of this fixture is answered raw");

    let (response, _) = body("{ ledgers { balance } }").await;
    assert_eq!(response.get("errors"), None, "{response}");
    let mut served: Vec<&str> = response["data"]["ledgers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["balance"].as_str().unwrap_or_else(|| panic!("{response}")))
        .collect();
    served.sort_unstable();
    let mut expected: Vec<&str> = ROWS.iter().map(|(_, n)| *n).collect();
    expected.sort_unstable();
    assert_eq!(served, expected);
}

/// A filter compares the column's integer, not an `f64`'s: `eq` 2^53 + 1 matches its row
/// and not 2^53's, which an `f64` reads as the same number. The operand is the digits as a
/// string literal, a JSON integer variable, or a string variable.
#[tokio::test]
async fn a_filter_on_a_big_int_matches_the_exact_integer() {
    let Some(exec) = executor("filtered").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let literal = exec
        .execute(r#"{ ledgers(where: {balance: {eq: "9007199254740993"}}) { id } }"#, None)
        .await
        .unwrap();
    assert_eq!(ids(&literal), [2]);

    let query = "query Q($n: BigInt!) { ledgers(where: {balance: {eq: $n}}) { id } }";
    for supplied in [json!(9_007_199_254_740_993_i64), json!("9007199254740993")] {
        let response = exec.execute(query, Some(&json!({"n": supplied}))).await.unwrap();
        assert_eq!(ids(&response), [2], "{supplied}");
    }

    let range = exec
        .execute(r#"{ ledgers(where: {balance: {gt: "9007199254740992"}}) { id } }"#, None)
        .await
        .unwrap();
    assert_eq!(ids(&range), [2, 4]);

    // As text `"-9223372036854775808"` sorts after `"-1"`; as an integer it is below it.
    let signed = exec
        .execute(r#"{ ledgers(where: {balance: {gte: "-1"}}) { id } }"#, None)
        .await
        .unwrap();
    assert_eq!(ids(&signed), [1, 2, 4]);

    let ends = exec
        .execute(
            r#"{ ledgers(where: {balance: {in: ["-9223372036854775808", "9223372036854775807"]}}) { id } }"#,
            None,
        )
        .await
        .unwrap();
    assert_eq!(ids(&ends), [3, 4]);
}

/// A variable that is not the digits of a signed 64-bit integer is refused before the
/// database sees it, naming the type and never the value.
#[tokio::test]
async fn a_filter_variable_outside_the_big_int_range_is_refused() {
    let Some(exec) = executor("refused").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let refusal = |refused: Result<Value, fraiseql_core::error::FraiseQLError>, what: &str| {
        let message = match refused {
            Err(error) => error.to_string(),
            Ok(response) => response["errors"][0]["message"]
                .as_str()
                .unwrap_or_else(|| panic!("{what} was served: {response}"))
                .to_string(),
        };
        assert!(message.contains("BigInt"), "{what}: {message}");
        assert!(!message.contains("12abc"), "the refusal quotes the value: {message}");
    };
    let query = "query Q($n: BigInt!) { ledgers(where: {balance: {eq: $n}}) { id } }";
    for supplied in [json!("9223372036854775808"), json!(1.5), json!("12abc")] {
        refusal(exec.execute(query, Some(&json!({"n": supplied}))).await, &supplied.to_string());
    }
}

/// Introspection publishes `BigInt` as a scalar, and the field as one.
#[tokio::test]
async fn introspection_publishes_big_int_as_a_scalar() {
    let Some(exec) = executor("introspected").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let response = exec
        .execute(
            r#"{ __type(name: "BigInt") { kind name }
                 ledger: __type(name: "Ledger") { fields { name type { ofType { kind name } } } } }"#,
            None,
        )
        .await
        .unwrap();
    assert_eq!(response.get("errors"), None, "{response}");
    assert_eq!(response["data"]["__type"], json!({"kind": "SCALAR", "name": "BigInt"}));
    let balance = response["data"]["ledger"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["name"] == "balance")
        .unwrap_or_else(|| panic!("{response}"));
    assert_eq!(balance["type"]["ofType"], json!({"kind": "SCALAR", "name": "BigInt"}));
}
