//! The `[security]` sections a project config now takes are honoured by the
//! server, not only accepted by the compiler.
//!
//! Each case compiles an SDK `schema.json` beside a project `fraiseql.toml` through the
//! compiler library, boots the real [`Server`] from the result on a database-free
//! [`FailingAdapter`], and observes the setting's effect against a control compiled without
//! it: whether a request reaches the database (`query_count`), or whether the server boots.
//!
//! **Execution engine:** none (`FailingAdapter`) · **Infrastructure:** none ·
//! **Parallelism:** the boot cases set process environment variables under `temp_env`'s
//! lock.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // Reason: test code
#![allow(clippy::future_not_send)] // Reason: `temp_env` holds its non-`Send` lock across the await; tests run on one thread
#![allow(clippy::large_futures)] // Reason: each case boots a whole server inline; boxing would obscure the case

use std::sync::Arc;

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::issuer::TestIssuer;
use fraiseql_test_utils::failing_adapter::{FailError, FailingAdapter};
use serde_json::{Value, json};
use tempfile::TempDir;

mod common;

use crate::common::{server_harness::TestServer, test_issuer::oidc_config};

/// The schema an SDK emits: one type and its list query.
fn document() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": []
    })
}

const QUERY: &str = "query { orders { id } }";

/// A trusted-documents manifest holding [`QUERY`] under the id `orders`.
fn manifest(dir: &TempDir) -> String {
    let path = dir.path().join("trusted.json");
    std::fs::write(&path, json!({"version": 1, "documents": {"orders": QUERY}}).to_string())
        .unwrap();
    path.display().to_string()
}

/// `schema.json` compiled beside a project config holding `security` (a TOML text that
/// may name `{manifest}`), with the project directory, which holds the manifest.
async fn compile(security: &str) -> (TempDir, CompiledSchema) {
    let dir = TempDir::new().unwrap();
    let security = security.replace("{manifest}", &manifest(&dir));
    std::fs::write(dir.path().join("schema.json"), document().to_string()).unwrap();
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("{security}\n[project]\nname = \"app\"\n"),
    )
    .unwrap();
    let input = dir.path().join("schema.json");
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(input.to_str().unwrap())
    })
    .await
    .unwrap_or_else(|e| panic!("compile {security:?}: {e:#}"));
    let mut schema = CompiledSchema::from_json(&compiled.schema.to_json().unwrap(), false).unwrap();
    schema.build_indexes();
    (dir, schema)
}

async fn boot(
    schema: CompiledSchema,
    adapter: Arc<FailingAdapter>,
) -> fraiseql_server::Result<Server> {
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    Box::pin(Server::new(config, schema, adapter, None)).await
}

/// One `/graphql` request on the booted server: the response, and whether it reached the
/// database.
struct Answer {
    status:  u16,
    body:    Value,
    reached: bool,
}

async fn graphql(security: &str, body: Value, headers: &[(&str, &str)]) -> Answer {
    graphql_on(FailingAdapter::new(), security, body, headers).await
}

