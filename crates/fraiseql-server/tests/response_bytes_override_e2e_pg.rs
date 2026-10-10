//! #1534: a server configuration's `[validation] max_response_bytes` is in force on every
//! executor the server builds, not only the one it booted with.
//!
//! The runtime value overrides the compiled one at boot. Every later rebuild re-derives the
//! schema-owned settings from a compiled schema: a hot reload from the reloaded artifact,
//! a tenant executor from the tenant's own. Both used to recompute the ceiling from that
//! schema alone, so the operator's value was gone after the first `SIGUSR1`, and a tenant
//! registered with a looser compiled ceiling escaped it. Nothing said so: the oversized
//! read was served under a `200`.
//!
//! The rig boots the real [`Server`] with a compiled ceiling far above the fixture's rows
//! and a runtime override far below them, so each read is refused by the override or
//! served by the compiled value, never by an accident of the fixture.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1534_bytes` schema and
//! `tenant_p1534t` tenant schema → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    cache::CachedDatabaseAdapter,
    db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy, VectorScanConfig},
    prelude::DatabaseAdapter as _,
    schema::{CompiledSchema, ValidationConfig},
};
use fraiseql_server::{Server, server_config::ServerConfig, tenancy::make_executor_factory};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use tempfile::TempDir;

const SCHEMA: &str = "p1534_bytes";
const TENANT: &str = "p1534t";
const ADMIN_TOKEN: &str = "p1534-admin-token-at-least-32-chars-long";

/// Far above what the fixture's read weighs.
const COMPILED_CEILING: u64 = 10_000_000;
/// Far below it: two rows of a 2 000-character payload.
const RUNTIME_OVERRIDE: u64 = 1_000;
const QUERY: &str = "query { items { id payload } }";

/// The document. `with_label` adds one field: the artifact the reload swaps in, which the
/// reload gate accepts and which therefore really rebuilds the executor. An identical artifact
/// would not: `swap_in_schema` returns early on an unchanged content hash, so a reload of the
/// boot artifact proves nothing about the rebuild.
fn fraiseql_toml(with_label: bool) -> String {
    let label = if with_label {
        "fields.label = { type = \"String\" }"
    } else {
        ""
    };
    format!(
        r#"
[schema]
name = "bytes-1534"
version = "1.0.0"
database_target = "postgresql"

[validation]
max_response_bytes = {COMPILED_CEILING}

[types.Item]
sql_source = "{SCHEMA}.v_item"
fields.id = {{ type = "Int" }}
fields.payload = {{ type = "String" }}
{label}

[queries.items]
return_type = "Item"
return_array = true
sql_source = "{SCHEMA}.v_item"
"#
    )
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<std::collections::HashMap<String, Value>> =
        adapter.execute_raw_query(sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
}

async fn seed(adapter: &PostgresAdapter) {
    exec(adapter, &format!("DROP SCHEMA IF EXISTS tenant_{TENANT} CASCADE")).await;
    exec(adapter, &format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")).await;
    exec(adapter, &format!("CREATE SCHEMA {SCHEMA}")).await;
    exec(
        adapter,
        &format!("CREATE TABLE {SCHEMA}.tb_item (id bigint PRIMARY KEY, payload text NOT NULL)"),
    )
    .await;
    exec(
        adapter,
        &format!(
            "INSERT INTO {SCHEMA}.tb_item VALUES (1, repeat('x', 2000)), (2, repeat('y', 2000))"
        ),
    )
    .await;
    exec(
        adapter,
        &format!(
            "CREATE VIEW {SCHEMA}.v_item AS SELECT id, jsonb_build_object('id', id, 'label', 'l', \
             'payload', payload) AS data FROM {SCHEMA}.tb_item"
        ),
    )
    .await;
}

/// Compile the document with the real compiler and write the artifact a reload reads.
async fn compile(dir: &TempDir, with_label: bool) -> (CompiledSchema, String) {
    let toml_path = dir.path().join(format!("fraiseql-{with_label}.toml"));
    std::fs::write(&toml_path, fraiseql_toml(with_label)).unwrap();
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(toml_path.to_str().unwrap())
    })
    .await
    .expect("the document must compile");
    let json = compiled.schema.to_json().unwrap();
    let mut schema = CompiledSchema::from_json(&json, false).unwrap();
    schema.build_indexes();
    (schema, json)
}

struct Rig {
    base:     String,
    http:     reqwest::Client,
    artifact: std::path::PathBuf,
    schema:   String,
    url:      String,
    stop:     Option<tokio::sync::oneshot::Sender<()>>,
    _dir:     TempDir,
}

impl Rig {
    async fn graphql(&self, tenant: Option<&str>) -> (u16, Value) {
        self.query(tenant, QUERY).await
    }

