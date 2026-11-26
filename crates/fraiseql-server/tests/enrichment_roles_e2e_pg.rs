//! #1858: a role kept in an application table drives the compiled schema's own gates.
//!
//! `[identity.enrichment] roles` names a column of the actor row; its values join the
//! principal's roles before dispatch. The token carries **only** `sub`, so every role a
//! case below observes came from the database.
//!
//! What is pinned:
//!
//! * `requires_role` on a query admits the row's role and refuses without it;
//! * `requires_scope` with `on_deny = "mask"`, granted through `role_definitions` (`cost_reader` →
//!   `read:cost`), masks the field for a viewer, serves it to a cost reader, and refuses `where` /
//!   `orderBy` on it for the viewer;
//! * a name no schema reader mentions (`pricng`, `admin`) grants nothing;
//! * a NULL roles column refuses the identity (403), as a NULL mapped column does;
//! * an agent acting for a cost reader keeps the token's roles only;
//! * a revoked role keeps working until `cache_ttl_secs`, then stops;
//! * REST answers the role gate as `/graphql` does.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test`
//! leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_p1858_*` fixtures → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        CompiledSchema, FieldDenyPolicy, FieldType, RestConfig, RoleDefinition, SecurityConfig,
    },
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::{issuer::TestIssuer, try_database_url};
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

const ACTOR_TABLE: &str = "tf_p1858_actor";
const INVOICE_TABLE: &str = "tf_p1858_invoice";
const INVOICE_VIEW: &str = "v_p1858_invoice";

/// Holds `viewer` only: reads invoices, never their costs.
const VIEWER: &str = "p1858-viewer";
/// Holds `pricing` and `cost_reader`.
const PRICER: &str = "p1858-pricer";
/// Holds a mistyped `pricng`, which no schema reader names.
const TYPO: &str = "p1858-typo";
/// Holds `admin`, which this schema never names.
const ADMIN_ROW: &str = "p1858-admin-row";
/// Its roles column is NULL.
const NULL_ROW: &str = "p1858-null-row";

const INVOICES: usize = 3;

async fn seed(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        format!("DROP VIEW IF EXISTS {INVOICE_VIEW}"),
        format!("DROP TABLE IF EXISTS {INVOICE_TABLE}"),
        format!("DROP TABLE IF EXISTS {ACTOR_TABLE}"),
        format!("CREATE TABLE {ACTOR_TABLE} (sub text PRIMARY KEY, role text[])"),
        format!(
            "INSERT INTO {ACTOR_TABLE} VALUES \
             ('{VIEWER}', ARRAY['viewer']), \
             ('{PRICER}', ARRAY['viewer', 'pricing', 'cost_reader']), \
             ('{TYPO}', ARRAY['viewer', 'pricng']), \
             ('{ADMIN_ROW}', ARRAY['admin']), \
             ('{NULL_ROW}', NULL)"
        ),
        format!("CREATE TABLE {INVOICE_TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
    ];
    for id in 1..=INVOICES {
        let data = json!({"id": id, "label": format!("invoice-{id}"), "unit_cost": id * 10});
        stmts.push(format!("INSERT INTO {INVOICE_TABLE} VALUES ({id}, '{data}'::jsonb)"));
    }
    stmts.push(format!("CREATE VIEW {INVOICE_VIEW} AS SELECT id, data FROM {INVOICE_TABLE}"));

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// `invoices` is open to any principal; `invoicePrices` requires `pricing`. `unit_cost`
/// requires `read:cost`, which only `cost_reader` grants, and is masked when denied.
fn build_schema() -> CompiledSchema {
    let invoice = |name: &str| {
        TestTypeBuilder::new(name, INVOICE_VIEW)
            .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
            .with_field(TestFieldBuilder::new("label", FieldType::String).build())
            .with_field(
                TestFieldBuilder::new("unit_cost", FieldType::Int)
                    .requires_scope("read:cost")
                    .on_deny(FieldDenyPolicy::Mask)
                    .build(),
            )
            .build()
    };

    let mut security = SecurityConfig::new();
    security.add_role(RoleDefinition::new("viewer", vec![]));
    security.add_role(RoleDefinition::new("pricing", vec![]));
    security.add_role(RoleDefinition::new("cost_reader", vec!["read:cost".to_string()]));

    let mut schema = TestSchemaBuilder::new()
        .with_type(invoice("P1858Invoice"))
        .with_type(invoice("P1858Price"))
        .with_query(
            TestQueryBuilder::new("invoices", "P1858Invoice")
                .returns_list(true)
                .with_sql_source(INVOICE_VIEW)
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("invoicePrices", "P1858Price")
                .returns_list(true)
                .with_sql_source(INVOICE_VIEW)
                .requires_role("pricing")
                .build(),
        )
        .with_security(security)
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// `[auth]` trusting `issuer`, and `[identity.enrichment]` taking roles from `role`.
fn server_config(issuer: &TestIssuer, cache_ttl_secs: u64) -> ServerConfig {
    let mut config = ServerConfig {
        auth: Some(common::test_issuer::oidc_config(issuer, true)),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    config.identity = serde_json::from_value(json!({
        "enrichment": {
            "enabled": true,
            "query": format!("SELECT role FROM {ACTOR_TABLE} WHERE sub = $sub"),
            "roles": "role",
            "cache_ttl_secs": cache_ttl_secs,
        }
    }))
    .expect("[identity.enrichment] config");
    config
}

type Mount = (String, tokio::sync::oneshot::Sender<()>, TestIssuer, Arc<PostgresAdapter>);

async fn start(cache_ttl_secs: u64) -> Option<Mount> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect to the test database"));
    seed(&adapter).await;

    let issuer = TestIssuer::start().await;
    let mut config = server_config(&issuer, cache_ttl_secs);
    config.database_url = url;
    let pool = sqlx::PgPool::connect(&config.database_url).await.expect("enrichment pool");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(config, build_schema(), adapter.clone(), Some(pool)))
        .await
        .expect("Server::new with [auth] and [identity.enrichment] roles");

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Some((format!("http://127.0.0.1:{port}"), tx, issuer, adapter))
}

/// A token carrying `sub` and no role.
fn token_for(issuer: &TestIssuer, sub: &str) -> String {
    issuer.mint(&issuer.claims().set("sub", sub))
}

struct Answer {
    status: u16,
    body:   Value,
}

impl Answer {
    /// The rows of `data.<root>`, or `None` when the root is absent or null.
    fn rows(&self, root: &str) -> Option<&Vec<Value>> {
        self.body.get("data").and_then(|d| d.get(root)).and_then(Value::as_array)
    }

    fn has_errors(&self) -> bool {
        self.body
            .get("errors")
            .is_some_and(|e| e.as_array().is_some_and(|e| !e.is_empty()))
    }
}

async fn graphql_post(base: &str, query: &str, token: &str) -> Answer {
    let response = reqwest::Client::new()
        .post(format!("{base}/graphql"))
        .header("authorization", format!("Bearer {token}"))
        .json(&json!({ "query": query }))
        .send()
        .await
        .expect("GraphQL POST");
    let status = response.status().as_u16();
    let text = response.text().await.expect("response body");
    Answer {
        status,
        body: serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text })),
    }
}