/// [`graphql`] on `adapter`, which may be set to fail.
async fn graphql_on(
    adapter: FailingAdapter,
    security: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> Answer {
    let adapter = Arc::new(adapter);
    let (_project, schema) = compile(security).await;
    let server = boot(schema, Arc::clone(&adapter))
        .await
        .unwrap_or_else(|e| panic!("boot {security:?}: {e}"));
    // What the boot itself asked of the database is not the request's doing.
    let before = adapter.query_count();
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

    let mut request = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .json(&body);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await.expect("graphql request");
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    let _ = stop.send(());
    Answer {
        status,
        body,
        reached: adapter.query_count() > before,
    }
}

/// The requests below run as a development deployment, so a production-only boot refusal
/// unrelated to the setting under test cannot answer for it.
async fn in_development<F: std::future::Future<Output = ()>>(case: F) {
    temp_env::async_with_vars([("FRAISEQL_ENV", Some("development"))], case).await;
}

// ── refused before the database ──────────────────────────────────────────────

#[tokio::test]
async fn a_cost_budget_in_a_project_config_refuses_an_operation_above_it() {
    in_development(async {
        let refused =
            graphql("[security.cost_budget]\nper_request_max = 1\n", json!({"query": QUERY}), &[])
                .await;
        assert!(
            !refused.reached,
            "an operation above the budget reached the database: {}",
            refused.body
        );
        assert!(refused.body["errors"].is_array(), "no refusal: {}", refused.body);

        let control = graphql("", json!({"query": QUERY}), &[]).await;
        assert!(control.reached, "control: {}", control.body);
    })
    .await;
}

#[tokio::test]
async fn strict_trusted_documents_in_a_project_config_refuse_a_raw_query() {
    in_development(async {
        let strict = "[security.trusted_documents]\nenabled = true\nmode = \"strict\"\n\
                      manifest_path = \"{manifest}\"\n";
        let raw = graphql(strict, json!({"query": QUERY}), &[]).await;
        assert!(!raw.reached, "a raw query reached the database under strict documents");
        assert!(raw.body["errors"].is_array(), "no refusal: {}", raw.body);

        let trusted = graphql(strict, json!({"documentId": "sha256:orders"}), &[]).await;
        assert!(trusted.reached, "the trusted document was not run: {}", trusted.body);
    })
    .await;
}

#[tokio::test]
async fn persisted_queries_only_in_a_project_config_refuses_a_raw_query() {
    in_development(async {
        let permissive = "[security.trusted_documents]\nenabled = true\n\
                          mode = \"permissive\"\nmanifest_path = \"{manifest}\"\n";
        let control = graphql(permissive, json!({"query": QUERY}), &[]).await;
        assert!(
            control.reached,
            "permissive documents alone refused a raw query: {}",
            control.body
        );

        let only = format!("[security]\npersisted_queries_only = true\n{permissive}");
        let raw = graphql(&only, json!({"query": QUERY}), &[]).await;
        assert!(!raw.reached, "a raw query reached the database with persisted_queries_only");
        assert!(raw.body["errors"].is_array(), "no refusal: {}", raw.body);
    })
    .await;
}

#[tokio::test]
async fn an_api_key_section_in_a_project_config_authenticates_on_its_header() {
    // sha256("p2011-key")
    let hash = format!(
        "sha256:{}",
        hex::encode(<sha2::Sha256 as sha2::Digest>::digest(b"p2011-key").as_slice())
    );
    let section = format!(
        "[security.api_keys]\nenabled = true\nheader = \"X-Service-Key\"\n\
         storage = \"env\"\n[[security.api_keys.static]]\nkey_hash = \"{hash}\"\n\
         scopes = [\"read:*\"]\nname = \"backend\"\n"
    );
    in_development(async {
        let wrong =
            graphql(&section, json!({"query": QUERY}), &[("X-Service-Key", "not-the-key")]).await;
        assert!(
            !wrong.reached,
            "a wrong key on the configured header reached the database: {} {}",
            wrong.status, wrong.body
        );
        assert_eq!(wrong.status, 401, "{}", wrong.body);

        let right =
            graphql(&section, json!({"query": QUERY}), &[("X-Service-Key", "p2011-key")]).await;
        assert!(right.reached, "the configured key was not admitted: {}", right.body);

        // Without the section the header means nothing.
        let control =
            graphql("", json!({"query": QUERY}), &[("X-Service-Key", "not-the-key")]).await;
        assert!(control.reached, "control: {}", control.body);
    })
    .await;
}

// ── read at boot ─────────────────────────────────────────────────────────────

/// Boot a server from `security` under `vars`; `Err` carries the refusal.
async fn boots(
    security: &str,
    vars: Vec<(&'static str, Option<&'static str>)>,
) -> Result<(), String> {
    let (_project, schema) = compile(security).await;
    temp_env::async_with_vars(vars, async {
        boot(schema, Arc::new(FailingAdapter::new()))
            .await
            .map(drop)
            .map_err(|e| e.to_string())
    })
    .await
}

/// A server compiled from `security` whose `[auth]` trusts `issuer` and requires a bearer,
/// with its project directory and its adapter.
async fn trusting(
    issuer: &TestIssuer,
    security: &str,
) -> (TempDir, Arc<FailingAdapter>, TestServer) {
    let (project, schema) = compile(security).await;
    let adapter = Arc::new(FailingAdapter::new());
    let config = ServerConfig {
        auth: Some(oidc_config(issuer, true)),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let server =
        Box::pin(TestServer::start_with_config(config, schema, Arc::clone(&adapter))).await;
    (project, adapter, server)
}

/// `/graphql` with `token` as the bearer: the status, and whether it reached the database.
async fn as_bearer(server: &TestServer, adapter: &FailingAdapter, token: &str) -> (u16, bool) {
    let before = adapter.query_count();
    let status = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .bearer_auth(token)
        .json(&json!({"query": QUERY}))
        .send()
        .await
        .expect("graphql request")
        .status()
        .as_u16();
    (status, adapter.query_count() > before)
}

#[tokio::test]
async fn token_revocation_in_a_project_config_refuses_a_token_without_jti_and_a_revoked_one() {
    in_development(async {
        let issuer = TestIssuer::start().await;
        let no_jti = issuer.mint(&issuer.claims().without("jti"));

        // Without the section, a token without `jti` is admitted.
        let (_control_project, adapter, control) = trusting(&issuer, "").await;
        let (status, reached) = as_bearer(&control, &adapter, &no_jti).await;
        assert!(reached, "control: a token without jti was refused ({status})");

        let section = "[security.token_revocation]\nenabled = true\nbackend = \"memory\"\n";
        let (_project, adapter, server) = trusting(&issuer, section).await;

        // `require_jti` defaults on: a token without `jti` is refused.
        let (status, reached) = as_bearer(&server, &adapter, &no_jti).await;
        assert_eq!(status, 401, "a token without jti was not refused");
        assert!(!reached, "a token without jti reached the database");

        // A token with one is admitted until it is revoked through the configured store.
        let token = issuer.mint(&issuer.claims());
        let (status, reached) = as_bearer(&server, &adapter, &token).await;
        assert!(reached, "a token with a jti was refused ({status})");

        let revoked = reqwest::Client::new()
            .post(format!("{}/auth/revoke", server.url))
            .bearer_auth(&token)
            .json(&json!({}))
            .send()
            .await
            .expect("revoke request");
        assert_eq!(revoked.status().as_u16(), 200, "{:?}", revoked.text().await);

        let (status, reached) = as_bearer(&server, &adapter, &token).await;
        assert_eq!(status, 401, "a revoked token was not refused");
        assert!(!reached, "a revoked token reached the database");
    })
    .await;
}

/// A provider whose discovery document names the endpoints a PKCE client resolves at boot.
#[cfg(feature = "auth")]
async fn oauth_provider() -> wiremock::MockServer {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };

    let provider = wiremock::MockServer::start().await;
    let base = provider.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issuer": base,
            "authorization_endpoint": format!("{base}/authorize"),
            "token_endpoint": format!("{base}/token"),
            "userinfo_endpoint": format!("{base}/userinfo"),
            "jwks_uri": format!("{base}/jwks"),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["ES256"],
        })))
        .mount(&provider)
        .await;
    provider
}

