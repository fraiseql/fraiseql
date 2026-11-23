//! The storage API admits a request through the same tenant step as every other
//! transport: the caller's tenant is resolved and dispatched before a handler runs.
//!
//! Storage buckets are not tenant-scoped, so the API never resolved a tenant, and a
//! suspended or unregistered tenant's users kept reading and writing objects while
//! `/graphql` refused them. Each route is pinned below for a suspended tenant's token,
//! one route each for an unregistered tenant and for an anonymous request naming a
//! suspended tenant, and an active tenant's users are served on the same server.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops `tenant_p2347*` → run `--test-threads=1`.
#![cfg(feature = "auth")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

mod common;

use std::{collections::HashMap, sync::Arc};

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy, VectorScanConfig},
    prelude::DatabaseAdapter as _,
    schema::CompiledSchema,
};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, StorageSectionConfig, build_storage_state},
    tenancy::make_executor_factory,
};
use fraiseql_test_support::{issuer::TestIssuer, try_database_url};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

const ADMIN: &str = "p2347-admin-token-at-least-32-characters";
/// The storage API's own admin credential, `storage_token`.
const STORAGE_ADMIN: &str = "p2347-storage-token-at-least-32-characters";
/// The tenant whose users are served.
const ACTIVE: &str = "p2347a";
/// The tenant suspended after registration.
const SUSPENDED: &str = "p2347s";
/// A tenant allowed one request in flight at a time.
const CAPPED: &str = "p2347c";
/// A key no tenant is registered under.
const UNREGISTERED: &str = "p2347none";
const BUCKET: &str = "p2347";

const TOML: &str = r#"
[project]
name = "p2347"
version = "1.0.0"
database_target = "postgresql"

[types.Item]
sql_source = "v_p2347_item"
fields.id = { type = "Int" }

[queries.items]
return_type = "Item"
return_array = true
sql_source = "v_p2347_item"

[tenancy]
mode = "schema"
tenant_claim = "tenant_id"
"#;

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> =
        adapter.execute_raw_query(sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// Each tenant's schema, and the default one, with the view the compiled schema reads.
async fn seed(adapter: &PostgresAdapter) {
    exec(adapter, "DROP VIEW IF EXISTS public.v_p2347_item").await;
    exec(
        adapter,
        "CREATE VIEW public.v_p2347_item AS SELECT 1 AS id, '{}'::jsonb AS data",
    )
    .await;
    for key in [ACTIVE, SUSPENDED, CAPPED] {
        exec(adapter, &format!("DROP SCHEMA IF EXISTS tenant_{key} CASCADE")).await;
        exec(adapter, &format!("CREATE SCHEMA tenant_{key}")).await;
        exec(
            adapter,
            &format!(
                "CREATE VIEW tenant_{key}.v_p2347_item AS SELECT 1 AS id, '{{}}'::jsonb AS data"
            ),
        )
        .await;
    }
}

async fn compiled() -> (CompiledSchema, Value) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("fraiseql.toml");
    std::fs::write(&path, TOML).unwrap();
    let (artifact, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(path.to_str().unwrap())
    })
    .await
    .expect("compile");
    let json = artifact.schema.to_json().unwrap();
    let mut schema = CompiledSchema::from_json(&json, false).unwrap();
    schema.build_indexes();
    (schema, serde_json::from_str(&json).unwrap())
}

/// A private local bucket any authenticated caller may read and write.
fn storage(dir: &std::path::Path) -> HashMap<String, StorageSectionConfig> {
    HashMap::from([(
        BUCKET.to_string(),
        StorageSectionConfig {
            backend:             "local".to_string(),
            path:                Some(dir.to_string_lossy().into_owned()),
            bucket:              None,
            region:              None,
            endpoint:            None,
            project_id:          None,
            account_name:        None,
            access:              Some("private".to_string()),
            max_object_bytes:    Some(4096),
            allowed_mime_types:  None,
            serve_inline:        None,
            policies:            None,
            upload_ttl_secs:     None,
            transform_presets:   None,
            default_resize_mode: None,
            watermark_font:      None,
        },
    )])
}

struct Rig {
    base:   String,
    http:   reqwest::Client,
    issuer: TestIssuer,
    _dir:   tempfile::TempDir,
    _stop:  tokio::sync::oneshot::Sender<()>,
}

