//! The transport refusal matrix (#1844): every transport, against the same five
//! requests, refuses the same way.
//!
//! The five requests (columns):
//!
//! 1. no credential;
//! 2. a token whose session was ended (otherwise valid);
//! 3. a valid token of a suspended tenant;
//! 4. a request from a peer over its rate limit, whose forwarded address names another client (a
//!    forwarded address from an untrusted peer is not believed);
//! 5. another tenant: (a) a token of tenant A naming tenant B, (b) A's identical query after B
//!    warmed the cache.
//!
//! Each row is a transport, one row per door where a transport has several. A cell is a
//! refusal pinned as measured, a cell pinned in the transport's own suite, a known gap
//! pinned to fail when it closes, `n/a` with its reason, or `pending (held)`. "Not 200"
//! is never an expectation.
//!
//! The rows are complete by construction. Every router the server mounts is recorded in
//! its mount ledger (`tools/check-mount-ledger.py` keeps every mount site recording), and
//! [`every_mounted_transport_has_a_row`] fails on a recorded transport this table has no
//! row for, and on a row whose transport did not mount. Each driver below iterates the
//! whole table and panics on a cell it has no way to drive, so no expectation sits in
//! the table without an assertion behind it.
//!
//! Differences between transports that still refuse, deliberately not pinned:
//!
//! - REST answers a suspended tenant `503` with the sanitized body and no `Retry-After`; GraphQL,
//!   its event stream and async operations name the suspension and send `Retry-After: 60`.
//! - gRPC words an ended session "Invalid or expired token", where HTTP says the token's session
//!   has ended.
//! - Case 5a is "Access denied" on GraphQL, REST and MCP; async operations name the mismatch.
//! - gRPC refuses a call with no credential before its rate limiter, so a per-address limit never
//!   charges it; HTTP charges such requests.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops `tenant_p1844*` and `p1844rls` → run `--test-threads=1`.
#![cfg(all(
    feature = "rest",
    feature = "mcp",
    feature = "auth",
    feature = "testing"
))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

mod common;

use std::{collections::BTreeSet, sync::Arc};

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy, VectorScanConfig},
    prelude::DatabaseAdapter as _,
    schema::CompiledSchema,
};
use fraiseql_server::{
    Server,
    server_config::{AsyncOperationsConfig, ServerConfig, StorageSectionConfig},
    tenancy::make_executor_factory,
    testing::mount_ledger::{MountLedger, MountRecord},
    token_revocation::{InMemoryRevocationStore, RevocationStore, TokenRevocationManager},
};
use fraiseql_test_support::{issuer::TestIssuer, try_database_url};
use reqwest::RequestBuilder;
use serde_json::{Value, json};

// ── The matrix ────────────────────────────────────────────────────────────────────

/// The five requests, case 5 split in two.
const CASES: [&str; 6] = [
    "1 no credential",
    "2 a token whose session was ended",
    "3 a valid token of a suspended tenant",
    "4 over the rate limit, a forwarded address from an untrusted peer",
    "5a a token of tenant A naming tenant B",
    "5b A's query after B warmed the cache",
];