/// `GET /auth/start` on a server compiled from `security`: the status and the `Location`.
#[cfg(feature = "auth")]
async fn auth_start(security: &str) -> (u16, String) {
    let (_project, schema) = compile(security).await;
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let server =
        Box::pin(TestServer::start_with_config(config, schema, Arc::new(FailingAdapter::new())))
            .await;
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!("{}/auth/start", server.url))
        .query(&[("redirect_uri", "https://app.example/after")])
        .send()
        .await
        .expect("auth/start request");
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    (response.status().as_u16(), location)
}

#[cfg(feature = "auth")]
#[tokio::test]
async fn pkce_in_a_project_config_mounts_auth_start_with_an_s256_challenge() {
    let provider = oauth_provider().await;
    let client = format!(
        "[auth]\ndiscovery_url = \"{}\"\nclient_id = \"p2011\"\n\
         client_secret_env = \"P2011_CLIENT_SECRET\"\n\
         server_redirect_uri = \"https://api.example/auth/callback\"\n",
        provider.uri()
    );
    let vars = [
        ("FRAISEQL_ENV", Some("development")),
        ("FRAISEQL_OIDC_ALLOW_INSECURE", Some("1")),
        ("P2011_CLIENT_SECRET", Some("p2011-secret")),
    ];
    temp_env::async_with_vars(vars, async {
        let (status, location) =
            auth_start(&format!("{client}[security.pkce]\nenabled = true\n")).await;
        assert!((300..400).contains(&status), "/auth/start answered {status}");
        assert_eq!(
            location.split('?').next(),
            Some(format!("{}/authorize", provider.uri()).as_str()),
            "redirected elsewhere: {location}"
        );
        assert!(location.contains("code_challenge_method=S256"), "{location}");
        assert!(location.contains("code_challenge="), "{location}");

        // The same client without the section mounts no PKCE flow.
        let (status, _) = auth_start(&client).await;
        assert_eq!(status, 404, "/auth/start is mounted without [security.pkce]");
    })
    .await;
}

