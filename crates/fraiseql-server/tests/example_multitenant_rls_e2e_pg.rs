//! #628: the shipped multi-tenant examples must demonstrate isolation they
//! actually have.
//!
//! `examples/multitenant` and `examples/saas` described a tenant-isolation story
//! and shipped none of it. Under #612 the false half — `[[security.rules]]` blocks
//! the runtime does not enforce — was removed, leaving both examples honest and
//! empty. This is the other half: the mechanism, wired, with a test that fails if
//! the story stops being true.
//!
//! What this drives, end to end:
//!
//! 1. the examples' `sql/01_schema.sql` applied to a real PostgreSQL — tables, RLS policies,
//!    `security_invoker` views, and an unprivileged application role;
//! 2. the examples' `fraiseql.toml` + domain JSON compiled through the **real** compile path
//!    (`SchemaMerger::merge_from_domains` → `SchemaConverter`), so a key rename that dropped
//!    `session_variables` would fail here;
//! 3. `Executor::execute_with_security` for two tenants, which resolves the compiled
//!    `session_variables` from each `SecurityContext` and applies them with `set_config` before the
//!    query;
//! 4. the assertion that each tenant sees only its own rows, and an anonymous caller sees none.
//!
//! **The connection role is load-bearing.** The harness's `DATABASE_URL` role is a
//! superuser with `rolbypassrls`, for which PostgreSQL skips every policy. A test
//! run as that role returns every tenant's rows however correct the policies are,
//! so it could only ever pass by asserting something weaker than isolation. Each
//! example's SQL creates an ordinary `LOGIN` role, and the assertions connect as
//! it — which is also the deployment advice in both READMEs.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! Each case provisions its **own database** rather than sharing the harness one.
//! The examples use idiomatic relation names (`v_organization`, `v_account`,
//! `v_invoice`), several of which already exist in the shared test database as
//! other suites' fixtures — and one of them as a *table*, which made applying the
//! example's SQL fail outright. An example must not have to rename its views to be
//! testable, and it must not clobber another suite's fixtures to run; a scratch
//! database gives both, and mirrors what each README tells an operator to do.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own databases and roles →
//! run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::{TenantId, UserId},
    runtime::Executor,
    schema::CompiledSchema,
    security::SecurityContext,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

/// Repository root, from this package's manifest directory.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<pkg> is two levels below the repository root")
        .to_path_buf()
}

/// Compile a shipped example through the production compile path.
///
/// Not by shelling out to the binary: `CARGO_BIN_EXE_fraiseql-cli` is only defined
/// for that package's own tests. `merge_from_domains` is the same entry point
/// `fraiseql compile` calls for a `[domain_discovery]` project, so the seam under
/// test is identical.
fn compile_example(name: &str) -> CompiledSchema {
    let dir = repo_root().join("examples").join(name);
    let toml = dir.join("fraiseql.toml");
    assert!(toml.exists(), "shipped example is missing: {}", toml.display());

    // `[domain_discovery] root_dir` is relative to the working directory.
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&dir).expect("enter the example directory");
    let intermediate = fraiseql_cli::schema::SchemaMerger::merge_from_domains("fraiseql.toml")
        .unwrap_or_else(|e| panic!("compile {name}: {e:#}"));
    std::env::set_current_dir(previous).expect("restore cwd");

    fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .unwrap_or_else(|e| panic!("convert {name}: {e:#}"))
}

