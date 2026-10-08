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
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{CompiledSchema, McpConfig, RestConfig},
};
use fraiseql_server::{
    Server,
    mcp::handler::FraiseQLMcpService,
    routes::graphql::AppState,
    server_config::{AsyncOperationsConfig, Hs256Config, ServerConfig},
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

/// The probe view and the actor table.
async fn seed(adapter: &PostgresAdapter) {
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
}

async fn start(schema: CompiledSchema, auth: Auth) -> Option<Running> {
    start_with(schema, auth, |_| {}).await
}

async fn start_with(
    schema: CompiledSchema,
    auth: Auth,
    tweak: impl FnOnce(&mut ServerConfig),
) -> Option<Running> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    seed(&adapter).await;
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
    if auth == Auth::Enriched {
        config.identity = serde_json::from_value(json!({
            "enrichment": {
                "enabled": true,
                "query": format!("SELECT locale FROM {ACTOR_TABLE} WHERE sub = $sub"),
                "map": {"locale": "user_locale"},
            }
        }))
        .unwrap();
    }
    // The pool backs `[identity.enrichment]` and async operations; inert otherwise.
    let pool = Some(sqlx::PgPool::connect(&config.database_url).await.unwrap());
    tweak(&mut config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    // REST writes are mounted only on request (#865).
    let server = Box::pin(Server::new(config, schema, adapter, pool))
        .await
        .unwrap()
        .with_rest_write_surface();
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
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    let body: Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{status} ({e}): {text}"));
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

/// Cycle 4: an anonymous request has no principal and so no session variables of its own, but
/// it still has a locale. GraphQL POST and GET.
#[tokio::test]
async fn an_anonymous_request_gets_its_locale_too() {
    let schema = compile(LOCALE_TOML).await.unwrap();
    let Some(server) = start(schema, Auth::None).await else {
        return;
    };
    assert_eq!(
        graphql_locale(&server, None, &[("accept-language", "de-DE")], None).await,
        json!("de-DE"),
        "anonymous POST"
    );
    let body: Value = reqwest::Client::new()
        .get(format!("{}/graphql", server.base))
        .query(&[("query", "{ localeProbes { locale } }")])
        .header("accept-language", "fr-BE")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["localeProbes"][0]["locale"], json!("fr"), "anonymous GET: {body}");
}

/// Cycle 4: REST reads resolve the locale from `?locale=` (the explicit argument), the
/// headers and the identity, for the JSON envelope and for an NDJSON export (whose statement
/// opens in the handler and whose rows are pulled after it returns).
#[tokio::test]
async fn rest_reads_run_in_the_request_locale() {
    let mut schema = compile(LOCALE_TOML).await.unwrap();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.queries[0].rest_stream = true;
    let Some(server) = start(schema, Auth::None).await else {
        return;
    };
    let client = reqwest::Client::new();
    let url = format!("{}/rest/v1/locale_probes", server.base);
    let rest = |url: String, accept_language: &'static str| {
        let client = client.clone();
        async move {
            let response = client
                .get(&url)
                .header("accept-language", accept_language)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let text = response.text().await.unwrap();
            assert!(status.is_success(), "GET {url}: {status} {text}");
            serde_json::from_str::<Value>(&text).unwrap()
        }
    };

    let body = rest(url.clone(), "fr-CA").await;
    assert_eq!(body["data"][0]["locale"], json!("fr-FR"), "REST header: {body}");

    let body = rest(format!("{url}?locale=de-DE"), "fr").await;
    assert_eq!(body["data"][0]["locale"], json!("de-DE"), "?locale= wins: {body}");

    let text = client
        .get(&url)
        .header("accept", "application/x-ndjson")
        .header("accept-language", "en-GB")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let row: Value = serde_json::from_str(text.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("NDJSON row ({e}): {text}"));
    assert_eq!(row["locale"], json!("en-GB"), "NDJSON export: {text}");
}

