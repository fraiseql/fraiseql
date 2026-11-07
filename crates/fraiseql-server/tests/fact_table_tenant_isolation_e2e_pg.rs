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
/// A relation over [`FACT_TABLE`] that does not filter its rows: declared
/// `row_scope = "relation"`, it must not be served.
const UNFILTERED_FACT_TABLE: &str = "tf_tenant_unfiltered";
/// The setting [`SCOPED_FACT_TABLE`] filters on.
const TENANT_SETTING: &str = "app.tenant_id";
/// The table behind the injected list root.
const ROW_TABLE: &str = "tb_tenant_probe_row";
/// The view the injected list root reads.
const ROW_VIEW: &str = "v_tenant_probe_row";
/// A fact table keeping its tenant in `org_id`, a column no query injects.
const ORG_FACT_TABLE: &str = "tf_org_sample";
/// The table behind a per-tenant type its author did not annotate.
const NOTE_TABLE: &str = "tb_tenant_note";
/// The view [`NOTE_TABLE`]'s type reads.
const NOTE_VIEW: &str = "v_tenant_note";

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
        format!("DROP VIEW IF EXISTS {NOTE_VIEW}"),
        format!("DROP TABLE IF EXISTS {NOTE_TABLE}"),
        format!("DROP TABLE IF EXISTS {ORG_FACT_TABLE}"),
        format!(
            "CREATE TABLE {ORG_FACT_TABLE} (amount numeric NOT NULL, data jsonb NOT NULL, \
             org_id text NOT NULL)"
        ),
        format!("CREATE TABLE {NOTE_TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
        format!("DROP VIEW IF EXISTS {SCOPED_FACT_TABLE}"),
        format!("DROP VIEW IF EXISTS {UNFILTERED_FACT_TABLE}"),
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
            stmts.push(format!(
                "INSERT INTO {ORG_FACT_TABLE} VALUES (10, '{{\"kind\":\"k\"}}'::jsonb, '{tenant}')"
            ));
            let note = json!({"id": id, "tenant_id": tenant, "title": format!("note of {tenant}")});
            stmts.push(format!("INSERT INTO {NOTE_TABLE} VALUES ({id}, '{note}'::jsonb)"));
            id += 1;
        }
    }
    stmts.push(format!("CREATE VIEW {ROW_VIEW} AS SELECT id, data FROM {ROW_TABLE}"));
    stmts.push(format!("CREATE VIEW {NOTE_VIEW} AS SELECT id, data FROM {NOTE_TABLE}"));
    stmts.push(format!(
        "CREATE VIEW {SCOPED_FACT_TABLE} AS SELECT * FROM {FACT_TABLE} \
         WHERE tenant_id = NULLIF(current_setting('{TENANT_SETTING}', true), '')"
    ));
    stmts.push(format!("CREATE VIEW {UNFILTERED_FACT_TABLE} AS SELECT * FROM {FACT_TABLE}"));
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
            column:     None,
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
        grain: None,
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