async fn rest_get(base: &str, resource: &str, token: &str) -> Answer {
    let response = reqwest::Client::new()
        .get(format!("{base}/rest/v1/{resource}"))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("REST GET");
    let status = response.status().as_u16();
    let text = response.text().await.expect("response body");
    Answer {
        status,
        body: serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text })),
    }
}

/// Whether `answer` is the role gate's refusal: no rows for `root`.
fn refused(answer: &Answer, root: &str) -> bool {
    answer.rows(root).is_none() && (answer.has_errors() || answer.status >= 400)
}

const PRICES: &str = "{ invoicePrices { id } }";
const COSTS: &str = "{ invoices { id unit_cost } }";

#[tokio::test]
async fn a_role_from_the_actor_table_opens_a_role_gated_query() {
    let Some((base, shutdown, issuer, _)) = start(60).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };

    let pricer = graphql_post(&base, PRICES, &token_for(&issuer, PRICER)).await;
    assert_eq!(
        pricer.rows("invoicePrices").map(Vec::len),
        Some(INVOICES),
        "`pricing` is in the row, not the token: only the enrichment can have granted it. \
         Body: {}",
        pricer.body
    );

    let viewer = graphql_post(&base, PRICES, &token_for(&issuer, VIEWER)).await;
    assert!(refused(&viewer, "invoicePrices"), "a viewer is refused. Body: {}", viewer.body);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn a_role_no_schema_reader_names_grants_nothing() {
    let Some((base, shutdown, issuer, _)) = start(60).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };

    let typo = graphql_post(&base, PRICES, &token_for(&issuer, TYPO)).await;
    assert!(refused(&typo, "invoicePrices"), "`pricng` opens nothing. Body: {}", typo.body);
    let typo_invoices = graphql_post(&base, COSTS, &token_for(&issuer, TYPO)).await;
    assert_eq!(
        typo_invoices.rows("invoices").map(Vec::len),
        Some(INVOICES),
        "and refuses nothing: the subject still reads what `viewer` reads. Body: {}",
        typo_invoices.body
    );

    let admin = graphql_post(&base, PRICES, &token_for(&issuer, ADMIN_ROW)).await;
    assert!(
        refused(&admin, "invoicePrices"),
        "`admin` has no meaning this schema does not give it. Body: {}",
        admin.body
    );
    let admin_costs = graphql_post(&base, COSTS, &token_for(&issuer, ADMIN_ROW)).await;
    let masked = admin_costs
        .rows("invoices")
        .is_some_and(|rows| rows.iter().all(|row| row["unit_cost"].is_null()));
    assert!(masked, "nor any scope. Body: {}", admin_costs.body);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn a_scope_granted_through_a_table_role_unmasks_the_field_and_its_cache_is_its_own() {
    let Some((base, shutdown, issuer, _)) = start(60).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };

    // The cost reader first, so a response cached for it is there to be wrongly served.
    let pricer = graphql_post(&base, COSTS, &token_for(&issuer, PRICER)).await;
    let costs: Vec<Value> = pricer
        .rows("invoices")
        .map_or_default(|rows| rows.iter().map(|row| row["unit_cost"].clone()).collect());
    assert_eq!(costs, [json!(10), json!(20), json!(30)], "Body: {}", pricer.body);

    let viewer = graphql_post(&base, COSTS, &token_for(&issuer, VIEWER)).await;
    let rows = viewer.rows("invoices").expect("the viewer reads invoices");
    assert_eq!(rows.len(), INVOICES, "Body: {}", viewer.body);
    assert!(
        rows.iter().all(|row| row["unit_cost"].is_null()),
        "the viewer holds no `read:cost`: masked, never the cost reader's cached answer. \
         Body: {}",
        viewer.body
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn a_viewer_cannot_filter_or_sort_on_the_masked_field() {
    let Some((base, shutdown, issuer, _)) = start(60).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };

    for query in [
        "{ invoices(where: { unit_cost: { gt: 15 } }) { id } }",
        "{ invoices(orderBy: [{field: unit_cost, direction: DESC}]) { id } }",
    ] {
        let pricer = graphql_post(&base, query, &token_for(&issuer, PRICER)).await;
        assert!(
            pricer.rows("invoices").is_some() && !pricer.has_errors(),
            "control: the cost reader may filter and sort on it ({query}). Body: {}",
            pricer.body
        );
        let viewer = graphql_post(&base, query, &token_for(&issuer, VIEWER)).await;
        assert!(
            refused(&viewer, "invoices"),
            "a viewer may not probe the cost through {query}. Body: {}",
            viewer.body
        );
    }

    let _ = shutdown.send(());
}

