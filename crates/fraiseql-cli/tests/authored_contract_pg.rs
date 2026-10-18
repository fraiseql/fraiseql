//! What an author declares in `schema.json` is what the runtime serves, end to end.
//!
//! Each test authors a schema the way an SDK exports it, compiles it with the real
//! `fraiseql compile`, and runs operations through `fraiseql query` (the real `Executor` and
//! `PostgresAdapter`) against rows stored in PostgreSQL. The declaration is the contract: a
//! shape the compiler accepts must reach the response as declared, not tightened or loosened
//! on the way.
//!
//! - #1547: a list argument reaches an array parameter of the function as the list it is.
//! - #1586: a list declared with nullable items (`[Float]`) serves a `null` item. A list declared
//!   with non-null items (`[Float!]`) still nulls the list on one (§ 6.4.4, #1522).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `authored_contract` schema → run
//! `--test-threads=1`.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code

use std::{path::Path, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_postgres::NoTls;

const SCHEMA: &str = "authored_contract";

async fn execute(url: &str, sql: &str) {
    let (client, conn) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client.batch_execute(sql).await.unwrap();
}

/// Compile `schema` into `dir`; the compiled artifact's path.
fn compile(dir: &Path, schema: &Value) -> std::path::PathBuf {
    let input = dir.join("schema.json");
    std::fs::write(&input, schema.to_string()).unwrap();
    let out = dir.join("schema.compiled.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "compile",
            input.to_str().unwrap(),
            "--skip-hash",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "compile failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

/// Run `operation` through `fraiseql query`: the response, whatever its exit code.
fn query(compiled: &Path, url: &str, operation: &str, variables: Option<&Value>) -> Value {
    let mut args = vec![
        "query".to_string(),
        operation.to_string(),
        "--schema".to_string(),
        compiled.to_str().unwrap().to_string(),
        "--database".to_string(),
        url.to_string(),
    ];
    if let Some(variables) = variables {
        args.extend(["--variables".to_string(), variables.to_string()]);
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli")).args(&args).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "`fraiseql query` printed no JSON response ({e}):\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

// ── #1586: list item nullability ────────────────────────────────────────────────────

/// `Report.monthlyValues: [Float]` (a month with no data is `null`),
/// `Report.strictValues: [Float!]`, and their nested forms `grid: [[Float]]` and
/// `strictGrid: [[Float!]!]`, over one row holding a `null` in each. The row stores its keys
/// in snake case, as the compiler's default naming convention reads them.
async fn reports(url: &str, dir: &Path) -> std::path::PathBuf {
    execute(
        url,
        &format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tb_report (id uuid PRIMARY KEY, data jsonb NOT NULL);
             INSERT INTO {SCHEMA}.tb_report VALUES ('00000000-0000-0000-0000-000000001586',
                 '{{\"id\": \"00000000-0000-0000-0000-000000001586\",
                    \"monthly_values\": [12.0, null, 7.5], \"strict_values\": [1.0, null],
                    \"grid\": [[1.0, null], null, [2.5]], \"strict_grid\": [[1.0], [null]]}}');
             CREATE VIEW {SCHEMA}.v_report AS SELECT id, data FROM {SCHEMA}.tb_report;"
        ),
    )
    .await;
    compile(
        dir,
        &json!({
            "types": [{
                "name": "Report",
                "sql_source": format!("{SCHEMA}.v_report"),
                "fields": [
                    { "name": "id", "type": "ID", "nullable": false },
                    { "name": "monthlyValues", "type": "[Float]", "nullable": true },
                    { "name": "strictValues", "type": "[Float!]", "nullable": true },
                    { "name": "grid", "type": "[[Float]]", "nullable": true },
                    { "name": "strictGrid", "type": "[[Float!]!]", "nullable": true }
                ]
            }],
            "queries": [{
                "name": "reports", "return_type": "Report", "returns_list": true,
                "nullable": false, "sql_source": format!("{SCHEMA}.v_report")
            }]
        }),
    )
}

#[tokio::test]
async fn a_list_declared_with_nullable_items_serves_a_null_item() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1586: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = reports(&url, dir.path()).await;

    let response = query(&compiled, &url, "{ reports { monthlyValues } }", None);

    assert_eq!(
        response,
        json!({ "data": { "reports": [{ "monthlyValues": [12.0, null, 7.5] }] } }),
        "a null item of a `[Float]` list is a value the field may take"
    );
}

#[tokio::test]
async fn a_list_declared_with_non_null_items_is_nulled_by_a_null_item() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1586: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = reports(&url, dir.path()).await;

    let response = query(&compiled, &url, "{ reports { strictValues } }", None);

    assert_eq!(response["data"], json!({ "reports": [{ "strictValues": null }] }));
    assert_eq!(response["errors"][0]["path"], json!(["reports", 0, "strictValues", 1]));
}

#[tokio::test]
async fn the_published_schema_carries_the_declared_item_nullability() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1586: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = reports(&url, dir.path()).await;

    let response = query(
        &compiled,
        &url,
        "{ __type(name: \"Report\") { fields { name type { kind ofType { kind name ofType { name } } } } } }",
        None,
    );

    let field = |name: &str| {
        response["data"]["__type"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("no field {name}: {response}"))["type"]
            .clone()
    };
    let monthly = field("monthlyValues");
    assert_eq!(
        (&monthly["kind"], &monthly["ofType"]["kind"], &monthly["ofType"]["name"]),
        (&json!("LIST"), &json!("SCALAR"), &json!("Float")),
        "`[Float]`: the items are nullable: {monthly}"
    );
    let strict = field("strictValues");
    assert_eq!(
        (&strict["kind"], &strict["ofType"]["kind"], &strict["ofType"]["ofType"]["name"]),
        (&json!("LIST"), &json!("NON_NULL"), &json!("Float")),
        "`[Float!]`: {strict}"
    );
}

/// A nested list carries its items' nullability at every level, as declared: published,
/// and served (a `null` inner item of `[[Float!]!]` propagates through the non-null inner
/// list and item to the field).
#[tokio::test]
async fn a_nested_list_is_published_and_served_as_declared() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1586: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = reports(&url, dir.path()).await;

    let served = query(&compiled, &url, "{ reports { grid strictGrid } }", None);
    let published = query(
        &compiled,
        &url,
        "{ __type(name: \"Report\") { fields { name type { kind name ofType { kind name \
         ofType { kind name ofType { kind name ofType { kind name } } } } } } } }",
        None,
    );

    assert_eq!(
        served["data"],
        json!({ "reports": [{ "grid": [[1.0, null], null, [2.5]], "strictGrid": null }] }),
        "{served}"
    );
    assert_eq!(
        served["errors"][0]["path"],
        json!(["reports", 0, "strictGrid", 1, 0]),
        "{served}"
    );
    let kinds = |name: &str| {
        let field = published["data"]["__type"]["fields"]
            .as_array()
            .unwrap_or_else(|| panic!("{published}"))
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("no field {name}: {published}"))
            .clone();
        let mut kinds = Vec::new();
        let mut at = &field["type"];
        while !at.is_null() {
            kinds.push(at["name"].as_str().map_or_else(|| at["kind"].to_string(), str::to_string));
            at = &at["ofType"];
        }
        kinds.join(" ")
    };
    assert_eq!(kinds("grid"), r#""LIST" "LIST" Float"#);
    assert_eq!(kinds("strictGrid"), r#""LIST" "NON_NULL" "LIST" "NON_NULL" Float"#);
}

// ── #1580: enum member values ───────────────────────────────────────────────────────

/// `Order.status: OrderStatus` (`PENDING = "pending"`, `SHIPPED = "shipped"`, `LOST` with no
/// value), over rows storing the values, and a mutation whose function stores what it is
/// given. The schema is what the Python SDK exports for those declarations.
async fn orders(url: &str, dir: &Path) -> std::path::PathBuf {
    execute(
        url,
        &format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE SCHEMA IF NOT EXISTS app;
             DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',
             'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed
             BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status
             SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB,
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB);
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             CREATE TABLE {SCHEMA}.tb_order (id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
                 status text);
             INSERT INTO {SCHEMA}.tb_order VALUES
                 ('00000000-0000-0000-0000-000000000001', 'shipped'),
                 ('00000000-0000-0000-0000-000000000002', 'pending'),
                 ('00000000-0000-0000-0000-000000000003', 'LOST');
             CREATE VIEW {SCHEMA}.v_order AS SELECT id,
                 jsonb_build_object('id', id, 'status', status) AS data FROM {SCHEMA}.tb_order;
             CREATE FUNCTION {SCHEMA}.fn_record_status(p_status text)
               RETURNS app.mutation_response LANGUAGE plpgsql AS $$
               DECLARE v app.mutation_response; new_id uuid; BEGIN
                 INSERT INTO {SCHEMA}.tb_order (status) VALUES (p_status) RETURNING id INTO new_id;
                 v.succeeded := true; v.state_changed := true; v.entity_type := 'Order';
                 v.entity_id := new_id;
                 v.entity := jsonb_build_object('id', new_id, 'status', p_status);
                 RETURN v; END; $$;"
        ),
    )
    .await;
    compile(
        dir,
        &json!({
            "enums": [{
                "name": "OrderStatus",
                "values": [
                    { "name": "PENDING", "value": "pending" },
                    { "name": "SHIPPED", "value": "shipped" },
                    { "name": "LOST" }
                ]
            }],
            "types": [{
                "name": "Order",
                "sql_source": format!("{SCHEMA}.v_order"),
                "fields": [
                    { "name": "id", "type": "ID", "nullable": false },
                    { "name": "status", "type": "OrderStatus", "nullable": true }
                ]
            }],
            "queries": [{
                "name": "orders", "return_type": "Order", "returns_list": true,
                "nullable": false, "sql_source": format!("{SCHEMA}.v_order")
            }],
            "mutations": [{
                "name": "recordStatus", "return_type": "Order", "returns_list": false,
                "nullable": false, "sql_source": format!("{SCHEMA}.fn_record_status"),
                "operation": "CREATE",
                "arguments": [{ "name": "status", "type": "OrderStatus", "nullable": false }]
            }]
        }),
    )
}

