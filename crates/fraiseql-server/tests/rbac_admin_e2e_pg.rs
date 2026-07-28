//! #748 / #769 / #768 regression: the RBAC management API against a real database.
//!
//! Every one of these three defects survived because the RBAC subsystem had never
//! executed a single statement against PostgreSQL. Its three test files
//! (`db_backend_tests.rs`, `integration_tests.rs`, `schema_tests.rs`) were
//! comment-only stubs — ~90 `#[test]` functions with empty bodies — so `cargo test`
//! reported them green while the schema DDL did not parse:
//!
//! * **#748** `ensure_schema` put `UNIQUE(name, COALESCE(tenant_id, …))` inside a `CREATE TABLE`. A
//!   table-level `UNIQUE` constraint accepts column names, never expressions, so PostgreSQL rejects
//!   it at parse time — and `serve_with_shutdown` runs that DDL unconditionally whenever
//!   `admin_token` is set. Setting the documented admin token on the shipped `-full` binary made
//!   the server refuse to boot.
//! * **#769** every handler passed `None` for tenant and `list_roles` hard-coded `limit 100, offset
//!   0`, so the 101st role was invisible with no indication, and `create_role` mapped *every*
//!   failure — including a malformed permission string and a dead database — to `409
//!   role_duplicate`.
//! * **#768** `GET /api/audit/permissions` returned a hard-coded `[]` while no handler recorded
//!   anything. An empty 200 under a compliance claim reads as "nothing happened", which is the
//!   dangerous shape.
//!
//! **Why this file rather than more cases in the stub files:** the stubs are deleted.
//! The subject here is SQL that a real PostgreSQL either accepts or rejects, and
//! handler behaviour observable only as a side effect in that database. Both need a
//! live server, so the boot case drives `Server::serve_on_listener` with a real pool
//! and a real `admin_token` over a real socket.
//!
//! Each test owns a scratch **database** rather than a scratch table: `ensure_schema`
//! creates unqualified `fraiseql_*` relations in whatever database it is pointed at,
//! and the shared test database is used by a dozen other suites.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_p05_*` databases → run
//! `--test-threads=1`.
#![cfg(feature = "observers")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_server::api::rbac_management::db_backend::{RbacDbBackend, RbacDbError};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, Row};

// ---------------------------------------------------------------------------
// Scratch-database plumbing
// ---------------------------------------------------------------------------

/// Replace the database component of a PostgreSQL URL.
fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

/// Drop and recreate `db`, returning a pool connected to it.
///
/// `WITH (FORCE)` because a previous run's pool may still hold connections and
/// `DROP DATABASE` refuses while any exist.
async fn scratch_pool(admin_url: &str, db: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {db}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    PgPool::connect(&with_database(admin_url, db))
        .await
        .expect("connect to scratch database")
}

/// Drop the scratch database. Best-effort: a failure here must not mask a test failure.
async fn drop_scratch(admin_url: &str, db: &str) {
    let Ok(admin) = PgPool::connect(admin_url).await else {
        return;
    };
    let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;
}

/// `Some(url)` when a database is configured, else `None` after printing why.
fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// Names of the relations `ensure_schema` is documented to create.
const RBAC_TABLES: [&str; 4] = [
    "fraiseql_roles",
    "fraiseql_permissions",
    "fraiseql_role_permissions",
    "fraiseql_user_roles",
];

async fn table_exists(pool: &PgPool, name: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT to_regclass($1) IS NOT NULL")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("to_regclass probe")
}

// ---------------------------------------------------------------------------
// #748 — the DDL must parse, and the constraint it intended must hold
// ---------------------------------------------------------------------------

