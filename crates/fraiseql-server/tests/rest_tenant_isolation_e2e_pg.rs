//! #812 / #739 / #810 regression: the REST read surface must carry an authenticated
//! security context and an enforced row filter.
//!
//! Three independent defects produced the same outcome — unfiltered rows to an
//! unauthenticated caller — and none of them was visible to the existing REST suite:
//!
//! * **#812** the REST router was merged into the app with **no auth middleware**, so
//!   `security_context` was `None` on every request.
//! * **#739** `execute_query_direct` resolved each `inject_param` into `let _value = …` and threw
//!   it away, so the tenant predicate never reached the WHERE clause.
//! * **#810** `require_auth` was consulted by the SSE route only, while the served `OpenAPI`
//!   document advertised `BearerAuth` + 401 on every operation.
//!
//! **Why this file exists rather than more cases in `rest_transport_e2e_test.rs`:**
//! that suite builds its router with `rest_router(&state, …)` **directly**. #812 is a
//! *mount-site* defect — the middleware is attached in `Server::build_router` — so no
//! test that constructs the sub-router itself could ever have observed it. It tested a
//! router that is not the one the binary serves. This file drives the real
//! `Server::serve_on_listener` mount over a real socket against real PostgreSQL, which
//! is the only shape that can hold all three defects down.
//!
//! The `Prefer: count=exact` case is the sharpest of the three: `count_rows` has always
//! enforced the inject filter while `execute_query_direct` did not, so the response
//! header and the response body disagreed about how many rows exist. That disagreement
//! was shipping, and it is asserted here directly.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_p03_orders` fixture → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        CompiledSchema, FieldDenyPolicy, FieldType, InjectedParamSource, RestConfig, SecurityConfig,
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

/// The fixture table. Deliberately **not** a shared seeded fixture: several suites
/// introspect `tf_sales` / `v_user`, and dropping one of those here would break them
/// against the shared test database.
const TABLE: &str = "tf_p03_orders";
/// The view the compiled query reads.
const VIEW: &str = "v_p03_order";

/// 32-byte HS256 secret — meets the minimum key-length requirement.
const SECRET: &str = "fraiseql-p03-secret-exactly-32by";
const SECRET_ENV: &str = "FRAISEQL_P03_HS256_SECRET";
const ISSUER: &str = "https://p03.fraiseql.test";
const AUDIENCE: &str = "fraiseql-p03-api";

const TENANT_A: &str = "tenant-a";
const TENANT_B: &str = "tenant-b";

/// Rows seeded per tenant. Deliberately different so a leaked row set is
/// distinguishable from a correctly-filtered one by length alone.
const ROWS_A: usize = 2;
const ROWS_B: usize = 3;

/// The `requires_scope`-gated field used by the `#886` field-gate tests.
///
/// Deliberately a field that **exists in the seeded data**, so a test cannot pass
/// merely because the value was absent from the row to begin with.
const GATED_FIELD: &str = "label";

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Create the table + view and seed two tenants' rows.
///
/// Entity fields live as JSON keys inside `data` (the repo's JSONB data-column model),
/// so `tenant_id` is **not** a native column and the inject predicate compiles to
/// `data->>'tenant_id' = $1`.
async fn seed(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
    ];

    let mut id = 1;
    for (tenant, count) in [(TENANT_A, ROWS_A), (TENANT_B, ROWS_B)] {
        for n in 1..=count {
            let data = json!({"id": id, "tenant_id": tenant, "label": format!("{tenant}-{n}")});
            stmts.push(format!("INSERT INTO {TABLE} VALUES ({id}, '{data}'::jsonb)"));
            id += 1;
        }
    }
    stmts.push(format!("CREATE VIEW {VIEW} AS SELECT id, data FROM {TABLE}"));

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// A compiled schema whose single query declares a JWT-sourced tenant filter.
///
/// `inject_params = { tenant_id: jwt:org_id }` is the whole point: every read of
/// `orders` must be scoped to the caller's own tenant, with no way for a client to
/// widen it.
fn build_schema(require_auth: bool) -> CompiledSchema {
    let mut query = TestQueryBuilder::new("orders", "P03Order")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    query
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("org_id".to_string()));

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("P03Order", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("tenant_id", FieldType::String).build())
                .with_field(TestFieldBuilder::new("label", FieldType::String).build())
                .build(),
        )
        .with_query(query)
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth,
        ..RestConfig::default()
    });
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