/// What a cell expects.
#[derive(Debug, Clone, Copy)]
enum Cell {
    /// This HTTP status, a body carrying these words (`""`: any), and no tenant's rows.
    Status(u16, &'static str),
    /// The call is answered as a tool error carrying these words, and no tenant's rows.
    ToolError(&'static str),
    /// Served with the caller's own tenant's rows only, from a cache the other tenant
    /// warmed, and the cache was hit.
    OwnRowsOnly,
    /// Pinned in the transport's own suite: `file::module::test`.
    PinnedBy(&'static str),
    /// Served today, a documented gap, pinned by the named test to fail when it closes.
    KnownGap(&'static str, &'static str),
    /// Does not apply, and why.
    NotApplicable(&'static str),
    /// The transport is held: no request is sent.
    PendingHeld,
}

/// Where a transport is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mounted {
    /// By `build_router`, so recorded in the mount ledger.
    Ledger,
    /// By the embedder, not the server binary.
    Embedder,
    /// On its own listener.
    OwnListener,
    /// Not a network transport.
    NotNetwork,
}

/// The request a row sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Door {
    GraphqlPost,
    GraphqlStream,
    Rest,
    AsyncSubmit,
    AsyncLookup,
    McpInitialize,
    McpToolsList,
    McpToolCall,
    StorageObject,
    AdminRoles,
    Subscriptions,
    Grpc,
    FunctionBridge,
    ArrowFlight,
}

struct Row {
    transport: &'static str,
    door:      Door,
    mounted:   Mounted,
    cells:     [Cell; 6],
}

const SESSION_ENDED: &str = "Token has been revoked";
const NO_CACHE: &str = "this door reads through no shared cache";
const HANDSHAKE: &str =
    "the handshake carries no tenant's data; the tenant is dispatched on the tool call";

use Cell::{KnownGap, NotApplicable, OwnRowsOnly, PendingHeld, PinnedBy, Status, ToolError};

const ROWS: &[Row] = &[
    Row {
        transport: "graphql",
        door:      Door::GraphqlPost,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, ""),
            Status(401, SESSION_ENDED),
            Status(503, "suspended"),
            Status(429, ""),
            Status(403, ""),
            OwnRowsOnly,
        ],
    },
    Row {
        transport: "graphql",
        door:      Door::GraphqlStream,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, ""),
            Status(401, SESSION_ENDED),
            Status(503, "suspended"),
            Status(429, ""),
            Status(403, ""),
            OwnRowsOnly,
        ],
    },
    Row {
        transport: "rest",
        door:      Door::Rest,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, ""),
            Status(401, SESSION_ENDED),
            Status(503, ""),
            Status(429, ""),
            Status(403, ""),
            OwnRowsOnly,
        ],
    },
    Row {
        transport: "async-operations",
        door:      Door::AsyncSubmit,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, ""),
            Status(401, SESSION_ENDED),
            Status(503, "suspended"),
            Status(429, ""),
            Status(403, ""),
            NotApplicable("a submission answers an id; the rows come back through the lookup"),
        ],
    },
    Row {
        transport: "async-operations",
        door:      Door::AsyncLookup,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, ""),
            Status(401, SESSION_ENDED),
            Status(404, ""),
            Status(429, ""),
            Status(404, ""),
            NotApplicable(NO_CACHE),
        ],
    },
    Row {
        transport: "mcp",
        door:      Door::McpInitialize,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, "Authentication required"),
            Status(401, SESSION_ENDED),
            NotApplicable(HANDSHAKE),
            Status(429, ""),
            NotApplicable(HANDSHAKE),
            NotApplicable(HANDSHAKE),
        ],
    },
    Row {
        transport: "mcp",
        door:      Door::McpToolsList,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, "Authentication required"),
            Status(401, SESSION_ENDED),
            NotApplicable(HANDSHAKE),
            Status(429, ""),
            NotApplicable(HANDSHAKE),
            NotApplicable(HANDSHAKE),
        ],
    },
    Row {
        transport: "mcp",
        door:      Door::McpToolCall,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, "Authentication required"),
            Status(401, SESSION_ENDED),
            ToolError("suspended"),
            Status(429, ""),
            ToolError("Access denied"),
            OwnRowsOnly,
        ],
    },
    Row {
        transport: "storage",
        door:      Door::StorageObject,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, ""),
            Status(401, SESSION_ENDED),
            Status(503, "suspended"),
            Status(429, ""),
            PendingHeld,
            NotApplicable(NO_CACHE),
        ],
    },
    Row {
        transport: "admin",
        door:      Door::AdminRoles,
        mounted:   Mounted::Ledger,
        cells:     [
            Status(401, ""),
            Status(403, "Invalid token"),
            NotApplicable(
                "disjoint tenant namespaces: a tenant administrator's tenant is an RBAC \
                 UUID, a runtime tenant's key is [A-Za-z0-9_], so suspending one cannot name \
                 the other",
            ),
            Status(429, ""),
            Status(403, "administers a different tenant"),
            NotApplicable(NO_CACHE),
        ],
    },
    Row {
        transport: "subscriptions",
        door:      Door::Subscriptions,
        mounted:   Mounted::Ledger,
        cells:     [PendingHeld; 6],
    },
    Row {
        transport: "grpc",
        door:      Door::Grpc,
        mounted:   Mounted::Embedder,
        cells:     [
            PinnedBy("grpc_transport_e2e_test::request_without_token_returns_unauthenticated"),
            PinnedBy(
                "grpc_transport_e2e_test::refusal_cells::\
                 a_token_whose_session_was_ended_is_unauthenticated",
            ),
            KnownGap(
                "#1550",
                "grpc_transport_e2e_test::refusal_cells::\
                 a_tokens_tenant_is_not_dispatched_known_gap_1550",
            ),
            PinnedBy(
                "grpc_transport_e2e_test::a_spoofed_forwarded_for_does_not_move_the_rate_limit_key",
            ),
            KnownGap(
                "#1550",
                "grpc_transport_e2e_test::refusal_cells::\
                 a_tokens_tenant_is_not_dispatched_known_gap_1550",
            ),
            KnownGap(
                "#1550",
                "grpc_transport_e2e_test::refusal_cells::\
                 a_tokens_tenant_is_not_dispatched_known_gap_1550",
            ),
        ],
    },
    Row {
        transport: "function-bridge",
        door:      Door::FunctionBridge,
        mounted:   Mounted::NotNetwork,
        cells:     [
            NotApplicable("no caller credential: a function runs as its declared identity"),
            NotApplicable("no caller credential: a function runs as its declared identity"),
            PinnedBy(
                "tenant_rest_dispatch_e2e_pg::function_writes::\
                 an_unregistered_or_suspended_tenants_function_write_is_refused",
            ),
            NotApplicable("no caller address: a function is not a network request"),
            PinnedBy(
                "tenant_rest_dispatch_e2e_pg::function_writes::\
                 a_tenant_bound_function_cannot_be_steered_to_another_tenant",
            ),
            NotApplicable(
                "a function reads on its own tenant's executor (5a), and each executor \
                 holds its own cache",
            ),
        ],
    },
    Row {
        transport: "arrow_flight",
        door:      Door::ArrowFlight,
        mounted:   Mounted::OwnListener,
        cells:     [PendingHeld; 6],
    },
];

/// Mounted routers that are not a transport onto tenant data, and why.
const NOT_TRANSPORTS: &[(&str, &str)] = &[
    ("health", "liveness and readiness: no data"),
    ("studio", "the admin dashboard's static shell; its API is the `admin` row"),
    ("security-txt", "a static contact document"),
    ("playground", "an HTML page; its queries go through the `graphql` row"),
    (
        "introspection",
        "the default schema's shape, the same for every caller it serves",
    ),
    (
        "schema-export",
        "the default schema's shape, the same for every caller it serves",
    ),
    (
        "schema-metadata",
        "the default schema's shape, the same for every caller it serves",
    ),
    ("metrics", "process-wide counters behind `metrics_token`, no rows"),
    ("design-audit", "audits of the compiled schema, behind the admin scope, no rows"),
    ("auth", "the flows that issue credentials take none"),
    (
        "auth-account",
        "the caller's own account (`/auth/me`, its sessions, its MFA), no tenant rows",
    ),
    ("scim", "provisioning under its own provisioning token, not a caller's session"),
    ("webhooks", "provider-signed messages, not a caller's credential"),
    (
        "observers",
        "observer configuration behind the auth layer's admin scope, no tenant rows",
    ),
    (
        "identity",
        "the identity-cache admin API, behind the admin bearer: the `admin` row",
    ),
    (
        "inbound_email",
        "the suppression admin API, behind the admin bearer: the `admin` row",
    ),
    ("inbound_webhooks", "its routes are recorded as `webhooks`"),
    ("saml", "the SAML provider admin API, behind the admin gate: the `admin` row"),
];

