//! #1799 regression: a fact table's aggregate and window roots must not read across
//! tenants in a schema that scopes its reads by `inject_params`.
//!
//! An aggregate or window root has no `QueryDefinition`, so no `inject_params` of its own.
//! Its only tenant clause came from `RuntimeConfig::rls_policy`, which the server never
//! installs. So in a schema whose list roots inject the caller's tenant, the analytics
//! roots over the same tenants' rows carried no predicate at all: tenant B's aggregate
//! counted tenant A's rows, and an anonymous one counted everyone's.
//!
//! A fact table is scoped in such a schema when its linked type is read through injected
//! queries, or when one of its denormalized filter columns is a column the schema
//! injects. Nothing scoping it, the server refuses the root rather than read across
//! tenants. Three declarations scope it: the fact table's own `inject_params` (#1800), its
//! linked type's, which it inherits, and `row_scope = "relation"` for a relation that
//! filters its own rows.
//!
//! Drives the real `Server::serve_on_listener` mount over a real socket, with bearer
//! tokens from a test issuer, against real PostgreSQL.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_tenant_probe` fixture → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, MeasureColumn,
        SqlType,
    },
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        CompiledSchema, FieldType, InjectedParamSource, SessionVariableMapping,
        SessionVariableSource, SessionVariablesConfig,
    },
};
use fraiseql_server::server_config::ServerConfig;
use fraiseql_test_support::{issuer::TestIssuer, try_database_url};
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

mod common;

use crate::common::{server_harness::TestServer, test_issuer::oidc_config};

/// The fact table `tenant_probe_aggregate` / `tenant_probe_window` resolve to.
const FACT_TABLE: &str = "tf_tenant_probe";
/// A relation over [`FACT_TABLE`] that filters its own rows on the session variable the
/// server sets from the caller's token: the `row_scope = "relation"` shape.
const SCOPED_FACT_TABLE: &str = "tf_tenant_probe_scoped";
/// The setting [`SCOPED_FACT_TABLE`] filters on.
const TENANT_SETTING: &str = "app.tenant_id";
/// The table behind the injected list root.
const ROW_TABLE: &str = "tb_tenant_probe_row";
/// The view the injected list root reads.
const ROW_VIEW: &str = "v_tenant_probe_row";

/// The claim the list root injects the tenant from.
const CLAIM: &str = "tid";

const TENANT_A: &str = "tenant-a";
const TENANT_B: &str = "tenant-b";

/// The refusal an unscoped analytics root answers with.
const REFUSAL: &str = "no row policy scopes its aggregate and window roots";

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Create the fact table and the list root's relation, each holding both tenants' rows.
///
/// Distinct row counts per tenant (2 vs 3), so a count read across tenants is told
/// apart from a scoped one by value alone.
async fn seed(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        format!("DROP VIEW IF EXISTS {ROW_VIEW}"),
        format!("DROP VIEW IF EXISTS {SCOPED_FACT_TABLE}"),
        format!("DROP TABLE IF EXISTS {ROW_TABLE}"),
        format!("DROP TABLE IF EXISTS {FACT_TABLE}"),
        format!(
            "CREATE TABLE {FACT_TABLE} (amount numeric NOT NULL, data jsonb NOT NULL, \
             tenant_id text NOT NULL)"
        ),
        format!("CREATE TABLE {ROW_TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
    ];
    let mut id = 1;
    for (tenant, count) in [(TENANT_A, 2), (TENANT_B, 3)] {
        for _ in 0..count {
            stmts.push(format!(
                "INSERT INTO {FACT_TABLE} VALUES (10, '{{\"kind\":\"k\"}}'::jsonb, '{tenant}')"
            ));
            let data = json!({"id": id, "tenant_id": tenant});
            stmts.push(format!("INSERT INTO {ROW_TABLE} VALUES ({id}, '{data}'::jsonb)"));
            id += 1;
        }
    }
    stmts.push(format!("CREATE VIEW {ROW_VIEW} AS SELECT id, data FROM {ROW_TABLE}"));
    stmts.push(format!(
        "CREATE VIEW {SCOPED_FACT_TABLE} AS SELECT * FROM {FACT_TABLE} \
         WHERE tenant_id = NULLIF(current_setting('{TENANT_SETTING}', true), '')"
    ));
    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// A fact table with a `tenant_id` denormalized filter and no link to a type.
fn fact_table(table_name: &str, row_scope: FactRowScope) -> FactTableMetadata {
    FactTableMetadata {
        table_name: table_name.to_string(),
        type_name: None,
        row_scope,
        inject_params: fraiseql_core::indexmap::IndexMap::default(),
        measures: vec![MeasureColumn {
            name:       "amount".to_string(),
            sql_type:   SqlType::Decimal,
            nullable:   false,
            additivity: Additivity::Additive,
        }],
        dimensions: DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![FilterColumn {
            name:      "tenant_id".to_string(),
            sql_type:  SqlType::Text,
            indexed:   true,
            hierarchy: None,
        }],
        calendar_dimensions: vec![],
        native_measures: std::collections::HashMap::new(),
        native_dimension_mapping: std::collections::HashMap::new(),
    }
}

/// A schema whose one list root injects `tenant_id` from the caller's token, and which
/// declares the fact table beside it.
fn build_schema() -> CompiledSchema {
    let mut query = TestQueryBuilder::new("probe_rows", "ProbeRow")
        .returns_list(true)
        .with_sql_source(ROW_VIEW)
        .build();
    query
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt(CLAIM.to_string()));

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("ProbeRow", ROW_VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("tenant_id", FieldType::String).build())
                .build(),
        )
        .with_query(query)
        .build();
    schema.add_fact_table(FACT_TABLE.to_string(), fact_table(FACT_TABLE, FactRowScope::Server));
    schema.build_indexes();
    schema
}