/// Mint an HS256 token carrying `org_id`, the claim the query injects from.
fn token_for(tenant: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();

    let claims = json!({
        "sub":    format!("user-of-{tenant}"),
        "iss":    ISSUER,
        "aud":    AUDIENCE,
        "org_id": tenant,
        "iat":    now,
        "exp":    now + 3600,
    });

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint HS256 token")
}

/// Outcome of one REST GET: status, body envelope, and the `Prefer`-reported total.
struct Read {
    status: reqwest::StatusCode,
    body:   Value,
}

impl Read {
    /// The `data` array length, or `None` when the response carried no array.
    fn rows(&self) -> Option<usize> {
        self.body.get("data").and_then(Value::as_array).map(Vec::len)
    }

    /// `meta.total`, populated only under `Prefer: count=exact`.
    fn total(&self) -> Option<u64> {
        self.body.get("meta").and_then(|m| m.get("total")).and_then(Value::as_u64)
    }

    /// Every row's value for `key`, as a string, skipping rows where it is absent.
    ///
    /// `#886`: before the projection repair this returned an empty vector for every
    /// key, because each row was `{}`.
    fn column(&self, key: &str) -> Vec<String> {
        self.body
            .get("data")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r.get(key))
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), ToString::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The keys present on the first row, sorted. Empty when there are no rows.
    fn first_row_keys(&self) -> Vec<String> {
        self.body
            .get("data")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .and_then(Value::as_object)
            .map(|o| {
                let mut keys: Vec<String> = o.keys().cloned().collect();
                keys.sort();
                keys
            })
            .unwrap_or_default()
    }
}

/// Issue `GET {base}/orders` with an optional bearer token and optional `Prefer` header.
///
/// The per-tenant row counts are deliberately **distinct** (2 vs 3) so that both "saw
/// everyone's rows" and "saw the wrong tenant's rows" fail on length alone; `#886`'s
/// repair means the tests below also assert the returned `tenant_id` values directly.
async fn get_orders(base: &str, token: Option<&str>, prefer: Option<&str>) -> Read {
    get_orders_query(base, token, prefer, "").await
}

/// As [`get_orders`], with a query string appended verbatim (e.g. `"?select=id,label"`).
async fn get_orders_query(
    base: &str,
    token: Option<&str>,
    prefer: Option<&str>,
    query: &str,
) -> Read {
    let mut req = reqwest::Client::new().get(format!("{base}/rest/v1/orders{query}"));
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    if let Some(p) = prefer {
        req = req.header("prefer", p);
    }
    let response = req.send().await.expect("REST GET");
    let status = response.status();
    let text = response.text().await.expect("response body");
    let body = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}));
    Read { status, body }
}

/// Stand up the fixture and a running server. `None` when there is no database.
///
/// `authenticated` selects whether the deployment configures HS256 at all. Both shapes
/// matter and they fail closed for **different** reasons: with auth configured the
/// middleware refuses an anonymous caller, and with no auth configured at all the
/// request reaches the executor, where the inject guard must refuse it. A test that
/// only ever runs the first shape would pass while the second stayed wide open — which
/// is exactly the state `#739` shipped in.
async fn start(authenticated: bool, require_auth: bool) -> Option<TestServer> {
    start_with_schema(authenticated, build_schema(require_auth)).await
}

