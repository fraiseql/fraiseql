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
//! injects. Until the server can apply that scope to an analytics root, it refuses the
//! root rather than read across tenants. A fact table whose relation filters its own rows
//! declares `row_scope = "relation"`.
//!
//! Drives the real `Server::serve_on_listener` mount over a real socket, with HS256
//! tokens, against real PostgreSQL.
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
use fraiseql_server::server_config::{Hs256Config, ServerConfig};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

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

/// 32-byte HS256 secret — meets the minimum key-length requirement.
const SECRET: &str = "fraiseql-1799-secret-exactly-32b";
const SECRET_ENV: &str = "FRAISEQL_1799_HS256_SECRET";
const ISSUER: &str = "https://t1799.fraiseql.test";
const AUDIENCE: &str = "fraiseql-1799-api";
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

/// Server config with HS256 auth wired the way an operator would wire it.
fn auth_config() -> ServerConfig {
    ServerConfig {
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

/// Mint an HS256 token carrying the tenant claim.
fn token_for(tenant: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();
    let claims = json!({
        "sub": format!("user-of-{tenant}"),
        "iss": ISSUER,
        "aud": AUDIENCE,
        CLAIM: tenant,
        "iat": now,
        "exp": now + 3600,
    });
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint HS256 token")
}

/// Stand up the fixture and a running server. `None` when there is no database.
///
/// `authenticated` selects whether the deployment configures HS256 at all: with none,
/// an anonymous request reaches the executor, which is where an analytics root must
/// refuse it.
async fn start(authenticated: bool, schema: CompiledSchema) -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let config = if authenticated {
        auth_config()
    } else {
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        }
    };
    // Boxed: `Server::new`'s future trips `clippy::large_futures` when awaited inline.
    Some(Box::pin(TestServer::start_with_config(config, schema, Arc::new(adapter))).await)
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
    aggregate_body_over("tenant_probe", FACT_TABLE)
}

/// [`aggregate_body`] over the root `{root}_aggregate` and its fact table.
fn aggregate_body_over(root: &str, table: &str) -> Value {
    json!({
        "query": format!("{{ {root}_aggregate {{ count }} }}"),
        "variables": {
            "table":      table,
            "groupBy":    {"tenant_id": true},
            "aggregates": [{"count": {}}]
        }
    })
}

/// Every row's `tenant_id`, numbered.
fn window_body() -> Value {
    window_body_over("tenant_probe", FACT_TABLE)
}

/// [`window_body`] over the root `{root}_window` and its fact table.
fn window_body_over(root: &str, table: &str) -> Value {
    json!({
        "query": format!("{{ {root}_window {{ rank }} }}"),
        "variables": {
            "table":   table,
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
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, build_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        let body = json!({"query": "{ probe_rows { id tenant_id } }"});
        let response = post_graphql(&server.url, Some(&token_for(TENANT_B)), &body).await;
        let rows = response["data"]["probe_rows"].as_array().cloned().unwrap_or_default();
        assert_eq!(rows.len(), 3, "tenant B owns 3 rows: {response}");
        assert!(rows.iter().all(|r| r["tenant_id"] == TENANT_B), "{response}");
    }))
    .await;
}

/// Tenant B's aggregate must not count tenant A's rows.
#[tokio::test]
async fn an_aggregate_does_not_read_another_tenants_rows() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, build_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        let response =
            post_graphql(&server.url, Some(&token_for(TENANT_B)), &aggregate_body()).await;
        assert_refused_without_rows(&response, &[TENANT_A], "aggregate as tenant B");
    }))
    .await;
}

/// Tenant B's window must not return tenant A's rows.
#[tokio::test]
async fn a_window_does_not_read_another_tenants_rows() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, build_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        let response = post_graphql(&server.url, Some(&token_for(TENANT_B)), &window_body()).await;
        assert_refused_without_rows(&response, &[TENANT_A], "window as tenant B");
    }))
    .await;
}

/// With no auth configured an anonymous request reaches the executor, which must not
/// answer it with every tenant's rows.
#[tokio::test]
async fn an_anonymous_aggregate_reads_no_tenants_rows() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(false, build_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        let response = post_graphql(&server.url, None, &aggregate_body()).await;
        assert_refused_without_rows(&response, &[TENANT_A, TENANT_B], "anonymous aggregate");
    }))
    .await;
}

/// As [`an_anonymous_aggregate_reads_no_tenants_rows`], for the window root.
#[tokio::test]
async fn an_anonymous_window_reads_no_tenants_rows() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(false, build_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        let response = post_graphql(&server.url, None, &window_body()).await;
        assert_refused_without_rows(&response, &[TENANT_A, TENANT_B], "anonymous window");
    }))
    .await;
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
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, build_relation_scoped_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        assert_eq!(rows_in_the_table(TENANT_A).await, 2, "tenant A's rows must be present");
        let body = aggregate_body_over("tenant_probe_scoped", SCOPED_FACT_TABLE);
        let response = post_graphql(&server.url, Some(&token_for(TENANT_B)), &body).await;
        assert_eq!(
            response["data"]["tenant_probe_scoped_aggregate"],
            json!([{"tenant_id": TENANT_B, "count": 3}]),
            "{response}"
        );
    }))
    .await;
}

/// As [`a_relation_scoped_aggregate_reads_exactly_the_callers_tenant`], for the window root.
#[tokio::test]
async fn a_relation_scoped_window_reads_exactly_the_callers_tenant() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, build_relation_scoped_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        assert_eq!(rows_in_the_table(TENANT_A).await, 2, "tenant A's rows must be present");
        let body = window_body_over("tenant_probe_scoped", SCOPED_FACT_TABLE);
        let response = post_graphql(&server.url, Some(&token_for(TENANT_B)), &body).await;
        let rows = response["data"]["tenant_probe_scoped_window"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(rows.len(), 3, "tenant B owns 3 rows: {response}");
        assert!(rows.iter().all(|r| r["tenant_id"] == TENANT_B), "{response}");
    }))
    .await;
}

/// A relation-scoped fact table read with no token: no setting, so the relation's own
/// filter keeps every row out.
#[tokio::test]
async fn an_anonymous_relation_scoped_aggregate_reads_no_rows() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(false, build_relation_scoped_schema()).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        let body = aggregate_body_over("tenant_probe_scoped", SCOPED_FACT_TABLE);
        let response = post_graphql(&server.url, None, &body).await;
        assert_eq!(response["data"]["tenant_probe_scoped_aggregate"], json!([]), "{response}");
    }))
    .await;
}