/// Provision a scratch database for one example and apply its `sql/01_schema.sql`
/// there as the superuser. Returns a connection to it.
///
/// `WITH (FORCE)` on the drop is required: a previous run's adapter pool may still
/// hold connections, and `DROP DATABASE` refuses while any exist.
async fn provision_example_database(url: &str, name: &str, db: &str) -> tokio_postgres::Client {
    let admin = admin_client(url).await;
    admin
        .execute(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"), &[])
        .await
        .unwrap_or_else(|e| panic!("drop {db}: {e}"));
    admin
        .execute(&format!("CREATE DATABASE {db}"), &[])
        .await
        .unwrap_or_else(|e| panic!("create {db}: {e}"));

    let scratch = admin_client(&with_database(url, db)).await;
    let path = repo_root().join("examples").join(name).join("sql/01_schema.sql");
    let sql =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    scratch
        .batch_execute(&sql)
        .await
        .unwrap_or_else(|e| panic!("apply {}: {e}", path.display()));
    scratch
}

/// Drop a scratch database, evicting whatever still holds a connection to it.
async fn drop_example_database(url: &str, db: &str) {
    let admin = admin_client(url).await;
    let _ = admin.execute(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"), &[]).await;
}

/// Rebuild `url` pointing at a different database, keeping credentials and host.
fn with_database(url: &str, db: &str) -> String {
    let cfg: tokio_postgres::Config = url.parse().expect("parse DATABASE_URL");
    let user = cfg.get_user().unwrap_or("postgres");
    let password = cfg
        .get_password()
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .unwrap_or_default();
    format!("postgres://{user}:{password}@{}:{}/{db}", host_of(&cfg), port_of(&cfg))
}

async fn admin_client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .expect("admin connection");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("admin connection error: {e}");
        }
    });
    client
}

fn host_of(cfg: &tokio_postgres::Config) -> String {
    match cfg.get_hosts().first().expect("a host") {
        tokio_postgres::config::Host::Tcp(h) => h.clone(),
        #[cfg(unix)]
        tokio_postgres::config::Host::Unix(p) => p.display().to_string(),
    }
}

fn port_of(cfg: &tokio_postgres::Config) -> u16 {
    cfg.get_ports().first().copied().unwrap_or(5432)
}

/// Connection string for an example's unprivileged application role against its
/// scratch database.
fn as_role(url: &str, db: &str, role: &str, password: &str) -> String {
    let cfg: tokio_postgres::Config = url.parse().expect("parse DATABASE_URL");
    format!("postgres://{role}:{password}@{}:{}/{db}", host_of(&cfg), port_of(&cfg))
}

/// A principal carrying the claims the examples' `session_variables` map.
fn principal(claims: &[(&str, &str)]) -> SecurityContext {
    let attributes: HashMap<String, Value> =
        claims.iter().map(|(k, v)| ((*k).to_string(), json!(v))).collect();
    SecurityContext {
        user_id: UserId::from("user-p04"),
        roles: vec!["authenticated".to_string()],
        tenant_id: claims
            .iter()
            .find(|(k, _)| *k == "tenant_id" || *k == "account_id")
            .map(|(_, v)| TenantId::from(*v)),
        scopes: vec![],
        attributes,
        request_id: "req-p04-example".to_string(),
        ip_address: None,
        authenticated_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
        issuer: None,
        audience: None,
        email: None,
        display_name: None,
    }
}

/// Run a GraphQL query as `ctx` and return the named root field's rows.
async fn rows_for(
    executor: &Executor<PostgresAdapter>,
    query: &str,
    field: &str,
    ctx: &SecurityContext,
) -> Vec<Value> {
    let response = executor
        .execute_with_security(query, None, ctx)
        .await
        .unwrap_or_else(|e| panic!("execute `{query}`: {e}"));
    assert!(response.get("errors").is_none(), "`{query}` returned errors: {response}");
    response
        .get("data")
        .and_then(|d| d.get(field))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| panic!("no `data.{field}` in {response}"))
}

const ORG_A: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const ORG_B: &str = "bbbbbbbb-0000-0000-0000-000000000001";
const TENANT_A: &str = "aaaaaaaa-0000-0000-0000-000000000002";
const TENANT_B: &str = "bbbbbbbb-0000-0000-0000-000000000002";
/// A user who belongs to account A but owns nothing in it — the principal #1070's
/// intra-account write rules exist to constrain.
const MEMBER_A: &str = "aaaaaaaa-0000-0000-0000-000000000003";

/// Scratch databases, one per example — see the module docs on why they are not
/// the harness database.
const DB_MULTITENANT: &str = "p04_example_multitenant";
const DB_SAAS: &str = "p04_example_saas";
/// The write-rule case gets its own database: it mutates the rows the read case
/// asserts on, and `--test-threads=1` is a rig convention, not a guarantee.
const DB_SAAS_WRITES: &str = "p04_example_saas_writes";