/// As [`start`], with a caller-supplied compiled schema.
async fn start_with_schema(authenticated: bool, schema: CompiledSchema) -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;

    let config = if authenticated {
        auth_config()
    } else {
        // #874: cors_enabled=false — production validate() refuses the default.
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        }
    };
    // Boxed: `Server::new`'s future is ~18 KiB and `clippy::large_futures` (pedantic,
    // denied) rejects awaiting it inline at every call site.
    Some(Box::pin(TestServer::start_with_config(config, schema, Arc::new(adapter))).await)
}

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

/// #812 + #810: an unauthenticated REST read must not return rows.
///
/// With `require_auth = true` the correct answer is 401. The shipped behaviour was
/// **200 with every tenant's rows** — and the in-repo test asserting that 200 was named
/// `test_auth_enforcement_with_require_auth`.
#[tokio::test]
async fn unauthenticated_rest_read_is_refused_when_require_auth_is_set() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, true).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let read = get_orders(&server.url, None, None).await;

        assert_eq!(
            read.status,
            reqwest::StatusCode::UNAUTHORIZED,
            "an anonymous REST GET must be refused when require_auth = true, got {} {}",
            read.status,
            read.body
        );
        assert_eq!(
            read.rows().unwrap_or(0),
            0,
            "a refused request must carry no rows, got {}",
            read.body
        );
    }))
    .await;
}

/// #812 + #739: an unauthenticated read of a tenant-scoped query must never return
/// rows, even when `require_auth` is false.
///
/// Absent authentication must not mean absent filtering. The query declares
/// `inject_params`, so with no principal there is no tenant to scope to and the only
/// safe answer is an error — never "all rows". `count_rows` has always behaved this
/// way; `execute_query_direct` returned everything.
#[tokio::test]
async fn anonymous_read_of_a_tenant_scoped_query_never_returns_rows() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(false, false).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let read = get_orders(&server.url, None, None).await;

        assert!(
            !read.status.is_success(),
            "an anonymous read of an inject-scoped query must fail closed, got {} {}",
            read.status,
            read.body
        );
        assert_eq!(
            read.rows().unwrap_or(0),
            0,
            "no rows may reach an anonymous caller of a tenant-scoped query, got {}",
            read.body
        );
    }))
    .await;
}

/// #739: an authenticated read must return only the caller's own tenant's rows.
///
/// This is the cross-tenant leak in its plainest form: tenant A asks for `orders` and
/// receives tenant B's rows too, because the resolved inject value was discarded.
#[tokio::test]
async fn authenticated_read_is_scoped_to_the_callers_tenant() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, true).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        for (tenant, expected) in [(TENANT_A, ROWS_A), (TENANT_B, ROWS_B)] {
            let read = get_orders(&server.url, Some(&token_for(tenant)), None).await;

            assert_eq!(
                read.status,
                reqwest::StatusCode::OK,
                "{tenant}: authenticated read should succeed, got {} {}",
                read.status,
                read.body
            );

            assert_eq!(
                read.rows(),
                Some(expected),
                "{tenant}: expected exactly its own {expected} rows of the {} seeded, got {}",
                ROWS_A + ROWS_B,
                read.body
            );

            // #886: assert the row *contents*, not just the count. Before the
            // projection repair every row was `{}`, so a count-only assertion could
            // not tell a correctly-filtered read from an empty one.
            let tenants = read.column("tenant_id");
            assert_eq!(
                tenants.len(),
                expected,
                "{tenant}: every returned row must carry a tenant_id, got {}",
                read.body
            );
            assert!(
                tenants.iter().all(|t| t == tenant),
                "{tenant}: every returned row must belong to the caller's tenant, got {tenants:?}"
            );
        }
    }))
    .await;
}