/// [`build_schema`], declared multi-tenant, plus [`ORG_FACT_TABLE`]: its tenant column is
/// `org_id`, which no query injects, and it declares no scope.
fn build_multi_tenant_schema() -> CompiledSchema {
    let mut schema = build_schema();
    schema.security = Some(fraiseql_core::schema::SecurityConfig {
        multi_tenant: true,
        ..fraiseql_core::schema::SecurityConfig::default()
    });
    let mut table = fact_table(ORG_FACT_TABLE, FactRowScope::Server);
    table.denormalized_filters[0].name = "org_id".to_string();
    schema.add_fact_table(ORG_FACT_TABLE.to_string(), table);
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

/// [`build_relation_scoped_schema`] plus [`UNFILTERED_FACT_TABLE`], declared
/// `row_scope = "relation"` although its relation does not filter its rows.
fn build_unfiltered_relation_schema() -> CompiledSchema {
    let mut schema = build_relation_scoped_schema();
    schema.add_fact_table(
        UNFILTERED_FACT_TABLE.to_string(),
        fact_table(UNFILTERED_FACT_TABLE, FactRowScope::Relation),
    );
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

/// [`start`] with `[auth]` and the result cache off: a multi-tenant schema with a cache
/// needs declared RLS to boot, and what these cases read is the scoping, not the cache.
async fn start_multi_tenant(schema: CompiledSchema) -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let issuer = TestIssuer::start().await;
    let config = ServerConfig {
        cache_enabled: false,
        ..auth_config(&issuer)
    };
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
    let data = response.get("data").map_or_default(ToString::to_string);
    for tenant in foreign {
        assert!(!data.contains(tenant), "{label}: read {tenant}'s rows: {response}");
    }
    let errors = response.get("errors").map_or_default(ToString::to_string);
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

/// A `row_scope = "relation"` fact table whose relation does not filter its rows must
/// not be served: the server adds no tenant predicate to its roots, so the server
/// refuses to start.
#[tokio::test]
async fn a_relation_scoped_fact_table_its_relation_does_not_filter_is_refused_at_boot() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let issuer = TestIssuer::start().await;
    let schema = build_unfiltered_relation_schema();
    let booted = Box::pin(fraiseql_server::Server::new(
        auth_config(&issuer),
        schema.clone(),
        Arc::new(adapter.clone()),
        None,
    ))
    .await;
    let Err(refusal) = booted else {
        let server = Box::pin(TestServer::start_with_config(
            auth_config(&issuer),
            schema,
            Arc::new(adapter),
        ))
        .await;
        let rig = Rig { server, issuer };
        let body = aggregate_body_over("tenant_unfiltered");
        let response = post_graphql(&rig.url, Some(&rig.token_for(TENANT_B)), &body).await;
        panic!(
            "the server started with a relation-scoped fact table its relation does not \
             filter, and tenant B's aggregate answered: {response}"
        );
    };
    let message = refusal.to_string();
    assert!(
        message.contains(UNFILTERED_FACT_TABLE) && message.contains("row_scope"),
        "the refusal must name the fact table and its declaration: {message}"
    );
}

/// A hot reload can change where a session variable comes from. One that re-sources the
/// setting a relation-scoped fact table's relation filters on from a request header leaves
/// that relation filtering on client input, so the reload is refused.
#[tokio::test]
async fn a_reload_that_sources_the_filtering_setting_from_a_header_is_refused() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let schema = build_relation_scoped_schema();
    let state = fraiseql_server::routes::graphql::AppState::new(Arc::new(
        fraiseql_core::runtime::Executor::new(schema.clone(), Arc::new(adapter)),
    ));
    let mut reloaded = schema;
    reloaded.session_variables.variables[0].source = SessionVariableSource::Header {
        header: "x-tenant".to_string(),
    };

    let result = state
        .reload_schema_from_json(&serde_json::to_string(&reloaded).expect("serialize"))
        .await;

    let refusal = result.err().unwrap_or_default();
    assert!(
        refusal.contains(SCOPED_FACT_TABLE) && refusal.contains("row_scope"),
        "the reload was not refused for the relation filtering on a header: {refusal:?}"
    );
}

/// A tenant registration builds an executor for the schema it is given; a relation-scoped
/// fact table its relation does not filter is refused there as at boot.
#[tokio::test]
async fn a_tenant_registered_with_an_unfiltered_relation_scoped_fact_table_is_refused() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let schema_json =
        serde_json::to_string(&build_unfiltered_relation_schema()).expect("serialize");
    let pool = fraiseql_server::tenancy::TenantPoolConfig {
        connection_string:    url,
        max_connections:      2,
        connect_timeout_secs: 10,
        idle_timeout_secs:    300,
        search_path:          None,
        tls:                  fraiseql_core::db::postgres::PostgresTlsConfig::default(),
        read_replica_urls:    Vec::new(),
        read_replica_policy:  fraiseql_core::db::postgres::ReadReplicaPolicy::default(),
        vector_scan:          fraiseql_core::db::postgres::VectorScanConfig::default(),
        transaction_retry:    fraiseql_core::db::postgres::TransactionRetry::off(),
    };

    let registered = fraiseql_server::tenancy::create_tenant_executor::<PostgresAdapter>(
        "acme",
        &schema_json,
        &pool,
        &fraiseql_core::runtime::RuntimeConfig::default(),
    )
    .await;

    let refusal = registered.err().map_or_else(String::new, |e| e.to_string());
    assert!(
        refusal.contains(UNFILTERED_FACT_TABLE) && refusal.contains("row_scope"),
        "the registration was not refused for the unfiltered relation: {refusal:?}"
    );
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
        let data = response.get("data").map_or_default(ToString::to_string);
        assert!(!data.contains(TENANT_A) && !data.contains(TENANT_B), "{label}: {response}");
        let errors = response.get("errors").map_or_default(ToString::to_string);
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
        let data = response.get("data").map_or_default(ToString::to_string);
        assert!(!data.contains(TENANT_A) && !data.contains(TENANT_B), "{label}: {response}");
        let errors = response.get("errors").map_or_default(ToString::to_string);
        assert!(errors.contains("without a security context"), "{label}: {response}");
    }
}

