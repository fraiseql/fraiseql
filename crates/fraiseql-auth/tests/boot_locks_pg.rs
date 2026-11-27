//! A second boot over a current auth schema takes no lock a live write holds.
//!
//! Every replica applies its auth scripts at boot while the others serve sign-ins. Each
//! test here applies one store's script to a database of its own, then applies it again
//! while a live write holds every table in `auth` and `_system`
//! ([`fraiseql_test_support::boot_locks`]): a statement in that script that needs a
//! `SHARE` or stronger table lock on a current schema fails it, named.
//!
//! Every boot script the crate declares is covered: a `pub const …SQL: &str` added under
//! `src/` with no test here fails [`every_declared_auth_script_is_covered`], as does a
//! script [`fraiseql_auth::auth_schema`] lists that no test here applies.
//!
//! The guards skip only what is already there, and a policy only when its whole definition
//! matches: each store's `…_boot_builds_the_pinned_shape` (which also boots over each of the
//! store's policies changed in one attribute),
//! [`a_boot_drops_the_provider_key_an_earlier_release_created`] and
//! [`a_changed_policy_definition_reaches_a_database_holding_the_old_one`] prove it.
//!
//! Each auth script boots as first-party auth's role does in production: a role created per
//! test with exactly the grants `docs/auth/database.md` prescribes (`LOGIN NOSUPERUSER
//! NOBYPASSRLS`, `CONNECT` and `CREATE` on the database, nothing on schema `public`), not
//! as the harness's `DATABASE_URL` role, which is a superuser on every rig. A statement that
//! only a superuser or another schema's owner may run fails its tests here. The session
//! state script boots on the engine's pool in production, so it boots as the harness role.
//!
//! Self-skips when no `DATABASE_URL` is set. It runs in the Dagger `integration: saml`
//! leg, which builds `fraiseql-auth` with `auth-saml` and binds Postgres.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` + `auth-saml` ·
//! **Parallelism:** each test creates its own database → safe in parallel.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{
    path::PathBuf,
    str::FromStr,
    sync::atomic::{AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use fraiseql_auth::{auth_schema, session_state::SESSION_STATE_SCHEMA_SQL};
use fraiseql_test_support::boot_locks::{
    FreshDatabase, boot_restores_the_pinned_shape, fresh_database, second_init_waits_on_nothing,
};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// The schemas the auth scripts create tables in.
const SCHEMAS: &[&str] = &["auth", "_system"];

/// Apply `script` the way a store's `init` does: whole, behind the workspace's DDL lock.
async fn apply(pool: PgPool, script: &str) -> Result<(), sqlx::Error> {
    // Reason: the crate's own DDL constant, behind the workspace's fixed lock statement.
    sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(script)))
        .execute(&pool)
        .await
        .map(drop)
}

/// The role a script boots as in production.
#[derive(Clone, Copy)]
enum BootsAs {
    /// First-party auth's own role (`[auth.database]`).
    Auth,
    /// The engine's role: the harness's `DATABASE_URL` role.
    Engine,
}

/// The password every per-test auth role logs in with.
const AUTH_ROLE_PASSWORD: &str = "fq-boot-auth-role";

/// First-party auth's role on one test database, with exactly the grants
/// `docs/auth/database.md` prescribes. The role is per test, as roles are cluster-wide and
/// the tests run in parallel; it is dropped with what it owns before its database is.
struct AuthRole {
    name:    String,
    /// The role's own connection to the test database.
    options: PgConnectOptions,
    /// The harness role's URL to the test database, to drop the role with.
    admin:   String,
}

impl AuthRole {
    async fn create(db: &FreshDatabase) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
        let name = format!(
            "fq_boot_auth_{}_{}_{nanos}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let pool = db.pool().await;
        // Reason: a role name built above from numbers, and a constant password.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE ROLE {name} WITH LOGIN NOSUPERUSER NOBYPASSRLS \
             PASSWORD '{AUTH_ROLE_PASSWORD}'"
        )))
        .execute(&pool)
        .await
        .unwrap();
        let grant: String =
            sqlx::query_scalar("SELECT format('GRANT CONNECT, CREATE ON DATABASE %I TO %I', current_database(), $1::text)")
                .bind(&name)
                .fetch_one(&pool)
                .await
                .unwrap();
        // Reason: built by `format(%I)` from the database's and the role's names.
        sqlx::raw_sql(sqlx::AssertSqlSafe(grant)).execute(&pool).await.unwrap();
        pool.close().await;
        let admin = db.url();
        let options = PgConnectOptions::from_str(&admin)
            .unwrap()
            .username(&name)
            .password(AUTH_ROLE_PASSWORD);
        Self {
            name,
            options,
            admin,
        }
    }

    /// Drop the role and what it owns, reporting a failure.
    async fn drop(mut self) {
        drop_role(&self.admin, &self.name).await.unwrap();
        self.name.clear();
    }
}