/// The recorded transports the table has no row for.
fn unrowed(records: &[MountRecord]) -> BTreeSet<&'static str> {
    records
        .iter()
        .map(|record| record.transport)
        .filter(|transport| {
            !ROWS.iter().any(|row| row.transport == *transport)
                && !NOT_TRANSPORTS.iter().any(|(name, _)| name == transport)
        })
        .collect()
}

/// The ledger-mounted rows whose transport the ledger does not hold.
fn unmounted(records: &[MountRecord]) -> BTreeSet<&'static str> {
    ROWS.iter()
        .filter(|row| row.mounted == Mounted::Ledger)
        .map(|row| row.transport)
        .filter(|transport| !records.iter().any(|record| record.transport == *transport))
        .collect()
}

/// A transport the table has never heard of is reported, and so is a row that mounted
/// nowhere: the completeness check can fail.
#[test]
fn a_transport_without_a_row_is_reported() {
    let records = [
        MountRecord::auth_layer("p1844-new-transport"),
        MountRecord::public("health"),
    ];
    assert_eq!(unrowed(&records), BTreeSet::from(["p1844-new-transport"]));
    assert!(unmounted(&records).contains("graphql"));
}

/// Every `n/a` says why, every known gap names its issue, and both transports held
/// are held across their whole row.
#[test]
fn every_cell_that_is_not_a_refusal_says_why() {
    for row in ROWS {
        for (case, cell) in row.cells.iter().enumerate() {
            let at = format!("{} {:?} — case {}", row.transport, row.door, CASES[case]);
            match cell {
                NotApplicable(reason) => assert!(reason.len() > 20, "{at}: n/a without a reason"),
                KnownGap(issue, _) => assert!(
                    issue.starts_with('#') && issue[1..].chars().all(|c| c.is_ascii_digit()),
                    "{at}: a known gap names its issue: {issue}"
                ),
                _ => {},
            }
        }
    }
    for held in ["subscriptions", "arrow_flight"] {
        let row = ROWS.iter().find(|row| row.transport == held).unwrap();
        assert!(row.cells.iter().all(|cell| matches!(cell, PendingHeld)), "{held}");
    }
}

/// Every cell pinned in another suite names a test that exists there.
#[test]
fn every_cell_pinned_elsewhere_names_a_test_that_exists() {
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut checked = 0;
    for row in ROWS {
        for cell in row.cells {
            let (PinnedBy(path) | KnownGap(_, path)) = cell else {
                continue;
            };
            let parts: Vec<&str> = path.split("::").collect();
            let (file, test, modules) =
                (parts[0], parts[parts.len() - 1], &parts[1..parts.len() - 1]);
            let source = std::fs::read_to_string(tests.join(format!("{file}.rs")))
                .unwrap_or_else(|e| panic!("{}: {path}: {e}", row.transport));
            assert!(
                source.contains(&format!("async fn {test}(")),
                "{} cell names `{path}`, which `{file}.rs` does not define",
                row.transport
            );
            for module in modules {
                assert!(source.contains(&format!("mod {module} {{")), "{path}: no `mod {module}`");
            }
            checked += 1;
        }
    }
    assert!(checked >= 7, "the gRPC and bridge cells are pinned elsewhere: {checked}");
}

// ── The deployment ────────────────────────────────────────────────────────────────

const ADMIN: &str = "p1844-admin-token-at-least-32-characters";
const BUCKET: &str = "p1844";
const ACTIVE: &str = "p1844a";
const OTHER: &str = "p1844b";
const SUSPENDED: &str = "p1844c";

const TOML: &str = r#"
[project]
name = "p1844"
version = "1.0.0"
database_target = "postgresql"

[types.Item]
sql_source = "v_p1844_item"
fields.id = { type = "Int" }
fields.marker = { type = "String" }

[queries.items]
return_type = "Item"
return_array = true
sql_source = "v_p1844_item"

[rest]
enabled = true
path = "/rest/v1"

[mcp]
enabled = true
transport = "http"
path = "/mcp"
require_auth = true

[tenancy]
mode = "schema"
tenant_claim = "tenant_id"
"#;

/// One table and its view per schema, each row marked with its schema's name.
async fn seed(adapter: &PostgresAdapter) {
    for schema in [
        "public".to_string(),
        format!("tenant_{ACTIVE}"),
        format!("tenant_{OTHER}"),
        format!("tenant_{SUSPENDED}"),
    ] {
        if schema == "public" {
            exec(adapter, "DROP VIEW IF EXISTS public.v_p1844_item").await;
            exec(adapter, "DROP TABLE IF EXISTS public.tb_p1844_item").await;
        } else {
            exec(adapter, &format!("DROP SCHEMA IF EXISTS {schema} CASCADE")).await;
            exec(adapter, &format!("CREATE SCHEMA {schema}")).await;
        }
        exec(adapter, &format!("CREATE TABLE {schema}.tb_p1844_item (id int, marker text)")).await;
        exec(
            adapter,
            &format!("INSERT INTO {schema}.tb_p1844_item VALUES (1, 'ROW-{schema}')"),
        )
        .await;
        exec(
            adapter,
            &format!(
                "CREATE VIEW {schema}.v_p1844_item AS SELECT id, jsonb_build_object('id', id, \
                 'marker', marker) AS data FROM {schema}.tb_p1844_item"
            ),
        )
        .await;
    }
}

const RLS_TOML: &str = r#"
[project]
name = "p1844rls"
version = "1.0.0"
database_target = "postgresql"

[types.Item]
sql_source = "p1844rls.v_item"
fields.id = { type = "Int" }
fields.marker = { type = "String" }

[queries.items]
return_type = "Item"
return_array = true
sql_source = "p1844rls.v_item"

[rest]
enabled = true
path = "/rest/v1"