// ---------------------------------------------------------------------------
// A multi-tenant schema declares every root's scope
// ---------------------------------------------------------------------------

/// In a multi-tenant schema, a fact table that declares no scope is refused, whatever its
/// tenant column is called: tenant B's aggregate must not count tenant A's rows.
#[tokio::test]
async fn a_multi_tenant_aggregate_over_an_undeclared_fact_table_reads_no_other_tenant() {
    let Some(rig) = start_multi_tenant(build_multi_tenant_schema()).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let body = json!({
        "query": "{ org_sample_aggregate { org_id count } }",
        "variables": {"groupBy": {"org_id": true}, "aggregates": [{"count": true}]}
    });
    let response = post_graphql(&rig.url, Some(&rig.token_for(TENANT_B)), &body).await;
    assert_refused_without_rows(&response, &[TENANT_A], "multi-tenant aggregate as tenant B");
}

/// Compile a row-isolated project whose `Note` type, read by the `notes` root, carries no
/// `@tenant_id` and is not declared shared.
async fn compile_row_isolated_with_an_undeclared_type() -> Result<CompiledSchema, String> {
    use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};

    let dir = tempfile::TempDir::new().expect("temp dir");
    let schema = json!({
        "types": [
            {"name": "ProbeRow", "sql_source": ROW_VIEW, "fields": [
                {"name": "id", "type": "Int", "nullable": false},
                {"name": "tenant_id", "type": "String", "nullable": false,
                 "directives": [{"name": "tenant_id"}]}
            ]},
            {"name": "Note", "sql_source": NOTE_VIEW, "fields": [
                {"name": "id", "type": "Int", "nullable": false},
                {"name": "title", "type": "String", "nullable": false}
            ]}
        ],
        "queries": [
            {"name": "probe_rows", "return_type": "ProbeRow", "returns_list": true,
             "sql_source": ROW_VIEW},
            {"name": "notes", "return_type": "Note", "returns_list": true, "sql_source": NOTE_VIEW}
        ]
    });
    let schema_path = dir.path().join("schema.json");
    std::fs::write(&schema_path, schema.to_string()).expect("write schema.json");
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[tenancy]\nmode = \"row\"\ntenant_claim = \"{CLAIM}\"\n"),
    )
    .expect("write fraiseql.toml");
    let compiled = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(schema_path.to_str().expect("utf-8 path"))
    })
    .await
    .map_err(|e| format!("{e:#}"))?;
    let mut schema = CompiledSchema::from_json(&compiled.0.schema.to_json().unwrap(), false)
        .expect("the compiled schema loads");
    schema.build_indexes();
    Ok(schema)
}

/// Row isolation scopes only annotated types. A per-tenant type its author did not annotate
/// must not compile into a root every tenant reads in full.
#[tokio::test]
async fn row_isolation_does_not_serve_an_undeclared_type_across_tenants() {
    let schema = match compile_row_isolated_with_an_undeclared_type().await {
        Err(refusal) => {
            assert!(
                refusal.contains("'Note'") && refusal.contains("shared_types"),
                "refused, but not for the undeclared type: {refusal}"
            );
            return;
        },
        Ok(schema) => schema,
    };
    let Some(rig) = start_multi_tenant(schema).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let body = json!({"query": "{ notes { id title } }"});
    let response = post_graphql(&rig.url, Some(&rig.token_for(TENANT_B)), &body).await;
    panic!("compiled, and tenant B's `notes` read: {response}");
}

/// The documents this suite serves load, checked with no database.
///
/// Every other test here reaches its document only after `try_database_url()`, so in a run
/// without a database a load-time refusal of one would report as a pass. This one needs
/// nothing but the loader, and goes through `to_json`/`from_json`, the path a served
/// document takes. The compiled project is decided here too: the compiler refuses it for
/// its undeclared type, and a project it accepted would have to load.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    for (name, schema) in [
        ("single-tenant", build_schema()),
        ("multi-tenant", build_multi_tenant_schema()),
    ] {
        let json = schema.to_json().expect("serialize the document");
        CompiledSchema::from_json(&json, false)
            .unwrap_or_else(|e| panic!("the {name} document must load: {e}"));
    }
    if let Err(refusal) = compile_row_isolated_with_an_undeclared_type().await {
        assert!(
            refusal.contains("'Note'"),
            "refused, but not for the undeclared type: {refusal}"
        );
    }
}