impl Drop for AuthRole {
    /// Drop the role of a test that did not: as [`FreshDatabase`] does, on a thread and a
    /// runtime of its own, a failure unreported. Declared after its database, so it runs
    /// while the database still exists.
    fn drop(&mut self) {
        if self.name.is_empty() {
            return;
        }
        let (admin, name) = (self.admin.clone(), std::mem::take(&mut self.name));
        let cleanup = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build()
            else {
                return;
            };
            drop(runtime.block_on(drop_role(&admin, &name)));
        });
        drop(cleanup.join());
    }
}

/// `DROP OWNED BY` then `DROP ROLE`, on the test database the harness role's `admin` URL
/// names: `DROP OWNED` also removes the role's grant on the database.
async fn drop_role(admin: &str, name: &str) -> Result<(), sqlx::Error> {
    let pool = PgPoolOptions::new().max_connections(1).connect(admin).await?;
    // Reason: a role name [`AuthRole::create`] built from numbers.
    let dropped =
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP OWNED BY {name}; DROP ROLE {name}")))
            .execute(&pool)
            .await;
    pool.close().await;
    dropped.map(drop)
}

/// Apply `script` as the role `options` connects as, on a pool of its own.
async fn apply_as(options: PgConnectOptions, script: &str) -> Result<(), sqlx::Error> {
    let pool = PgPoolOptions::new().max_connections(1).connect_with(options).await?;
    let applied = apply(pool.clone(), script).await;
    pool.close().await;
    applied
}

/// Apply every script listed before `script` once, so its foreign keys resolve, then boot
/// `script` twice and fail if the second boot waits on a live write.
async fn second_boot_waits_on_nothing(tag: &str, script: &'static str, boots_as: BootsAs) {
    let Some(db) = fresh_database(tag).await else {
        eprintln!("SKIP {tag}: no postgres");
        return;
    };
    let role = AuthRole::create(&db).await;
    let before = auth_schema::SCRIPTS.iter().take_while(|s| **s != script);
    for prerequisite in before {
        apply_as(role.options.clone(), prerequisite).await.unwrap();
    }
    match boots_as {
        BootsAs::Auth => {
            let options = role.options.clone();
            second_init_waits_on_nothing(&db, SCHEMAS, move |_| apply_as(options.clone(), script))
                .await;
        },
        BootsAs::Engine => {
            second_init_waits_on_nothing(&db, SCHEMAS, move |pool| apply(pool, script)).await;
        },
    }
    role.drop().await;
    db.drop().await;
}

/// Apply every script listed before `script` once, so its foreign keys resolve, then assert
/// a boot builds the shape pinned in `tests/fixtures/boot_shape/<tag>.txt` on the tables
/// `script` creates, and builds it again over a schema that lost what its guards skip on.
async fn boot_builds_the_pinned_shape(tag: &str, script: &'static str, boots_as: BootsAs) {
    let Some(db) = fresh_database(&format!("{tag}_shape")).await else {
        eprintln!("SKIP {tag}: no postgres");
        return;
    };
    let role = AuthRole::create(&db).await;
    for prerequisite in auth_schema::SCRIPTS.iter().take_while(|s| **s != script) {
        apply_as(role.options.clone(), prerequisite).await.unwrap();
    }
    let fixture: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "boot_shape",
        &format!("{tag}.txt"),
    ]
    .iter()
    .collect();
    match boots_as {
        BootsAs::Auth => {
            let options = role.options.clone();
            boot_restores_the_pinned_shape(&db, SCHEMAS, &[script], &fixture, move |_| {
                apply_as(options.clone(), script)
            })
            .await;
        },
        BootsAs::Engine => {
            boot_restores_the_pinned_shape(&db, SCHEMAS, &[script], &fixture, move |pool| {
                apply(pool, script)
            })
            .await;
        },
    }
    role.drop().await;
    db.drop().await;
}

