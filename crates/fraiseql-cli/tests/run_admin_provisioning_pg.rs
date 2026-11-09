//! `fraiseql run --admin-database-url`: in development, the engine's role and first-party
//! auth's role are created and granted before the server starts.
//!
//! - in development, both roles exist after the run, can log in and are not superusers; the server
//!   serves on them, a sign-up's session lands in `auth.sessions`, every auth table belongs to the
//!   auth role, and the engine role holds no grant to read the sessions;
//! - a second run creates nothing and fails nothing;
//! - a role that already exists keeps its attributes: only its password is set, from its URL;
//! - outside development the flag is refused, naming it and `FRAISEQL_ENV`, before any role is
//!   created;
//! - an admin URL naming another database is refused;
//! - a schema without `[auth.database]` provisions the engine's role only;
//! - a URL whose user is the admin URL's user leaves that role untouched;
//! - two URLs naming one user with different passwords are refused;
//! - `run --watch` provisions too;
//! - nothing the command prints carries either role's password or the admin URL.
//!
//! Self-skips when no `DATABASE_URL` is set. The URL must be able to create roles and
//! databases.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` · **Parallelism:**
//! creates and drops its own `fraiseql-admin-prov-*` databases and `fq-*-admin-prov-*` roles →
//! run `--test-threads=1`.
#![cfg(all(feature = "test-postgres", feature = "run-server"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use tempfile::TempDir;
use tokio_postgres::NoTls;

const HS256_SECRET: &str = "admin-provisioning-hs256-secret-32chars";

/// The two roles' passwords: distinctive, so a leak into the output is unambiguous, and each
/// carrying a quote, which a statement that does not quote it cannot survive.
const ENGINE_PASSWORD: &str = "engine-pw-Zq7x'K2mN";
const AUTH_PASSWORD: &str = "auth-pw-Vb4w'R9tL";

const fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_fraiseql-cli")
}

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = fraiseql_test_support::try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// A test database URL's parts: `(scheme, user info, host and port, database, query)`. The
/// suite's URLs are `scheme://user[:password]@host[:port]/database[?query]`.
fn parts(url: &str) -> (&str, &str, &str, &str, &str) {
    let (scheme, rest) = url.split_once("://").expect("DATABASE_URL is a URL");
    let (userinfo, rest) = rest.rsplit_once('@').expect("DATABASE_URL names a user");
    let (host, rest) = rest.split_once('/').expect("DATABASE_URL names a database");
    let (db, query) = rest.split_once('?').unwrap_or((rest, ""));
    (scheme, userinfo, host, db, query)
}

fn assemble(scheme: &str, userinfo: &str, host: &str, db: &str, query: &str) -> String {
    let query = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };
    format!("{scheme}://{userinfo}@{host}/{db}{query}")
}

/// `url` with its database replaced by `db`.
fn with_database(url: &str, db: &str) -> String {
    let (scheme, userinfo, host, _, query) = parts(url);
    assemble(scheme, userinfo, host, db, query)
}

/// `url` with its user and password replaced.
fn with_user(url: &str, user: &str, password: &str) -> String {
    let (scheme, _, host, db, query) = parts(url);
    assemble(scheme, &format!("{user}:{password}"), host, db, query)
}

fn user_of(url: &str) -> String {
    let (_, userinfo, ..) = parts(url);
    userinfo.split_once(':').map_or(userinfo, |(user, _)| user).to_string()
}