[mcp]
enabled = true
transport = "http"
path = "/mcp"
require_auth = true

[security]
multi_tenant = true

[security.rls]
enabled = true

[[session_variables.variables]]
claim = "tenant_id"
name = "app.tenant_id"
source = "jwt"
"#;

/// Both tenants' rows in one table under a policy on `app.tenant_id`, read as a role that
/// cannot skip policies. Returns that role's URL.
async fn seed_rls(adapter: &PostgresAdapter, url: &str) -> String {
    for sql in [
        "DROP SCHEMA IF EXISTS p1844rls CASCADE",
        "CREATE SCHEMA p1844rls",
        "CREATE TABLE p1844rls.tb_item (id int PRIMARY KEY, tenant_id text NOT NULL, marker text)",
        "INSERT INTO p1844rls.tb_item VALUES (1, 'p1844a', 'ROW-tenant_p1844a'), \
         (2, 'p1844b', 'ROW-tenant_p1844b')",
        "ALTER TABLE p1844rls.tb_item ENABLE ROW LEVEL SECURITY",
        "ALTER TABLE p1844rls.tb_item FORCE ROW LEVEL SECURITY",
        "CREATE POLICY p ON p1844rls.tb_item USING \
         (tenant_id = NULLIF(current_setting('app.tenant_id', true), ''))",
        "CREATE VIEW p1844rls.v_item WITH (security_invoker = true) AS SELECT id, \
         jsonb_build_object('id', id, 'marker', marker) AS data FROM p1844rls.tb_item",
        "DO $$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'p1844_app') THEN \
         CREATE ROLE p1844_app LOGIN PASSWORD 'p1844'; END IF; END $$",
        "GRANT USAGE ON SCHEMA p1844rls TO p1844_app",
        "GRANT SELECT ON ALL TABLES IN SCHEMA p1844rls TO p1844_app",
    ] {
        exec(adapter, sql).await;
    }
    let host = url.split_once('@').unwrap().1;
    format!("postgres://p1844_app:p1844@{host}")
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<std::collections::HashMap<String, Value>> =
        adapter.execute_raw_query(sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// The compiled schema, every query cached for five minutes (caching is opted into per
/// query in the compiled schema, which `fraiseql.toml` does not express).
async fn compiled(toml: &str) -> (CompiledSchema, Value) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("fraiseql.toml");
    std::fs::write(&path, toml).unwrap();
    let (artifact, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(path.to_str().unwrap())
    })
    .await
    .expect("compile");
    let mut value: Value = serde_json::from_str(&artifact.schema.to_json().unwrap()).unwrap();
    for query in value["queries"].as_array_mut().unwrap() {
        query["cache_ttl_seconds"] = json!(300);
    }
    let json = value.to_string();
    let mut schema = CompiledSchema::from_json(&json, false).unwrap();
    schema.build_indexes();
    (schema, serde_json::from_str(&json).unwrap())
}

/// A private local bucket any authenticated caller may read and write.
fn storage(dir: &std::path::Path) -> std::collections::HashMap<String, StorageSectionConfig> {
    std::collections::HashMap::from([(
        BUCKET.to_string(),
        StorageSectionConfig {
            backend:             "local".to_string(),
            path:                Some(dir.to_string_lossy().into_owned()),
            bucket:              None,
            region:              None,
            endpoint:            None,
            project_id:          None,
            account_name:        None,
            access:              Some("private".to_string()),
            max_object_bytes:    Some(4096),
            allowed_mime_types:  None,
            serve_inline:        None,
            policies:            None,
            upload_ttl_secs:     None,
            transform_presets:   None,
            default_resize_mode: None,
            watermark_font:      None,
        },
    )])
}

/// A running server.
struct Rig {
    base:     String,
    http:     reqwest::Client,
    issuer:   TestIssuer,
    sessions: Arc<TokenRevocationManager>,
    ledger:   MountLedger,
    _dir:     tempfile::TempDir,
    _stop:    tokio::sync::oneshot::Sender<()>,
}

/// Which deployment a rig serves.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Schema-mode runtime tenancy, with `ACTIVE`, `OTHER` and a suspended `SUSPENDED`,
    /// storage, async operations and the admin plane.
    Tenants,
    /// As `Tenants`, under a per-address rate limit.
    RateLimited,
    /// One schema under row-level security, its reads cached across tenants.
    SharedCache,
}