/// The whole of #748: `ensure_schema` against a real PostgreSQL.
///
/// Pre-fix this fails with `syntax error at or near "("` on the `COALESCE` inside
/// the table-level `UNIQUE`, and none of the four tables exists afterwards.
#[tokio::test]
async fn ensure_schema_runs_against_real_postgres_and_is_idempotent() {
    let Some(url) = database_url_or_skip("ensure_schema_runs_against_real_postgres") else {
        return;
    };
    let db = "fraiseql_p05_ensure";
    let pool = scratch_pool(&url, db).await;

    let backend = RbacDbBackend::new(pool.clone());
    backend
        .ensure_schema()
        .await
        .expect("ensure_schema must succeed against real PostgreSQL");

    for table in RBAC_TABLES {
        assert!(table_exists(&pool, table).await, "ensure_schema did not create {table}");
    }

    // Idempotent: boot runs it on every start.
    backend.ensure_schema().await.expect("ensure_schema must be idempotent");

    pool.close().await;
    drop_scratch(&url, db).await;
}

/// The constraint the broken DDL was *trying* to express, asserted as behaviour.
///
/// Fixing the syntax error by simply deleting the `UNIQUE` clause would make this
/// test fail, which is the point: the per-tenant uniqueness is a property of the
/// schema, not an incidental detail of how it is spelled.
#[tokio::test]
async fn role_names_are_unique_per_tenant_with_null_treated_as_one_tenant() {
    let Some(url) = database_url_or_skip("role_names_are_unique_per_tenant") else {
        return;
    };
    let db = "fraiseql_p05_unique";
    let pool = scratch_pool(&url, db).await;
    let backend = RbacDbBackend::new(pool.clone());
    backend.ensure_schema().await.expect("ensure_schema");

    let tenant_a = "11111111-1111-1111-1111-111111111111";
    let tenant_b = "22222222-2222-2222-2222-222222222222";

    // Same name, different tenants — allowed.
    backend
        .create_role("editor", None, vec![], Some(tenant_a))
        .await
        .expect("tenant A editor");
    backend
        .create_role("editor", None, vec![], Some(tenant_b))
        .await
        .expect("tenant B editor");

    // Same name, same tenant — refused.
    let dup = backend.create_role("editor", None, vec![], Some(tenant_a)).await;
    assert!(
        matches!(dup, Err(RbacDbError::RoleDuplicate)),
        "duplicate role in the same tenant must be refused, got: {:?}",
        dup.map(|r| r.id)
    );

    // The global (NULL-tenant) namespace behaves the same way: NULL must collapse to
    // a single sentinel tenant, not to "every NULL is distinct".
    backend.create_role("global", None, vec![], None).await.expect("global role");
    let dup_global = backend.create_role("global", None, vec![], None).await;
    assert!(
        matches!(dup_global, Err(RbacDbError::RoleDuplicate)),
        "duplicate global role must be refused, got: {:?}",
        dup_global.map(|r| r.id)
    );

    pool.close().await;
    drop_scratch(&url, db).await;
}

/// The store round-trip: every method against a real database, asserting the
/// observable side effect rather than the return value.
#[tokio::test]
async fn rbac_store_round_trips_against_real_postgres() {
    let Some(url) = database_url_or_skip("rbac_store_round_trips") else {
        return;
    };
    let db = "fraiseql_p05_store";
    let pool = scratch_pool(&url, db).await;
    let backend = RbacDbBackend::new(pool.clone());
    backend.ensure_schema().await.expect("ensure_schema");

    // Role with permissions.
    let role = backend
        .create_role("auditor", Some("reads everything"), vec!["report:read".into()], None)
        .await
        .expect("create_role");
    assert_eq!(role.permissions, vec!["report:read".to_string()]);

    let fetched = backend.get_role(&role.id).await.expect("get_role");
    assert_eq!(fetched.name, "auditor");
    assert_eq!(fetched.permissions, vec!["report:read".to_string()]);

    // The permission row was created as a side effect of the role.
    let perms = backend.list_permissions(100, 0).await.expect("list_permissions");
    assert_eq!(perms.total, 1, "ensure_permission must have created exactly one permission");
    assert_eq!(perms.items[0].resource, "report");
    assert_eq!(perms.items[0].action, "read");

    // Update replaces the permission set.
    let updated = backend
        .update_role(&role.id, "auditor", Some("now writes too"), vec!["report:write".into()])
        .await
        .expect("update_role");
    assert_eq!(updated.permissions, vec!["report:write".to_string()]);

    // Assignment.
    backend
        .assign_role_to_user("user-1", &role.id, None)
        .await
        .expect("assign_role_to_user");
    let assignments =
        backend.list_user_roles("user-1", None, 100, 0).await.expect("list_user_roles");
    assert_eq!(assignments.total, 1);
    assert_eq!(assignments.items[0].role_id, role.id);

    // A permission bound to a role cannot be deleted.
    let refreshed = backend.list_permissions(100, 0).await.expect("list_permissions");
    let bound = refreshed.items.iter().find(|p| p.resource == "report" && p.action == "write");
    if let Some(bound) = bound {
        let in_use = backend.delete_permission(&bound.id).await;
        assert!(
            matches!(in_use, Err(RbacDbError::PermissionInUse)),
            "bound permission is in use"
        );
    }

    // Revoke, then delete.
    backend
        .revoke_role_from_user("user-1", &role.id)
        .await
        .expect("revoke_role_from_user");
    assert_eq!(
        backend
            .list_user_roles("user-1", None, 100, 0)
            .await
            .expect("list after revoke")
            .total,
        0
    );
    backend.delete_role(&role.id).await.expect("delete_role");
    assert!(matches!(backend.get_role(&role.id).await, Err(RbacDbError::RoleNotFound)));

    pool.close().await;
    drop_scratch(&url, db).await;
}

