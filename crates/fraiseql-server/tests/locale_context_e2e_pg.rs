//! #1512: the request locale is first-class context. Every request resolves one locale from
//! `[locale]` (explicit argument, `Accept-Language`, enriched identity, default), and SQL reads
//! it as `fraiseql.locale` on read transactions.
//!
//! Each case compiles a real project (`schema.json` + `fraiseql.toml` with `[locale]`) through
//! the compiler, loads the artifact through `CompiledSchema::from_json` (the server's own load
//! path), boots a `Server` against PostgreSQL and reads the setting back through a view that
//! selects `current_setting('fraiseql.locale', true)`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `v_locale_probe` view and sets a process-global
//! env var for the HS256 secret → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    db::postgres::PostgresAdapter, prelude::DatabaseAdapter as _, schema::CompiledSchema,
};
use fraiseql_server::{
    Server,
    server_config::{Hs256Config, ServerConfig},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use tempfile::TempDir;

const VIEW: &str = "v_locale_probe";
const SECRET: &str = "fraiseql-locale-secret-exactly-32";
const SECRET_ENV: &str = "FRAISEQL_LOCALE_HS256_SECRET";
const ISSUER: &str = "https://locale.fraiseql.test";
const AUDIENCE: &str = "fraiseql-locale-api";

const SCHEMA_JSON: &str = r#"{
  "types": [{
    "name": "LocaleProbe",
    "fields": [
      {"name": "id", "type": "Int", "nullable": false},
      {"name": "locale", "type": "String", "nullable": true}
    ],
    "sql_source": "v_locale_probe"
  }],
  "queries": [{
    "name": "locale_probes",
    "return_type": "LocaleProbe",
    "returns_list": true,
    "sql_source": "v_locale_probe",
    "nullable": false,
    "arguments": []
  }],
  "mutations": [],
  "subscriptions": [],
  "version": "2.0.0"
}"#;

/// The plan's example declaration: `fr-CA` is not served, its explicit fallback is `fr-FR`;
/// `fr-BE` has none, so it truncates to `fr`.
const LOCALE_TOML: &str = r#"
[locale]
default  = "en-US"
allowed  = ["en-US", "en-GB", "fr", "fr-FR", "de-DE"]
fallback = { "fr-CA" = "fr-FR" }
"#;

/// Compile `schema.json` with a `fraiseql.toml` holding `locale_toml`, and load the artifact
/// the way the server loads `schema.compiled.json`.
async fn compile(locale_toml: &str) -> anyhow::Result<CompiledSchema> {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("schema.json"), SCHEMA_JSON).unwrap();
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[project]\nname = \"locale\"\n\n[fraiseql]\nschema_file = \"schema.json\"\n{locale_toml}"),
    )
    .unwrap();
    let input = dir.path().join("schema.json");
    let (artifact, _) = compile_to_schema(CompileOptions::new(input.to_str().unwrap())).await?;
    let json = serde_json::to_string(&artifact.schema)?;
    Ok(CompiledSchema::from_json(&json, false)?)
}

/// A running server; dropping it shuts the server down.
struct Running {
    base:      String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

async fn start(schema: CompiledSchema, authenticated: bool) -> Option<Running> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    for ddl in [
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!(
            "CREATE VIEW {VIEW} AS SELECT 1 AS id, jsonb_build_object('id', 1, 'locale', \
             current_setting('fraiseql.locale', true)) AS data"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    std::env::set_var(SECRET_ENV, SECRET);
    let config = ServerConfig {
        database_url: url,
        auth_hs256: authenticated.then(|| Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema, adapter, None)).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Some(Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    })
}

fn token() -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    encode(
        &Header::new(Algorithm::HS256),
        &json!({ "sub": "probe", "iss": ISSUER, "aud": AUDIENCE, "iat": now, "exp": now + 3600 }),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

/// `fraiseql.locale` as a GraphQL read sees it, for a request with `headers` and `body`
/// extras merged into the request body.
async fn graphql_locale(
    server: &Running,
    authenticated: bool,
    headers: &[(&str, &str)],
    extensions: Option<Value>,
) -> Value {
    let mut request =
        reqwest::Client::new().post(format!("{}/graphql", server.base)).json(&json!({
            "query": "{ localeProbes { locale } }",
            "extensions": extensions,
        }));
    if authenticated {
        request = request.header("authorization", format!("Bearer {}", token()));
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body: Value = request.send().await.unwrap().json().await.unwrap();
    assert!(body.get("errors").is_none(), "the read succeeds: {body}");
    body["data"]["localeProbes"][0]["locale"].clone()
}

/// Cycle 2: `[locale]` compiles, loads, and reaches SQL. `fr-CA` is not served and has an
/// explicit fallback, which wins over truncating to `fr`; `fr-BE` has none and truncates.
#[tokio::test]
async fn the_request_locale_reaches_a_read_transaction() {
    let schema = compile(LOCALE_TOML).await.expect("a project with [locale] compiles");
    let Some(server) = start(schema, true).await else {
        eprintln!("skipping #1512 locale context: DATABASE_URL not set");
        return;
    };
    for (accept, expected) in [("fr-CA,fr;q=0.9", "fr-FR"), ("fr-BE", "fr")] {
        assert_eq!(
            graphql_locale(&server, true, &[("accept-language", accept)], None).await,
            json!(expected),
            "Accept-Language: {accept}"
        );
    }
}

/// A tag becomes SQL text, so `[locale]` is checked again when a compiled schema is loaded:
/// a hand-edited `schema.compiled.json` cannot pass what the compiler refuses.
#[tokio::test]
async fn a_hand_written_artifact_with_an_injected_tag_is_refused_at_load() {
    let schema = compile(LOCALE_TOML).await.unwrap();
    let mut json = serde_json::to_value(&schema).unwrap();
    json["locale"]["allowed"] = json!(["en-US", "en'); DROP TABLE x;--"]);
    let err = CompiledSchema::from_json(&json.to_string(), false)
        .expect_err("an injected tag must not load");
    assert!(err.to_string().contains("well-formed BCP 47"), "{err}");

    let mut json = serde_json::to_value(&schema).unwrap();
    json["locale"]["chains"] = json!({ "en-US": ["en'); DROP TABLE x;--"] });
    assert!(
        CompiledSchema::from_json(&json.to_string(), false).is_err(),
        "a chain is never read from the artifact"
    );
}