/// [`build_schema`] plus a fact table whose relation filters its own rows on a session
/// variable the server sets from the token's tenant claim.
fn build_relation_scoped_schema() -> CompiledSchema {
    let mut schema = build_schema();
    schema.add_fact_table(
        SCOPED_FACT_TABLE.to_string(),
        fact_table(SCOPED_FACT_TABLE, FactRowScope::Relation),
    );
    schema.session_variables = SessionVariablesConfig {
        variables: vec![SessionVariableMapping {
            name:   TENANT_SETTING.to_string(),
            source: SessionVariableSource::Jwt {
                claim: CLAIM.to_string(),
            },
        }],
        ..SessionVariablesConfig::default()
    };
    schema.build_indexes();
    schema
}

/// [`build_schema`] with the fact table scoped by its own `inject_params` (#1800): its
/// `tenant_id` column from the claim the list root injects it from.
fn build_inject_scoped_schema() -> CompiledSchema {
    let mut schema = build_schema();
    let mut table = fact_table(FACT_TABLE, FactRowScope::Server);
    table
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt(CLAIM.to_string()));
    schema.add_fact_table(FACT_TABLE.to_string(), table);
    schema.build_indexes();
    schema
}

/// [`build_schema`] with the fact table read as type `Probe`, which injects `tenant_id`:
/// the fact table inherits the type's `inject_params` (#1800).
fn build_inheriting_schema() -> CompiledSchema {
    let mut schema = build_schema();
    let mut probe = TestTypeBuilder::new("Probe", FACT_TABLE)
        .with_field(TestFieldBuilder::new("amount", FieldType::Float).build())
        .with_field(TestFieldBuilder::new("tenant_id", FieldType::String).build())
        .build();
    probe
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt(CLAIM.to_string()));
    schema.types.push(probe);
    let mut table = fact_table(FACT_TABLE, FactRowScope::Server);
    table.type_name = Some("Probe".to_string());
    schema.add_fact_table(FACT_TABLE.to_string(), table);
    schema.build_indexes();
    schema
}