#[tokio::test]
async fn account_linking_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(
        "account",
        fraiseql_auth::account_linking::SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn account_linking_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(
        "account",
        fraiseql_auth::account_linking::SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn sessions_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(
        "sessions",
        fraiseql_auth::session_postgres::SESSIONS_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn sessions_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(
        "sessions",
        fraiseql_auth::session_postgres::SESSIONS_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn password_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("password", fraiseql_auth::PASSWORD_SCHEMA_SQL, BootsAs::Auth)
        .await;
}

#[tokio::test]
async fn password_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("password", fraiseql_auth::PASSWORD_SCHEMA_SQL, BootsAs::Auth)
        .await;
}

#[tokio::test]
async fn password_reset_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("reset", fraiseql_auth::PASSWORD_RESET_SCHEMA_SQL, BootsAs::Auth)
        .await;
}

#[tokio::test]
async fn password_reset_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("reset", fraiseql_auth::PASSWORD_RESET_SCHEMA_SQL, BootsAs::Auth)
        .await;
}

#[tokio::test]
async fn email_verification_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(
        "verify",
        fraiseql_auth::EMAIL_VERIFICATION_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn email_verification_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(
        "verify",
        fraiseql_auth::EMAIL_VERIFICATION_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn otp_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("otp", fraiseql_auth::PG_OTP_SCHEMA_SQL, BootsAs::Auth).await;
}

#[tokio::test]
async fn otp_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("otp", fraiseql_auth::PG_OTP_SCHEMA_SQL, BootsAs::Auth).await;
}

#[tokio::test]
async fn totp_mfa_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("mfa", fraiseql_auth::PG_MFA_SCHEMA_SQL, BootsAs::Auth).await;
}

#[tokio::test]
async fn totp_mfa_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("mfa", fraiseql_auth::PG_MFA_SCHEMA_SQL, BootsAs::Auth).await;
}

#[tokio::test]
async fn scim_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("scim", fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL, BootsAs::Auth)
        .await;
}

#[tokio::test]
async fn scim_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("scim", fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL, BootsAs::Auth)
        .await;
}