/// `examples/multitenant`: two organizations, two tenants, zero crossings.
#[tokio::test]
async fn multitenant_example_isolates_two_tenants() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let admin = provision_example_database(&url, "multitenant", DB_MULTITENANT).await;

    admin
        .batch_execute(&format!(
            "INSERT INTO tb_organization (id, data) VALUES
               ('{ORG_A}', '{{\"name\":\"Org A\"}}'), ('{ORG_B}', '{{\"name\":\"Org B\"}}');
             INSERT INTO tb_tenant (id, organization_id, data) VALUES
               ('{TENANT_A}', '{ORG_A}', '{{\"name\":\"Tenant A\"}}'),
               ('{TENANT_B}', '{ORG_B}', '{{\"name\":\"Tenant B\"}}');
             INSERT INTO tb_resource (id, tenant_id, data) VALUES
               ('aaaaaaaa-0000-0000-0000-000000000003', '{TENANT_A}', '{{\"name\":\"A-1\"}}'),
               ('aaaaaaaa-0000-0000-0000-000000000004', '{TENANT_A}', '{{\"name\":\"A-2\"}}'),
               ('bbbbbbbb-0000-0000-0000-000000000003', '{TENANT_B}', '{{\"name\":\"B-1\"}}');"
        ))
        .await
        .expect("seed");

    let schema = compile_example("multitenant");
    assert_eq!(
        schema.session_variables.variables.len(),
        2,
        "#628: the example's JWT → session-variable mappings must survive compilation; \
         without them every policy below matches nothing and the test would pass vacuously"
    );
    assert!(schema.is_multi_tenant() && schema.has_rls_configured());

    let adapter =
        PostgresAdapter::new(&as_role(&url, DB_MULTITENANT, "multitenant_app", "multitenant_app"))
            .await
            .expect("connect as the example's application role");
    let executor = Executor::new(schema, Arc::new(adapter));

    // Deliberately asymmetric counts (2 vs 1): "saw everyone" and "saw the wrong
    // tenant" both fail, which an equal split would not distinguish.
    let a = rows_for(
        &executor,
        "{ listResources { id name tenantId } }",
        "listResources",
        &principal(&[("organization_id", ORG_A), ("tenant_id", TENANT_A)]),
    )
    .await;
    assert_eq!(a.len(), 2, "tenant A must see exactly its own 2 resources: {a:?}");
    assert!(
        a.iter().all(|r| r["tenantId"] == json!(TENANT_A)),
        "every row tenant A receives must carry its own tenant id: {a:?}"
    );

    let b = rows_for(
        &executor,
        "{ listResources { id name tenantId } }",
        "listResources",
        &principal(&[("organization_id", ORG_B), ("tenant_id", TENANT_B)]),
    )
    .await;
    assert_eq!(b.len(), 1, "tenant B must see exactly its own 1 resource: {b:?}");
    assert_eq!(b[0]["tenantId"], json!(TENANT_B), "{b:?}");
    assert_eq!(b[0]["name"], json!("B-1"), "{b:?}");

    // The organization scope uses a different claim and a different policy, so it
    // is a genuinely separate assertion rather than a restatement of the above.
    let orgs = rows_for(
        &executor,
        "{ listOrganizations { id name } }",
        "listOrganizations",
        &principal(&[("organization_id", ORG_A), ("tenant_id", TENANT_A)]),
    )
    .await;
    assert_eq!(orgs.len(), 1, "a caller must see only its own organization: {orgs:?}");
    assert_eq!(orgs[0]["id"], json!(ORG_A), "{orgs:?}");

    // No claims at all: the policies match nothing. This is the assertion that the
    // isolation fails *closed* rather than defaulting to everything.
    let anonymous =
        rows_for(&executor, "{ listResources { id } }", "listResources", &principal(&[])).await;
    assert!(
        anonymous.is_empty(),
        "a principal with no tenant claim must see nothing: {anonymous:?}"
    );

    drop(executor);
    drop(admin);
    drop_example_database(&url, DB_MULTITENANT).await;
}

