//! #1530: every scalar the engine knows compiles as a scalar and is served as its value.
//!
//! The SDKs write a field's type as its GraphQL name. The compiler resolved built-in names
//! (`String`, `Date`, …), declared types and declared custom scalars, and fell back to an
//! **object reference** for anything else, which took in all 51 of the engine's own rich
//! scalars (`Email`, `Hostname`, `IPAddress`, …). The field compiled as
//! `{"Object": "Hostname"}`, introspection called it `OBJECT`, and the server answered it
//! with no value (`{}` on 2.15.0, `null` on 2.16.0) under a `200`.
//!
//! The table is generated from `RICH_SCALARS` itself, so a scalar added there later is
//! covered here without anyone remembering to.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1530_scalar` schema → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{CompiledSchema, FieldType, RICH_SCALARS},
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use tempfile::TempDir;

const SCHEMA: &str = "p1530_scalar";

/// The field each scalar is stored under: lowercase letters only, so the document reads the
/// same under either naming convention and the stored key is the field name (a digit would be
/// split off as its own snake-case word: `IPv4` → `ipv_4`).
fn field_name(scalar: &str) -> String {
    let letters: String = scalar
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_digit() {
                char::from(b'a' + (c as u8 - b'0'))
            } else {
                c
            }
        })
        .collect();
    format!("f{letters}")
}

/// The value stored for each scalar, and expected back.
fn value(scalar: &str) -> Value {
    json!(format!("value of {scalar}"))
}

/// The `schema.json` an SDK writes: one type with a field per rich scalar, plus the issue's
/// own `Host` (`hostname: Hostname`, `address: IPAddress`).
fn schema_json() -> Value {
    let probe_fields: Vec<Value> = RICH_SCALARS
        .iter()
        .map(|s| json!({ "name": field_name(s), "type": s, "nullable": false }))
        .collect();
    json!({
        "types": [
            {
                "name": "Probe",
                "fields": probe_fields,
                "sql_source": format!("{SCHEMA}.v_probe"),
                "is_input": false
            },
            {
                "name": "Host",
                "fields": [
                    { "name": "name", "type": "String", "nullable": false },
                    { "name": "hostname", "type": "Hostname", "nullable": false },
                    { "name": "address", "type": "IPAddress", "nullable": false }
                ],
                "sql_source": format!("{SCHEMA}.v_host"),
                "is_input": false
            }
        ],
        "queries": [
            {
                "name": "probes", "return_type": "Probe", "returns_list": true,
                "sql_source": format!("{SCHEMA}.v_probe"), "nullable": false, "arguments": []
            },
            {
                "name": "hosts", "return_type": "Host", "returns_list": true,
                "sql_source": format!("{SCHEMA}.v_host"), "nullable": false, "arguments": []
            }
        ],
        "mutations": [],
        "subscriptions": [],
        "version": "2.0.0"
    })
}