/// The ids `{ orders(where: …) { id } }` answers, in id order.
fn ids(response: &Value) -> Vec<String> {
    let mut ids: Vec<String> = response["data"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|o| o["id"].as_str().unwrap()[35..].to_string())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn a_valued_enum_member_filters_the_rows_storing_its_value() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1580: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = orders(&url, dir.path()).await;

    let eq = query(&compiled, &url, "{ orders(where: {status: {eq: SHIPPED}}) { id } }", None);
    let within = query(
        &compiled,
        &url,
        "{ orders(where: {status: {in: [PENDING, LOST]}}) { id } }",
        None,
    );
    let variable = query(
        &compiled,
        &url,
        "query($w: OrderWhereInput) { orders(where: $w) { id } }",
        Some(&json!({ "w": { "status": { "eq": "SHIPPED" } } })),
    );

    assert_eq!(ids(&eq), ["1"], "{eq}");
    assert_eq!(ids(&within), ["2", "3"], "a member with no value is stored as its name");
    assert_eq!(ids(&variable), ["1"], "{variable}");
}

#[tokio::test]
async fn a_stored_enum_value_is_served_as_its_member_name() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1580: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = orders(&url, dir.path()).await;

    let response = query(&compiled, &url, "{ orders { id status } }", None);

    let mut served: Vec<(String, Value)> = response["data"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|o| (o["id"].as_str().unwrap()[35..].to_string(), o["status"].clone()))
        .collect();
    served.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        served,
        [
            ("1".to_string(), json!("SHIPPED")),
            ("2".to_string(), json!("PENDING")),
            ("3".to_string(), json!("LOST")),
        ],
        "{response}"
    );
}