#[tokio::test]
async fn signing_key_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(
        "signing",
        fraiseql_auth::signing_key::SIGNING_KEY_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn signing_key_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(
        "signing",
        fraiseql_auth::signing_key::SIGNING_KEY_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn saml_replay_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(
        "saml_replay",
        fraiseql_auth::saml::PG_SAML_REPLAY_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn saml_replay_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(
        "saml_replay",
        fraiseql_auth::saml::PG_SAML_REPLAY_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn saml_idp_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(
        "saml_idp",
        fraiseql_auth::saml::PG_SAML_IDP_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn saml_idp_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(
        "saml_idp",
        fraiseql_auth::saml::PG_SAML_IDP_SCHEMA_SQL,
        BootsAs::Auth,
    )
    .await;
}

#[tokio::test]
async fn session_state_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("session_state", SESSION_STATE_SCHEMA_SQL, BootsAs::Engine).await;
}

#[tokio::test]
async fn session_state_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("session_state", SESSION_STATE_SCHEMA_SQL, BootsAs::Engine).await;
}

/// Each boot script the crate declares, by `<file under src/>::<const>`, with the script
/// the tests above apply for it.
const COVERED: &[(&str, &str)] = &[
    (
        "account_linking/postgres.rs::SCHEMA_SQL",
        fraiseql_auth::account_linking::SCHEMA_SQL,
    ),
    (
        "session_postgres.rs::SESSIONS_SCHEMA_SQL",
        fraiseql_auth::session_postgres::SESSIONS_SCHEMA_SQL,
    ),
    ("local_password.rs::PASSWORD_SCHEMA_SQL", fraiseql_auth::PASSWORD_SCHEMA_SQL),
    (
        "local_password/reset.rs::PASSWORD_RESET_SCHEMA_SQL",
        fraiseql_auth::PASSWORD_RESET_SCHEMA_SQL,
    ),
    (
        "local_password/verification.rs::EMAIL_VERIFICATION_SCHEMA_SQL",
        fraiseql_auth::EMAIL_VERIFICATION_SCHEMA_SQL,
    ),
    ("otp/postgres.rs::PG_OTP_SCHEMA_SQL", fraiseql_auth::PG_OTP_SCHEMA_SQL),
    ("totp_mfa/postgres.rs::PG_MFA_SCHEMA_SQL", fraiseql_auth::PG_MFA_SCHEMA_SQL),
    ("scim/store.rs::PG_SCIM_SCHEMA_SQL", fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL),
    (
        "signing_key.rs::SIGNING_KEY_SCHEMA_SQL",
        fraiseql_auth::signing_key::SIGNING_KEY_SCHEMA_SQL,
    ),
    (
        "saml/replay.rs::PG_SAML_REPLAY_SCHEMA_SQL",
        fraiseql_auth::saml::PG_SAML_REPLAY_SCHEMA_SQL,
    ),
    (
        "saml/store.rs::PG_SAML_IDP_SCHEMA_SQL",
        fraiseql_auth::saml::PG_SAML_IDP_SCHEMA_SQL,
    ),
    ("session_state/postgres.rs::SESSION_STATE_SCHEMA_SQL", SESSION_STATE_SCHEMA_SQL),
];

/// Declared `…SQL` constants that are not a store's boot script, each with why.
const NOT_A_STORE: &[(&str, &str)] = &[(
    "auth_schema.rs::CLAIM_SQL",
    "the claim prefix every auth script starts with, not a store",
)];

/// Every `pub const <NAME>SQL: &str` under `src/`, outside test files, as
/// `<file under src/>::<NAME>`.
fn declared_scripts() -> Vec<String> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                walk(&path, root, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") && !name.ends_with("tests.rs")
            {
                let file = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                for line in std::fs::read_to_string(&path).unwrap().lines() {
                    let line = line.trim_start();
                    let Some(rest) = ["pub const ", "pub(crate) const "]
                        .iter()
                        .find_map(|head| line.strip_prefix(head))
                    else {
                        continue;
                    };
                    if let Some((name, ty)) = rest.split_once(':') {
                        if name.ends_with("SQL") && ty.trim_start().starts_with("&str") {
                            found.push(format!("{file}::{name}"));
                        }
                    }
                }
            }
        }
    }
    let root: PathBuf = [env!("CARGO_MANIFEST_DIR"), "src"].iter().collect();
    let mut found = Vec::new();
    walk(&root, &root, &mut found);
    found.sort();
    found
}

/// Every boot script under `src/` is covered by a test above or named as not a store, every
/// entry of either list is still declared, and every script [`auth_schema::SCRIPTS`] lists
/// is one a test above applies.
#[test]
fn every_declared_auth_script_is_covered() {
    let declared = declared_scripts();
    assert!(
        declared.iter().any(|d| d == COVERED[0].0),
        "the scan found none of the scripts listed: {declared:?}"
    );
    let listed = || COVERED.iter().map(|(d, _)| *d).chain(NOT_A_STORE.iter().map(|(d, _)| *d));
    let uncovered: Vec<&String> =
        declared.iter().filter(|d| !listed().any(|l| l == d.as_str())).collect();
    assert!(uncovered.is_empty(), "an auth boot script has no boot-lock test: {uncovered:?}");
    for entry in listed() {
        assert!(declared.iter().any(|d| d == entry), "{entry} is listed, and not declared");
    }
    for script in auth_schema::SCRIPTS {
        assert!(
            COVERED.iter().any(|(_, covered)| covered == script),
            "an auth script has no boot-lock test"
        );
    }
}

/// The edits a release makes to a guarded policy: each read and insert policy's `CREATE`
/// and the stored form its guard compares.
const POLICY_EDITS: &[(&str, &str)] = &[
    (
        "$q$(tenant_id = (NULLIF(current_setting('fraiseql.tenant_id'::text, true), ''::text))::uuid)$q$",
        "$q$(tenant_id IS NOT NULL)$q$",
    ),
    (
        "FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid)",
        "FOR SELECT USING (tenant_id IS NOT NULL)",
    ),
    ("with_check = 'true'", "with_check = '(tenant_id IS NULL)'"),
    ("FOR INSERT WITH CHECK (true)", "FOR INSERT WITH CHECK (tenant_id IS NULL)"),
];