/// The unique index #748's fix introduces must actually be an index on the
/// expression, not a plain `(name, tenant_id)` one — otherwise two roles named
/// `x` with `tenant_id IS NULL` would both be accepted (NULLs compare distinct).
///
/// Asserted structurally as well as behaviourally so a future refactor that keeps
/// the behaviour by accident still records the intent.
#[tokio::test]
async fn role_uniqueness_is_backed_by_an_expression_index() {
    let Some(url) = database_url_or_skip("role_uniqueness_is_backed_by_an_expression_index") else {
        return;
    };
    let db = "fraiseql_p05_index";
    let pool = scratch_pool(&url, db).await;
    RbacDbBackend::new(pool.clone()).ensure_schema().await.expect("ensure_schema");

    let rows = sqlx::query(
        "SELECT indexdef FROM pg_indexes
         WHERE tablename = 'fraiseql_roles' AND indexdef LIKE '%UNIQUE%'",
    )
    .fetch_all(&pool)
    .await
    .expect("pg_indexes probe");

    let defs: Vec<String> = rows.iter().map(|r| r.get::<String, _>("indexdef")).collect();
    assert!(
        defs.iter().any(|d| d.contains("COALESCE")),
        "expected a UNIQUE index over COALESCE(tenant_id, …); found: {defs:?}"
    );

    pool.close().await;
    drop_scratch(&url, db).await;
}

// ---------------------------------------------------------------------------
// The live server: boot with `admin_token` and drive the HTTP surface
// ---------------------------------------------------------------------------

/// A real `fraiseql-server` bound to an ephemeral port, backed by a scratch database.
///
/// The handler defects (#769, #768) are about what the HTTP surface does, not what
/// the store can do — the store already supported tenant filtering and pagination
/// that no handler ever supplied. Asserting against the store would have passed
/// throughout. So every case below goes over a real socket.
struct RbacServer {
    base:      String,
    admin_url: String,
    db:        &'static str,
    pool:      sqlx::PgPool,
    shutdown:  Option<tokio::sync::oneshot::Sender<()>>,
    handle:    tokio::task::JoinHandle<Result<(), fraiseql_server::ServerError>>,
    client:    reqwest::Client,
}

/// 38 characters — comfortably over the configured admin-token minimum.
const ADMIN_TOKEN: &str = "p05-admin-token-at-least-32-chars-long";