async fn connect(url: &str) -> tokio_postgres::Client {
    let (client, conn) = tokio_postgres::connect(url, NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

/// One test's scratch database and role names, dropped on [`Scratch::drop_all`]. Each name
/// needs quoting (a hyphen, and capitals in the roles'), as an unquoted identifier would not.
struct Scratch {
    admin_url:  String,
    db:         String,
    engine:     String,
    auth:       String,
    /// The admin URL on the scratch database: what `--admin-database-url` receives.
    admin_here: String,
}

impl Scratch {
    async fn new(admin_url: &str, tag: &str) -> Self {
        let db = format!("fraiseql-admin-prov-{tag}");
        let scratch = Self {
            admin_url: admin_url.to_string(),
            engine: format!("fq-Engine-admin-prov-{tag}"),
            auth: format!("fq-Auth-admin-prov-{tag}"),
            admin_here: with_database(admin_url, &db),
            db,
        };
        scratch.drop_all().await;
        let admin = connect(admin_url).await;
        admin
            .batch_execute(&format!("CREATE DATABASE \"{}\"", scratch.db))
            .await
            .expect("create scratch database");
        // The application's own objects, which its migrations would create: one view the
        // schema reads, readable by any role.
        let here = connect(&scratch.admin_here).await;
        here.batch_execute(
            "CREATE SCHEMA app;
             CREATE TABLE app.tb_note (id uuid PRIMARY KEY, title text NOT NULL);
             CREATE VIEW app.v_note AS
                 SELECT id, jsonb_build_object('id', id, 'title', title) AS data FROM app.tb_note;
             GRANT USAGE ON SCHEMA app TO PUBLIC;
             GRANT SELECT ON app.v_note TO PUBLIC;",
        )
        .await
        .expect("create the application's view");
        scratch
    }

    fn engine_url(&self) -> String {
        with_user(&self.admin_here, &self.engine, ENGINE_PASSWORD)
    }

    fn auth_url(&self) -> String {
        with_user(&self.admin_here, &self.auth, AUTH_PASSWORD)
    }

    async fn drop_all(&self) {
        let admin = connect(&self.admin_url).await;
        let _ = admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.db))
            .await;
        // `DROP OWNED` first: it also revokes a role's grants on shared objects, such as the
        // admin's own database, which a run that provisioned the wrong database would leave
        // behind, and which `DROP ROLE` refuses to drop past.
        for role in [&self.engine, &self.auth] {
            let _ = admin
                .batch_execute(&format!(
                    "DO $$ BEGIN IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{role}') THEN \
                     EXECUTE 'DROP OWNED BY \"{role}\"'; END IF; END $$; \
                     DROP ROLE IF EXISTS \"{role}\""
                ))
                .await;
        }
    }
}

/// `rolname`'s `(rolcanlogin, rolsuper, rolcreatedb)`, or `None` when it does not exist.
async fn role(admin_url: &str, rolname: &str) -> Option<(bool, bool, bool)> {
    connect(admin_url)
        .await
        .query_opt(
            "SELECT rolcanlogin, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = $1",
            &[&rolname],
        )
        .await
        .expect("read pg_roles")
        .map(|row| (row.get(0), row.get(1), row.get(2)))
}

/// `rolname`'s stored password verifier. A SCRAM verifier carries a fresh salt each time a
/// password is set, so any `ALTER ROLE … PASSWORD` changes it, even to the same password.
async fn verifier(admin_url: &str, rolname: &str) -> Option<String> {
    connect(admin_url)
        .await
        .query_one("SELECT rolpassword FROM pg_authid WHERE rolname = $1", &[&rolname])
        .await
        .expect("read pg_authid")
        .get(0)
}