/// Cycle 4: an MCP tool call runs in the request's locale. Driven through
/// `call_tool_authenticated`, the seam under `ServerHandler::call_tool`, with the headers the
/// HTTP transport hands it.
#[tokio::test]
async fn an_mcp_tool_call_runs_in_the_request_locale() {
    let Some(url) = try_database_url() else {
        return;
    };
    let schema = compile(LOCALE_TOML).await.unwrap();
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    seed(&adapter).await;
    let service = FraiseQLMcpService::new(
        AppState::new(Arc::new(Executor::new(schema, adapter))),
        McpConfig {
            enabled: true,
            require_auth: false,
            ..McpConfig::default()
        },
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("accept-language", "fr-CA".parse().unwrap());
    let result = service
        .call_tool_authenticated("localeProbes", None, None, "mcp-locale".to_string(), &headers)
        .await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    assert_ne!(result.is_error, Some(true), "{text}");
    assert!(text.contains("\"fr-FR\""), "the tool read ran in fr-FR: {text}");
}

/// Cycle 4: a GraphQL `@stream` over SSE runs its continuation batches after the handler has
/// returned, where no scope reaches. `initialCount: 0` puts the probe row in a continuation.
#[tokio::test]
async fn a_streamed_continuation_batch_runs_in_the_request_locale() {
    let schema = compile(LOCALE_TOML).await.unwrap();
    let Some(server) = start_with(schema, Auth::None, |config| {
        config.enable_graphql_incremental = true;
        config.graphql_incremental_batch_size = Some(1);
    })
    .await
    else {
        return;
    };
    let text = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .header("accept", "text/event-stream")
        .header("accept-language", "de-DE")
        .json(&json!({ "query": "{ localeProbes @stream(initialCount: 0) { locale } }" }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let streamed: Vec<Value> = text
        .split("\n\n")
        .filter_map(|block| block.lines().find_map(|l| l.strip_prefix("data:")))
        .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
        .flat_map(|payload| {
            payload["incremental"].as_array().cloned().unwrap_or_default().into_iter()
        })
        .flat_map(|entry| entry["items"].as_array().cloned().unwrap_or_default().into_iter())
        .collect();
    assert_eq!(streamed, vec![json!({ "locale": "de-DE" })], "the continuation batch: {text}");
}

/// Cycle 4: an async operation executes later, on a worker with no request to resolve a locale
/// from. The locale is resolved at submission and stored with the operation.
#[tokio::test]
async fn an_async_operation_executes_in_the_locale_it_was_submitted_in() {
    let schema = compile(LOCALE_TOML).await.unwrap();
    let Some(server) = start_with(schema, Auth::Hs256, |config| {
        config.async_operations = Some(AsyncOperationsConfig {
            operations: vec!["localeProbes".to_string()],
            workers: 1,
            poll_interval_ms: 100,
            ..AsyncOperationsConfig::default()
        });
    })
    .await
    else {
        return;
    };
    let client = reqwest::Client::new();
    let token = token_for("async-locale");
    let submitted: Value = client
        .post(format!("{}/operations/v1/localeProbes", server.base))
        .bearer_auth(&token)
        .header("accept-language", "fr-CA")
        .json(&json!({ "query": "{ localeProbes { locale } }" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let op_id = submitted["op_id"].as_str().unwrap_or_else(|| panic!("{submitted}")).to_string();
    let mut terminal = Value::Null;
    for _ in 0..100 {
        terminal = client
            .get(format!("{}/operations/v1/{op_id}", server.base))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if terminal["status"] != json!("queued") && terminal["status"] != json!("running") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        terminal["result"]["data"]["localeProbes"][0]["locale"],
        json!("fr-FR"),
        "executed in the submission's locale: {terminal}"
    );
}

/// Cycle 4: a federation `_entities` lookup is a read like any other, through its own
/// session-variable builder (one of four copies before #1512 unified them).
#[cfg(feature = "federation")]
#[tokio::test]
async fn a_federation_entity_lookup_runs_in_the_request_locale() {
    let Some(url) = try_database_url() else {
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    for ddl in [
        "DROP VIEW IF EXISTS v_locale_fed_probe",
        "CREATE VIEW v_locale_fed_probe AS SELECT 'p1'::text AS id, jsonb_build_object('id', \
         'p1', 'locale', current_setting('fraiseql.locale', true)) AS data",
    ] {
        adapter.execute_raw_query(ddl).await.unwrap();
    }
    let schema = CompiledSchema::from_json(
        &json!({
            "fraiseql_version": env!("CARGO_PKG_VERSION"),
            "types": [{
                "name": "LocaleFedProbe",
                "sql_source": "v_locale_fed_probe",
                "fields": [
                    {"name": "id", "field_type": "ID", "nullable": false},
                    {"name": "locale", "field_type": "String", "nullable": true}
                ]
            }],
            "queries": [{
                "name": "localeFedProbe", "return_type": "LocaleFedProbe", "returns_list": false,
                "nullable": true, "sql_source": "v_locale_fed_probe", "jsonb_column": "data",
                "arguments": [{"name": "id", "arg_type": "ID", "nullable": false}]
            }],
            "mutations": [], "subscriptions": [],
            "federation": {
                "enabled": true, "version": "v2", "service_name": "locale",
                "entities": [{"name": "LocaleFedProbe", "key_fields": ["id"]}]
            },
            "locale": {"default": "en-US", "allowed": ["en-US", "fr", "de-DE"]}
        })
        .to_string(),
        false,
    )
    .unwrap();
    let config = ServerConfig {
        database_url: url,
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema, adapter, None)).await.unwrap();
    let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let body: Value = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .header("accept-language", "fr-BE")
        .json(&json!({
            "query": "query($representations: [_Any!]!) { _entities(representations: \
                      $representations) { ... on LocaleFedProbe { id locale } } }",
            "variables": { "representations": [{ "__typename": "LocaleFedProbe", "id": "p1" }] }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["_entities"][0]["locale"], json!("fr"), "{body}");
}

/// The schema the write fixture lives in.
const WRITE_SCHEMA: &str = "locale_write";

/// A note table whose `AFTER INSERT` trigger, and the mutation function that inserts into
/// it, each record `current_setting('fraiseql.locale', true)` in an audit table. The trigger
/// stands for a stored projection refreshed inside the write (a TVIEW, `pg_tviews#193`).
async fn provision_writes(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        format!("DROP SCHEMA IF EXISTS {WRITE_SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {WRITE_SCHEMA}"),
        format!("CREATE TABLE {WRITE_SCHEMA}.tb_audit (via text, seen text, label text)"),
        format!("CREATE TABLE {WRITE_SCHEMA}.tb_note (id uuid PRIMARY KEY, label text NOT NULL)"),
        format!(
            "CREATE VIEW {WRITE_SCHEMA}.v_note AS SELECT id, jsonb_build_object('id', id, \
             'label', label) AS data FROM {WRITE_SCHEMA}.tb_note"
        ),
        format!(
            "CREATE FUNCTION {WRITE_SCHEMA}.trg_note_audit() RETURNS trigger LANGUAGE plpgsql \
             AS $$ BEGIN INSERT INTO {WRITE_SCHEMA}.tb_audit VALUES ('trigger', \
             current_setting('fraiseql.locale', true), NEW.label); RETURN NEW; END; $$"
        ),
        format!(
            "CREATE TRIGGER note_audit AFTER INSERT ON {WRITE_SCHEMA}.tb_note FOR EACH ROW \
             EXECUTE FUNCTION {WRITE_SCHEMA}.trg_note_audit()"
        ),
        format!(
            "CREATE FUNCTION {WRITE_SCHEMA}.fn_create_note(p_label text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid; BEGIN \
             INSERT INTO {WRITE_SCHEMA}.tb_audit VALUES ('function', \
               current_setting('fraiseql.locale', true), p_label); \
             n := gen_random_uuid(); \
             INSERT INTO {WRITE_SCHEMA}.tb_note (id, label) VALUES (n, p_label); \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'LocaleNote'; v.entity_id := n; \
             v.entity := jsonb_build_object('id', n, 'label', p_label); \
             RETURN v; END; $$"
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// The compiled locale schema plus `LocaleNote` and its `createLocaleNote` mutation.
async fn schema_with_writes() -> CompiledSchema {
    use fraiseql_core::schema::{
        ArgumentDefinition, FieldDefinition, FieldType, MutationDefinition, MutationOperation,
        QueryDefinition, TypeDefinition,
    };
    let mut schema = compile(LOCALE_TOML).await.unwrap();
    let mut note = TypeDefinition::new("LocaleNote", format!("{WRITE_SCHEMA}.v_note"));
    note.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(note);
    schema.queries.push(
        QueryDefinition::new("notes", "LocaleNote")
            .returning_list()
            .with_sql_source(format!("{WRITE_SCHEMA}.v_note")),
    );
    let mut note = QueryDefinition::new("note", "LocaleNote")
        .with_sql_source(format!("{WRITE_SCHEMA}.v_note"));
    note.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    schema.queries.push(note);
    let mut create = MutationDefinition::new("createLocaleNote", "LocaleNote");
    create.sql_source = Some(format!("{WRITE_SCHEMA}.fn_create_note"));
    create.operation = MutationOperation::Insert {
        table: "tb_note".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
    schema.mutations.push(create);
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// What the audit table recorded for writes labelled `label`: `(via, seen)` pairs.
async fn audited(url: &str, label: &str) -> Vec<(String, Option<String>)> {
    let adapter = PostgresAdapter::new(url).await.unwrap();
    let rows = adapter
        .execute_raw_query(&format!(
            "SELECT via, seen FROM {WRITE_SCHEMA}.tb_audit WHERE label = '{label}' ORDER BY via"
        ))
        .await
        .unwrap();
    rows.iter()
        .map(|r| (r["via"].as_str().unwrap().to_string(), r["seen"].as_str().map(str::to_string)))
        .collect()
}

/// Cycle 5, the projection proof: a write never carries the locale. Inside the mutation
/// function and inside a trigger on the written table, `fraiseql.locale` is unset, whatever
/// the request sent, on every write entry point. A read in the same deployment sees it.
#[tokio::test]
async fn a_write_never_carries_the_locale() {
    let Some(url) = try_database_url() else {
        return;
    };
    provision_writes(&PostgresAdapter::new(&url).await.unwrap()).await;
    let schema = schema_with_writes().await;
    let mcp_schema = schema.clone();
    let Some(server) = start(schema, Auth::None).await else {
        return;
    };
    let client = reqwest::Client::new();
    // Recorded once by the function and once by the trigger, each seeing no locale.
    let unset = || {
        vec![
            ("function".to_string(), None::<String>),
            ("trigger".to_string(), None),
        ]
    };

    let body: Value = client
        .post(format!("{}/graphql", server.base))
        .header("accept-language", "de-DE")
        .json(&json!({ "query": "mutation { createLocaleNote(label: \"via-graphql\") { id } }" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body.get("errors").is_none(), "GraphQL mutation: {body}");
    assert_eq!(audited(&url, "via-graphql").await, unset(), "GraphQL mutation");

    let response = client
        .post(format!("{}/rest/v1/notes?locale=de-DE", server.base))
        .header("accept-language", "de-DE")
        .json(&json!({ "label": "via-rest" }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    assert!(status.is_success(), "REST POST: {status} {}", response.text().await.unwrap());
    assert_eq!(audited(&url, "via-rest").await, unset(), "REST POST");

    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let service = FraiseQLMcpService::new(
        AppState::new(Arc::new(Executor::new(mcp_schema, adapter))),
        McpConfig {
            enabled: true,
            require_auth: false,
            read_only: false,
            ..McpConfig::default()
        },
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("accept-language", "de-DE".parse().unwrap());
    let arguments = json!({ "label": "via-mcp" });
    let result = service
        .call_tool_authenticated(
            "createLocaleNote",
            arguments.as_object(),
            None,
            "mcp-write".to_string(),
            &headers,
        )
        .await;
    assert_ne!(result.is_error, Some(true), "MCP mutation: {:?}", result.content);
    assert_eq!(audited(&url, "via-mcp").await, unset(), "MCP mutation");

    // The control: the same deployment's reads do see it.
    assert_eq!(
        graphql_locale(&server, None, &[("accept-language", "de-DE")], None).await,
        json!("de-DE")
    );
}

/// Cycle 6: with the result cache on, the same query in two locales is two entries. The
/// probe's answer depends on the locale, so a shared entry serves one locale the other's.
#[tokio::test]
async fn the_result_cache_keeps_each_locale_apart() {
    for (auth, subject) in [(Auth::None, None), (Auth::Hs256, Some("cached"))] {
        let mut schema = compile(LOCALE_TOML).await.unwrap();
        // The cache is opt-in per view.
        schema.queries[0].cache_ttl_seconds = Some(60);
        let Some(server) = start_with(schema, auth, |config| config.cache_enabled = true).await
        else {
            return;
        };
        for (accept, expected) in [("fr", "fr"), ("de-DE", "de-DE"), ("fr", "fr")] {
            assert_eq!(
                graphql_locale(&server, subject, &[("accept-language", accept)], None).await,
                json!(expected),
                "{subject:?}, Accept-Language: {accept}"
            );
        }
    }
}

/// Phase 03: a REST `?sort=` on a text field sorts under the request locale's collation (the
/// direct-read entry, which REST, exports and gRPC share).
#[tokio::test]
async fn a_rest_sort_follows_the_request_locale() {
    use fraiseql_core::schema::{
        FieldDefinition, FieldType, LocaleConfig, LocaleSource, QueryDefinition, TypeDefinition,
    };
    let Some(url) = try_database_url() else {
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let words = ["cote", "côte", "coté", "côté", "apfel", "Äpfel", "Zebra"];
    let values: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, w)| format!("({i}, jsonb_build_object('id', '{i}', 'word', '{w}'))"))
        .collect();
    for ddl in [
        "DROP TABLE IF EXISTS v_locale_rest_word".to_string(),
        "CREATE TABLE v_locale_rest_word (pk bigint, data jsonb)".to_string(),
        format!("INSERT INTO v_locale_rest_word VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let oracle: Vec<String> = adapter
        .execute_raw_query(
            "SELECT data->>'word' AS w FROM v_locale_rest_word ORDER BY data->>'word' COLLATE \
             \"fr-CA-x-icu\"",
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r["w"].as_str().unwrap().to_string())
        .collect();

    let mut schema = CompiledSchema::new();
    let mut word = TypeDefinition::new("RestWord", "v_locale_rest_word");
    word.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("word", FieldType::String),
    ];
    schema.types.push(word);
    schema.queries.push(
        QueryDefinition::new("restWords", "RestWord")
            .returning_list()
            .with_sql_source("v_locale_rest_word"),
    );
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".into(), "fr-CA".into()],
            std::collections::BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema.build_indexes();
    let Some(server) = start(schema, Auth::None).await else {
        return;
    };
    let body: Value = reqwest::Client::new()
        .get(format!("{}/rest/v1/restWords?sort=word", server.base))
        .header("accept-language", "fr-CA")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let got: Vec<String> = body["data"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|r| r["word"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(got, oracle, "{body}");
}

/// Phase 03: an allowed locale whose ICU collation the database lacks refuses the boot, naming
/// it. Klingon (`tlh-Latn`) is well-formed BCP 47 and ships no collation.
#[tokio::test]
async fn a_locale_without_a_collation_refuses_to_boot() {
    let Some(url) = try_database_url() else {
        return;
    };
    let toml = "\n[locale]\ndefault = \"en-US\"\nallowed = [\"en-US\", \"tlh-Latn\"]\n";
    let schema = compile(toml).await.expect("a well-formed tag compiles");
    let config = ServerConfig {
        database_url: url.clone(),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let err = Box::pin(Server::new(config, schema, adapter, None))
        .await
        .err()
        .expect("a locale with no collation must not boot");
    let message = err.to_string();
    assert!(
        message.contains("tlh-Latn-x-icu") && message.contains("`tlh-Latn`"),
        "names the collation and the tag: {message}"
    );
}