#[tokio::test]
async fn a_null_roles_column_refuses_the_identity() {
    let Some((base, shutdown, issuer, _)) = start(60).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };

    let answer = graphql_post(&base, "{ invoices { id } }", &token_for(&issuer, NULL_ROW)).await;
    assert_eq!(
        answer.status, 403,
        "the roles column is part of the identity: NULL denies it whole. Body: {}",
        answer.body
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn an_agent_acting_for_a_cost_reader_keeps_the_tokens_roles_only() {
    let Some((base, shutdown, issuer, _)) = start(60).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };

    let delegated =
        issuer.mint(&issuer.claims().set("sub", PRICER).set("act", json!({"sub": "agent-7"})));
    let answer = graphql_post(&base, PRICES, &delegated).await;
    assert!(
        refused(&answer, "invoicePrices"),
        "a delegate never inherits what is stored for the person. Body: {}",
        answer.body
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn a_revoked_role_holds_until_the_cache_ttl_then_stops() {
    let Some((base, shutdown, issuer, adapter)) = start(1).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };
    let token = token_for(&issuer, PRICER);

    let before = graphql_post(&base, PRICES, &token).await;
    assert_eq!(before.rows("invoicePrices").map(Vec::len), Some(INVOICES), "{}", before.body);

    let _: Vec<std::collections::HashMap<String, Value>> = adapter
        .execute_raw_query(&format!(
            "UPDATE {ACTOR_TABLE} SET role = ARRAY['viewer'] WHERE sub = '{PRICER}'"
        ))
        .await
        .expect("revoke `pricing`");

    let cached = graphql_post(&base, PRICES, &token).await;
    assert_eq!(
        cached.rows("invoicePrices").map(Vec::len),
        Some(INVOICES),
        "within cache_ttl_secs the cached identity still holds the role. Body: {}",
        cached.body
    );

    tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;
    let after = graphql_post(&base, PRICES, &token).await;
    assert!(
        refused(&after, "invoicePrices"),
        "past cache_ttl_secs the removal applies. Body: {}",
        after.body
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn rest_answers_the_role_gate_as_graphql_does() {
    let Some((base, shutdown, issuer, _)) = start(60).await else {
        eprintln!("skipping #1858 enrichment roles: DATABASE_URL not set");
        return;
    };

    let pricer = rest_get(&base, "invoicePrices", &token_for(&issuer, PRICER)).await;
    assert_eq!(pricer.status, 200, "Body: {}", pricer.body);
    let viewer = rest_get(&base, "invoicePrices", &token_for(&issuer, VIEWER)).await;
    assert!(
        viewer.status >= 400,
        "the viewer holds no `pricing` on REST either. Status {}, body: {}",
        viewer.status,
        viewer.body
    );

    let _ = shutdown.send(());
}