/// A project directory: `fraiseql.toml` (anonymous sign-up, and `[auth.database]` reading
/// `AUTH_DATABASE_URL` when `auth_database`) and the server file `FRAISEQL_CONFIG` names.
fn project(auth_database: Option<&str>) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut toml = String::from(
        r#"[schema]
name = "admin-provisioning"

[types.Note]
sql_source = "app.v_note"
fields.id = { type = "ID" }
fields.title = { type = "String" }

[queries.notes]
return_type = "Note"
return_array = true
sql_source = "app.v_note"
"#,
    );
    if let Some(url_env) = auth_database {
        write!(
            toml,
            "\n[auth.local]\nanonymous = true\n\n[auth.database]\nurl_env = \"{url_env}\"\n"
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    std::fs::write(
        dir.path().join("server.toml"),
        r#"cors_enabled = false

[auth_hs256]
secret_env = "FRAISEQL_TEST_ADMIN_PROV_HS256"
issuer = "https://sp.example.com"
audience = "fraiseql"
"#,
    )
    .unwrap();
    dir
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A `fraiseql run` child, its output captured to files, killed on drop.
struct Run {
    child: Child,
    out:   PathBuf,
    err:   PathBuf,
    base:  String,
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Run {
    /// Everything the command printed so far, stdout then stderr.
    fn output(&self) -> String {
        format!(
            "{}{}",
            std::fs::read_to_string(&self.out).unwrap_or_default(),
            std::fs::read_to_string(&self.err).unwrap_or_default()
        )
    }

    /// Wait until the server answers `/health` (`Ok`) or the command exits (`Err` with its
    /// output).
    async fn ready(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_mins(2);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Err(format!("exited with {status} before serving:\n{}", self.output()));
            }
            if reqwest::get(format!("{}/health", self.base)).await.is_ok() {
                return Ok(());
            }
            assert!(
                Instant::now() < deadline,
                "neither served nor exited within 120s:\n{}",
                self.output()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Wait for the command to exit; its status code and output.
    fn finished(mut self) -> (Option<i32>, String) {
        let deadline = Instant::now() + Duration::from_mins(2);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return (status.code(), self.output());
            }
            assert!(Instant::now() < deadline, "did not exit within 120s:\n{}", self.output());
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// Start `fraiseql run` on `dir`'s project with `env`, `FRAISEQL_ENV` among them or not.
fn run(dir: &Path, admin_url: Option<&str>, env: &[(&str, &str)]) -> Run {
    run_with(dir, admin_url, env, &[])
}

/// [`run`], with `extra` arguments after the input.
fn run_with(dir: &Path, admin_url: Option<&str>, env: &[(&str, &str)], extra: &[&str]) -> Run {
    let port = free_port();
    let out = dir.join(format!("run-{port}.out"));
    let err = dir.join(format!("run-{port}.err"));
    let mut cmd = Command::new(cli_bin());
    cmd.arg("run")
        .arg(dir.join("fraiseql.toml"))
        .args(["--bind", "127.0.0.1", "--port", &port.to_string()])
        .args(extra)
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("FRAISEQL_CONFIG", dir.join("server.toml"))
        .env("FRAISEQL_TEST_ADMIN_PROV_HS256", HS256_SECRET)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&out).unwrap())
        .stderr(std::fs::File::create(&err).unwrap());
    if let Some(admin_url) = admin_url {
        cmd.args(["--admin-database-url", admin_url]);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    Run {
        child: cmd.spawn().expect("spawn fraiseql-cli run"),
        out,
        err,
        base: format!("http://127.0.0.1:{port}"),
    }
}

/// The output carries neither role's password nor the admin URL.
fn assert_no_secret(output: &str, admin_url: &str) {
    for secret in [ENGINE_PASSWORD, AUTH_PASSWORD, admin_url] {
        assert!(!output.contains(secret), "the output carries a secret:\n{output}");
    }
}

async fn anonymous_signup(base: &str) -> reqwest::StatusCode {
    reqwest::Client::new()
        .post(format!("{base}/auth/v1/signup"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("sign-up request")
        .status()
}

#[tokio::test]
async fn in_development_both_roles_are_created_and_the_server_serves_on_them() {
    let Some(admin_url) =
        database_url_or_skip("in_development_both_roles_are_created_and_the_server_serves_on_them")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "both").await;
    let dir = project(Some("AUTH_DATABASE_URL"));
    let (engine_url, auth_url) = (s.engine_url(), s.auth_url());
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
        ("AUTH_DATABASE_URL", auth_url.as_str()),
    ];

    let mut served = run(dir.path(), Some(&s.admin_here), &env);
    served.ready().await.unwrap();
    assert_eq!(role(&admin_url, &s.engine).await, Some((true, false, false)));
    assert_eq!(role(&admin_url, &s.auth).await, Some((true, false, false)));
    assert_eq!(anonymous_signup(&served.base).await, reqwest::StatusCode::OK);
    let output = served.output();
    drop(served);
    assert_no_secret(&output, &s.admin_here);

    let here = connect(&s.admin_here).await;
    let owners: Vec<String> = here
        .query("SELECT DISTINCT tableowner::text FROM pg_tables WHERE schemaname = 'auth'", &[])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(owners, vec![s.auth.clone()], "every auth table belongs to the auth role");
    let sessions: i64 =
        here.query_one("SELECT count(*) FROM auth.sessions", &[]).await.unwrap().get(0);
    assert_eq!(sessions, 1, "the sign-up's session landed in auth.sessions");
    for (who, privilege) in [
        (&s.engine, "CREATE"),
        (&s.engine, "CONNECT"),
        (&s.auth, "CREATE"),
        (&s.auth, "CONNECT"),
    ] {
        let granted: bool = here
            .query_one(
                "SELECT has_database_privilege($1, current_database(), $2)",
                &[who, &privilege],
            )
            .await
            .unwrap()
            .get(0);
        assert!(granted, "{who} holds {privilege} on the database");
    }
    let engine_creates_in_public: bool = here
        .query_one("SELECT has_schema_privilege($1, 'public', 'CREATE')", &[&s.engine])
        .await
        .unwrap()
        .get(0);
    assert!(engine_creates_in_public, "the engine role holds CREATE on schema public");
    let engine_reads_sessions: bool = here
        .query_one("SELECT has_table_privilege($1, 'auth.sessions', 'SELECT')", &[&s.engine])
        .await
        .unwrap()
        .get(0);
    assert!(!engine_reads_sessions, "the engine role holds no grant to read auth.sessions");

    s.drop_all().await;
}

#[tokio::test]
async fn a_second_run_creates_nothing_and_fails_nothing() {
    let Some(admin_url) = database_url_or_skip("a_second_run_creates_nothing_and_fails_nothing")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "rerun").await;
    let dir = project(Some("AUTH_DATABASE_URL"));
    let (engine_url, auth_url) = (s.engine_url(), s.auth_url());
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
        ("AUTH_DATABASE_URL", auth_url.as_str()),
    ];
    for _ in 0..2 {
        let mut again = run(dir.path(), Some(&s.admin_here), &env);
        again.ready().await.unwrap();
    }
    assert_eq!(role(&admin_url, &s.engine).await, Some((true, false, false)));
    assert_eq!(role(&admin_url, &s.auth).await, Some((true, false, false)));

    s.drop_all().await;
}

#[tokio::test]
async fn an_existing_role_keeps_its_attributes_and_takes_its_urls_password() {
    let Some(admin_url) =
        database_url_or_skip("an_existing_role_keeps_its_attributes_and_takes_its_urls_password")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "existing").await;
    let admin = connect(&admin_url).await;
    admin
        .batch_execute(&format!(
            "CREATE ROLE \"{}\" LOGIN CREATEDB PASSWORD 'an-older-password'",
            s.engine
        ))
        .await
        .unwrap();
    let before = verifier(&admin_url, &s.engine).await;
    let dir = project(None);
    let engine_url = s.engine_url();
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
    ];

    let mut served = run(dir.path(), Some(&s.admin_here), &env);
    served.ready().await.unwrap();
    drop(served);

    assert_eq!(
        role(&admin_url, &s.engine).await,
        Some((true, false, true)),
        "an existing role keeps its attributes, CREATEDB included"
    );
    assert_ne!(
        verifier(&admin_url, &s.engine).await,
        before,
        "an existing role's password is set from its URL"
    );

    s.drop_all().await;
}

