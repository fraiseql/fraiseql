//! #962: the operator SQL console, end to end against PostgreSQL.
//!
//! G4 answered "mount the full arbitrary-SQL endpoint, gated", so what this suite
//! has to prove is not that SQL runs — it is that every gate holds when it does.
//! Each of these properties is a *database* behaviour, invisible to any test that
//! inspects the response alone, and several of them are invisible even to a test
//! that inspects the response *and* the request:
//!
//! | Defect | Caught by |
//! |---|---|
//! | a preview that actually persists | `rollback_is_the_default_and_the_write_does_not_persist` |
//! | a commit opt-in that commits nothing | `commit_opt_in_persists_the_write` |
//! | a read-only token that can write | `a_readonly_token_cannot_update` |
//! | a read-only token refused a read too | `a_readonly_token_can_select` |
//! | a commit smuggled past the read-only mode | `a_readonly_token_cannot_ask_for_a_commit` |
//! | a row cap that silently returns everything | `the_row_cap_truncates_and_says_so` |
//! | a request that raises the server's ceilings | `a_request_may_lower_a_bound_but_never_raise_it` |
//! | a timeout that never fires | `the_statement_timeout_cancels_a_long_statement` |
//! | `; DROP TABLE` appended past the first statement | `only_one_statement_is_accepted` |
//! | an RLS preview that previews nothing | `impersonation_applies_the_session_variables` |
//! | a console whose ledger holds only what worked | `every_execution_lands_in_the_audit_ledger` |
//! | a console mounted without being asked for | `the_console_is_absent_when_it_is_not_enabled` |
//!
//! **Why the impersonation test asserts a `current_setting()` view and not an RLS
//! policy.** The harness `DATABASE_URL` role is a superuser with `rolbypassrls`,
//! for which PostgreSQL skips every policy — an RLS assertion here would pass
//! whatever the console did with the claims, which is the "green at RED" shape.
//! A view whose `WHERE` reads `current_setting('app.tenant_id', true)` is subject
//! to no such bypass: it returns one tenant's rows, the other tenant's rows, or
//! none, entirely according to what the console set. That the same session
//! variables then drive real RLS is `example_multitenant_rls_e2e_pg`'s proof, and
//! this console resolves them through the *same* `resolve_session_variables` the
//! executor calls, so there is one implementation to be right.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p20_console` schema → run
//! `--test-threads=1`.
//! Gated by `[[test]] required-features = ["admin-sql"]` in `Cargo.toml`, not by
//! a file-level `#![cfg]` — the latter compiles to an empty binary that reports
//! zero tests and reads green wherever the feature is off (#1082).
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::{Arc, LazyLock, Mutex};

use fraiseql_auth::audit::logger::{AuditEntry, AuditLogger, init_audit_logger};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        CompiledSchema, FieldType, QueryDefinition, SessionVariableMapping, SessionVariableSource,
        TypeDefinition,
    },
};
use fraiseql_server::server_config::{AdminSqlConfig, ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

const SCHEMA: &str = "p20_console";
const WRITE_TOKEN: &str = "p20-console-write-token-0123456789abcdef";
const READONLY_TOKEN: &str = "p20-console-readonly-token-0123456789abc";

const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";
const TENANT_B: &str = "22222222-2222-2222-2222-222222222222";

/// The session-variable name the fixture's scoped view reads.
const TENANT_VAR: &str = "app.tenant_id";

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

// ── The audit ledger, captured ───────────────────────────────────────────────

/// An [`AuditLogger`] that keeps what it was given.
///
/// The global logger is a `OnceLock`, so the first caller in a test binary wins
/// for the whole process. Every test therefore installs *this same instance*
/// (through [`ledger`]), which makes installation order irrelevant — the
/// alternative, one logger per test, would silently capture nothing in every test
/// but the first.
struct CapturingAuditLogger {
    entries: Mutex<Vec<AuditEntry>>,
}

impl AuditLogger for CapturingAuditLogger {
    fn log_entry(&self, entry: AuditEntry) {
        self.entries.lock().unwrap().push(entry);
    }
}

static LEDGER: LazyLock<Arc<CapturingAuditLogger>> = LazyLock::new(|| {
    let logger = Arc::new(CapturingAuditLogger {
        entries: Mutex::new(Vec::new()),
    });
    init_audit_logger(logger.clone());
    logger
});

/// Install (once) and return the capturing ledger.
fn ledger() -> &'static Arc<CapturingAuditLogger> {
    &LEDGER
}