/// A valid 32-byte hex key.
#[cfg(feature = "auth")]
const STATE_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

/// With state encryption on, production gets past the state guard to the login
/// client, whose discovery URL here is one the SSRF gate refuses offline: the refusal
/// that follows is the client's, not the state guard's.
#[cfg(feature = "auth")]
#[tokio::test]
async fn pkce_in_a_project_config_requires_state_encryption_in_production() {
    let client = "[auth]\ndiscovery_url = \"https://127.0.0.1:1\"\nclient_id = \"p2011\"\n\
                  client_secret_env = \"P2011_CLIENT_SECRET\"\n\
                  server_redirect_uri = \"https://api.example/auth/callback\"\n";
    let state_refusal = "[security.state_encryption] is\n  missing or disabled";
    let production = || {
        vec![
            ("FRAISEQL_ENV", None),
            ("STATE_ENCRYPTION_KEY", Some(STATE_KEY)),
            ("P2011_CLIENT_SECRET", Some("p2011-secret")),
            ("FRAISEQL_OIDC_ALLOW_INSECURE", None),
        ]
    };
    let refused = boots(&format!("{client}[security.pkce]\nenabled = true\n"), production())
        .await
        .expect_err("PKCE without state encryption booted in production");
    assert!(refused.contains(state_refusal), "{refused}");

    let refused = boots(
        &format!(
            "{client}[security.pkce]\nenabled = true\n[security.state_encryption]\n\
             enabled = true\n"
        ),
        production(),
    )
    .await
    .expect_err("booted with a discovery URL the SSRF gate refuses");
    assert!(!refused.contains(state_refusal), "the state key was not honoured: {refused}");
    assert!(refused.contains("SSRF"), "the client's discovery refused it: {refused}");
}

#[cfg(feature = "auth")]
#[tokio::test]
async fn state_encryption_key_env_in_a_project_config_names_the_variable_read() {
    let section = "[security.state_encryption]\nenabled = true\n\
                   key_env = \"P2011_STATE_KEY\"\n";
    // The declared variable is read, with the default name unset.
    boots(
        section,
        vec![
            ("FRAISEQL_ENV", Some("development")),
            ("STATE_ENCRYPTION_KEY", None),
            ("P2011_STATE_KEY", Some(STATE_KEY)),
        ],
    )
    .await
    .unwrap_or_else(|e| panic!("the key in P2011_STATE_KEY: {e}"));

    // And it is the one read: the default name set does not stand in for it.
    let refused = boots(
        section,
        vec![
            ("FRAISEQL_ENV", Some("development")),
            ("STATE_ENCRYPTION_KEY", Some(STATE_KEY)),
            ("P2011_STATE_KEY", None),
        ],
    )
    .await
    .expect_err("booted without the declared key variable");
    assert!(refused.contains("P2011_STATE_KEY"), "{refused}");
}

// ── error_sanitization, rate_limiting, enterprise ────────────────────────────

/// Text the database put in an error, which a sanitizer replaces.
const DATABASE_TEXT: &str = "relation \"v_order\" has column secret_x";

