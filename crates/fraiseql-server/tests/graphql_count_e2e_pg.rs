//! #938: the `<name>Count` sibling query, end to end against PostgreSQL.
//!
//! Drives the real `Server::serve_on_listener` mount over a real socket. A count
//! query is a second door onto rows the list query guards — it answers "how many
//! rows match?" without returning one — so the properties worth pinning here are
//! mostly *refusals*, and none of them can be observed from a unit test that
//! calls the executor directly: the `requires_role` and anonymous guards live at
//! the mount, and the tenant scoping is a WHERE clause the database applies.
//!
//! What a wrong implementation looks like, and what catches it:
//!
//! | Defect | Caught by |
//! |---|---|
//! | count honours `limit`/`offset` | `count_is_independent_of_limit_and_offset` |
//! | count ignores `where` | `count_reflects_the_filter` |
//! | count ignores `inject_params` (cross-tenant row-count oracle) | `count_is_tenant_scoped` |
//! | count ignores `requires_role` | `count_inherits_requires_role` |
//! | count answers anonymously what the list refuses | `count_refuses_anonymous_when_list_does` |
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p19_count` schema → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{CompiledSchema, FieldType, QueryDefinition, TypeDefinition},
};
use fraiseql_server::server_config::{Hs256Config, ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

const SCHEMA: &str = "p19_count";
const SECRET_ENV: &str = "FRAISEQL_TEST_P19_COUNT_HS256_SECRET";
const SECRET: &str = "p19-count-secret-0123456789-0123456789";
const ISSUER: &str = "https://count.test.fraiseql";
const AUDIENCE: &str = "count-test";

const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";
const TENANT_B: &str = "22222222-2222-2222-2222-222222222222";

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// Seven rows over two tenants, with two `archived` rows in tenant A.
///
/// The counts are deliberately all different from each other and from the page
/// sizes used below — 7 total, 5 in tenant A, 3 unarchived in tenant A, 2 in
/// tenant B — so an implementation that returns a page length, a tenant total or
/// the grand total cannot coincide with the right answer.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_item (
               id bigint PRIMARY KEY,
               tenant_id uuid NOT NULL,
               label text NOT NULL,
               archived boolean NOT NULL DEFAULT false
             )"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_item (id, tenant_id, label, archived) VALUES
               (1,'{TENANT_A}','a-one',   false),
               (2,'{TENANT_A}','a-two',   false),
               (3,'{TENANT_A}','a-three', false),
               (4,'{TENANT_A}','a-four',  true),
               (5,'{TENANT_A}','a-five',  true),
               (6,'{TENANT_B}','b-one',   false),
               (7,'{TENANT_B}','b-two',   false)"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_item AS
               SELECT id, tenant_id,
                      jsonb_build_object(
                        'id', id, 'label', label, 'archived', archived,
                        -- The inject filter lowers to `data->>'tenant_id'` unless the
                        -- schema was compiled against a live database (which populates
                        -- `native_columns`), so the scoping key has to be in the JSONB.
                        'tenant_id', tenant_id::text
                      ) AS data
               FROM {SCHEMA}.tb_item ORDER BY id"
        ),
    ];
    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn list_query(name: &str) -> QueryDefinition {
    let mut q = QueryDefinition::new(name, "CountItem")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_item"));
    q.auto_params.has_where = true;
    q.auto_params.has_limit = true;
    q.auto_params.has_offset = true;
    q.auto_params.has_order_by = true;
    q
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut item = TypeDefinition::new("CountItem", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        fraiseql_core::schema::FieldDefinition::new("id", FieldType::Int),
        fraiseql_core::schema::FieldDefinition::new("label", FieldType::String),
        fraiseql_core::schema::FieldDefinition::new("archived", FieldType::Boolean),
    ];
    schema.types.push(item);

    // Plain list + its count sibling.
    let items = list_query("items");
    schema.queries.push(items.count_sibling());
    schema.queries.push(items);

    // Tenant-scoped list: the `tenant_id` column is filtered from the caller's
    // `org_id` claim, so neither the list nor its count can be asked about
    // another tenant.
    let mut scoped = list_query("scopedItems");
    scoped.inject_params.insert(
        "tenant_id".to_string(),
        fraiseql_core::schema::security_config::InjectedParamSource::Jwt("org_id".to_string()),
    );
    schema.queries.push(scoped.count_sibling());
    schema.queries.push(scoped);

    // Role-gated list.
    let mut gated = list_query("gatedItems");
    gated.requires_role = Some("auditor".to_string());
    schema.queries.push(gated.count_sibling());
    schema.queries.push(gated);

    schema.build_indexes();
    schema
}

fn config() -> ServerConfig {
    // The server reads the secret from the environment when it boots, so it must
    // be set before `TestServer::start_with_config`. This binary owns the whole
    // process and runs single-threaded, so there is no sibling to race.
    std::env::set_var(SECRET_ENV, SECRET);
    ServerConfig {
        cors_enabled: false,
        // Off by default for production safety; the count sibling has to be
        // visible to generated clients, which is what this asserts.
        introspection_enabled: true,
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        ..ServerConfig::default()
    }
}

fn mint_token(tenant: Option<&str>, roles: &[&str]) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let mut claims = json!({
        "sub": "count-user",
        "iss": ISSUER,
        "aud": AUDIENCE,
        "iat": now,
        "exp": now + 3600,
        "roles": roles,
    });
    if let Some(t) = tenant {
        claims["org_id"] = json!(t);
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint token")
}

async fn gql_raw(
    server: &TestServer,
    query: &str,
    bearer: Option<&str>,
) -> (reqwest::StatusCode, String) {
    let mut req = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("content-type", "application/json")
        .json(&json!({ "query": query }));
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.expect("request");
    let status = resp.status();
    (status, resp.text().await.expect("body"))
}

async fn gql(server: &TestServer, query: &str, bearer: Option<&str>) -> Value {
    let (status, body) = gql_raw(server, query, bearer).await;
    serde_json::from_str(&body)
        .unwrap_or_else(|e| panic!("expected a JSON body, got {status}: {body:?} ({e})"))
}

async fn boot() -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("adapter"));
    seed(&adapter).await;
    Some(Box::pin(TestServer::start_with_config(config(), schema(), adapter)).await)
}

// ── The count is the total, not the page ─────────────────────────────────────

/// The issue's core ask: a client paging by offset needs the size of the whole
/// filtered set, or it cannot render page numbers. A count that moved with
/// `limit`/`offset` would return 2 here and look plausible in every hand test
/// that happens to request page 1.
#[tokio::test]
async fn count_is_independent_of_limit_and_offset() {
    if database_url_or_skip("count_is_independent_of_limit_and_offset").is_none() {
        return;
    }
    let server = Box::pin(boot()).await.unwrap();
    let token = mint_token(None, &[]);

    let body = gql(&server, "{ items(limit: 2, offset: 4) { id } itemsCount }", Some(&token)).await;

    assert_eq!(
        body["data"]["items"].as_array().map(Vec::len),
        Some(2),
        "the page itself is still limited: {body}"
    );
    assert_eq!(
        body["data"]["itemsCount"].as_i64(),
        Some(7),
        "the count must describe the full set, not the page: {body}"
    );
}

/// The count composes with the same `where` the list does. A count that ignored
/// the filter would return the table size — right for an unfiltered first page,
/// wrong for every filtered one.
#[tokio::test]
async fn count_reflects_the_filter() {
    if database_url_or_skip("count_reflects_the_filter").is_none() {
        return;
    }
    let server = Box::pin(boot()).await.unwrap();
    let token = mint_token(None, &[]);

    let body =
        gql(&server, r"{ itemsCount(where: { archived: { eq: false } }) }", Some(&token)).await;

    assert_eq!(
        body["data"]["itemsCount"].as_i64(),
        Some(5),
        "5 of the 7 rows are unarchived: {body}"
    );
}

// ── The count inherits every restriction of the list it counts ───────────────

/// A count that ignored `inject_params` would be a cross-tenant row-count
/// oracle: it returns no row, so nothing looks leaked, while reporting exactly
/// how many records another tenant holds — and it would pass any test that only
/// checks the returned rows.
#[tokio::test]
async fn count_is_tenant_scoped() {
    if database_url_or_skip("count_is_tenant_scoped").is_none() {
        return;
    }
    let server = Box::pin(boot()).await.unwrap();

    let a = gql(&server, "{ scopedItemsCount }", Some(&mint_token(Some(TENANT_A), &[]))).await;
    assert_eq!(
        a["data"]["scopedItemsCount"].as_i64(),
        Some(5),
        "tenant A holds 5 of the 7 rows: {a}"
    );

    let b = gql(&server, "{ scopedItemsCount }", Some(&mint_token(Some(TENANT_B), &[]))).await;
    assert_eq!(
        b["data"]["scopedItemsCount"].as_i64(),
        Some(2),
        "tenant B must not learn tenant A's row count: {b}"
    );
}

/// `requires_role` on the list must gate its count identically — and with the
/// same "not found" wording, so the count cannot be used to enumerate which
/// role-gated queries exist.
#[tokio::test]
async fn count_inherits_requires_role() {
    if database_url_or_skip("count_inherits_requires_role").is_none() {
        return;
    }
    let server = Box::pin(boot()).await.unwrap();

    let denied = gql(&server, "{ gatedItemsCount }", Some(&mint_token(None, &["viewer"]))).await;
    let msg = denied["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("not found in schema"),
        "a caller without the role must not distinguish a gated count from a \
         missing one: {denied}"
    );
    assert!(
        denied["data"]["gatedItemsCount"].as_i64().is_none(),
        "no count may be returned to an unauthorized caller: {denied}"
    );

    let allowed = gql(&server, "{ gatedItemsCount }", Some(&mint_token(None, &["auditor"]))).await;
    assert_eq!(
        allowed["data"]["gatedItemsCount"].as_i64(),
        Some(7),
        "the role holder gets the count: {allowed}"
    );
}

/// An anonymous caller is refused a count on an inject-scoped query for the same
/// reason it is refused the list: without a principal there is no tenant to scope
/// to, and "count everything" is the one answer that must never be the fallback.
#[tokio::test]
async fn count_refuses_anonymous_when_list_does() {
    if database_url_or_skip("count_refuses_anonymous_when_list_does").is_none() {
        return;
    }
    let server = Box::pin(boot()).await.unwrap();

    let (status, body) = gql_raw(&server, "{ scopedItemsCount }", None).await;
    assert!(
        !status.is_success(),
        "an anonymous count over a tenant-scoped query must not succeed; got {status}: {body}"
    );
    assert!(
        !body.contains("\"scopedItemsCount\":"),
        "no count may appear in the refusal body: {body}"
    );
}

// ── Schema surface ───────────────────────────────────────────────────────────

/// The sibling is a real schema field, so generated clients and tooling see it.
/// `Int!` specifically: a nullable count would put "matched nothing" and "could
/// not answer" in the same branch for every consumer.
#[tokio::test]
async fn count_is_introspectable_as_non_null_int() {
    if database_url_or_skip("count_is_introspectable_as_non_null_int").is_none() {
        return;
    }
    let server = Box::pin(boot()).await.unwrap();
    let token = mint_token(None, &[]);

    // `__schema.queryType` is a shallow type reference (`{ name: "Query" }`); the
    // field list lives on the matching entry of `__schema.types`.
    let body = gql(
        &server,
        "{ __schema { types { name fields { name args { name } type { kind ofType { name } } } } } }",
        Some(&token),
    )
    .await;

    let types = body["data"]["__schema"]["types"]
        .as_array()
        .unwrap_or_else(|| panic!("introspection must list types: {body}"));
    let query_type = types
        .iter()
        .find(|t| t["name"] == "Query")
        .unwrap_or_else(|| panic!("introspection must expose the Query type: {body}"));
    let fields = query_type["fields"].as_array().expect("Query fields");
    let field = fields
        .iter()
        .find(|f| f["name"] == "itemsCount")
        .unwrap_or_else(|| panic!("itemsCount must be introspectable: {query_type}"));

    assert_eq!(field["type"]["kind"], "NON_NULL", "count must be non-null: {field}");
    assert_eq!(field["type"]["ofType"]["name"], "Int", "count must be Int: {field}");

    // `where` and nothing else — a count advertising `limit`/`offset` would offer
    // a total that moves with the page, which is the defect the runtime test rules
    // out. Pinning the argument list here stops it being reintroduced as surface.
    let args: Vec<&str> = field["args"]
        .as_array()
        .expect("args")
        .iter()
        .filter_map(|a| a["name"].as_str())
        .collect();
    assert_eq!(args, vec!["where"], "the count takes the filter and nothing else: {field}");
}