async fn rig(shape: Shape) -> Option<Rig> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.unwrap();
    seed(&admin).await;
    let rls = shape == Shape::SharedCache;
    let served_url = if rls {
        seed_rls(&admin, &url).await
    } else {
        url.clone()
    };
    let (schema, schema_json) = compiled(if rls { RLS_TOML } else { TOML }).await;
    let dir = tempfile::tempdir().unwrap();
    let issuer = TestIssuer::start().await;
    let store: Arc<dyn RevocationStore> = Arc::new(InMemoryRevocationStore::new());
    let sessions = Arc::new(TokenRevocationManager::new(store, false, false, 86_400));
    let mut config = ServerConfig {
        cors_enabled: false,
        database_url: served_url.clone(),
        auth: Some(common::test_issuer::oidc_config(&issuer, true)),
        admin_api_enabled: true,
        admin_token: Some(ADMIN.to_string()),
        enable_graphql_incremental: true,
        cache_enabled: rls,
        ..ServerConfig::default()
    };
    if !rls {
        config.tenancy.runtime.enabled = true;
        config.storage = storage(dir.path());
        config.async_operations = Some(AsyncOperationsConfig {
            operations: vec!["items".to_string()],
            workers: 1,
            poll_interval_ms: 100,
            ..AsyncOperationsConfig::default()
        });
    }
    if shape == Shape::RateLimited {
        config.rate_limiting = Some(fraiseql_server::middleware::ServerRateLimitingConfig {
            policy: fraiseql_core::schema::RateLimitingSecurityConfig {
                enabled: true,
                requests_per_second: 1,
                burst_size: 2,
                ..fraiseql_core::schema::RateLimitingSecurityConfig::default()
            },
            ..fraiseql_server::middleware::ServerRateLimitingConfig::default()
        });
    }
    let storage_state = if rls {
        None
    } else {
        fraiseql_server::server_config::build_storage_state(&config).await.unwrap()
    };
    // The RLS role may not provision the admin plane's tables, so that rig has no pool.
    let pool = if rls {
        None
    } else {
        Some(sqlx::PgPool::connect(&url).await.unwrap())
    };
    let adapter = Arc::new(PostgresAdapter::new(&served_url).await.unwrap());
    let mut server = Box::pin(Server::new(config, schema, adapter, pool))
        .await
        .expect("Server::new")
        .with_revocation_manager(Arc::clone(&sessions))
        .with_tenant_executor_factory(make_executor_factory::<
            fraiseql_core::cache::CachedDatabaseAdapter<PostgresAdapter>,
        >(
            PostgresTlsConfig::default(),
            ReadReplicaPolicy::default(),
            VectorScanConfig::default(),
            fraiseql_core::db::postgres::TransactionRetry::off(),
        ));
    if let Some(state) = storage_state {
        server = server.with_storage_state(state);
    }
    let ledger = server.mount_ledger();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let rig = Rig {
        base: format!("http://127.0.0.1:{port}"),
        http: reqwest::Client::new(),
        issuer,
        sessions,
        ledger,
        _dir: dir,
        _stop: stop,
    };
    if !rls {
        for key in [ACTIVE, OTHER, SUSPENDED] {
            rig.under_the_limit(shape).await;
            let (status, body) = rig
                .send(rig.http.put(rig.url(&format!("/api/v1/admin/tenants/{key}"))).bearer_auth(ADMIN).json(
                    &json!({ "schema": schema_json, "connection": { "connection_string": url } }),
                ))
                .await;
            assert!(status < 300, "register {key}: {status} {body}");
        }
        rig.under_the_limit(shape).await;
        let (status, body) = rig
            .send(
                rig.http
                    .post(rig.url(&format!("/api/v1/admin/tenants/{SUSPENDED}/suspend")))
                    .bearer_auth(ADMIN),
            )
            .await;
        assert!(status < 300, "suspend: {status} {body}");
    }
    Some(rig)
}

impl Rig {
    /// On the rate-limited rig, wait for the bucket to refill (one request a second, a
    /// burst of two), so the next request is under the limit.
    async fn under_the_limit(&self, shape: Shape) {
        if shape == Shape::RateLimited {
            tokio::time::sleep(std::time::Duration::from_millis(2_100)).await;
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// A token of `sub`, bound to `tenant`.
    fn token(&self, sub: &str, tenant: &str) -> String {
        self.issuer.mint(&self.issuer.claims().set("sub", sub).set("tenant_id", tenant))
    }

    async fn send(&self, request: RequestBuilder) -> (u16, String) {
        let response = request.send().await.unwrap();
        (response.status().as_u16(), response.text().await.unwrap_or_default())
    }
}

// ── Doors ─────────────────────────────────────────────────────────────────────────

/// Who sends a request: a credential and the tenant it names, if any.
#[derive(Clone, Copy)]
struct Caller<'a> {
    token: Option<&'a str>,
    hint:  Option<&'a str>,
}

const ITEMS: &str = "{ items { id marker } }";

fn with(request: RequestBuilder, caller: Caller<'_>) -> RequestBuilder {
    let request = match caller.token {
        Some(token) => request.bearer_auth(token),
        None => request,
    };
    match caller.hint {
        Some(hint) => request.header("X-Tenant-ID", hint),
        None => request,
    }
}

fn mcp_post(rig: &Rig, caller: Caller<'_>, body: &Value, session: Option<&str>) -> RequestBuilder {
    let request = with(rig.http.post(rig.url("/mcp")), caller)
        .header("accept", "application/json, text/event-stream")
        .json(body);
    match session {
        Some(session) => request.header("mcp-session-id", session),
        None => request,
    }
}

fn mcp_initialize() -> Value {
    json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
        "protocolVersion":"2025-03-26","capabilities":{},
        "clientInfo":{"name":"p1844","version":"0"}}})
}

/// The answer to `door` from `caller`. An MCP list or call opens its session first when
/// the endpoint admits the caller, and is sent without one when it does not.
async fn answer(rig: &Rig, door: Door, caller: Caller<'_>, target: &str) -> (u16, String) {
    match door {
        Door::GraphqlPost => {
            rig.send(
                with(rig.http.post(rig.url("/graphql")), caller).json(&json!({ "query": ITEMS })),
            )
            .await
        },
        Door::GraphqlStream => {
            rig.send(
                with(rig.http.post(rig.url("/graphql")), caller)
                    .header("accept", "text/event-stream")
                    .json(&json!({ "query": "{ items @stream(initialCount: 0) { id marker } }" })),
            )
            .await
        },
        Door::Rest => rig.send(with(rig.http.get(rig.url("/rest/v1/items")), caller)).await,
        Door::AsyncSubmit => {
            rig.send(
                with(rig.http.post(rig.url("/operations/v1/items")), caller)
                    .json(&json!({ "query": ITEMS })),
            )
            .await
        },
        Door::AsyncLookup => {
            rig.send(with(rig.http.get(rig.url(&format!("/operations/v1/{target}"))), caller))
                .await
        },
        Door::StorageObject => {
            rig.send(with(
                rig.http.get(rig.url(&format!("/storage/v1/object/{BUCKET}/{target}"))),
                caller,
            ))
            .await
        },
        Door::AdminRoles => rig.send(with(rig.http.get(rig.url("/api/roles")), caller)).await,
        Door::McpInitialize => rig.send(mcp_post(rig, caller, &mcp_initialize(), None)).await,
        Door::McpToolsList | Door::McpToolCall => {
            let body = if door == Door::McpToolsList {
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}})
            } else {
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                       "params":{"name":"items","arguments":{}}})
            };
            let opened = mcp_post(rig, caller, &mcp_initialize(), None).send().await.unwrap();
            let session = opened
                .headers()
                .get("mcp-session-id")
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string);
            let Some(session) = session else {
                return rig.send(mcp_post(rig, caller, &body, None)).await;
            };
            let initialized = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
            let _ = mcp_post(rig, caller, &initialized, Some(&session)).send().await;
            rig.send(mcp_post(rig, caller, &body, Some(&session))).await
        },
        Door::Subscriptions | Door::Grpc | Door::FunctionBridge | Door::ArrowFlight => {
            panic!("{door:?} is not driven over HTTP")
        },
    }
}