/// Entries recorded for the SQL console, newest last.
fn console_entries() -> Vec<AuditEntry> {
    ledger()
        .entries
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.event_type.as_str() == "admin_sql_execution")
        .cloned()
        .collect()
}

// ── Fixture ──────────────────────────────────────────────────────────────────

/// Five documents over two tenants, plus a view scoped by a session variable.
///
/// The per-tenant counts differ from each other and from every page size used
/// below (3 in A, 2 in B, 5 total), so an implementation that returns the grand
/// total, a page, or one hardcoded tenant cannot coincide with the right answer.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_doc (
               id bigint PRIMARY KEY,
               tenant_id uuid NOT NULL,
               title text NOT NULL
             )"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_doc (id, tenant_id, title) VALUES
               (1,'{TENANT_A}','a-one'),
               (2,'{TENANT_A}','a-two'),
               (3,'{TENANT_A}','a-three'),
               (4,'{TENANT_B}','b-one'),
               (5,'{TENANT_B}','b-two')"
        ),
        // Scoped by the session variable the console's impersonation sets. Not an
        // RLS policy: the harness role bypasses those (see the module docs).
        format!(
            "CREATE VIEW {SCHEMA}.v_scoped_doc AS
               SELECT id, tenant_id, title FROM {SCHEMA}.tb_doc
               WHERE tenant_id::text = current_setting('{TENANT_VAR}', true)"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_doc AS
               SELECT id, tenant_id,
                      jsonb_build_object('id', id, 'title', title) AS data
               FROM {SCHEMA}.tb_doc ORDER BY id"
        ),
    ];
    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// A minimal schema that nonetheless declares a session variable, because the
/// console resolves impersonation against the *compiled schema's* mappings — a
/// schema with none has no identity to preview, and the console correctly sets
/// nothing.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut doc = TypeDefinition::new("ConsoleDoc", format!("{SCHEMA}.v_doc"));
    doc.fields = vec![
        fraiseql_core::schema::FieldDefinition::new("id", FieldType::Int),
        fraiseql_core::schema::FieldDefinition::new("title", FieldType::String),
    ];
    schema.types.push(doc);
    schema.queries.push(
        QueryDefinition::new("docs", "ConsoleDoc")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_doc")),
    );
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   TENANT_VAR.to_string(),
        source: SessionVariableSource::Jwt {
            claim: "tenant_id".to_string(),
        },
    });
    schema.build_indexes();
    schema
}

const fn console_config() -> AdminSqlConfig {
    AdminSqlConfig {
        enabled:              true,
        // Short enough that the timeout test does not have to wait, long enough
        // that no other test in this suite trips it.
        statement_timeout_ms: 2_000,
        max_rows:             3,
        allow_commit:         true,
    }
}

fn config(admin_sql: Option<AdminSqlConfig>) -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        admin_api_enabled: true,
        admin_token: Some(WRITE_TOKEN.to_string()),
        admin_readonly_token: Some(READONLY_TOKEN.to_string()),
        admin_sql,
        ..ServerConfig::default()
    }
}

/// POST a statement to the console and return `(status, parsed body)`.
async fn run_sql(server: &TestServer, token: &str, body: Value) -> (reqwest::StatusCode, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/v1/admin/sql", server.url))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = resp.status();
    let text = resp.text().await.expect("body");
    let parsed = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, parsed)
}

