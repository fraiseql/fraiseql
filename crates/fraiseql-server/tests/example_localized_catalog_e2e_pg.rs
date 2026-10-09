//! `examples/localized-catalog` end to end (#1512, #1513): the shipped example's SQL applied
//! to a scratch PostgreSQL 18 database, its Python-authored `schema.json` compiled through
//! the production compile path with its own `fraiseql.toml`, and the server serving it over
//! HTTP.
//!
//! What it proves, as the example's README states it:
//!
//! - a caller reads the catalog in their `Accept-Language`, over their tenant's stored locale
//!   (through `fr-CA → fr-FR → fr → en-US`);
//! - a tenant's caller with no header reads it in the tenant's stored locale
//!   (`[identity.enrichment]` → `tenant_locale`), and the tenant's orders carry labels the view
//!   rendered from that stored locale, whatever the request asks;
//! - `orderBy` on a localized field follows the request locale;
//! - the result cache keeps each locale's labels apart;
//! - a `{value: …}` write stores a merged map, and the write guard in the example's SQL (which
//!   raises if a write sees `fraiseql.locale`) lets it through;
//! - the projection gate: the example's catalog check finds nothing, and finds an injected view
//!   that reads the setting.
//!
//! Self-skips when no `DATABASE_URL` is set, so it runs in the `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own database.
// The tenant locale comes from the identity enrichment.
#![cfg(feature = "auth")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{path::PathBuf, sync::Arc};

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{Hs256Config, ServerConfig},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const DATABASE: &str = "fraiseql_example_localized_catalog";
const SECRET: &str = "fraiseql-catalog-secret-exactly-32";
const SECRET_ENV: &str = "FRAISEQL_CATALOG_HS256_SECRET";
const ISSUER: &str = "https://catalog.fraiseql.test";
const AUDIENCE: &str = "fraiseql-catalog";
const ALICE_TENANT: &str = "00000000-0000-0000-0000-0000000000a1";
const APPLE: &str = "00000000-0000-0000-0000-000000000001";

fn example_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/localized-catalog")
}

/// The example compiled the way `fraiseql compile` compiles it: its `schema.json` with its
/// `fraiseql.toml` beside it.
async fn compile_example() -> CompiledSchema {
    let dir = tempfile::TempDir::new().unwrap();
    for file in ["schema.json", "fraiseql.toml"] {
        std::fs::copy(example_dir().join(file), dir.path().join(file)).unwrap();
    }
    let input = dir.path().join("schema.json");
    let (artifact, _) = compile_to_schema(CompileOptions::new(input.to_str().unwrap()))
        .await
        .unwrap_or_else(|e| panic!("compile the example: {e:#}"));
    CompiledSchema::from_json(&serde_json::to_string(&artifact.schema).unwrap(), false)
        .unwrap_or_else(|e| panic!("load the example: {e}"))
}

fn with_database(url: &str, db: &str) -> String {
    let (base, _) = url.rsplit_once('/').expect("a database URL ends with /<db>");
    format!("{base}/{db}")
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {e}");
        }
    });
    client
}

/// A scratch database with the example's `sql/01_schema.sql` applied; its URL.
async fn provision(url: &str) -> String {
    let admin = client(url).await;
    // Separate statements: a batch runs as one implicit transaction, which these refuse.
    for statement in [
        format!("DROP DATABASE IF EXISTS {DATABASE} WITH (FORCE)"),
        format!("CREATE DATABASE {DATABASE}"),
    ] {
        admin.batch_execute(&statement).await.unwrap();
    }
    let scratch_url = with_database(url, DATABASE);
    let scratch = client(&scratch_url).await;
    // The change-log contract FraiseQL's setup installs (mutations record into it).
    for statement in fraiseql_test_support::changelog::entity_change_log_provision_statements() {
        scratch.batch_execute(&statement).await.unwrap();
    }
    let sql = std::fs::read_to_string(example_dir().join("sql/01_schema.sql")).unwrap();
    scratch.batch_execute(&sql).await.unwrap();
    scratch_url
}

struct Running {
    base:      String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// The server, configured as the example's README shows: HS256 tokens, and
/// `[identity.enrichment]` reading the caller's tenant locale.
async fn start(url: &str, schema: CompiledSchema) -> Running {
    std::env::set_var(SECRET_ENV, SECRET);
    let mut config = ServerConfig {
        database_url: url.to_string(),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        cache_enabled: true,
        ..ServerConfig::default()
    };
    config.identity = serde_json::from_value(json!({
        "enrichment": {
            "enabled": true,
            "query": "SELECT locale FROM tb_tenant WHERE sub = $sub",
            "map": {"locale": "tenant_locale"},
        }
    }))
    .unwrap();
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    let pool = Some(sqlx::PgPool::connect(url).await.unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema, adapter, pool))
        .await
        .unwrap_or_else(|e| panic!("boot the example: {e}"));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    }
}

fn token(sub: &str, tenant_id: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    encode(
        &Header::new(Algorithm::HS256),
        &json!({ "sub": sub, "tenant_id": tenant_id, "iss": ISSUER, "aud": AUDIENCE,
                 "iat": now, "exp": now + 3600 }),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

async fn graphql(server: &Running, query: &str, accept: Option<&str>, auth: Option<&str>) -> Value {
    let mut request = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query }));
    if let Some(accept) = accept {
        request = request.header("accept-language", accept);
    }
    if let Some(auth) = auth {
        request = request.bearer_auth(auth);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    let body: Value =
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("{query}: {status} {text}"));
    assert!(body.get("errors").is_none(), "{query}: {body}");
    body
}