#[tokio::test]
async fn outside_development_the_flag_is_refused_before_any_role_is_created() {
    let Some(admin_url) =
        database_url_or_skip("outside_development_the_flag_is_refused_before_any_role_is_created")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "prod").await;
    let dir = project(Some("AUTH_DATABASE_URL"));
    let (engine_url, auth_url) = (s.engine_url(), s.auth_url());
    // `FRAISEQL_ENV` unset: production.
    let env = [
        ("DATABASE_URL", engine_url.as_str()),
        ("AUTH_DATABASE_URL", auth_url.as_str()),
    ];

    let (code, output) = run(dir.path(), Some(&s.admin_here), &env).finished();
    assert_ne!(code, Some(0), "refused:\n{output}");
    assert!(output.contains("--admin-database-url"), "names the flag:\n{output}");
    assert!(output.contains("FRAISEQL_ENV"), "names the variable:\n{output}");
    assert_no_secret(&output, &s.admin_here);
    assert_eq!(role(&admin_url, &s.engine).await, None, "no engine role");
    assert_eq!(role(&admin_url, &s.auth).await, None, "no auth role");

    s.drop_all().await;
}

#[tokio::test]
async fn the_flag_reads_fraiseql_admin_database_url_too() {
    let Some(admin_url) = database_url_or_skip("the_flag_reads_fraiseql_admin_database_url_too")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "envvar").await;
    let dir = project(None);
    let engine_url = s.engine_url();
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
        ("FRAISEQL_ADMIN_DATABASE_URL", s.admin_here.as_str()),
    ];

    let mut served = run(dir.path(), None, &env);
    served.ready().await.unwrap();
    drop(served);
    assert_eq!(role(&admin_url, &s.engine).await, Some((true, false, false)));

    s.drop_all().await;
}

#[tokio::test]
async fn an_admin_url_naming_another_database_is_refused() {
    let Some(admin_url) = database_url_or_skip("an_admin_url_naming_another_database_is_refused")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "otherdb").await;
    let dir = project(Some("AUTH_DATABASE_URL"));
    let (engine_url, auth_url) = (s.engine_url(), s.auth_url());
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
        ("AUTH_DATABASE_URL", auth_url.as_str()),
    ];

    // The admin URL as given, on its own database rather than the scratch one.
    let (code, output) = run(dir.path(), Some(&admin_url), &env).finished();
    assert_ne!(code, Some(0), "refused:\n{output}");
    assert!(output.contains("--admin-database-url"), "names the flag:\n{output}");
    assert!(output.contains("same database"), "says why:\n{output}");
    assert_no_secret(&output, &admin_url);
    assert_eq!(role(&admin_url, &s.engine).await, None, "no engine role");
    assert_eq!(role(&admin_url, &s.auth).await, None, "no auth role");

    s.drop_all().await;
}