/// `examples/saas`: the same property on the account-scoped schema.
#[tokio::test]
async fn saas_example_isolates_two_accounts() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let admin = provision_example_database(&url, "saas", DB_SAAS).await;

    admin
        .batch_execute(&format!(
            "INSERT INTO tb_account (id, owner_id, data) VALUES
               ('{ORG_A}', '{TENANT_A}', '{{\"name\":\"Acme\"}}'),
               ('{ORG_B}', '{TENANT_B}', '{{\"name\":\"Globex\"}}');
             INSERT INTO tb_invoice (id, account_id, data) VALUES
               ('aaaaaaaa-0000-0000-0000-000000000005', '{ORG_A}', '{{\"amount\":10}}'),
               ('aaaaaaaa-0000-0000-0000-000000000006', '{ORG_A}', '{{\"amount\":20}}'),
               ('aaaaaaaa-0000-0000-0000-000000000007', '{ORG_A}', '{{\"amount\":30}}'),
               ('bbbbbbbb-0000-0000-0000-000000000005', '{ORG_B}', '{{\"amount\":99}}');"
        ))
        .await
        .expect("seed");

    let schema = compile_example("saas");
    assert_eq!(schema.session_variables.variables.len(), 3);
    assert!(schema.is_multi_tenant() && schema.has_rls_configured());

    let adapter = PostgresAdapter::new(&as_role(&url, DB_SAAS, "saas_app", "saas_app"))
        .await
        .expect("connect as the example's application role");
    let executor = Executor::new(schema, Arc::new(adapter));

    let a = rows_for(
        &executor,
        "{ listInvoices { id accountId } }",
        "listInvoices",
        &principal(&[("account_id", ORG_A)]),
    )
    .await;
    assert_eq!(a.len(), 3, "account A must see exactly its own 3 invoices: {a:?}");
    assert!(
        a.iter().all(|r| r["accountId"] == json!(ORG_A)),
        "every row account A receives must carry its own account id: {a:?}"
    );

    let b = rows_for(
        &executor,
        "{ listInvoices { id accountId } }",
        "listInvoices",
        &principal(&[("account_id", ORG_B)]),
    )
    .await;
    assert_eq!(b.len(), 1, "account B must see exactly its own 1 invoice: {b:?}");
    assert_eq!(b[0]["accountId"], json!(ORG_B), "{b:?}");

    let anonymous =
        rows_for(&executor, "{ listInvoices { id } }", "listInvoices", &principal(&[])).await;
    assert!(
        anonymous.is_empty(),
        "a principal with no account claim must see nothing: {anonymous:?}"
    );

    drop(executor);
    drop(admin);
    drop_example_database(&url, DB_SAAS).await;
}

/// Apply the three `app.*` claims for one statement's transaction, run it, and
/// report **whether the write happened** — not how it was refused.
///
/// RLS refuses in two shapes and the choice between them is an implementation
/// detail, not a security property: a row excluded by `USING` is simply not
/// visible to the UPDATE and the statement affects zero rows, while a row rejected
/// by `WITH CHECK` raises 42501. Asserting on one shape would fail an equally
/// correct policy set that refuses in the other, so the assertions below are about
/// the row, not the error.
async fn wrote(
    client: &tokio_postgres::Client,
    account: &str,
    user: &str,
    role: &str,
    sql: &str,
) -> bool {
    client.batch_execute("BEGIN").await.expect("begin");
    client
        .batch_execute(&format!(
            "SELECT set_config('app.account_id', '{account}', true),
                    set_config('app.user_id', '{user}', true),
                    set_config('app.account_role', '{role}', true)"
        ))
        .await
        .expect("apply claims");
    let result = client.execute(sql, &[]).await;
    client.batch_execute("COMMIT").await.expect("commit");
    match result {
        Ok(rows) => rows > 0,
        // 42501 — `new row violates row-level security policy`. A refusal.
        Err(e) if e.code().map(tokio_postgres::error::SqlState::code) == Some("42501") => false,
        Err(e) => panic!("unexpected database error (not an RLS refusal): {e}"),
    }
}

/// The account's current owner, read with the policies bypassed.
async fn owner_of(admin: &tokio_postgres::Client, account: &str) -> String {
    admin
        .query_one(&format!("SELECT owner_id::text FROM tb_account WHERE id = '{account}'"), &[])
        .await
        .expect("read owner")
        .get::<_, String>(0)
}

/// The subscription's current plan, read with the policies bypassed.
async fn plan_of(admin: &tokio_postgres::Client, account: &str) -> String {
    admin
        .query_one(
            &format!("SELECT data->>'plan' FROM tb_subscription WHERE account_id = '{account}'"),
            &[],
        )
        .await
        .expect("read plan")
        .get::<_, String>(0)
}