/// Run a statement expected to succeed, returning the `data` payload.
async fn ok_sql(server: &TestServer, token: &str, body: Value) -> Value {
    let (status, parsed) = run_sql(server, token, body).await;
    assert_eq!(status, 200, "expected success, got {status}: {parsed}");
    parsed["data"].clone()
}

/// The number of rows in the fixture table, read *outside* any console
/// transaction — the only way to tell a preview from a change.
async fn table_count(adapter: &PostgresAdapter) -> i64 {
    let rows: Vec<std::collections::HashMap<String, Value>> = adapter
        .execute_raw_query(&format!("SELECT count(*)::bigint AS n FROM {SCHEMA}.tb_doc"))
        .await
        .expect("count");
    rows[0]["n"].as_i64().expect("count is a number")
}

struct Rig {
    server:  TestServer,
    adapter: Arc<PostgresAdapter>,
}

async fn boot(admin_sql: Option<AdminSqlConfig>) -> Option<Rig> {
    let _ = ledger();
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("adapter"));
    seed(&adapter).await;
    let server =
        Box::pin(TestServer::start_with_config(config(admin_sql), schema(), adapter.clone())).await;
    Some(Rig { server, adapter })
}

async fn boot_console() -> Option<Rig> {
    boot(Some(console_config())).await
}

// ── Rollback by default, commit on request ───────────────────────────────────

/// The acceptance's first backend-effect test. A statement runs — its `RETURNING`
/// row comes back, so it really executed — and then does not exist.
///
/// This is the property that makes the default safe to hand an operator: they can
/// see what a write *would* do without doing it. An implementation that committed
/// unconditionally passes every assertion about the response.
#[tokio::test]
async fn rollback_is_the_default_and_the_write_does_not_persist() {
    if database_url_or_skip("rollback_is_the_default_and_the_write_does_not_persist").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();
    let before = table_count(&rig.adapter).await;

    let data = ok_sql(
        &rig.server,
        WRITE_TOKEN,
        json!({
            "sql": format!(
                "INSERT INTO {SCHEMA}.tb_doc (id, tenant_id, title) \
                 VALUES (900, '{TENANT_A}', 'preview-only') RETURNING id, title"
            )
        }),
    )
    .await;

    assert_eq!(
        data["rows"],
        json!([[900, "preview-only"]]),
        "the statement must actually have run: {data}"
    );
    assert_eq!(data["committed"], json!(false), "the default must not commit: {data}");
    assert_eq!(table_count(&rig.adapter).await, before, "the row must not survive the request");
}

/// The acceptance's second backend-effect test, and the reason the first one is
/// not enough: a console that rolled back *everything* would pass
/// `rollback_is_the_default…` perfectly while being unable to change anything.
#[tokio::test]
async fn commit_opt_in_persists_the_write() {
    if database_url_or_skip("commit_opt_in_persists_the_write").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();
    let before = table_count(&rig.adapter).await;

    let data = ok_sql(
        &rig.server,
        WRITE_TOKEN,
        json!({
            "sql": format!(
                "INSERT INTO {SCHEMA}.tb_doc (id, tenant_id, title) \
                 VALUES (901, '{TENANT_B}', 'committed')"
            ),
            "commit": true
        }),
    )
    .await;

    assert_eq!(data["committed"], json!(true), "the opt-in must be honoured: {data}");
    assert_eq!(data["rows_affected"], json!(1), "one row inserted: {data}");
    assert_eq!(
        table_count(&rig.adapter).await,
        before + 1,
        "the committed row must survive the request"
    );
}

// ── The read-only token ──────────────────────────────────────────────────────