/// Server config with the issuer's `[auth]` section, wired the way an operator would
/// wire it.
fn auth_config(issuer: &TestIssuer) -> ServerConfig {
    ServerConfig {
        auth: Some(oidc_config(issuer, true)),
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

/// A running server and the issuer its tokens come from. The issuer must outlive the
/// server: the validator fetches its key set on first use.
struct Rig {
    server: TestServer,
    issuer: TestIssuer,
}

impl std::ops::Deref for Rig {
    type Target = TestServer;

    fn deref(&self) -> &TestServer {
        &self.server
    }
}

impl Rig {
    /// A token carrying the tenant claim.
    fn token_for(&self, tenant: &str) -> String {
        self.token_with(Some(tenant))
    }

    /// A token carrying the tenant claim when `tenant` is `Some`.
    fn token_with(&self, tenant: Option<&str>) -> String {
        let mut claims = self
            .issuer
            .claims()
            .set("sub", format!("user-of-{}", tenant.unwrap_or("nobody")));
        if let Some(tenant) = tenant {
            claims = claims.set(CLAIM, tenant);
        }
        self.issuer.mint(&claims)
    }
}

/// Stand up the fixture and a running server. `None` when there is no database.
///
/// `authenticated` selects whether the deployment configures `[auth]` at all: with none,
/// an anonymous request reaches the executor, which is where an analytics root must
/// refuse it.
async fn start(authenticated: bool, schema: CompiledSchema) -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let issuer = TestIssuer::start().await;
    let config = if authenticated {
        auth_config(&issuer)
    } else {
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        }
    };
    // Boxed: `Server::new`'s future trips `clippy::large_futures` when awaited inline.
    let server = Box::pin(TestServer::start_with_config(config, schema, Arc::new(adapter))).await;
    Some(Rig { server, issuer })
}

async fn post_graphql(base: &str, token: Option<&str>, body: &Value) -> Value {
    let mut req = reqwest::Client::new().post(format!("{base}/graphql")).json(body);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let response = req.send().await.expect("GraphQL POST");
    let status = response.status();
    let text = response.text().await.expect("response body");
    serde_json::from_str(&text).unwrap_or_else(|_| json!({"status": status.as_u16(), "raw": text}))
}

/// Count per tenant, grouped by the fact table's `tenant_id` column.
fn aggregate_body() -> Value {
    aggregate_body_over("tenant_probe")
}

/// [`aggregate_body`] over the root `{root}_aggregate`.
fn aggregate_body_over(root: &str) -> Value {
    json!({
        "query": format!("{{ {root}_aggregate {{ tenant_id count }} }}"),
        "variables": {
            "groupBy":    {"tenant_id": true},
            "aggregates": [{"count": true}]
        }
    })
}

/// Every row's `tenant_id`, numbered.
fn window_body() -> Value {
    window_body_over("tenant_probe")
}

/// [`window_body`] over the root `{root}_window`.
fn window_body_over(root: &str) -> Value {
    json!({
        "query": format!("{{ {root}_window }}"),
        "variables": {
            "select":  [{"type": "filter", "name": "tenant_id", "alias": "tenant_id"}],
            "windows": [{"function": {"type": "row_number"}, "alias": "rank"}]
        }
    })
}

/// Assert `response` carries none of `foreign`'s rows, and that the root was refused.
///
/// The leak is checked first: before the fix the root answered with rows, and that is
/// the failure this suite exists to report.
fn assert_refused_without_rows(response: &Value, foreign: &[&str], label: &str) {
    let data = response.get("data").map(ToString::to_string).unwrap_or_default();
    for tenant in foreign {
        assert!(!data.contains(tenant), "{label}: read {tenant}'s rows: {response}");
    }
    let errors = response.get("errors").map(ToString::to_string).unwrap_or_default();
    assert!(errors.contains(REFUSAL), "{label}: expected the #1799 refusal, got {response}");
}

// ---------------------------------------------------------------------------
// The cases
// ---------------------------------------------------------------------------