/// Compile the document through the real compiler, as `fraiseql compile schema.json` does.
async fn compile() -> CompiledSchema {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.json");
    std::fs::write(&path, schema_json().to_string()).unwrap();
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        "[project]\nname = \"p1530\"\n\n[fraiseql]\nschema_file = \"schema.json\"\n",
    )
    .unwrap();
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(path.to_str().unwrap())
    })
    .await
    .expect("the document must compile");
    let mut schema = CompiledSchema::from_json(&compiled.schema.to_json().unwrap(), false).unwrap();
    schema.build_indexes();
    schema
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> =
        adapter.execute_raw_query(sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// The real server over the compiled document, and a client for `/graphql`.
async fn serve() -> Option<impl Fn(&'static str) -> futures::future::BoxFuture<'static, Value>> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    let probe: serde_json::Map<String, Value> =
        RICH_SCALARS.iter().map(|s| (field_name(s), value(s))).collect();
    exec(&adapter, &format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")).await;
    exec(&adapter, &format!("CREATE SCHEMA {SCHEMA}")).await;
    exec(
        &adapter,
        &format!(
            "CREATE VIEW {SCHEMA}.v_probe AS SELECT 1 AS id, '{}'::jsonb AS data",
            Value::Object(probe).to_string().replace('\'', "''")
        ),
    )
    .await;
    exec(
        &adapter,
        &format!(
            "CREATE VIEW {SCHEMA}.v_host AS SELECT 1 AS id, jsonb_build_object('name', 'a', \
             'hostname', 'a.example.com', 'address', '10.0.0.1') AS data"
        ),
    )
    .await;

    let config = ServerConfig {
        cors_enabled: false,
        database_url: url,
        introspection_enabled: true,
        introspection_require_auth: false,
        ..ServerConfig::default()
    };
    let server = Box::pin(Server::new(config, compile().await, adapter, None))
        .await
        .expect("Server::new");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { server.serve_on_listener(listener, std::future::pending()).await });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let http = reqwest::Client::new();
    Some(move |query: &'static str| -> futures::future::BoxFuture<'static, Value> {
        let http = http.clone();
        Box::pin(async move {
            http.post(format!("http://127.0.0.1:{port}/graphql"))
                .json(&json!({ "query": query }))
                .send()
                .await
                .expect("graphql request")
                .json()
                .await
                .expect("JSON body")
        })
    })
}

/// The issue's own reproduction: `Hostname` and `IPAddress` are served as their strings.
#[tokio::test]
async fn the_issues_host_is_served_with_its_values() {
    let Some(graphql) = serve().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let body = graphql("{ hosts { name hostname address } }").await;
    assert_eq!(
        body.pointer("/data/hosts/0"),
        Some(&json!({ "name": "a", "hostname": "a.example.com", "address": "10.0.0.1" })),
        "{body}"
    );
}

/// Every rich scalar: compiled as that scalar, reported `SCALAR` by introspection, served.
#[tokio::test]
async fn every_rich_scalar_compiles_introspects_and_serves_as_a_scalar() {
    let schema = compile().await;
    let probe = schema.find_type("Probe").expect("Probe compiled");
    let mut wrong = Vec::new();
    for scalar in RICH_SCALARS {
        let field = probe.fields.iter().find(|f| f.name == field_name(scalar)).unwrap();
        if field.field_type != FieldType::Scalar((*scalar).to_string()) {
            wrong.push(format!("{scalar}: {:?}", field.field_type));
        }
    }
    assert!(wrong.is_empty(), "compiled as something other than the scalar: {wrong:#?}");

    let Some(graphql) = serve().await else {
        eprintln!("skipping the served half: DATABASE_URL not set");
        return;
    };
    let introspected = graphql(
        "{ __type(name: \"Probe\") { fields { name type { kind name ofType { kind name } } } } }",
    )
    .await;
    let fields = introspected
        .pointer("/data/__type/fields")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{introspected}"));
    let selection: Vec<String> = RICH_SCALARS.iter().map(|s| field_name(s)).collect();
    let document: &'static str =
        Box::leak(format!("{{ probes {{ {} }} }}", selection.join(" ")).into_boxed_str());
    let served = graphql(document).await;

    for scalar in RICH_SCALARS {
        let name = field_name(scalar);
        let reported = fields
            .iter()
            .find(|f| f["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} missing from introspection: {introspected}"));
        let leaf = if reported["type"]["kind"] == "NON_NULL" {
            &reported["type"]["ofType"]
        } else {
            &reported["type"]
        };
        if *leaf != json!({ "kind": "SCALAR", "name": scalar, "ofType": null })
            && (leaf["kind"] != "SCALAR" || leaf["name"] != json!(scalar))
        {
            wrong.push(format!("{scalar}: introspected as {leaf}"));
        }
        let got = served.pointer(&format!("/data/probes/0/{name}"));
        if got != Some(&value(scalar)) {
            wrong.push(format!("{scalar}: served {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}\nserved: {served}");
}

/// The document is what the rig serves, so it must compile and load where there is no
/// database, with the issue's `Host` fields typed as their scalars.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    let schema = compile().await;
    let host = schema.find_type("Host").expect("Host compiled");
    for (field, scalar) in [("hostname", "Hostname"), ("address", "IPAddress")] {
        let declared = host.fields.iter().find(|f| f.name == field).unwrap();
        assert_eq!(declared.field_type, FieldType::Scalar(scalar.to_string()), "{field}");
    }
}