/// The acceptance's third backend-effect test. The refusal comes from
/// PostgreSQL's `READ ONLY` transaction mode (SQLSTATE 25006), not from reading
/// the statement — which is why it also covers the writes that do not look like
/// writes.
#[tokio::test]
async fn a_readonly_token_cannot_update() {
    if database_url_or_skip("a_readonly_token_cannot_update").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let (status, body) = run_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({ "sql": format!("UPDATE {SCHEMA}.tb_doc SET title = 'hijacked'") }),
    )
    .await;

    assert_eq!(status, 403, "a read-only token's write must be refused: {body}");
    let hijacked: Vec<std::collections::HashMap<String, Value>> = rig
        .adapter
        .execute_raw_query(&format!(
            "SELECT count(*)::bigint AS n FROM {SCHEMA}.tb_doc WHERE title = 'hijacked'"
        ))
        .await
        .unwrap();
    assert_eq!(hijacked[0]["n"].as_i64(), Some(0), "no row may have been touched");
}

/// The other half of the same gate: a read-only token must still be able to
/// *read*, or the console has simply been broken for the credential most
/// operators will use.
#[tokio::test]
async fn a_readonly_token_can_select() {
    if database_url_or_skip("a_readonly_token_can_select").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let data = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({ "sql": format!("SELECT title FROM {SCHEMA}.tb_doc WHERE id = 1") }),
    )
    .await;

    assert_eq!(data["rows"], json!([["a-one"]]), "{data}");
    assert_eq!(data["read_only"], json!(true), "the transaction ran READ ONLY: {data}");
}

/// A commit under a read-only token is refused *before* the database is touched.
///
/// Not merely cosmetic: `COMMIT` on a `READ ONLY` transaction succeeds and
/// persists nothing, so without this the console would answer `committed: true`
/// over a change that never happened — #749's fabricated-success shape.
#[tokio::test]
async fn a_readonly_token_cannot_ask_for_a_commit() {
    if database_url_or_skip("a_readonly_token_cannot_ask_for_a_commit").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let (status, body) =
        run_sql(&rig.server, READONLY_TOKEN, json!({ "sql": "SELECT 1", "commit": true })).await;

    assert_eq!(status, 403, "{body}");
    assert!(
        body["error"].as_str().unwrap_or_default().contains("read-only"),
        "the refusal must name the reason: {body}"
    );
}

// ── Blast-radius bounds ──────────────────────────────────────────────────────

/// The row cap stops the read and says so. A console that returned everything
/// while reporting `truncated: false` is a memory bound that does not exist.
#[tokio::test]
async fn the_row_cap_truncates_and_says_so() {
    if database_url_or_skip("the_row_cap_truncates_and_says_so").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    // The fixture has 5 rows; the configured cap is 3.
    let data = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({ "sql": format!("SELECT id FROM {SCHEMA}.tb_doc ORDER BY id") }),
    )
    .await;

    assert_eq!(data["rows"].as_array().map(Vec::len), Some(3), "capped at 3: {data}");
    assert_eq!(data["truncated"], json!(true), "and it must say so: {data}");

    // Exactly-at-the-cap is complete, not truncated: reporting a whole answer as
    // partial sends an operator looking for rows that do not exist.
    let exact = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({ "sql": format!("SELECT id FROM {SCHEMA}.tb_doc ORDER BY id LIMIT 3") }),
    )
    .await;
    assert_eq!(exact["rows"].as_array().map(Vec::len), Some(3), "{exact}");
    assert_eq!(exact["truncated"], json!(false), "3 of 3 is complete: {exact}");
}