/// #739's smoking gun: `Prefer: count=exact` and the body must agree.
///
/// `count_rows` enforced the inject filter and `execute_query_direct` did not, so the
/// header reported the tenant's row count while the body carried everyone's. A response
/// that contradicts its own metadata is the most legible possible proof of the bug.
#[tokio::test]
async fn count_exact_header_agrees_with_the_body_under_a_tenant_filter() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, true).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let read = get_orders(&server.url, Some(&token_for(TENANT_A)), Some("count=exact")).await;

        assert_eq!(read.status, reqwest::StatusCode::OK, "read should succeed: {}", read.body);

        let total = read.total().expect("meta.total must be present under Prefer: count=exact");
        let rows = read.rows().expect("data must be an array");

        assert_eq!(
            u64::try_from(rows).unwrap(),
            total,
            "body row count ({rows}) and Prefer: count=exact total ({total}) disagree — the \
             count path enforces the tenant filter and the body path does not: {}",
            read.body
        );
        assert_eq!(
            total,
            u64::try_from(ROWS_A).unwrap(),
            "both must equal the caller's own row count"
        );
    }))
    .await;
}

// ---------------------------------------------------------------------------
// #886 — projection, and the field gates it was masking
// ---------------------------------------------------------------------------

/// A schema whose `P03Order` carries an extra field gated by `requires_scope`.
///
/// `on_deny` selects what must happen to a caller who lacks the scope: `Reject` is a
/// 403 for the whole request, `Mask` is a null in the field's requested position.
fn build_schema_with_gated_field(on_deny: FieldDenyPolicy) -> CompiledSchema {
    let mut schema = build_schema(true);

    // Gate the *existing* `label` field rather than appending a second one: the seeded
    // rows already carry a value for it, so a passing test cannot be explained by the
    // field simply being absent from the data.
    let gated = schema
        .types
        .iter_mut()
        .find(|t| t.name == "P03Order")
        .expect("the fixture type must exist")
        .fields
        .iter_mut()
        .find(|f| f.name == GATED_FIELD)
        .expect("the fixture type must declare the gated field");
    gated.requires_scope = Some("read:P03Order.label".to_string());
    gated.on_deny = on_deny;

    // `apply_field_rbac_filtering` is a no-op unless the schema declares security at
    // all, so an absent `security` block would make this test vacuous.
    schema.security = Some(SecurityConfig::default());
    schema.build_indexes();
    schema
}

/// #886: a REST read must return the fields of each row.
///
/// `QueryMatch::from_operation` built a **flat** selection set — every requested field
/// its own top-level `FieldSelection` with empty `nested_fields` — while
/// `ExecutionPlanner::extract_projection_fields` reads `selections.first().nested_fields`.
/// The planner therefore saw no fields, `ResultProjector::new(vec![])` projected nothing,
/// and every REST read answered `{"data":[{},{},{}]}`: the right row count, zero data.
///
/// No test in the repo asserted the field *content* of a REST GET response, which is
/// exactly why this shipped. `rest_transport_e2e_test.rs` asserts `is_array()` and array
/// lengths; its single content assertion is on a mutation result that never passes
/// through this projector.
#[tokio::test]
async fn a_rest_read_returns_the_fields_of_each_row() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, true).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let read = get_orders(&server.url, Some(&token_for(TENANT_A)), None).await;

        assert_eq!(read.status, reqwest::StatusCode::OK, "read should succeed: {}", read.body);
        assert_eq!(read.rows(), Some(ROWS_A), "row count: {}", read.body);

        assert!(
            !read.first_row_keys().is_empty(),
            "every REST row came back as an empty object — the projection carried no \
             fields at all: {}",
            read.body
        );
        assert_eq!(
            read.column("label").len(),
            ROWS_A,
            "every row must carry its `label`, got {}",
            read.body
        );
        assert_eq!(
            read.column("id").len(),
            ROWS_A,
            "every row must carry its `id`, got {}",
            read.body
        );
    }))
    .await;
}