/// `examples/saas`: the **write** rules the README says PostgreSQL enforces on every
/// write, "including writes that do not go through FraiseQL at all" (#1070).
///
/// That claim had no test. `saas_example_isolates_two_accounts` issues only
/// `listInvoices` reads, so it was silent on writes — and the two write policies were
/// permissive with no `FOR` clause on `account_isolation`, which meant PostgreSQL OR'd
/// their checks with `account_id = current_account_id()` and neither rule fired. The
/// example declares no mutations, so this drives the policies the way the README's own
/// threat model does: direct SQL as `saas_app`, the role it tells operators to use.
///
/// Every case is two-sided. A policy set that refused *every* write would satisfy the
/// refusals below while breaking the example, so each refusal is paired with the
/// authorised write that must still succeed.
#[tokio::test]
async fn saas_example_enforces_its_intra_account_write_rules() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let admin = provision_example_database(&url, "saas", DB_SAAS_WRITES).await;

    // ORG_A is owned by TENANT_A. MEMBER_A belongs to the account but owns nothing.
    admin
        .batch_execute(&format!(
            "INSERT INTO tb_account (id, owner_id, data) VALUES
               ('{ORG_A}', '{TENANT_A}', '{{\"name\":\"Acme\"}}');
             INSERT INTO tb_subscription (id, account_id, data) VALUES
               ('aaaaaaaa-0000-0000-0000-000000000010', '{ORG_A}', '{{\"plan\":\"starter\"}}');"
        ))
        .await
        .expect("seed");

    let app = admin_client(&as_role(&url, DB_SAAS_WRITES, "saas_app", "saas_app")).await;

    // ── tb_subscription: only an owner or billing admin may write ──────────────
    let upgrade = format!(
        "UPDATE tb_subscription SET data = data || '{{\"plan\":\"enterprise\"}}' \
         WHERE account_id = '{ORG_A}'"
    );

    assert!(
        !wrote(&app, ORG_A, MEMBER_A, "member", &upgrade).await,
        "a plain member of the account must not be able to change its subscription — this \
         is the exact statement #1070 found succeeding"
    );
    assert_eq!(
        plan_of(&admin, ORG_A).await,
        "starter",
        "the refused upgrade must not have landed"
    );

    assert!(
        wrote(&app, ORG_A, MEMBER_A, "billing_admin", &upgrade).await,
        "a billing admin must still be able to change the subscription"
    );
    assert_eq!(plan_of(&admin, ORG_A).await, "enterprise", "the authorised upgrade must land");

    // ── tb_account: only the owner may write ───────────────────────────────────
    let rename = format!(
        "UPDATE tb_account SET data = data || '{{\"name\":\"Renamed\"}}' WHERE id = '{ORG_A}'"
    );
    assert!(
        !wrote(&app, ORG_A, MEMBER_A, "member", &rename).await,
        "a non-owner must not be able to modify the account row"
    );

    // The sharpest case, and the one that separates this fix from the tempting
    // half-fix: `AS RESTRICTIVE` alone, with the ownership test left in `WITH CHECK`,
    // still lets a member set `owner_id` to themselves — the check passes precisely
    // *because* they named themselves. The ownership test has to be on the `USING`
    // side, which decides which existing rows may be updated at all.
    let promote = format!("UPDATE tb_account SET owner_id = '{MEMBER_A}' WHERE id = '{ORG_A}'");
    assert!(
        !wrote(&app, ORG_A, MEMBER_A, "member", &promote).await,
        "a member must not be able to make themselves the owner"
    );
    assert_eq!(
        owner_of(&admin, ORG_A).await,
        TENANT_A,
        "the account owner must be unchanged after a refused self-promotion"
    );

    assert!(
        wrote(&app, ORG_A, TENANT_A, "owner", &rename).await,
        "the account's owner must still be able to modify it"
    );

    // ── and the isolation property still holds on the write side ───────────────
    // A caller in another account cannot reach these rows even holding 'owner'.
    assert!(
        !wrote(&app, ORG_B, TENANT_B, "owner", &upgrade).await,
        "a caller in another account must reach no rows here"
    );

    drop(app);
    drop(admin);
    drop_example_database(&url, DB_SAAS_WRITES).await;
}