#[tokio::test]
async fn without_auth_database_only_the_engine_role_is_provisioned() {
    let Some(admin_url) =
        database_url_or_skip("without_auth_database_only_the_engine_role_is_provisioned")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "engineonly").await;
    let dir = project(None);
    let (engine_url, auth_url) = (s.engine_url(), s.auth_url());
    // `AUTH_DATABASE_URL` is set, and no `[auth.database]` names it.
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
        ("AUTH_DATABASE_URL", auth_url.as_str()),
    ];

    let mut served = run(dir.path(), Some(&s.admin_here), &env);
    served.ready().await.unwrap();
    drop(served);
    assert_eq!(role(&admin_url, &s.engine).await, Some((true, false, false)));
    assert_eq!(role(&admin_url, &s.auth).await, None, "no auth role");

    s.drop_all().await;
}

#[tokio::test]
async fn a_url_naming_the_admin_user_leaves_that_role_untouched() {
    let Some(admin_url) =
        database_url_or_skip("a_url_naming_the_admin_user_leaves_that_role_untouched")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "adminuser").await;
    let admin_user = user_of(&admin_url);
    let before = verifier(&admin_url, &admin_user).await;
    // The single-superuser development layout: the engine and auth run as the admin.
    let dir = project(Some("DATABASE_URL"));
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", s.admin_here.as_str()),
    ];

    let mut served = run(dir.path(), Some(&s.admin_here), &env);
    served.ready().await.unwrap();
    drop(served);
    assert_eq!(
        verifier(&admin_url, &admin_user).await,
        before,
        "the admin user's password is never set"
    );

    s.drop_all().await;
}

#[tokio::test]
async fn two_urls_naming_one_user_with_different_passwords_are_refused() {
    let Some(admin_url) =
        database_url_or_skip("two_urls_naming_one_user_with_different_passwords_are_refused")
    else {
        return;
    };
    let s = Scratch::new(&admin_url, "twopasswords").await;
    let dir = project(Some("AUTH_DATABASE_URL"));
    let engine_url = s.engine_url();
    let other = with_user(&s.admin_here, &s.engine, AUTH_PASSWORD);
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
        ("AUTH_DATABASE_URL", other.as_str()),
    ];

    let (code, output) = run(dir.path(), Some(&s.admin_here), &env).finished();
    assert_ne!(code, Some(0), "refused:\n{output}");
    assert!(output.contains("different passwords"), "says why:\n{output}");
    assert_no_secret(&output, &s.admin_here);
    assert_eq!(role(&admin_url, &s.engine).await, None, "no role");

    s.drop_all().await;
}

#[tokio::test]
async fn run_watch_provisions_too() {
    let Some(admin_url) = database_url_or_skip("run_watch_provisions_too") else {
        return;
    };
    let s = Scratch::new(&admin_url, "watch").await;
    let dir = project(Some("AUTH_DATABASE_URL"));
    let (engine_url, auth_url) = (s.engine_url(), s.auth_url());
    let env = [
        ("FRAISEQL_ENV", "development"),
        ("DATABASE_URL", engine_url.as_str()),
        ("AUTH_DATABASE_URL", auth_url.as_str()),
    ];

    let mut served = run_with(dir.path(), Some(&s.admin_here), &env, &["--watch"]);
    served.ready().await.unwrap();
    drop(served);
    assert_eq!(role(&admin_url, &s.engine).await, Some((true, false, false)));
    assert_eq!(role(&admin_url, &s.auth).await, Some((true, false, false)));

    s.drop_all().await;
}

#[test]
fn the_flags_help_says_what_a_rerun_changes() {
    let out = Command::new(cli_bin()).args(["run", "--help"]).output().expect("run --help");
    let help = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for phrase in [
        "--admin-database-url",
        "FRAISEQL_ADMIN_DATABASE_URL",
        "FRAISEQL_ENV=development",
        "only its password",
        "never changes",
        "admin user",
    ] {
        assert!(help.contains(phrase), "`run --help` says {phrase:?}:\n{help}");
    }
}