/// The first error's message for [`QUERY`] on an adapter that fails with [`DATABASE_TEXT`].
///
/// Whatever the section says, the error carries no `extensions.detail`: no key decides
/// whether one is served, so none may be (#2023).
async fn database_error_message(security: &str) -> String {
    let adapter = FailingAdapter::new().fail_with_error(FailError::Database {
        message:   DATABASE_TEXT.to_owned(),
        sql_state: Some("XX000".to_owned()),
    });
    let answer = graphql_on(adapter, security, json!({"query": QUERY}), &[]).await;
    assert!(answer.reached, "the query never reached the failing adapter: {}", answer.body);
    let error = &answer.body["errors"][0];
    assert!(
        error.get("extensions").and_then(|e| e.get("detail")).is_none(),
        "{security:?} served an error detail: {error}"
    );
    error["message"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn error_sanitization_keys_in_a_project_config_shape_a_database_error() {
    in_development(async {
        let table = "[security.error_sanitization]\n";

        // Written with no keys: on, so the database text is replaced.
        let sanitized = database_error_message(table).await;
        assert!(
            !sanitized.contains("secret_x"),
            "the default section kept the database text: {sanitized}"
        );

        let custom =
            database_error_message(&format!("{table}custom_error_message = \"Try again\"\n")).await;
        assert_eq!(custom, "Try again");
        assert_ne!(sanitized, "Try again", "the control already said it");

        let raw =
            database_error_message(&format!("{table}sanitize_database_errors = false\n")).await;
        assert!(raw.contains("secret_x"), "sanitize_database_errors = false was ignored: {raw}");
    })
    .await;
}

/// The status of each of `n` requests from one verified caller, and the
/// `X-RateLimit-Limit` the last one reported.
async fn as_one_caller(server: &TestServer, token: &str, n: usize) -> (Vec<u16>, String) {
    let client = reqwest::Client::new();
    let mut statuses = Vec::new();
    let mut limit = String::new();
    for _ in 0..n {
        let response = client
            .post(format!("{}/graphql", server.url))
            .bearer_auth(token)
            .json(&json!({"query": QUERY}))
            .send()
            .await
            .expect("graphql request");
        statuses.push(response.status().as_u16());
        if let Some(value) = response.headers().get("X-RateLimit-Limit") {
            value.to_str().unwrap_or_default().clone_into(&mut limit);
        }
    }
    (statuses, limit)
}

#[tokio::test]
async fn requests_per_second_per_user_in_a_project_config_limits_a_verified_caller() {
    in_development(async {
        let issuer = TestIssuer::start().await;
        let token = issuer.mint(&issuer.claims());
        let table = "[security.rate_limiting]\nrequests_per_second = 1000\nburst_size = 2\n";

        // Without the key, a verified caller refills at 10 × 1000 a second.
        let (_project, _adapter, control) = trusting(&issuer, table).await;
        let (statuses, limit) = as_one_caller(&control, &token, 3).await;
        assert!(!statuses.contains(&429), "control: {statuses:?}");
        assert_eq!(limit, "10000");

        let (_project, _adapter, limited) =
            trusting(&issuer, &format!("{table}requests_per_second_per_user = 1\n")).await;
        let (statuses, limit) = as_one_caller(&limited, &token, 3).await;
        assert_eq!(statuses.last(), Some(&429), "the third request in a second: {statuses:?}");
        assert_eq!(limit, "1");
    })
    .await;
}

#[cfg(feature = "redis-rate-limiting")]
#[tokio::test]
async fn rate_limiting_redis_url_in_a_project_config_is_connected_at_boot() {
    let production = || vec![("FRAISEQL_ENV", None)];
    let table = "[security.rate_limiting]\nenabled = true\n";
    boots(table, production())
        .await
        .unwrap_or_else(|e| panic!("control, in-memory limiter: {e}"));

    // Nothing listens on port 1: the operator asked for a shared limiter it cannot have.
    let refused = boots(&format!("{table}redis_url = \"redis://127.0.0.1:1\"\n"), production())
        .await
        .expect_err("booted without the Redis limiter it was given");
    assert!(refused.contains("[security.rate_limiting] redis_url"), "{refused}");
}

#[tokio::test]
async fn failed_login_keys_in_a_project_config_are_refused_in_production() {
    let production = || vec![("FRAISEQL_ENV", None)];
    let table = "[security.rate_limiting]\n";
    boots(table, production())
        .await
        .unwrap_or_else(|e| panic!("control, default failed-login budget: {e}"));

    for key in [
        "failed_login_max_attempts = 5",
        "failed_login_lockout_secs = 60",
    ] {
        let refused = boots(&format!("{table}{key}\n"), production())
            .await
            .expect_err("booted with a lockout the binary cannot enforce");
        assert!(
            refused.contains("failed_login_max_attempts / failed_login_lockout_secs"),
            "{key}: {refused}"
        );
    }
}

#[tokio::test]
async fn audit_logging_enabled_in_a_project_config_reaches_the_executor() {
    let audits = |security: &'static str| async move {
        let (_project, schema) = compile(security).await;
        fraiseql_core::runtime::RuntimeConfig::from_compiled_schema(&schema)
            .unwrap()
            .audit_mutations
    };
    assert!(audits("").await, "control: audit logging is on unless written off");
    assert!(!audits("[security.enterprise]\naudit_logging_enabled = false\n").await);
}