/// Assert `cell` of `row` against `answer`.
fn assert_cell(row: &Row, case: usize, cell: Cell, (status, body): &(u16, String)) {
    let at = format!("{} {:?} — case {}", row.transport, row.door, CASES[case]);
    assert!(!body.contains("ROW-"), "{at}: refused, yet carried rows: {status} {body}");
    match cell {
        Status(want, words) => {
            assert_eq!(*status, want, "{at}: {body}");
            assert!(body.contains(words), "{at}: {status} without '{words}': {body}");
        },
        ToolError(words) => {
            assert_eq!(*status, 200, "{at}: {body}");
            assert!(body.contains("\"isError\":true"), "{at}: not a tool error: {body}");
            assert!(body.contains(words), "{at}: tool error without '{words}': {body}");
        },
        other => panic!("{at}: {other:?} is not a refusal to assert here"),
    }
}

/// What the owner, `ACTIVE`'s caller, reads through a door: its tenant's row, or its
/// stored object. `None` for a door whose answer is not the resource itself.
const fn owner_reads(door: Door) -> Option<&'static str> {
    match door {
        Door::GraphqlPost | Door::GraphqlStream | Door::Rest | Door::McpToolCall => {
            Some("ROW-tenant_p1844a")
        },
        Door::StorageObject => Some("owned"),
        _ => None,
    }
}

/// Whether a cell is one this file drives.
const fn driven(cell: Cell) -> bool {
    matches!(cell, Status(..) | ToolError(_) | OwnRowsOnly)
}

// ── The cells ─────────────────────────────────────────────────────────────────────

/// Every transport the server mounts has a row, and every row of a mounted transport
/// mounted.
#[tokio::test]
async fn every_mounted_transport_has_a_row() {
    let Some(rig) = Box::pin(rig(Shape::Tenants)).await else {
        return;
    };
    let records = rig.ledger.records();
    assert!(!records.is_empty(), "the server built its router without recording a mount");
    assert_eq!(unrowed(&records), BTreeSet::new(), "mounted with no row in the matrix");
    assert_eq!(unmounted(&records), BTreeSet::new(), "a row whose transport did not mount");
}

/// Cases 1, 2, 3 and 5a, on every row the tenant deployment drives except the admin
/// plane's, which refuses its own credentials (below).
#[tokio::test]
async fn cases_1_2_3_and_5a_refuse_on_every_transport() {
    let Some(rig) = Box::pin(rig(Shape::Tenants)).await else {
        return;
    };
    let own = rig.token("u-active", ACTIVE);
    // The owner's subject in every tenant, so a refusal is the tenant's, never the
    // subject's.
    let other = rig.token("u-active", OTHER);
    let suspended = rig.token("u-active", SUSPENDED);
    let ended = rig.token("u-ended", ACTIVE);

    // What the doors that address a resource address: an object and an operation of
    // `ACTIVE`'s, each served to its owner first, so a refusal is the case's.
    let (status, body) = rig
        .send(
            rig.http
                .put(rig.url(&format!("/storage/v1/object/{BUCKET}/owned.txt")))
                .bearer_auth(&own)
                .header("content-type", "text/plain")
                .body("owned"),
        )
        .await;
    assert!(status < 300, "the owner's upload: {status} {body}");
    let (status, body) = rig
        .send(
            rig.http
                .post(rig.url("/operations/v1/items"))
                .bearer_auth(&own)
                .json(&json!({ "query": ITEMS })),
        )
        .await;
    assert!(status < 300, "the owner's submission: {status} {body}");
    let operation = serde_json::from_str::<Value>(&body).unwrap()["op_id"]
        .as_str()
        .unwrap()
        .to_string();

    let rows = ROWS
        .iter()
        .filter(|row| row.mounted == Mounted::Ledger && row.door != Door::AdminRoles);
    for row in rows.clone() {
        let target = match row.door {
            Door::StorageObject => "owned.txt",
            Door::AsyncLookup => operation.as_str(),
            _ => "",
        };
        // The owner is served: the refusals below are the cases', not the door's.
        if !matches!(row.cells[0], PendingHeld) {
            let served = answer(
                &rig,
                row.door,
                Caller {
                    token: Some(&own),
                    hint:  None,
                },
                target,
            )
            .await;
            assert!(
                served.0 < 300,
                "{} {:?}: the owner is not served: {served:?}",
                row.transport,
                row.door
            );
            if let Some(own_rows) = owner_reads(row.door) {
                assert!(
                    served.1.contains(own_rows) && !served.1.contains("ROW-public"),
                    "{} {:?}: the owner is not served its own rows: {served:?}",
                    row.transport,
                    row.door
                );
            }
        }
    }
    let ended_before = answer(
        &rig,
        Door::GraphqlPost,
        Caller {
            token: Some(&ended),
            hint:  None,
        },
        "",
    )
    .await;
    assert_eq!(ended_before.0, 200, "the token is served before its session ends");
    rig.sessions.revoke_all_for_user("u-ended").await.unwrap();

    for row in rows {
        let target = match row.door {
            Door::StorageObject => "owned.txt",
            Door::AsyncLookup => operation.as_str(),
            _ => "",
        };
        // 5a: on a door that addresses `ACTIVE`'s resource, the other tenant's own token;
        // elsewhere, `ACTIVE`'s token naming the other tenant.
        let cross = if target.is_empty() {
            Caller {
                token: Some(&own),
                hint:  Some(OTHER),
            }
        } else {
            Caller {
                token: Some(&other),
                hint:  None,
            }
        };
        for (case, caller) in [
            (
                0,
                Caller {
                    token: None,
                    hint:  None,
                },
            ),
            (
                1,
                Caller {
                    token: Some(&ended),
                    hint:  None,
                },
            ),
            (
                2,
                Caller {
                    token: Some(&suspended),
                    hint:  None,
                },
            ),
            (4, cross),
        ] {
            let cell = row.cells[case];
            match cell {
                PendingHeld | NotApplicable(_) => {},
                cell if driven(cell) => {
                    let answer = answer(&rig, row.door, caller, target).await;
                    assert_cell(row, case, cell, &answer);
                },
                other => {
                    panic!("{} {:?} case {case}: no driver for {other:?}", row.transport, row.door)
                },
            }
        }
    }
}