    async fn query(&self, tenant: Option<&str>, query: &str) -> (u16, Value) {
        let mut req = self
            .http
            .post(format!("{}/graphql", self.base))
            .json(&json!({ "query": query }));
        if let Some(key) = tenant {
            req = req.header("X-Tenant-ID", key);
        }
        let resp = req.send().await.expect("graphql request");
        let status = resp.status().as_u16();
        (status, resp.json().await.expect("graphql JSON body"))
    }

    async fn admin(&self, method: reqwest::Method, path: &str, body: Value) -> (u16, Value) {
        let resp = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(ADMIN_TOKEN)
            .json(&body)
            .send()
            .await
            .expect("admin request");
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text })))
    }

    /// The same rebuild `SIGUSR1` runs (`AppState::swap_in_schema`), reached through the
    /// admin endpoint so the request crosses the reload gate exactly as an operator's does.
    async fn reload(&self) {
        let (status, body) = self
            .admin(
                reqwest::Method::POST,
                "/api/v1/admin/reload-schema",
                json!({ "schema_path": self.artifact.to_str().unwrap(), "validate_only": false }),
            )
            .await;
        assert_eq!(status, 200, "precondition: the reload must be applied: {body}");
    }

    async fn register_tenant(&self) {
        let schema: Value = serde_json::from_str(&self.schema).unwrap();
        let (status, body) = self
            .admin(
                reqwest::Method::PUT,
                &format!("/api/v1/admin/tenants/{TENANT}"),
                json!({ "schema": schema, "connection": { "connection_string": self.url } }),
            )
            .await;
        assert!(status < 300, "precondition: the tenant must register: {status} {body}");
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

async fn rig() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let dir = TempDir::new().unwrap();
    let (schema, json) = compile(&dir, false).await;
    let (_, reloaded) = compile(&dir, true).await;
    let artifact = dir.path().join("schema.compiled.json");
    std::fs::write(&artifact, &reloaded).unwrap();

    let mut config = ServerConfig {
        cors_enabled: false,
        database_url: url.clone(),
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        validation: Some(ValidationConfig {
            max_response_bytes: Some(RUNTIME_OVERRIDE),
            ..ValidationConfig::default()
        }),
        ..ServerConfig::default()
    };
    // The multi-tenant runtime, as `[tenancy.runtime] enabled = true` mounts it.
    config.tenancy.runtime.enabled = true;
    let server = Box::pin(Server::new(config, schema, adapter, None))
        .await
        .expect("Server::new")
        .with_tenant_executor_factory(make_executor_factory::<
            CachedDatabaseAdapter<PostgresAdapter>,
        >(
            PostgresTlsConfig::default(),
            ReadReplicaPolicy::default(),
            VectorScanConfig::default(),
        ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    Some(Rig {
        base: format!("http://127.0.0.1:{port}"),
        http: reqwest::Client::new(),
        artifact,
        schema: json,
        url,
        stop: Some(stop),
        _dir: dir,
    })
}

/// The refusal the override produces, read off the response rather than assumed: a
/// `data` with rows in it is the defect.
fn assert_refused_by_the_override(status: u16, body: &Value, when: &str) {
    let rows = body.pointer("/data/items").and_then(Value::as_array).map_or(0, Vec::len);
    assert_eq!(
        rows, 0,
        "{when}: the runtime override must refuse this read, it was served: {status} {body}"
    );
    let message = body.pointer("/errors/0/message").and_then(Value::as_str).unwrap_or_default();
    assert!(
        message.contains(&format!("maximum of {RUNTIME_OVERRIDE} bytes")),
        "{when}: refused by the runtime ceiling ({RUNTIME_OVERRIDE}), not another error: {status} {body}"
    );
}

#[tokio::test]
async fn the_runtime_override_survives_a_hot_reload() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.graphql(None).await;
    assert_refused_by_the_override(status, &body, "at boot");

    rig.reload().await;
    // The swap happened: the reloaded artifact's field validates and is served (a small
    // read, under either ceiling).
    let (status, body) = rig.query(None, "query { items { id label } }").await;
    assert_eq!(
        body.pointer("/data/items/1/label"),
        Some(&json!("l")),
        "precondition: the reload must have rebuilt the executor: {status} {body}"
    );

    let (status, body) = rig.graphql(None).await;
    assert_refused_by_the_override(status, &body, "after a hot reload");
}

#[tokio::test]
async fn a_tenant_executor_is_bound_by_the_runtime_override() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    rig.register_tenant().await;

    let (status, body) = rig.graphql(Some(TENANT)).await;
    assert_refused_by_the_override(
        status,
        &body,
        "on a tenant registered with a looser compiled ceiling",
    );
}

/// The document is what the rig serves, so it must load where there is no database.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    let dir = TempDir::new().unwrap();
    let (schema, _) = compile(&dir, false).await;
    assert_eq!(
        schema.validation_config.as_ref().and_then(|v| v.max_response_bytes),
        Some(COMPILED_CEILING),
        "the compiled ceiling is the looser one the override must win against"
    );
}
