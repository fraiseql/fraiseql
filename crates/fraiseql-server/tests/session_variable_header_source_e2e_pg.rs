//! A `source = "header"` session variable is the value of the request's HTTP header (#1520).
//!
//! It used to read `SecurityContext.attributes`, whose only writer on the request path copies
//! **JWT claims**: a mapping of `app.region` to the `x-region` header resolved to a claim named
//! `x-region`, or to nothing. And a request without a principal got no session variables at
//! all, `literal` ones included. This suite boots a real server against PostgreSQL and reads
//! the variables back through a view that selects `current_setting(…, true)`.
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
                .with_field(TestFieldBuilder::nullable("flavor", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("subject", FieldType::String).build())
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
        variables:         vec![
            SessionVariableMapping {
                name:   "app.region".to_string(),
                source: SessionVariableSource::Header {
                    header: "x-region".to_string(),
                },
            },
            SessionVariableMapping {
                name:   "app.flavor".to_string(),
                source: SessionVariableSource::Literal {
                    value: "vanilla".to_string(),
                },
            },
            SessionVariableMapping {
                name:   "app.subject".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "sub".to_string(),
                },
            },
        ],
        inject_started_at: false,
    };
    schema.build_indexes();
    schema
}

fn token(extra: &Value) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut claims =
        json!({ "sub": "probe", "iss": ISSUER, "aud": AUDIENCE, "iat": now, "exp": now + 3600 });
    if let (Some(claims), Some(extra)) = (claims.as_object_mut(), extra.as_object()) {
        claims.extend(extra.clone());
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

/// A running server against `url`, with HS256 authentication or none (every request is
/// anonymous), and the sender that stops it.
async fn serve(url: String, authenticated: bool) -> (u16, tokio::sync::oneshot::Sender<()>) {
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    for ddl in [
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!(
            "CREATE VIEW {VIEW} AS SELECT 1 AS id, jsonb_build_object('id', 1, \
             'region', current_setting('app.region', true), \
             'flavor', current_setting('app.flavor', true), \
             'subject', current_setting('app.subject', true)) AS data"
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
    (port, tx)
}

/// Whether a probed setting is unset. A pooled connection that once had a setting applied
/// transaction-locally reads it back as `''`, not `NULL`, once the transaction ends.
fn unset(value: &Value) -> bool {
    value.is_null() || value == &json!("")
}

/// `POST /graphql` reading the probe, with `token` (if any) and `headers`.
async fn probe(port: u16, token: Option<String>, headers: &[(&str, &str)]) -> Value {
    let mut request = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .json(&json!({ "query": "{ regionProbes { region flavor subject } }" }));
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    serde_json::from_str(&text)
        .unwrap_or_else(|_| json!({ "status": status.as_u16(), "body": text }))
}

#[tokio::test]
async fn a_header_session_variable_reads_the_request_header() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (port, stop) = serve(url.clone(), true).await;
    let sent = probe(port, Some(token(&json!({}))), &[("x-region", "eu")]).await;
    let claimed = probe(port, Some(token(&json!({ "x-region": "claim" }))), &[]).await;
    let _ = stop.send(());

    let (port, stop) = serve(url, false).await;
    let anonymous = probe(port, None, &[("x-region", "eu")]).await;
    let twice = probe(port, None, &[("x-region", "eu"), ("x-region", "us")]).await;
    let _ = stop.send(());

    assert_eq!(
        sent["data"]["regionProbes"][0]["region"],
        json!("eu"),
        "app.region is the x-region header the request sent: {sent}"
    );
    assert!(
        unset(&claimed["data"]["regionProbes"][0]["region"]),
        "a claim named x-region is not the header, and no header was sent: {claimed}"
    );
    let row = &anonymous["data"]["regionProbes"][0];
    assert!(
        row["region"] == json!("eu") && row["flavor"] == json!("vanilla") && unset(&row["subject"]),
        "an anonymous request gets its header and literal variables, not the claim: {anonymous}"
    );
    assert!(
        twice["data"].is_null()
            && twice["errors"][0]["message"]
                .as_str()
                .is_some_and(|m| m.contains("more than once")),
        "a header sent twice is refused, not joined: {twice}"
    );
}