/// Case 4: over a per-address limit, a request whose forwarded address names another
/// client is still refused, on every transport.
#[tokio::test]
async fn case_4_a_forwarded_address_does_not_lift_the_limit() {
    let Some(rig) = Box::pin(rig(Shape::RateLimited)).await else {
        return;
    };
    let own = rig.token("u-active", ACTIVE);
    for row in ROWS.iter().filter(|row| row.mounted == Mounted::Ledger) {
        let cell = row.cells[3];
        match cell {
            PendingHeld | NotApplicable(_) => continue,
            Status(..) => {},
            other => panic!("{} {:?} case 4: no driver for {other:?}", row.transport, row.door),
        }
        let token = if row.door == Door::AdminRoles {
            ADMIN
        } else {
            own.as_str()
        };
        rig.under_the_limit(Shape::RateLimited).await;
        let mut answers = Vec::new();
        for client in 0..5 {
            let forwarded = format!("203.0.113.{client}");
            let caller = Caller {
                token: Some(token),
                hint:  None,
            };
            let answer = match row.door {
                Door::McpToolsList | Door::McpToolCall | Door::McpInitialize => {
                    rig.send(
                        mcp_post(&rig, caller, &mcp_initialize(), None)
                            .header("x-forwarded-for", &forwarded)
                            .header("x-real-ip", &forwarded),
                    )
                    .await
                },
                _ => {
                    let request = match row.door {
                        Door::GraphqlPost | Door::GraphqlStream => {
                            rig.http.post(rig.url("/graphql")).json(&json!({ "query": ITEMS }))
                        },
                        Door::Rest => rig.http.get(rig.url("/rest/v1/items")),
                        Door::AsyncSubmit => rig
                            .http
                            .post(rig.url("/operations/v1/items"))
                            .json(&json!({ "query": ITEMS })),
                        Door::AsyncLookup => rig
                            .http
                            .get(rig.url("/operations/v1/00000000-0000-0000-0000-000000000000")),
                        Door::StorageObject => {
                            rig.http.get(rig.url(&format!("/storage/v1/list/{BUCKET}")))
                        },
                        Door::AdminRoles => rig.http.get(rig.url("/api/roles")),
                        door => panic!("{door:?} is not driven over HTTP"),
                    };
                    rig.send(
                        with(request, caller)
                            .header("x-forwarded-for", &forwarded)
                            .header("x-real-ip", &forwarded),
                    )
                    .await
                },
            };
            answers.push(answer);
        }
        // The first request is under the limit, so the refusal below is the limit's.
        assert_ne!(answers[0].0, 429, "{} {:?}: refused before the limit", row.transport, row.door);
        assert_cell(row, 3, cell, answers.last().unwrap());
    }
}

/// Case 5b: one schema under row-level security, its reads cached. Each tenant's
/// identical query after the other warmed the cache answers its own rows only, on every
/// door that reads, and the cache was hit.
#[tokio::test]
async fn case_5b_a_cached_read_serves_only_its_own_tenants_rows() {
    let Some(rig) = Box::pin(rig(Shape::SharedCache)).await else {
        return;
    };
    let tokens = [
        (ACTIVE, rig.token("u-active", ACTIVE)),
        (OTHER, rig.token("u-other", OTHER)),
    ];
    let mut checked = 0;
    for row in ROWS.iter().filter(|row| row.mounted == Mounted::Ledger) {
        match row.cells[5] {
            OwnRowsOnly => {},
            PendingHeld | NotApplicable(_) => continue,
            other => panic!("{} {:?} case 5b: no driver for {other:?}", row.transport, row.door),
        }
        for _ in 0..2 {
            for (tenant, token) in &tokens {
                let (status, body) = answer(
                    &rig,
                    row.door,
                    Caller {
                        token: Some(token),
                        hint:  None,
                    },
                    "",
                )
                .await;
                let at = format!("{} {:?} as {tenant}", row.transport, row.door);
                assert_eq!(status, 200, "{at}: {body}");
                assert!(
                    body.contains(&format!("ROW-tenant_{tenant}")),
                    "{at}: not its rows: {body}"
                );
                let foreign = tokens.iter().find(|(t, _)| t != tenant).unwrap().0;
                assert!(
                    !body.contains(&format!("ROW-tenant_{foreign}")),
                    "{at}: {foreign}'s rows: {body}"
                );
            }
        }
        checked += 1;
    }
    assert!(checked >= 4, "graphql, its stream, REST and the MCP tool call read: {checked}");
    let (status, stats) = rig
        .send(rig.http.get(rig.url("/api/v1/admin/cache/stats")).bearer_auth(ADMIN))
        .await;
    assert_eq!(status, 200, "{stats}");
    let stats: Value = serde_json::from_str(&stats).unwrap();
    let hits = stats["data"]["caches"]
        .as_array()
        .and_then(|caches| caches.iter().find(|cache| cache["cache"] == "query_result"))
        .and_then(|cache| cache["hits"].as_u64())
        .unwrap_or(0);
    assert!(hits > 0, "the cache was never hit, so case 5b tested nothing: {stats}");
}

