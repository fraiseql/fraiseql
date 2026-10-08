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
/// The actor table `[identity.enrichment]` reads each subject's stored locale from.
const ACTOR_TABLE: &str = "tb_locale_actor";
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
allowed  = ["en-GB", "en-US", "fr", "fr-FR", "de-DE"]   # default deliberately not first
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

/// How a test server authenticates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Auth {
    /// No auth configured: every request is anonymous.
    None,
    /// HS256 tokens.
    Hs256,
    /// HS256 tokens, and `[identity.enrichment]` resolving `user_locale` from the actor table.
    Enriched,
}

async fn start(schema: CompiledSchema, auth: Auth) -> Option<Running> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    for ddl in [
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!(
            "CREATE VIEW {VIEW} AS SELECT 1 AS id, jsonb_build_object('id', 1, 'locale', \
             current_setting('fraiseql.locale', true)) AS data"
        ),
        format!("DROP TABLE IF EXISTS {ACTOR_TABLE}"),
        format!("CREATE TABLE {ACTOR_TABLE} (sub text PRIMARY KEY, locale text NOT NULL)"),
        format!("INSERT INTO {ACTOR_TABLE} VALUES ('german', 'de-DE'), ('unmatched', 'zz')"),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    std::env::set_var(SECRET_ENV, SECRET);
    let authenticated = auth != Auth::None;
    let mut config = ServerConfig {
        database_url: url,
        auth_hs256: authenticated.then(|| Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let pool = if auth == Auth::Enriched {
        config.identity = serde_json::from_value(json!({
            "enrichment": {
                "enabled": true,
                "query": format!("SELECT locale FROM {ACTOR_TABLE} WHERE sub = $sub"),
                "map": {"locale": "user_locale"},
            }
        }))
        .unwrap();
        Some(sqlx::PgPool::connect(&config.database_url).await.unwrap())
    } else {
        None
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema, adapter, pool)).await.unwrap();
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

fn token_for(sub: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    encode(
        &Header::new(Algorithm::HS256),
        &json!({ "sub": sub, "iss": ISSUER, "aud": AUDIENCE, "iat": now, "exp": now + 3600 }),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

/// `fraiseql.locale` as a GraphQL read sees it, for a request with `headers` and `body`
/// extras merged into the request body.
async fn graphql_locale(
    server: &Running,
    subject: Option<&str>,
    headers: &[(&str, &str)],
    extensions: Option<Value>,
) -> Value {
    let mut request =
        reqwest::Client::new().post(format!("{}/graphql", server.base)).json(&json!({
            "query": "{ localeProbes { locale } }",
            "extensions": extensions,
        }));
    if let Some(sub) = subject {
        request = request.header("authorization", format!("Bearer {}", token_for(sub)));
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
    let Some(server) = start(schema, Auth::Hs256).await else {
        eprintln!("skipping #1512 locale context: DATABASE_URL not set");
        return;
    };
    for (accept, expected) in [("fr-CA,fr;q=0.9", "fr-FR"), ("fr-BE", "fr")] {
        assert_eq!(
            graphql_locale(&server, Some("probe"), &[("accept-language", accept)], None).await,
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

/// Cycle 3: the resolution order, one full request per row. `resolve` is argument, then
/// `Accept-Language`, then the enriched `user_locale` (`german` → `de-DE`, `unmatched` → `zz`,
/// which no rule matches).
#[tokio::test]
async fn sources_resolve_in_order_and_fall_through() {
    let toml = format!(
        "{LOCALE_TOML}resolve = [{{ argument = \"locale\" }}, {{ header = \"Accept-Language\" }}, \
         {{ enrichment = \"user_locale\" }}]\n"
    );
    let schema = compile(&toml).await.unwrap();
    let Some(server) = start(schema, Auth::Enriched).await else {
        eprintln!("skipping #1512 locale resolution: DATABASE_URL not set");
        return;
    };
    // Over 1024 bytes, with a matching tag first: only the length bound refuses it.
    let long = format!("fr,{}", "x-y;q=0.1,".repeat(410));
    // Within 1024 bytes, with the only match as entry 41: only the entry cap refuses it.
    let crowded = format!("{}de-DE", "da,".repeat(40));
    let rows: [(&str, Option<&str>, Option<&str>, &str, &str); 14] = [
        ("argument over header", Some("en-GB"), Some("fr"), "german", "en-GB"),
        ("header over enrichment", None, Some("fr"), "german", "fr"),
        ("enrichment when nothing else", None, None, "german", "de-DE"),
        ("an unknown argument falls through", Some("xx"), Some("fr"), "german", "fr"),
        ("an argument's fallback entry", Some("fr-CA"), None, "german", "fr-FR"),
        ("truncation, then the next source", None, Some("de-AT, fr-BE"), "german", "fr"),
        ("the fallback before the next source", None, Some("fr-CA"), "german", "fr-FR"),
        ("`*` falls through", None, Some("*"), "german", "de-DE"),
        ("a malformed header falls through", None, Some("fr;q=banana"), "german", "de-DE"),
        ("a 4 KB header is not parsed", None, Some(long.as_str()), "german", "de-DE"),
        ("nothing matches: the default", Some("xx"), Some("da"), "unmatched", "en-US"),
        (
            "best q-value first",
            None,
            Some("da;q=0.2, de-DE;q=0.5, en-GB;q=0.8"),
            "german",
            "en-GB",
        ),
        ("q=0 is a refusal", None, Some("fr;q=0"), "german", "de-DE"),
        (
            "at most 32 entries are read",
            None,
            Some(crowded.as_str()),
            "unmatched",
            "en-US",
        ),
    ];
    for (row, argument, accept, subject, expected) in rows {
        let headers: Vec<(&str, &str)> =
            accept.map(|a| vec![("accept-language", a)]).unwrap_or_default();
        let extensions = argument.map(|a| json!({ "locale": a }));
        assert_eq!(
            graphql_locale(&server, Some(subject), &headers, extensions).await,
            json!(expected),
            "{row}"
        );
    }
}

/// A `[locale]` enrichment source the enrichment `map` does not produce would never match:
/// the server refuses to boot rather than serve every user the default.
#[tokio::test]
async fn an_enrichment_source_nothing_produces_refuses_to_boot() {
    let Some(url) = try_database_url() else {
        return;
    };
    let toml = format!("{LOCALE_TOML}resolve = [{{ enrichment = \"favourite_locale\" }}]\n");
    let schema = compile(&toml).await.unwrap();
    std::env::set_var(SECRET_ENV, SECRET);
    let mut config = ServerConfig {
        database_url: url.clone(),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    config.identity = serde_json::from_value(json!({
        "enrichment": { "enabled": true, "query": "SELECT 'x' AS locale", "map": {"locale": "user_locale"} }
    }))
    .unwrap();
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let err = Box::pin(Server::new(config, schema, adapter, Some(pool)))
        .await
        .err()
        .expect("must refuse to boot");
    assert!(
        err.to_string().contains("favourite_locale") && err.to_string().contains("user_locale"),
        "{err}"
    );

    // And with no resolver at all, a [locale] enrichment source is an enrichment consumer
    // like any other: refused at boot rather than silently never matching.
    let schema = compile(&toml).await.unwrap();
    let config = ServerConfig {
        database_url: url.clone(),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let err = Box::pin(Server::new(config, schema, adapter, None))
        .await
        .err()
        .expect("must refuse to boot without a resolver");
    assert!(
        err.to_string().contains("is not enabled"),
        "refused as an enrichment consumer without a resolver: {err}"
    );
}
