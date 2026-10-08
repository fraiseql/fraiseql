//! A `source = "header"` session variable is documented as "the value of an HTTP request
//! header". Nothing in the server feeds it one.
//!
//! `resolve_session_variables` reads a `Header` mapping from `SecurityContext.attributes`,
//! and the only writer of attributes on the request path copies **JWT claims**. So a mapping
//! of `app.region` to the `x-region` header resolves to a claim named `x-region`, or to
//! nothing: the header itself is never read. This suite boots a real server against
//! PostgreSQL, sends `x-region: eu` with a token that has no such claim, and reads the
//! variable back through a view that selects `current_setting('app.region', true)`.
//!
//! Found while planning #1512 (request locale), which therefore does not route the locale
//! through `attributes`. Kept `#[ignore]` until the finding is fixed: it is the RED for it.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `v_issue_header_var` view and sets a
//! process-global env var for the HS256 secret → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        CompiledSchema, FieldType, SessionVariableMapping, SessionVariableSource,
        SessionVariablesConfig,
    },
};
use fraiseql_server::{
    Server,
    server_config::{Hs256Config, ServerConfig},
};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

const VIEW: &str = "v_issue_header_var";
const SECRET: &str = "fraiseql-hdrvar-secret-exactly-32";
const SECRET_ENV: &str = "FRAISEQL_HDRVAR_HS256_SECRET";
const ISSUER: &str = "https://hdrvar.fraiseql.test";
const AUDIENCE: &str = "fraiseql-hdrvar-api";

fn schema() -> CompiledSchema {
    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("RegionProbe", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::nullable("region", FieldType::String).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("regionProbes", "RegionProbe")
                .returns_list(true)
                .with_sql_source(VIEW)
                .build(),
        )
        .build();
    schema.session_variables = SessionVariablesConfig {
        variables:         vec![SessionVariableMapping {
            name:   "app.region".to_string(),
            source: SessionVariableSource::Header {
                header: "x-region".to_string(),
            },
        }],
        inject_started_at: false,
    };
    schema.build_indexes();
    schema
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

#[tokio::test]
#[ignore = "#1520: a `source = \"header\"` session variable is never fed from the HTTP header"]
async fn a_header_session_variable_reads_the_request_header() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    for ddl in [
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!(
            "CREATE VIEW {VIEW} AS SELECT 1 AS id, jsonb_build_object('id', 1, 'region', \
             current_setting('app.region', true)) AS data"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    std::env::set_var(SECRET_ENV, SECRET);
    let config = ServerConfig {
        database_url: url,
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema(), adapter, None)).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
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
        .header("authorization", format!("Bearer {}", token()))
        .header("x-region", "eu")
        .json(&json!({ "query": "{ regionProbes { region } }" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let _ = tx.send(());

    assert_eq!(
        body["data"]["regionProbes"][0]["region"],
        json!("eu"),
        "app.region is the x-region header the request sent: {body}"
    );
}