impl RbacServer {
    async fn start(admin_url: &str, db: &'static str) -> Self {
        use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
        use fraiseql_server::{Server, server_config::ServerConfig};

        let pool = scratch_pool(admin_url, db).await;
        let scratch_url = with_database(admin_url, db);

        let schema: CompiledSchema = serde_json::from_value(serde_json::json!({
            "version": "2.0.0",
            "types": [],
            "queries": [],
            "mutations": [],
        }))
        .expect("compiled schema");

        let mut config = ServerConfig::default();
        config.database_url.clone_from(&scratch_url);
        config.admin_api_enabled = true;
        config.admin_token = Some(ADMIN_TOKEN.to_string());

        let adapter =
            Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));

        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();

        // #748's assertion: constructing and serving must not fail on the RBAC DDL.
        let server = Box::pin(Server::new(config, schema, adapter, Some(pool.clone())))
            .await
            .expect("Server::new with admin_token must succeed");

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            server
                .serve_on_listener(listener, async {
                    let _ = rx.await;
                })
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        Self {
            base: format!("http://127.0.0.1:{port}"),
            admin_url: admin_url.to_string(),
            db,
            pool,
            shutdown: Some(tx),
            handle,
            client: reqwest::Client::new(),
        }
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.get(format!("{}{path}", self.base)).bearer_auth(ADMIN_TOKEN)
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.post(format!("{}{path}", self.base)).bearer_auth(ADMIN_TOKEN)
    }

    fn delete(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.delete(format!("{}{path}", self.base)).bearer_auth(ADMIN_TOKEN)
    }

    /// `(status, body)` for a request, with the body parsed as JSON when possible.
    async fn send(req: reqwest::RequestBuilder) -> (u16, serde_json::Value) {
        let resp = req.send().await.expect("request");
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let body = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
        (status, body)
    }

    async fn shutdown(mut self) {
        drop(self.shutdown.take());
        let _ = (&mut self.handle).await;
        self.pool.close().await;
        drop_scratch(&self.admin_url, self.db).await;
    }
}

/// Create a role through the API and return its id.
async fn create_role(server: &RbacServer, body: serde_json::Value) -> String {
    let (status, created) = RbacServer::send(server.post("/api/roles").json(&body)).await;
    assert_eq!(status, 201, "POST /api/roles ({body}) must succeed; got {created}");
    created["id"].as_str().expect("created role carries an id").to_string()
}

// ---------------------------------------------------------------------------
// #748 — the boot path itself
// ---------------------------------------------------------------------------