#[tokio::test]
async fn a_valued_enum_member_reaches_the_function_as_its_value() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1580: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = orders(&url, dir.path()).await;

    let response =
        query(&compiled, &url, "mutation { recordStatus(status: SHIPPED) { status } }", None);

    assert_eq!(response["data"]["recordStatus"]["status"], json!("SHIPPED"), "{response}");
    let (client, conn) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let stored: Vec<String> = client
        .query(&format!("SELECT status FROM {SCHEMA}.tb_order ORDER BY status"), &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(stored, ["LOST", "pending", "shipped", "shipped"], "the function got `shipped`");
}

// ── #1547: a list argument binds to an array parameter ──────────────────────────────

/// `tag(labels: [String!]!, scores: [Float!]!)` over a function taking `text[]` and
/// `float8[]`, which stores what it is given.
async fn taggings(url: &str, dir: &Path) -> std::path::PathBuf {
    execute(
        url,
        &format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE SCHEMA IF NOT EXISTS app;
             DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',
             'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed
             BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status
             SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB,
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB);
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             CREATE TABLE {SCHEMA}.tb_tagging (id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
                 labels text[], scores float8[]);
             CREATE VIEW {SCHEMA}.v_tagging AS SELECT id, jsonb_build_object('id', id,
                 'labels', labels, 'scores', scores) AS data FROM {SCHEMA}.tb_tagging;
             CREATE FUNCTION {SCHEMA}.fn_tag(p_labels text[], p_scores float8[])
               RETURNS app.mutation_response LANGUAGE plpgsql AS $$
               DECLARE v app.mutation_response; new_id uuid; BEGIN
                 INSERT INTO {SCHEMA}.tb_tagging (labels, scores) VALUES (p_labels, p_scores)
                   RETURNING id INTO new_id;
                 v.succeeded := true; v.state_changed := true; v.entity_type := 'Tagging';
                 v.entity_id := new_id;
                 v.entity := jsonb_build_object('id', new_id, 'labels', p_labels,
                                                'scores', p_scores);
                 RETURN v; END; $$;"
        ),
    )
    .await;
    compile(
        dir,
        &json!({
            "types": [{
                "name": "Tagging",
                "sql_source": format!("{SCHEMA}.v_tagging"),
                "fields": [
                    { "name": "id", "type": "ID", "nullable": false },
                    { "name": "labels", "type": "[String!]", "nullable": false },
                    { "name": "scores", "type": "[Float!]", "nullable": false }
                ]
            }],
            "queries": [{
                "name": "taggings", "return_type": "Tagging", "returns_list": true,
                "nullable": false, "sql_source": format!("{SCHEMA}.v_tagging")
            }],
            "mutations": [{
                "name": "tag", "return_type": "Tagging", "returns_list": false,
                "nullable": false, "sql_source": format!("{SCHEMA}.fn_tag"),
                "operation": "CREATE",
                "arguments": [
                    { "name": "labels", "type": "[String!]", "nullable": false },
                    { "name": "scores", "type": "[Float!]", "nullable": false }
                ]
            }]
        }),
    )
}