/// The list root is scoped: the baseline every analytics assertion below is held to.
/// Without it, a refusal below could be a broken fixture rather than the guard.
#[tokio::test]
async fn the_injected_list_root_reads_only_the_callers_tenant() {
    let Some(server) = start(true, build_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let body = json!({"query": "{ probe_rows { id tenant_id } }"});
    let response = post_graphql(&server.url, Some(&server.token_for(TENANT_B)), &body).await;
    let rows = response["data"]["probe_rows"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 3, "tenant B owns 3 rows: {response}");
    assert!(rows.iter().all(|r| r["tenant_id"] == TENANT_B), "{response}");
}

/// Tenant B's aggregate must not count tenant A's rows.
#[tokio::test]
async fn an_aggregate_does_not_read_another_tenants_rows() {
    let Some(server) = start(true, build_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let response =
        post_graphql(&server.url, Some(&server.token_for(TENANT_B)), &aggregate_body()).await;
    assert_refused_without_rows(&response, &[TENANT_A], "aggregate as tenant B");
}

/// Tenant B's window must not return tenant A's rows.
#[tokio::test]
async fn a_window_does_not_read_another_tenants_rows() {
    let Some(server) = start(true, build_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let response =
        post_graphql(&server.url, Some(&server.token_for(TENANT_B)), &window_body()).await;
    assert_refused_without_rows(&response, &[TENANT_A], "window as tenant B");
}

/// With no auth configured an anonymous request reaches the executor, which must not
/// answer it with every tenant's rows.
#[tokio::test]
async fn an_anonymous_aggregate_reads_no_tenants_rows() {
    let Some(server) = start(false, build_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let response = post_graphql(&server.url, None, &aggregate_body()).await;
    assert_refused_without_rows(&response, &[TENANT_A, TENANT_B], "anonymous aggregate");
}

/// As [`an_anonymous_aggregate_reads_no_tenants_rows`], for the window root.
#[tokio::test]
async fn an_anonymous_window_reads_no_tenants_rows() {
    let Some(server) = start(false, build_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let response = post_graphql(&server.url, None, &window_body()).await;
    assert_refused_without_rows(&response, &[TENANT_A, TENANT_B], "anonymous window");
}

/// How many of `tenant`'s rows [`FACT_TABLE`] holds, read straight from the database: the
/// rows a scoped read must leave out are really there.
async fn rows_in_the_table(tenant: &str) -> i64 {
    let url = try_database_url().expect("DATABASE_URL");
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    let rows: Vec<std::collections::HashMap<String, Value>> = adapter
        .execute_raw_query(&format!(
            "SELECT count(*)::bigint AS n FROM {FACT_TABLE} WHERE tenant_id = '{tenant}'"
        ))
        .await
        .expect("count rows");
    rows[0]["n"].as_i64().expect("a count")
}

/// A `row_scope = "relation"` fact table is served, and its relation's own filter is what
/// scopes it: tenant B sees exactly its own rows while tenant A's are in the table.
#[tokio::test]
async fn a_relation_scoped_aggregate_reads_exactly_the_callers_tenant() {
    let Some(server) = start(true, build_relation_scoped_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert_eq!(rows_in_the_table(TENANT_A).await, 2, "tenant A's rows must be present");
    let body = aggregate_body_over("tenant_probe_scoped");
    let response = post_graphql(&server.url, Some(&server.token_for(TENANT_B)), &body).await;
    assert_eq!(
        response["data"]["tenant_probe_scoped_aggregate"],
        json!([{"tenant_id": TENANT_B, "count": 3}]),
        "{response}"
    );
}

/// As [`a_relation_scoped_aggregate_reads_exactly_the_callers_tenant`], for the window root.
#[tokio::test]
async fn a_relation_scoped_window_reads_exactly_the_callers_tenant() {
    let Some(server) = start(true, build_relation_scoped_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert_eq!(rows_in_the_table(TENANT_A).await, 2, "tenant A's rows must be present");
    let body = window_body_over("tenant_probe_scoped");
    let response = post_graphql(&server.url, Some(&server.token_for(TENANT_B)), &body).await;
    let rows = response["data"]["tenant_probe_scoped_window"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(rows.len(), 3, "tenant B owns 3 rows: {response}");
    assert!(rows.iter().all(|r| r["tenant_id"] == TENANT_B), "{response}");
}

/// A relation-scoped fact table read with no token: no setting, so the relation's own
/// filter keeps every row out.
#[tokio::test]
async fn an_anonymous_relation_scoped_aggregate_reads_no_rows() {
    let Some(server) = start(false, build_relation_scoped_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let body = aggregate_body_over("tenant_probe_scoped");
    let response = post_graphql(&server.url, None, &body).await;
    assert_eq!(response["data"]["tenant_probe_scoped_aggregate"], json!([]), "{response}");
}

// ---------------------------------------------------------------------------
// Scoped by `inject_params` (#1800)
// ---------------------------------------------------------------------------

/// Each tenant's aggregate over a fact table its own `inject_params` scope counts exactly
/// its own rows, while the other tenant's are in the table.
#[tokio::test]
async fn an_inject_scoped_aggregate_reads_exactly_the_callers_tenant() {
    let Some(server) = start(true, build_inject_scoped_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert_eq!(rows_in_the_table(TENANT_A).await, 2, "tenant A's rows must be present");
    assert_eq!(rows_in_the_table(TENANT_B).await, 3, "tenant B's rows must be present");
    for (tenant, count) in [(TENANT_A, 2), (TENANT_B, 3)] {
        let response =
            post_graphql(&server.url, Some(&server.token_for(tenant)), &aggregate_body()).await;
        assert_eq!(
            response["data"]["tenant_probe_aggregate"],
            json!([{"tenant_id": tenant, "count": count}]),
            "{tenant}: {response}"
        );
    }
}

/// As [`an_inject_scoped_aggregate_reads_exactly_the_callers_tenant`], for the window root.
#[tokio::test]
async fn an_inject_scoped_window_reads_exactly_the_callers_tenant() {
    let Some(server) = start(true, build_inject_scoped_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    for (tenant, count) in [(TENANT_A, 2), (TENANT_B, 3)] {
        let response =
            post_graphql(&server.url, Some(&server.token_for(tenant)), &window_body()).await;
        let rows = response["data"]["tenant_probe_window"].as_array().cloned().unwrap_or_default();
        assert_eq!(rows.len(), count, "{tenant} owns {count} rows: {response}");
        assert!(rows.iter().all(|r| r["tenant_id"] == tenant), "{tenant}: {response}");
    }
}

/// A fact table read as a type that injects `tenant_id` inherits the filter: the read #1799
/// refused is served, scoped.
#[tokio::test]
async fn an_inherited_scope_reads_exactly_the_callers_tenant() {
    let Some(server) = start(true, build_inheriting_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let response =
        post_graphql(&server.url, Some(&server.token_for(TENANT_B)), &aggregate_body()).await;
    assert_eq!(
        response["data"]["tenant_probe_aggregate"],
        json!([{"tenant_id": TENANT_B, "count": 3}]),
        "{response}"
    );
    let response =
        post_graphql(&server.url, Some(&server.token_for(TENANT_B)), &window_body()).await;
    let rows = response["data"]["tenant_probe_window"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 3, "tenant B owns 3 rows: {response}");
    assert!(rows.iter().all(|r| r["tenant_id"] == TENANT_B), "{response}");
}

/// The filters cannot be resolved without a principal, or without the claim: refused, and
/// no tenant's rows are read.
#[tokio::test]
async fn an_inject_scoped_read_without_the_claim_reads_no_tenants_rows() {
    let Some(server) = start(true, build_inject_scoped_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let token = server.token_with(None);
    for (label, body) in [("aggregate", aggregate_body()), ("window", window_body())] {
        let response = post_graphql(&server.url, Some(&token), &body).await;
        let data = response.get("data").map(ToString::to_string).unwrap_or_default();
        assert!(!data.contains(TENANT_A) && !data.contains(TENANT_B), "{label}: {response}");
        let errors = response.get("errors").map(ToString::to_string).unwrap_or_default();
        assert!(
            errors.contains(&format!("JWT claim '{CLAIM}' not present")),
            "{label}: {response}"
        );
    }
}

/// With no auth configured, an anonymous request reaches the executor, which refuses it.
#[tokio::test]
async fn an_anonymous_inject_scoped_read_reads_no_tenants_rows() {
    let Some(server) = start(false, build_inject_scoped_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    for (label, body) in [("aggregate", aggregate_body()), ("window", window_body())] {
        let response = post_graphql(&server.url, None, &body).await;
        let data = response.get("data").map(ToString::to_string).unwrap_or_default();
        assert!(!data.contains(TENANT_A) && !data.contains(TENANT_B), "{label}: {response}");
        let errors = response.get("errors").map(ToString::to_string).unwrap_or_default();
        assert!(errors.contains("without a security context"), "{label}: {response}");
    }
}