/// #886: `?select=` must narrow the projection, not be ignored.
///
/// Before the repair the response was byte-identical with and without `?select=` —
/// `{}` either way — so no test could tell the parameter was doing nothing.
#[tokio::test]
async fn a_rest_read_honours_select() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, true).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let read =
            get_orders_query(&server.url, Some(&token_for(TENANT_A)), None, "?select=id,label")
                .await;

        assert_eq!(read.status, reqwest::StatusCode::OK, "read should succeed: {}", read.body);
        assert_eq!(
            read.first_row_keys(),
            vec!["id".to_string(), "label".to_string()],
            "?select=id,label must project exactly those two keys, got {}",
            read.body
        );
    }))
    .await;
}

/// #886's second half: the field-authorization gate must fire on the REST path.
///
/// `execute_query_direct` calls `deny_if_gated_field_selected` with
/// `selections.first().nested_fields` — an empty slice under the flat selection set — so
/// the guard inspected nothing and never fired, on a path whose own comment calls it
/// "leak-proof". **Repairing projection without this makes it a live bypass**, which is
/// why the two halves ship together.
///
/// `apply_field_rbac_filtering` — the `requires_scope` enforcement — was not called from
/// `execute_query_direct` at all, only from the two GraphQL paths.
#[tokio::test]
async fn a_scope_gated_field_is_refused_over_rest() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) =
            start_with_schema(true, build_schema_with_gated_field(FieldDenyPolicy::Reject)).await
        else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        // The token carries no roles, so it grants no scope.
        let read = get_orders_query(
            &server.url,
            Some(&token_for(TENANT_A)),
            None,
            &format!("?select=id,{GATED_FIELD}"),
        )
        .await;

        assert!(
            !read.status.is_success(),
            "selecting a scope-gated field without the scope must be refused over REST, \
             got {} {}",
            read.status,
            read.body
        );
        assert!(
            read.column(GATED_FIELD).is_empty(),
            "the gated field's value must never reach the client, got {}",
            read.body
        );
    }))
    .await;
}

/// The `on_deny = Mask` half of the same gate: the key stays, the value is nulled.
///
/// A masked field that is simply *absent* is not the contract — the response must keep
/// the key in its requested position with a null value, as both GraphQL paths do.
#[tokio::test]
async fn a_masked_field_is_nulled_not_served_over_rest() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) =
            start_with_schema(true, build_schema_with_gated_field(FieldDenyPolicy::Mask)).await
        else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let read = get_orders_query(
            &server.url,
            Some(&token_for(TENANT_A)),
            None,
            &format!("?select=id,{GATED_FIELD}"),
        )
        .await;

        assert_eq!(read.status, reqwest::StatusCode::OK, "read should succeed: {}", read.body);

        let values: Vec<&Value> = read
            .body
            .get("data")
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter_map(|r| r.get(GATED_FIELD)).collect())
            .unwrap_or_default();

        assert_eq!(
            values.len(),
            ROWS_A,
            "a masked field must keep its key in every row, got {}",
            read.body
        );
        assert!(
            values.iter().all(|v| v.is_null()),
            "a masked field must be nulled, not served: {}",
            read.body
        );
    }))
    .await;
}

// ---------------------------------------------------------------------------
// The same contract on the streaming export (#958)
// ---------------------------------------------------------------------------

/// One NDJSON export: the raw body, plus the status it came back with.
///
/// Deliberately not parsed as one JSON document — an NDJSON body is a sequence of
/// them, and a failed export can be a plain error line rather than any of the
/// shapes [`Read`] models.
async fn export_orders_ndjson(base: &str, token: Option<&str>) -> (reqwest::StatusCode, String) {
    let mut req = reqwest::Client::new()
        .get(format!("{base}/rest/v1/orders"))
        .header("accept", "application/x-ndjson");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let response = req.send().await.expect("REST NDJSON GET");
    let status = response.status();
    (status, response.text().await.expect("NDJSON body"))
}