/// Boot `old`, then `old` with [`POLICY_EDITS`] made to each of its `tables` tables, on a
/// database of its own: the policies `auth` then holds, by name, or `None` with no Postgres.
async fn policies_after_a_changed_definition(
    tag: &str,
    old: &str,
    tables: usize,
) -> Option<Vec<(String, Option<String>, Option<String>)>> {
    let Some(db) = fresh_database(tag).await else {
        eprintln!("SKIP {tag}: no postgres");
        return None;
    };
    let mut new = old.to_string();
    for (from, to) in POLICY_EDITS {
        assert_eq!(new.matches(from).count(), tables, "the policy text moved: {from}");
        new = new.replace(from, to);
    }

    let pool = db.pool().await;
    apply(pool.clone(), old).await.unwrap();
    apply(pool.clone(), &new).await.unwrap();
    let policies: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT policyname::text, qual, with_check FROM pg_policies \
         WHERE schemaname = 'auth' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    pool.close().await;
    db.drop().await;
    Some(policies)
}

/// The definitions [`POLICY_EDITS`] writes.
fn edited(read: &str, insert: &str) -> [(String, Option<String>, Option<String>); 2] {
    [
        (insert.to_string(), None, Some("(tenant_id IS NULL)".to_string())),
        (read.to_string(), Some("(tenant_id IS NOT NULL)".to_string()), None),
    ]
}

/// A release that changes a policy's definition reaches a database that holds the old one,
/// for the SAML identity provider store as for the account store.
#[tokio::test]
async fn a_changed_saml_idp_policy_definition_reaches_a_database_holding_the_old_one() {
    let Some(policies) = policies_after_a_changed_definition(
        "idp_policy_change",
        fraiseql_auth::saml::PG_SAML_IDP_SCHEMA_SQL,
        1,
    )
    .await
    else {
        return;
    };
    assert_eq!(policies, edited("p_saml_idp_tenant_read", "p_saml_idp_insert"));
}

/// A release that changes a policy's definition reaches a database that holds the old one:
/// the guard compares the whole definition, not the policy's name. A release changes both
/// texts a guarded policy is written in, the `CREATE POLICY` and the stored form the guard
/// compares, as this test does.
#[tokio::test]
async fn a_changed_policy_definition_reaches_a_database_holding_the_old_one() {
    // One per table: the account script guards the policies of two.
    let Some(policies) = policies_after_a_changed_definition(
        "policy_change",
        fraiseql_auth::account_linking::SCHEMA_SQL,
        2,
    )
    .await
    else {
        return;
    };
    let read = Some("(tenant_id IS NOT NULL)".to_string());
    let insert = Some("(tenant_id IS NULL)".to_string());
    assert_eq!(
        policies,
        [
            ("p_auth_identity_insert".to_string(), None, insert.clone()),
            ("p_auth_identity_tenant_read".to_string(), read.clone(), None),
            ("p_user_insert".to_string(), None, insert),
            ("p_user_tenant_read".to_string(), read, None),
        ]
    );
}

/// A database an earlier release created holds a global `UNIQUE (provider, provider_id)` on
/// the identity table; a boot drops it, so the per-space key alone decides. The guard skips
/// the drop only where the constraint is gone.
#[tokio::test]
async fn a_boot_drops_the_provider_key_an_earlier_release_created() {
    let Some(db) = fresh_database("legacy_identity").await else {
        eprintln!("SKIP legacy_identity: no postgres");
        return;
    };
    let script = fraiseql_auth::account_linking::SCHEMA_SQL;
    let pool = db.pool().await;
    apply(pool.clone(), script).await.unwrap();
    sqlx::raw_sql(
        "ALTER TABLE auth.tb_auth_identity ADD CONSTRAINT \
         tb_auth_identity_provider_provider_id_key UNIQUE (provider, provider_id)",
    )
    .execute(&pool)
    .await
    .unwrap();
    apply(pool.clone(), script).await.unwrap();
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint WHERE conrelid = 'auth.tb_auth_identity'::regclass \
         AND conname = 'tb_auth_identity_provider_provider_id_key'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    db.drop().await;
    assert_eq!(left, 0, "the boot kept the earlier release's global provider key");
}