#[tokio::test]
async fn a_list_argument_reaches_an_array_parameter() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1547: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = taggings(&url, dir.path()).await;

    let inline = query(
        &compiled,
        &url,
        r#"mutation { tag(labels: ["a,b", "say \"hi\""], scores: [1.5, 2.5]) { labels scores } }"#,
        None,
    );
    let variables = query(
        &compiled,
        &url,
        "mutation($l: [String!]!, $s: [Float!]!) { tag(labels: $l, scores: $s) { labels } }",
        Some(&json!({ "l": ["{x}", "NULL"], "s": [] })),
    );

    assert_eq!(inline["data"]["tag"]["labels"], json!(["a,b", "say \"hi\""]), "{inline}");
    assert_eq!(variables["data"]["tag"]["labels"], json!(["{x}", "NULL"]), "{variables}");
    let (client, conn) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let stored: Vec<(Vec<String>, Vec<f64>)> = client
        .query(&format!("SELECT labels, scores FROM {SCHEMA}.tb_tagging ORDER BY labels"), &[])
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    assert_eq!(
        stored,
        [
            (vec!["a,b".to_string(), "say \"hi\"".to_string()], vec![1.5, 2.5]),
            (vec!["{x}".to_string(), "NULL".to_string()], vec![]),
        ]
    );
}