/// Every `tenant_id` in an NDJSON body.
fn ndjson_tenants(body: &str) -> Vec<String> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let row: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("NDJSON line is not JSON ({e}): {line}"));
            assert!(row.get("error").is_none(), "export emitted an error line: {line}");
            row.get("tenant_id")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("NDJSON row carries no tenant_id: {line}"))
                .to_string()
        })
        .collect()
}

/// #958: the streaming export applies the tenant filter, like every other read.
///
/// The export is a **second** execution route to the same rows — it resolves through
/// `resolve_direct_read` and then streams rather than collecting. #739 is the standing
/// proof that "the other reader of the same query" is exactly where a row filter goes
/// missing, and it went missing silently: the rows were right-looking, just too many
/// of them. This asserts the filter on the surface rather than inferring it from the
/// shared code path.
#[tokio::test]
async fn an_ndjson_export_is_scoped_to_the_callers_tenant() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(true, true).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        for (tenant, expected) in [(TENANT_A, ROWS_A), (TENANT_B, ROWS_B)] {
            let (status, body) = export_orders_ndjson(&server.url, Some(&token_for(tenant))).await;

            assert_eq!(status, reqwest::StatusCode::OK, "{tenant}: export should succeed: {body}");

            let tenants = ndjson_tenants(&body);
            assert_eq!(
                tenants.len(),
                expected,
                "{tenant}: expected exactly its own {expected} rows of the {} seeded, got: {body}",
                ROWS_A + ROWS_B
            );
            assert!(
                tenants.iter().all(|t| t == tenant),
                "{tenant}: every exported row must belong to the caller's tenant, got {tenants:?}"
            );
        }
    }))
    .await;
}

/// #958: an anonymous streaming export of a tenant-scoped query fails **closed, and
/// as an HTTP status**.
///
/// Both halves matter. The refusal is the #739 contract. That it arrives as a status
/// rather than as an error line inside a `200` is a property of opening the read before
/// the response headers are sent: a client that streams a `200` and parses lines has no
/// reason to inspect the last one, so a refusal delivered that way reads as an empty
/// export.
#[tokio::test]
async fn an_anonymous_ndjson_export_of_a_tenant_scoped_query_is_refused_with_a_status() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) = start(false, false).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let (status, body) = export_orders_ndjson(&server.url, None).await;

        assert!(
            !status.is_success(),
            "an anonymous export of an inject-scoped query must be refused before the \
             stream opens, got {status} {body}"
        );
        assert!(
            !body.contains("\"tenant_id\""),
            "no row may reach an anonymous caller of a tenant-scoped query, got {body}"
        );
    }))
    .await;
}

/// #958: the field-authorization gate fires on the streaming export too.
///
/// `deny_if_gated_field_selected` and the `requires_scope` classification both live in
/// the read's resolution, not its delivery — but "both" is a claim about code that a
/// test on one delivery cannot make about the other.
#[tokio::test]
async fn a_scope_gated_field_is_refused_on_the_streaming_export() {
    Box::pin(temp_env::async_with_vars([(SECRET_ENV, Some(SECRET))], async {
        let Some(server) =
            start_with_schema(true, build_schema_with_gated_field(FieldDenyPolicy::Reject)).await
        else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };

        let mut req = reqwest::Client::new()
            .get(format!("{}/rest/v1/orders?select=id,{GATED_FIELD}", server.url))
            .header("accept", "application/x-ndjson");
        req = req.header("authorization", format!("Bearer {}", token_for(TENANT_A)));
        let response = req.send().await.expect("REST NDJSON GET");
        let status = response.status();
        let body = response.text().await.expect("NDJSON body");

        assert!(
            !status.is_success(),
            "selecting a scope-gated field without the scope must be refused on the \
             export too, got {status} {body}"
        );
        assert!(
            !body.contains(GATED_FIELD),
            "the gated field must never reach the client, got {body}"
        );
    }))
    .await;
}