/// The `name` of each product, as listed.
async fn names(
    server: &Running,
    arguments: &str,
    accept: Option<&str>,
    auth: Option<&str>,
) -> Vec<String> {
    let query = format!("{{ products{arguments} {{ name }} }}");
    graphql(server, &query, accept, auth).await["data"]["products"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_localized_catalog_example_works_end_to_end() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping the localized-catalog example: DATABASE_URL not set");
        return;
    };
    let scratch = provision(&url).await;
    let server = start(&scratch, compile_example().await).await;
    let alice = token("alice", ALICE_TENANT);
    // A visitor, in the guest tenant (stored locale en-US).
    let visitor = token("visitor", "00000000-0000-0000-0000-0000000000c3");
    let sorted = "(orderBy: { name: ASC })";

    // The browser's locale wins over the stored one: fr-CA → fr-FR → fr → en-US.
    assert_eq!(
        names(&server, sorted, Some("fr-CA"), Some(&visitor)).await,
        ["Cerise", "Poire", "Pomme"]
    );
    // The sort follows the locale.
    assert_eq!(
        names(&server, sorted, Some("en-US"), Some(&visitor)).await,
        ["Apple", "Cherry", "Pear"]
    );
    // Alice sends no header: her tenant's stored locale (de-DE) through the enrichment.
    assert_eq!(names(&server, sorted, None, Some(&alice)).await, ["Apfel", "Birne", "Kirsche"]);

    // Her orders carry the label the view rendered from the tenant's stored locale: a stored
    // fact, so the request's locale does not change it.
    for accept in [None, Some("fr-FR")] {
        let orders =
            graphql(&server, "{ tenantOrders { quantity productLabel } }", accept, Some(&alice))
                .await;
        assert_eq!(
            orders["data"]["tenantOrders"],
            json!([{"quantity": 3, "productLabel": "Apfel"}]),
            "{accept:?}"
        );
    }

    // The cache keeps each locale apart: the same query, French then German.
    assert_eq!(
        names(&server, "", Some("fr-FR"), Some(&visitor)).await,
        ["Pomme", "Poire", "Cerise"]
    );
    assert_eq!(
        names(&server, "", Some("de-DE"), Some(&visitor)).await,
        ["Apfel", "Birne", "Kirsche"]
    );

    // A write in the request locale merges into the stored map; the example's write guard
    // would have refused it had the write session carried a locale.
    let renamed = graphql(
        &server,
        &format!(r#"mutation {{ renameProduct(id: "{APPLE}", name: {{ value: "Pomme rouge" }}) {{ name }} }}"#),
        Some("fr-FR"),
        Some(&alice),
    )
    .await;
    assert_eq!(renamed["data"]["renameProduct"]["name"], json!("Pomme rouge"), "{renamed}");
    let db = client(&scratch).await;
    let stored: Value = db
        .query_one(&format!("SELECT data->'name' FROM tv_product WHERE id = '{APPLE}'"), &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(stored, json!({"en-US": "Apple", "fr-FR": "Pomme rouge", "de-DE": "Apfel"}));

    // The projection gate: the example's catalog check finds nothing reading the setting…
    let check =
        std::fs::read_to_string(example_dir().join("sql/check_locale_free_projections.sql"))
            .unwrap();
    let offenders = |rows: Vec<tokio_postgres::Row>| -> Vec<String> {
        rows.iter()
            .map(|r| format!("{} {}", r.get::<_, String>(0), r.get::<_, String>(1)))
            .collect()
    };
    assert_eq!(offenders(db.query(check.as_str(), &[]).await.unwrap()), Vec::<String>::new());
    // …and finds a view that does.
    db.batch_execute(
        "CREATE VIEW v_product_label AS SELECT id, data->'name'->>current_setting('fraiseql.locale', true) AS label FROM tv_product",
    )
    .await
    .unwrap();
    assert_eq!(
        offenders(db.query(check.as_str(), &[]).await.unwrap()),
        ["view public.v_product_label".to_string()]
    );

    drop(server);
    drop(db);
    let _ = client(&url)
        .await
        .batch_execute(&format!("DROP DATABASE IF EXISTS {DATABASE} WITH (FORCE)"))
        .await;
}

/// Every code block in `docs/guides/localization.md` is copied from the example, which the
/// test above runs: a block that is in none of the example's files has drifted from what is
/// executed.
#[test]
fn the_localization_guide_quotes_the_example() {
    let guide =
        std::fs::read_to_string(example_dir().join("../../docs/guides/localization.md")).unwrap();
    let sources: Vec<String> = [
        "schema.py",
        "fraiseql.toml",
        "sql/01_schema.sql",
        "sql/check_locale_free_projections.sql",
    ]
    .iter()
    .map(|f| std::fs::read_to_string(example_dir().join(f)).unwrap())
    .collect();
    let mut blocks = Vec::new();
    let mut lines = guide.lines();
    while let Some(line) = lines.next() {
        if line.starts_with("```") {
            let block: Vec<&str> = lines.by_ref().take_while(|l| !l.starts_with("```")).collect();
            blocks.push(block.join("\n"));
        }
    }
    assert!(blocks.len() >= 6, "the guide's blocks were found: {}", blocks.len());
    for block in blocks {
        assert!(
            sources.iter().any(|source| source.contains(&block)),
            "a guide block is not in the example:\n{block}"
        );
    }
}

/// The example compiles and loads with no database.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    let compiled = compile_example().await;
    assert!(compiled.locale.is_some(), "the example declares [locale]");
}