/// A server configured the documented way to turn on the admin API must boot, and
/// its RBAC endpoints must be backed by tables that really exist.
#[tokio::test]
async fn server_boots_with_admin_token_and_serves_the_rbac_api() {
    let Some(url) = database_url_or_skip("server_boots_with_admin_token") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_boot").await;

    let (status, page) = RbacServer::send(server.get("/api/roles")).await;
    assert_eq!(status, 200, "the RBAC list endpoint must answer; body: {page}");

    create_role(
        &server,
        serde_json::json!({"name": "boot-role", "permissions": ["report:read"]}),
    )
    .await;

    let (_, page) = RbacServer::send(server.get("/api/roles")).await;
    let names: Vec<&str> = page["items"]
        .as_array()
        .map_or_else(Vec::new, |a| a.iter().filter_map(|r| r["name"].as_str()).collect());
    assert!(names.contains(&"boot-role"), "created role must be listed; got {page}");

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// #769 — tenant scoping, pagination, and error mapping
// ---------------------------------------------------------------------------

/// Roles created for a tenant must be listable *as* that tenant's roles, and a
/// different tenant must not see them.
///
/// Pre-fix every handler passed `None` for tenant, so `RoleDto.tenant_id` was always
/// `null` and every tenant's admin tooling saw every tenant's roles.
#[tokio::test]
async fn roles_are_scoped_to_the_tenant_they_were_created_for() {
    let Some(url) = database_url_or_skip("roles_are_scoped_to_the_tenant") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_tenant").await;

    let tenant_a = "11111111-1111-1111-1111-111111111111";
    let tenant_b = "22222222-2222-2222-2222-222222222222";

    create_role(
        &server,
        serde_json::json!({"name": "shared-name", "permissions": [], "tenant_id": tenant_a}),
    )
    .await;
    create_role(
        &server,
        serde_json::json!({"name": "shared-name", "permissions": [], "tenant_id": tenant_b}),
    )
    .await;
    create_role(&server, serde_json::json!({"name": "global-role", "permissions": []})).await;

    let (status, page) =
        RbacServer::send(server.get(&format!("/api/roles?tenant_id={tenant_a}"))).await;
    assert_eq!(status, 200, "tenant-scoped list must succeed; got {page}");
    let items = page["items"].as_array().expect("items array").clone();
    assert_eq!(items.len(), 1, "tenant A must see exactly its own role; got {page}");
    assert_eq!(items[0]["tenant_id"].as_str(), Some(tenant_a), "tenant_id must round-trip");

    let (_, page_b) =
        RbacServer::send(server.get(&format!("/api/roles?tenant_id={tenant_b}"))).await;
    assert_eq!(page_b["items"].as_array().map(Vec::len), Some(1), "tenant B sees only its own");

    // An unfiltered list is the operator's global view and still sees all three.
    let (_, all) = RbacServer::send(server.get("/api/roles?limit=500")).await;
    assert_eq!(all["total"].as_u64(), Some(3), "unfiltered list is the global view; got {all}");

    server.shutdown().await;
}

/// A malformed tenant id must be a client error, not a silent global-scope fallback
/// and not a `409 role_duplicate`.
#[tokio::test]
async fn a_malformed_tenant_id_is_refused() {
    let Some(url) = database_url_or_skip("a_malformed_tenant_id_is_refused") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_badtenant").await;

    let (status, body) = RbacServer::send(
        server
            .post("/api/roles")
            .json(&serde_json::json!({"name": "x", "permissions": [], "tenant_id": "not-a-uuid"})),
    )
    .await;
    assert_eq!(status, 400, "a malformed tenant id is the caller's error; got {body}");

    let (status, body) = RbacServer::send(server.get("/api/roles?tenant_id=not-a-uuid")).await;
    assert_eq!(status, 400, "a malformed tenant filter must not silently widen; got {body}");

    server.shutdown().await;
}

/// The 101st role must be reachable, and a truncated page must say it is truncated.
///
/// Pre-fix `list_roles(None, 100, 0)` was hard-coded with no query parameters at all:
/// the 101st role existed, was grantable by id, and was invisible to every listing.
#[tokio::test]
async fn role_listing_pages_and_never_silently_truncates() {
    let Some(url) = database_url_or_skip("role_listing_pages") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_page").await;

    // 101 roles: one more than the old hard-coded limit.
    for i in 1..=101 {
        create_role(
            &server,
            serde_json::json!({"name": format!("role_{i:03}"), "permissions": []}),
        )
        .await;
    }

    let (status, first) = RbacServer::send(server.get("/api/roles")).await;
    assert_eq!(status, 200, "default list must succeed; got {first}");
    assert_eq!(first["total"].as_u64(), Some(101), "total must report every role; got {first}");
    assert_eq!(
        first["has_more"].as_bool(),
        Some(true),
        "a truncated page must say so rather than look complete; got {first}"
    );
    let page_size = first["items"].as_array().expect("items").len();
    assert!(page_size < 101, "the default page is bounded; got {page_size}");

    // The tail is reachable.
    let (_, second) = RbacServer::send(server.get(&format!("/api/roles?offset={page_size}"))).await;
    let tail = second["items"].as_array().expect("items");
    assert!(
        tail.iter().any(|r| r["name"].as_str() == Some("role_101")),
        "the 101st role must be reachable; got {second}"
    );
    assert_eq!(second["has_more"].as_bool(), Some(false), "the last page says so; got {second}");

    server.shutdown().await;
}

/// Every failure mode of `POST /api/roles` used to answer `409 role_duplicate`.
#[tokio::test]
async fn role_creation_failures_are_diagnosed_rather_than_all_called_duplicates() {
    let Some(url) = database_url_or_skip("role_creation_failures_are_diagnosed") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_errors").await;

    // A genuine duplicate is still a 409.
    create_role(&server, serde_json::json!({"name": "dup", "permissions": []})).await;
    let (status, body) = RbacServer::send(
        server
            .post("/api/roles")
            .json(&serde_json::json!({"name": "dup", "permissions": []})),
    )
    .await;
    assert_eq!(status, 409, "a real duplicate is a conflict; got {body}");

    // A malformed permission string is the caller's mistake, not a conflict.
    let (status, body) = RbacServer::send(
        server
            .post("/api/roles")
            .json(&serde_json::json!({"name": "bad-perm", "permissions": ["no_colon"]})),
    )
    .await;
    assert_eq!(status, 400, "'no_colon' is not 'resource:action'; got {body}");
    assert!(
        body.to_string().contains("resource:action"),
        "the error must name what was wrong; got {body}"
    );

    // An unknown key must not be silently dropped — that is how a tenant scope
    // evaporates without anyone noticing (#757 / #806 class).
    let (status, body) = RbacServer::send(
        server
            .post("/api/roles")
            .json(&serde_json::json!({"name": "typo", "permissions": [], "tenantId": "x"})),
    )
    .await;
    assert!(
        (400..500).contains(&status),
        "a misspelled field must be refused, not ignored; got {status} {body}"
    );

    server.shutdown().await;
}

/// `GET /api/user-roles` with no `user_id` used to answer `200 []`, which reads as
/// "this user holds no roles" rather than "you did not name a user".
#[tokio::test]
async fn listing_user_roles_without_a_user_is_a_client_error() {
    let Some(url) = database_url_or_skip("listing_user_roles_without_a_user") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_userroles").await;

    let (status, body) = RbacServer::send(server.get("/api/user-roles")).await;
    assert_eq!(status, 400, "an unfiltered assignment list must not answer 200 []; got {body}");

    let role = create_role(&server, serde_json::json!({"name": "r", "permissions": []})).await;
    let (status, _) = RbacServer::send(
        server
            .post("/api/user-roles")
            .json(&serde_json::json!({"user_id": "u1", "role_id": role})),
    )
    .await;
    assert_eq!(status, 201);

    let (status, page) = RbacServer::send(server.get("/api/user-roles?user_id=u1")).await;
    assert_eq!(status, 200);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1), "got {page}");

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// #768 — the audit trail must have a writer
// ---------------------------------------------------------------------------