/// A request may tighten the server's bounds and may not loosen them, and the
/// response reports what was actually applied.
///
/// The reporting is the load-bearing half: an operator who asked for a 10-minute
/// timeout and silently got 2 seconds reads the cancellation as a hung database.
#[tokio::test]
async fn a_request_may_lower_a_bound_but_never_raise_it() {
    if database_url_or_skip("a_request_may_lower_a_bound_but_never_raise_it").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let raised = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({
            "sql": format!("SELECT id FROM {SCHEMA}.tb_doc ORDER BY id"),
            "max_rows": 10_000,
            "statement_timeout_ms": 600_000
        }),
    )
    .await;
    assert_eq!(raised["max_rows"], json!(3), "the ceiling wins: {raised}");
    assert_eq!(raised["statement_timeout_ms"], json!(2000), "the ceiling wins: {raised}");
    assert_eq!(raised["rows"].as_array().map(Vec::len), Some(3), "{raised}");

    let lowered = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({
            "sql": format!("SELECT id FROM {SCHEMA}.tb_doc ORDER BY id"),
            "max_rows": 1
        }),
    )
    .await;
    assert_eq!(lowered["max_rows"], json!(1), "a tighter request is honoured: {lowered}");
    assert_eq!(lowered["rows"].as_array().map(Vec::len), Some(1), "{lowered}");

    // Zero is refused rather than clamped: PostgreSQL reads a zero timeout as *no*
    // timeout, so accepting it would turn the strictest-looking request into the
    // one with no limit at all.
    let (status, body) = run_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({ "sql": "SELECT 1", "statement_timeout_ms": 0 }),
    )
    .await;
    assert_eq!(status, 400, "a zero timeout must be refused: {body}");
}

/// The statement timeout is PostgreSQL's, so it stops the backend rather than
/// abandoning a query that keeps running.
#[tokio::test]
async fn the_statement_timeout_cancels_a_long_statement() {
    if database_url_or_skip("the_statement_timeout_cancels_a_long_statement").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let (status, body) = run_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({ "sql": "SELECT pg_sleep(30)", "statement_timeout_ms": 400 }),
    )
    .await;

    assert_eq!(status, 408, "a cancelled statement is a timeout, not a server fault: {body}");
    assert!(
        body["error"].as_str().unwrap_or_default().contains("statement_timeout"),
        "the refusal must name the bound that fired: {body}"
    );
}

/// One statement per request, enforced by the extended query protocol's Parse.
///
/// This is what stops `; COMMIT` escaping rollback-by-default and `; DROP TABLE`
/// riding behind a statement that looks harmless. It is not a `split(';')` — a
/// semicolon inside a string literal is not a statement boundary, and any parser
/// that thinks it is has a bypass.
#[tokio::test]
async fn only_one_statement_is_accepted() {
    if database_url_or_skip("only_one_statement_is_accepted").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let (status, body) = run_sql(
        &rig.server,
        WRITE_TOKEN,
        json!({
            "sql": format!("SELECT 1; DROP TABLE {SCHEMA}.tb_doc"),
            "commit": true
        }),
    )
    .await;
    assert_ne!(status, 200, "a two-statement request must not succeed: {body}");

    // The table is still there, which is the assertion that matters.
    assert_eq!(table_count(&rig.adapter).await, 5, "the second statement must not have run");

    // And a semicolon inside a literal is not a statement boundary.
    let data = ok_sql(&rig.server, READONLY_TOKEN, json!({ "sql": "SELECT 'a;b' AS s" })).await;
    assert_eq!(data["rows"], json!([["a;b"]]), "{data}");
}

// ── RLS preview ──────────────────────────────────────────────────────────────