/// A resource a caller addresses is its owner's: another subject of the same tenant reads
/// a stored object and an async operation as absent, after the owner was served both.
#[tokio::test]
async fn another_subject_reads_an_owners_resource_as_absent() {
    let Some(rig) = Box::pin(rig(Shape::Tenants)).await else {
        return;
    };
    let owner = rig.token("u-owner", ACTIVE);
    let neighbour = rig.token("u-neighbour", ACTIVE);
    let (status, body) = rig
        .send(
            rig.http
                .put(rig.url(&format!("/storage/v1/object/{BUCKET}/kept.txt")))
                .bearer_auth(&owner)
                .header("content-type", "text/plain")
                .body("kept"),
        )
        .await;
    assert!(status < 300, "the owner's upload: {status} {body}");
    let (status, body) = rig
        .send(
            rig.http
                .post(rig.url("/operations/v1/items"))
                .bearer_auth(&owner)
                .json(&json!({ "query": ITEMS })),
        )
        .await;
    assert!(status < 300, "the owner's submission: {status} {body}");
    let operation = serde_json::from_str::<Value>(&body).unwrap()["op_id"]
        .as_str()
        .unwrap()
        .to_string();
    for (door, target) in [
        (Door::StorageObject, "kept.txt"),
        (Door::AsyncLookup, operation.as_str()),
    ] {
        let served = answer(
            &rig,
            door,
            Caller {
                token: Some(&owner),
                hint:  None,
            },
            target,
        )
        .await;
        assert!(served.0 < 300, "{door:?}: the owner is not served: {served:?}");
        let (status, body) = answer(
            &rig,
            door,
            Caller {
                token: Some(&neighbour),
                hint:  None,
            },
            target,
        )
        .await;
        assert_eq!(status, 404, "{door:?}: another subject: {body}");
        assert!(!body.contains("kept") && !body.contains("ROW-"), "{door:?}: {body}");
    }
}

/// The admin plane, under tenant administrator tokens: a token with no credential, a
/// tenant admin token that was withdrawn, the limiter on wrong tokens, and another tenant's roles.
#[tokio::test]
async fn the_admin_plane_refuses_its_cases() {
    let Some(rig) = Box::pin(rig(Shape::Tenants)).await else {
        return;
    };
    let row = ROWS.iter().find(|row| row.door == Door::AdminRoles).unwrap();
    let mint = |tenant: &'static str| {
        rig.send(
            rig.http
                .post(rig.url("/api/admin-tokens"))
                .bearer_auth(ADMIN)
                .json(&json!({ "tenant_id": tenant, "description": "p1844" })),
        )
    };
    let a = "11111111-1111-4111-8111-111111111111";
    let b = "22222222-2222-4222-8222-222222222222";
    let (status, minted) = mint(a).await;
    assert!(status < 300, "{status} {minted}");
    let admin_a = serde_json::from_str::<Value>(&minted).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, minted) = mint(a).await;
    assert!(status < 300, "{status} {minted}");
    let minted: Value = serde_json::from_str(&minted).unwrap();
    let ended = minted["token"].as_str().unwrap().to_string();
    let ended_id = minted["id"].as_str().unwrap().to_string();

    let roles = |token: Option<&str>, tenant: Option<&str>| {
        let path = tenant
            .map_or_else(|| "/api/roles".to_string(), |t| format!("/api/roles?tenant_id={t}"));
        let request = rig.http.get(rig.url(&path));
        rig.send(match token {
            Some(token) => request.bearer_auth(token),
            None => request,
        })
    };
    let served = roles(Some(&admin_a), None).await;
    assert_eq!(served.0, 200, "a tenant administrator lists its own roles: {served:?}");
    let before = roles(Some(&ended), None).await;
    assert_eq!(before.0, 200, "the token is served before it is withdrawn: {before:?}");
    let (status, body) = rig
        .send(
            rig.http
                .delete(rig.url(&format!("/api/admin-tokens/{ended_id}")))
                .bearer_auth(ADMIN),
        )
        .await;
    assert!(status < 300, "{status} {body}");

    assert_cell(row, 0, row.cells[0], &roles(None, None).await);
    assert_cell(row, 1, row.cells[1], &roles(Some(&ended), None).await);
    assert_cell(row, 4, row.cells[4], &roles(Some(&admin_a), Some(b)).await);
    assert!(matches!(row.cells[2], NotApplicable(_)) && matches!(row.cells[5], NotApplicable(_)));

    // Case 4 on the admin plane is its failure limiter: wrong tokens from one peer, each
    // naming another client, and then the right token from that peer is refused too, on
    // the platform's routes and on the routes tenant administrators share. Each serves
    // the right token before.
    let admin_doors = ["/api/v1/admin/tenants", "/api/roles"];
    let right = |path: &'static str| {
        rig.send(
            rig.http
                .get(rig.url(path))
                .bearer_auth(ADMIN)
                .header("x-forwarded-for", "198.51.100.200"),
        )
    };
    for path in admin_doors {
        let served = right(path).await;
        assert_eq!(served.0, 200, "{path}: the right token before: {served:?}");
    }
    for client in 0..14 {
        let forwarded = format!("198.51.100.{client}");
        let _ = rig
            .send(
                rig.http
                    .get(rig.url("/api/v1/admin/tenants"))
                    .bearer_auth("p1844-wrong-token-wrong-token-wrong")
                    .header("x-forwarded-for", &forwarded)
                    .header("x-real-ip", &forwarded),
            )
            .await;
    }
    for path in admin_doors {
        assert_cell(row, 3, row.cells[3], &right(path).await);
    }
}

/// The suite's documents compile with no database.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    for toml in [TOML, RLS_TOML] {
        let (schema, _) = compiled(toml).await;
        assert!(schema.find_type("Item").is_some());
    }
}