/// Every permission-changing operation must leave a readable trace.
///
/// Pre-fix `GET /api/audit/permissions` returned a hard-coded `[]` and no handler
/// recorded anything, so the endpoint asserted "no permission changes occurred"
/// regardless of activity — the shape that misleads a compliance reviewer.
#[tokio::test]
async fn permission_changes_are_recorded_and_readable() {
    let Some(url) = database_url_or_skip("permission_changes_are_recorded") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_audit").await;

    // A fresh installation genuinely has nothing.
    let (status, page) = RbacServer::send(server.get("/api/audit/permissions")).await;
    assert_eq!(status, 200);
    assert_eq!(page["total"].as_u64(), Some(0), "a fresh store has no events; got {page}");

    let role = create_role(
        &server,
        serde_json::json!({"name": "audited", "permissions": ["report:read"]}),
    )
    .await;
    let (status, _) = RbacServer::send(
        server
            .post("/api/user-roles")
            .json(&serde_json::json!({"user_id": "alice", "role_id": role})),
    )
    .await;
    assert_eq!(status, 201);
    let (status, _) =
        RbacServer::send(server.delete(&format!("/api/user-roles/alice/{role}"))).await;
    assert_eq!(status, 204);
    let (status, _) = RbacServer::send(server.delete(&format!("/api/roles/{role}"))).await;
    assert_eq!(status, 204);

    let (status, page) = RbacServer::send(server.get("/api/audit/permissions?limit=100")).await;
    assert_eq!(status, 200);
    let events: Vec<&str> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|e| e["event_type"].as_str())
        .collect();
    for expected in [
        "role_created",
        "role_assigned",
        "role_revoked",
        "role_deleted",
    ] {
        assert!(events.contains(&expected), "missing {expected} in {events:?} ({page})");
    }

    // The documented filters must filter.
    let (_, by_user) = RbacServer::send(server.get("/api/audit/permissions?user_id=alice")).await;
    let user_events = by_user["items"].as_array().expect("items");
    assert_eq!(
        user_events.iter().filter(|e| e["event_type"].is_string()).count(),
        2,
        "alice's trail is the assign and the revoke; got {by_user}"
    );

    let (_, none) =
        RbacServer::send(server.get("/api/audit/permissions?start_time=2999-01-01T00:00:00Z"))
            .await;
    assert_eq!(none["total"].as_u64(), Some(0), "a future window is empty; got {none}");

    // A malformed timestamp must be refused, not silently ignored — an ignored
    // filter turns a narrow query into an unnoticed full read.
    let (status, body) =
        RbacServer::send(server.get("/api/audit/permissions?start_time=yesterday")).await;
    assert_eq!(status, 400, "a malformed time filter must be refused; got {body}");

    server.shutdown().await;
}