/// Impersonation sets the session variables the executor would set, and the
/// database sees them.
///
/// Both tenants are asserted, and the un-impersonated case as well: a console
/// that hardcoded one tenant passes the first assertion, one that dropped the
/// claims entirely passes the third.
#[tokio::test]
async fn impersonation_applies_the_session_variables() {
    if database_url_or_skip("impersonation_applies_the_session_variables").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();
    let scoped = format!("SELECT title FROM {SCHEMA}.v_scoped_doc ORDER BY id");

    let as_a = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({
            "sql": scoped,
            "max_rows": 100,
            "impersonate": { "user_id": "operator-preview", "tenant_id": TENANT_A }
        }),
    )
    .await;
    assert_eq!(
        as_a["rows"],
        json!([["a-one"], ["a-two"], ["a-three"]]),
        "tenant A's rows only: {as_a}"
    );

    let as_b = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({
            "sql": scoped,
            "max_rows": 100,
            "impersonate": { "user_id": "operator-preview", "tenant_id": TENANT_B }
        }),
    )
    .await;
    assert_eq!(as_b["rows"], json!([["b-one"], ["b-two"]]), "tenant B's rows only: {as_b}");

    let unscoped =
        ok_sql(&rig.server, READONLY_TOKEN, json!({ "sql": scoped, "max_rows": 100 })).await;
    assert_eq!(
        unscoped["rows"],
        json!([]),
        "with no impersonation the variable is unset and the view admits nothing: {unscoped}"
    );

    // The variable itself, read back: the mapping's *name* is what the fixture
    // view depends on, so this pins the whole `jwt:tenant_id → app.tenant_id`
    // resolution rather than just its effect.
    let read_back = ok_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({
            "sql": format!("SELECT current_setting('{TENANT_VAR}', true) AS v"),
            "impersonate": { "user_id": "operator-preview", "tenant_id": TENANT_A }
        }),
    )
    .await;
    assert_eq!(read_back["rows"], json!([[TENANT_A]]), "{read_back}");
}

/// A claim in the reserved namespace is refused by name.
///
/// The token extractor strips `fraiseql.` from real tokens precisely so a client
/// cannot write the attributes the server derives (`actor_type`, `transport`,
/// `acting_for`). An operator endpoint that accepted them would be the hole the
/// extractor exists to close.
#[tokio::test]
async fn a_reserved_namespace_claim_is_refused() {
    if database_url_or_skip("a_reserved_namespace_claim_is_refused").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let (status, body) = run_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({
            "sql": "SELECT 1",
            "impersonate": {
                "user_id": "operator-preview",
                "claims": { "fraiseql.actor_type": "system_job" }
            }
        }),
    )
    .await;

    assert_eq!(status, 400, "{body}");
    assert!(
        body["error"].as_str().unwrap_or_default().contains("fraiseql.actor_type"),
        "the refusal must name the claim: {body}"
    );
}

// ── The ledger ───────────────────────────────────────────────────────────────