impl Rig {
    /// A token for `sub`, bound to `tenant`.
    fn token(&self, sub: &str, tenant: &str) -> String {
        self.issuer.mint(&self.issuer.claims().set("sub", sub).set("tenant_id", tenant))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    async fn admin(&self, method: Method, path: &str, body: Value) {
        let response = self
            .http
            .request(method, self.url(path))
            .bearer_auth(ADMIN)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{path}: {}", response.text().await.unwrap());
    }
}

/// A server with runtime tenancy and the storage API, `ACTIVE`, `SUSPENDED` and `CAPPED`
/// registered, `SUSPENDED` suspended, and `CAPPED` allowed one request in flight.
async fn rig() -> Option<Rig> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.unwrap();
    seed(&admin).await;
    let (schema, schema_json) = compiled().await;
    let dir = tempfile::tempdir().unwrap();
    let storage = storage(dir.path());
    let storage_state = build_storage_state(&ServerConfig {
        database_url: url.clone(),
        storage: storage.clone(),
        ..ServerConfig::default()
    })
    .await
    .unwrap()
    .unwrap();
    let issuer = TestIssuer::start().await;
    let mut config = ServerConfig {
        cors_enabled: false,
        database_url: url.clone(),
        auth: Some(common::test_issuer::oidc_config(&issuer, false)),
        admin_api_enabled: true,
        admin_token: Some(ADMIN.to_string()),
        storage_token: Some(STORAGE_ADMIN.to_string()),
        storage,
        cache_enabled: false,
        ..ServerConfig::default()
    };
    config.tenancy.runtime.enabled = true;
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let server = Box::pin(Server::new(config, schema, adapter, Some(pool)))
        .await
        .expect("Server::new with runtime tenancy and storage")
        .with_storage_state(storage_state)
        .with_tenant_executor_factory(make_executor_factory::<PostgresAdapter>(
            PostgresTlsConfig::default(),
            ReadReplicaPolicy::default(),
            VectorScanConfig::default(),
            fraiseql_core::db::postgres::TransactionRetry::off(),
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
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let rig = Rig {
        base: format!("http://127.0.0.1:{port}"),
        http: reqwest::Client::new(),
        issuer,
        _dir: dir,
        _stop: stop,
    };
    for key in [ACTIVE, SUSPENDED, CAPPED] {
        let max_concurrent = (key == CAPPED).then_some(1);
        rig.admin(
            Method::PUT,
            &format!("/api/v1/admin/tenants/{key}"),
            json!({
                "schema": schema_json,
                "connection": { "connection_string": url },
                "max_concurrent": max_concurrent,
            }),
        )
        .await;
    }
    rig.admin(Method::POST, &format!("/api/v1/admin/tenants/{SUSPENDED}/suspend"), json!({}))
        .await;
    Some(rig)
}

/// One request to every storage route, each addressing `key` in the bucket.
fn every_route(rig: &Rig, key: &str) -> Vec<(&'static str, reqwest::RequestBuilder)> {
    let object = rig.url(&format!("/storage/v1/object/{BUCKET}/{key}"));
    let metadata = rig.url(&format!("/storage/v1/metadata/{BUCKET}/{key}"));
    let upload = rig.url("/storage/v1/uploads/00000000-0000-0000-0000-000000000000");
    vec![
        (
            "put object",
            rig.http.put(&object).header("content-type", "text/plain").body("body"),
        ),
        ("get object", rig.http.get(&object)),
        ("delete object", rig.http.delete(&object)),
        ("list", rig.http.get(rig.url(&format!("/storage/v1/list/{BUCKET}")))),
        ("get metadata", rig.http.get(&metadata)),
        ("put metadata", rig.http.put(&metadata).json(&json!({ "k": "v" }))),
        (
            "presign",
            rig.http
                .post(rig.url(&format!("/storage/v1/presign/{BUCKET}/{key}")))
                .json(&json!({})),
        ),
        (
            "create upload",
            rig.http
                .post(rig.url(&format!("/storage/v1/uploads/{BUCKET}/{key}")))
                .header("tus-resumable", "1.0.0")
                .header("upload-length", "4"),
        ),
        (
            "patch upload",
            rig.http
                .patch(&upload)
                .header("tus-resumable", "1.0.0")
                .header("upload-offset", "0")
                .header("content-type", "application/offset+octet-stream")
                .body("body"),
        ),
        ("head upload", rig.http.head(&upload).header("tus-resumable", "1.0.0")),
        ("delete upload", rig.http.delete(&upload).header("tus-resumable", "1.0.0")),
    ]
}

/// Status and body of `request`.
async fn send(request: reqwest::RequestBuilder) -> (StatusCode, String) {
    let response = request.send().await.unwrap();
    (response.status(), response.text().await.unwrap_or_default())
}

#[tokio::test]
async fn a_suspended_tenants_user_is_refused_on_every_storage_route() {
    let Some(rig) = Box::pin(rig()).await else {
        return;
    };
    let token = rig.token("u-suspended", SUSPENDED);
    for (route, request) in every_route(&rig, "s.txt") {
        let (status, body) = send(request.bearer_auth(&token)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{route}: {body}");
        // A `HEAD` answer has no body; its status is the refusal.
        assert!(route == "head upload" || body.contains("suspended"), "{route}: {body}");
    }
}

#[tokio::test]
async fn an_unregistered_tenants_user_is_refused() {
    let Some(rig) = Box::pin(rig()).await else {
        return;
    };
    let token = rig.token("u-unregistered", UNREGISTERED);
    for (route, request) in every_route(&rig, "u.txt") {
        let (status, body) = send(request.bearer_auth(&token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{route}: {body}");
        // A `HEAD` answer has no body; its status is the refusal.
        assert!(route == "head upload" || body.contains("not registered"), "{route}: {body}");
    }
}

/// A request with no credential that names a tenant is dispatched to it, as on
/// `/graphql`, so a suspended one is refused before the bucket's access rules run.
#[tokio::test]
async fn an_anonymous_request_naming_a_suspended_tenant_is_refused() {
    let Some(rig) = Box::pin(rig()).await else {
        return;
    };
    for (route, request) in every_route(&rig, "n.txt") {
        let (status, body) = send(request.header("X-Tenant-ID", SUSPENDED)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{route}: {body}");
    }
}

/// The positive twin: on the same server, an active tenant's user writes, reads, lists
/// and deletes, so the refusals above are the tenant step's and not storage failing.
#[tokio::test]
async fn an_active_tenants_user_is_served() {
    let Some(rig) = Box::pin(rig()).await else {
        return;
    };
    let token = rig.token("u-active", ACTIVE);
    let object = rig.url(&format!("/storage/v1/object/{BUCKET}/a.txt"));
    let (status, body) = send(
        rig.http
            .put(&object)
            .bearer_auth(&token)
            .header("content-type", "text/plain")
            .body("active"),
    )
    .await;
    assert!(status.is_success(), "put: {status} {body}");
    let (status, body) = send(rig.http.get(&object).bearer_auth(&token)).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "active"));
    let (status, body) =
        send(rig.http.get(rig.url(&format!("/storage/v1/list/{BUCKET}"))).bearer_auth(&token))
            .await;
    assert_eq!(status, StatusCode::OK, "list: {body}");
    assert!(body.contains("a.txt"), "list: {body}");
    let (status, body) = send(rig.http.delete(&object).bearer_auth(&token)).await;
    assert!(status.is_success(), "delete: {status} {body}");
}

/// The storage admin is not a tenant's principal: it is served, and dispatched to the
/// tenant a request names like any request without one, so naming a suspended tenant is
/// refused.
#[tokio::test]
async fn the_storage_admin_is_dispatched_to_the_tenant_its_request_names() {
    let Some(rig) = Box::pin(rig()).await else {
        return;
    };
    let object = rig.url(&format!("/storage/v1/object/{BUCKET}/admin.txt"));
    let put = || {
        rig.http
            .put(&object)
            .bearer_auth(STORAGE_ADMIN)
            .header("content-type", "text/plain")
            .body("admin")
    };
    let (status, body) = send(put()).await;
    assert!(status.is_success(), "admin put: {status} {body}");
    let (status, body) = send(put().header("X-Tenant-ID", SUSPENDED)).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "admin naming a suspended tenant: {body}"
    );
}

/// The suite's document compiles with no database.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    let (schema, _) = compiled().await;
    assert!(schema.find_type("Item").is_some());
}

/// A storage request holds its tenant's concurrency permit until it has been answered,
/// not just while it is admitted: with one request of a tenant capped at one in flight,
/// its next request is refused, and served once the first has answered.
///
/// The first request is an upload whose body is sent in two halves over a raw
/// connection, so it stays in its handler, past admission, until the second half
/// arrives.
#[tokio::test]
async fn a_storage_request_holds_its_tenants_concurrency_permit_until_it_answers() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let Some(rig) = Box::pin(rig()).await else {
        return;
    };
    let token = rig.token("u-capped", CAPPED);
    let list = || rig.http.get(rig.url(&format!("/storage/v1/list/{BUCKET}"))).bearer_auth(&token);
    let (status, body) = send(list()).await;
    assert_eq!(status, StatusCode::OK, "the capped tenant is served alone: {body}");

    let address = rig.base.trim_start_matches("http://").to_string();
    let mut held = tokio::net::TcpStream::connect(&address).await.unwrap();
    let head = format!(
        "PUT /storage/v1/object/{BUCKET}/held.txt HTTP/1.1\r\nHost: {address}\r\n\
         Authorization: Bearer {token}\r\nContent-Type: text/plain\r\n\
         Content-Length: 8\r\nConnection: close\r\n\r\nhalf"
    );
    held.write_all(head.as_bytes()).await.unwrap();

    // Bounded wait for the held request to be admitted: until then a request is served.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let refused = loop {
        let (status, body) = send(list()).await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            break body;
        }
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            tokio::time::Instant::now() < deadline,
            "a second request was served all along: the held request holds no permit"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert!(!refused.is_empty());

    held.write_all(b"done").await.unwrap();
    let mut answer = Vec::new();
    held.read_to_end(&mut answer).await.unwrap();
    let answer = String::from_utf8_lossy(&answer);
    assert!(answer.starts_with("HTTP/1.1 2"), "the held upload: {answer}");

    let (status, body) = send(list()).await;
    assert_eq!(status, StatusCode::OK, "the permit is released once it answered: {body}");
}