/// The recording must be atomic with the change it describes.
///
/// A failed operation that still writes an audit row is as misleading as a
/// successful one that does not.
#[tokio::test]
async fn a_refused_change_records_no_audit_event() {
    let Some(url) = database_url_or_skip("a_refused_change_records_no_audit_event") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_atomic").await;

    create_role(&server, serde_json::json!({"name": "only", "permissions": []})).await;
    let (status, _) = RbacServer::send(
        server
            .post("/api/roles")
            .json(&serde_json::json!({"name": "only", "permissions": []})),
    )
    .await;
    assert_eq!(status, 409);

    let (_, page) = RbacServer::send(server.get("/api/audit/permissions?limit=100")).await;
    assert_eq!(
        page["total"].as_u64(),
        Some(1),
        "the refused duplicate must not have been recorded; got {page}"
    );

    server.shutdown().await;
}

/// **The gate that stops the next instance.** Every declared audit event type must
/// be produced by an operation reachable through the API.
///
/// A `#[non_exhaustive]` enum of event types is only worth as much as the number of
/// them something actually writes; declaring `PermissionDeleted` and never
/// recording it would leave the same hole #768 describes, one variant smaller.
/// Adding a variant without a producer fails here.
#[tokio::test]
async fn every_declared_audit_event_type_has_a_producer() {
    use fraiseql_server::api::rbac_management::db_backend::AuditEventType;

    let Some(url) = database_url_or_skip("every_declared_audit_event_type_has_a_producer") else {
        return;
    };
    let server = RbacServer::start(&url, "fraiseql_p05_corpus").await;

    // role_created + role_updated + role_assigned + role_revoked + role_deleted
    let role = create_role(&server, serde_json::json!({"name": "corpus", "permissions": []})).await;
    let (status, body) = RbacServer::send(
        server
            .client
            .put(format!("{}/api/roles/{role}", server.base))
            .bearer_auth(ADMIN_TOKEN)
            .json(&serde_json::json!({"name": "corpus2", "permissions": []})),
    )
    .await;
    assert_eq!(status, 200, "PUT /api/roles/{{id}}: {body}");
    let (status, _) = RbacServer::send(
        server
            .post("/api/user-roles")
            .json(&serde_json::json!({"user_id": "corpus-user", "role_id": role})),
    )
    .await;
    assert_eq!(status, 201);
    let (status, _) =
        RbacServer::send(server.delete(&format!("/api/user-roles/corpus-user/{role}"))).await;
    assert_eq!(status, 204);
    let (status, _) = RbacServer::send(server.delete(&format!("/api/roles/{role}"))).await;
    assert_eq!(status, 204);

    // permission_created + permission_deleted
    let (status, perm) = RbacServer::send(
        server
            .post("/api/permissions")
            .json(&serde_json::json!({"resource": "corpus", "action": "read"})),
    )
    .await;
    assert_eq!(status, 201, "POST /api/permissions: {perm}");
    let perm_id = perm["id"].as_str().expect("permission id");
    let (status, _) = RbacServer::send(server.delete(&format!("/api/permissions/{perm_id}"))).await;
    assert_eq!(status, 204);

    let (_, page) = RbacServer::send(server.get("/api/audit/permissions?limit=1000")).await;
    let recorded: Vec<&str> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|e| e["event_type"].as_str())
        .collect();

    for declared in AuditEventType::ALL {
        assert!(
            recorded.contains(&declared.as_str()),
            "no operation recorded {}; declared event types must all have a producer. \
             Recorded: {recorded:?}",
            declared.as_str()
        );
    }

    server.shutdown().await;
}