/// Every execution lands in the audit ledger — including the ones that failed and
/// the ones that were refused.
///
/// A ledger holding only successful statements answers "what did the operator
/// run?" with the subset that worked, which is the opposite of what an audit is
/// for: the refused `DROP` and the mistyped `UPDATE` are the entries an
/// investigation starts from.
#[tokio::test]
async fn every_execution_lands_in_the_audit_ledger() {
    if database_url_or_skip("every_execution_lands_in_the_audit_ledger").is_none() {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();
    let before = console_entries().len();

    // 1. a successful read
    let _ = ok_sql(&rig.server, READONLY_TOKEN, json!({ "sql": "SELECT 42 AS answer" })).await;
    // 2. a statement the database refused
    let _ = run_sql(
        &rig.server,
        READONLY_TOKEN,
        json!({ "sql": format!("UPDATE {SCHEMA}.tb_doc SET title = 'x'") }),
    )
    .await;
    // 3. a request the endpoint refused before reaching the database
    let _ =
        run_sql(&rig.server, READONLY_TOKEN, json!({ "sql": "SELECT 1", "commit": true })).await;
    // 4. a committed write
    let _ = ok_sql(
        &rig.server,
        WRITE_TOKEN,
        json!({
            "sql": format!(
                "INSERT INTO {SCHEMA}.tb_doc (id, tenant_id, title) \
                 VALUES (902, '{TENANT_A}', 'audited')"
            ),
            "commit": true
        }),
    )
    .await;

    let entries = console_entries();
    assert_eq!(
        entries.len(),
        before + 4,
        "all four executions are recorded, not just the ones that worked"
    );
    let recent = &entries[before..];

    assert!(recent[0].success, "the successful read is recorded as a success");
    assert!(!recent[1].success, "the database's refusal is recorded as a failure");
    assert!(!recent[2].success, "the endpoint's own refusal is recorded too");
    assert!(recent[3].success, "the committed write is recorded as a success");

    // The entry has to identify what ran and under which credential, or it
    // answers no question an audit asks.
    let ctx = recent[0].context.clone().unwrap_or_default();
    assert!(ctx.contains("SELECT 42 AS answer"), "the statement is named: {ctx}");
    assert!(ctx.contains("sha256="), "and identified beyond the truncation point: {ctx}");
    assert_eq!(
        recent[0].subject.as_deref(),
        Some("admin_readonly_token"),
        "the credential that authenticated is named"
    );
    assert_eq!(
        recent[3].subject.as_deref(),
        Some("admin_token"),
        "and it distinguishes the write credential"
    );
    assert!(
        recent[3].context.clone().unwrap_or_default().contains("committed=true"),
        "whether it persisted is the fact the audit is for: {:?}",
        recent[3].context
    );
    assert_eq!(recent[0].secret_type.as_str(), "admin_token");
}

// ── Mount conditions ─────────────────────────────────────────────────────────

/// With no `[admin_sql]` section at all the route does not exist, on a server
/// whose admin API is otherwise fully mounted.
///
/// The control is the section, not "the admin API is on": every other admin
/// endpoint answers on this server.
#[tokio::test]
async fn the_console_is_absent_when_there_is_no_section() {
    if database_url_or_skip("the_console_is_absent_when_there_is_no_section").is_none() {
        return;
    }
    let rig = Box::pin(boot(None)).await.unwrap();

    let (status, _) = run_sql(&rig.server, WRITE_TOKEN, json!({ "sql": "SELECT 1" })).await;
    assert_eq!(status, 404, "the console must not be mounted");

    // …while the rest of the admin API is up, so this is not a mis-booted server.
    let resp = reqwest::Client::new()
        .get(format!("{}/api/v1/admin/config", rig.server.url))
        .bearer_auth(READONLY_TOKEN)
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), 200, "the admin API itself is mounted");
}

/// A **present but disabled** section is the case an operator actually writes,
/// and it is a different code path from an absent one.
///
/// Added because the absent-section test above stayed green under a mutation that
/// removed the `enabled` check entirely: the `None` arm returns first, so that
/// test never reaches the flag it is named for. A server carrying
/// `[admin_sql] enabled = false` — the natural way to turn the console off
/// without deleting its tuning — would have mounted it.
#[tokio::test]
async fn the_console_is_absent_when_the_section_disables_it() {
    if database_url_or_skip("the_console_is_absent_when_the_section_disables_it").is_none() {
        return;
    }
    let rig = Box::pin(boot(Some(AdminSqlConfig {
        enabled: false,
        ..console_config()
    })))
    .await
    .unwrap();

    let (status, body) = run_sql(&rig.server, WRITE_TOKEN, json!({ "sql": "SELECT 1" })).await;
    assert_eq!(status, 404, "enabled = false must not mount the console: {body}");
}

/// No credential, and the wrong credential, are both refused — and neither
/// reaches the database.
#[tokio::test]
async fn the_console_refuses_an_unauthenticated_or_wrong_credential() {
    if database_url_or_skip("the_console_refuses_an_unauthenticated_or_wrong_credential").is_none()
    {
        return;
    }
    let rig = Box::pin(boot_console()).await.unwrap();

    let anonymous = reqwest::Client::new()
        .post(format!("{}/api/v1/admin/sql", rig.server.url))
        .json(&json!({ "sql": "SELECT 1" }))
        .send()
        .await
        .expect("request");
    assert_eq!(anonymous.status(), 401, "no Authorization header");

    let (status, _) = run_sql(
        &rig.server,
        "not-the-token-but-long-enough-0123456789",
        json!({ "sql": "SELECT 1" }),
    )
    .await;
    assert_eq!(status, 403, "a token matching neither admin credential");
}
