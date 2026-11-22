//! An `after:mutation` or `after:capture` dispatch still retrying when the server process
//! dies runs again once a server is back, with the same idempotency token, exactly once more.
//!
//! The function under test records every attempt through its `fraiseql_query` bridge (the
//! write commits whatever the attempt then does), and fails until it is on its third. Its
//! dispatch is durable (the default), so it is retried with backoff. The server is a separate
//! process: this test binary, re-executed to run [`serve_a_restart_child`]. It is killed
//! with `SIGKILL` once the second attempt is recorded, while the dispatch waits out its
//! backoff before the third. A second process is then started on the same database, as a
//! restarted deployment is.
//!
//! What must follow is judged from the database alone: a third attempt, carrying the token
//! the first two carried, and no fourth.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test owns a scratch database and its server processes; one V8
//! isolate per process, so each server is a process of its own → `--test-threads=1`.
#![cfg(all(feature = "functions-runtime-deno", feature = "observers"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use fraiseql_test_support::try_database_url;
use serde_json::json;
use sqlx::{PgPool, Row as _};

/// Set on the re-executed child: the scratch directory holding what it serves.
const CHILD_SCRATCH: &str = "FUNCTION_DISPATCH_RESTART_CHILD_SCRATCH";

/// The attempt the function first succeeds on.
const SUCCEEDS_ON: i64 = 3;

/// Records the attempt through the bridge, then fails until it is the third.
const FLAKY: &str = r"
export default async (event) => {
  const token = Deno.core.ops.fraiseql_idempotency_token();
  const raw = await Deno.core.ops.fraiseql_query(
    `mutation ($token: String!) { recordAttempt(token: $token) { attempt } }`,
    JSON.stringify({ token }),
  );
  const attempt = JSON.parse(raw).data.recordAttempt.attempt;
  if (attempt < 3) {
    throw new Error(`attempt ${attempt} fails, as this test needs it to`);
  }
  return { attempt };
};
";

const SETUP_SQL: &str = r"
CREATE SCHEMA app;
CREATE TABLE app.tb_order (id uuid PRIMARY KEY, label text NOT NULL);
CREATE TABLE app.tb_attempt (
    id uuid PRIMARY KEY, token text NOT NULL, attempt integer NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE VIEW app.v_order AS
    SELECT id, jsonb_build_object('id', id, 'label', label) AS data FROM app.tb_order;
CREATE VIEW app.v_attempt AS
    SELECT id, jsonb_build_object('id', id, 'token', token, 'attempt', attempt) AS data
    FROM app.tb_attempt;
CREATE FUNCTION app.fn_create_order(p_label text) RETURNS TABLE(succeeded boolean,
    state_changed boolean, error_class text, status_detail text, http_status smallint,
    message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[],
    cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$ DECLARE v uuid := gen_random_uuid(); BEGIN
    INSERT INTO app.tb_order VALUES (v, p_label);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        jsonb_build_object('id', v, 'label', p_label), v, 'Order');
END $$;
CREATE FUNCTION app.fn_record_attempt(p_token text) RETURNS TABLE(succeeded boolean,
    state_changed boolean, error_class text, status_detail text, http_status smallint,
    message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[],
    cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$ DECLARE v uuid := gen_random_uuid(); n integer; BEGIN
    SELECT count(*) + 1 INTO n FROM app.tb_attempt WHERE token = p_token;
    INSERT INTO app.tb_attempt (id, token, attempt) VALUES (v, p_token, n);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        jsonb_build_object('id', v, 'token', p_token, 'attempt', n), v, 'Attempt');
END $$;
";

/// Which trigger kind drives the function.
#[derive(Clone, Copy, Debug)]
enum Trigger {
    /// A mutation served over HTTP.
    Mutation,
    /// A change the external-write capture trigger wrote to the change log.
    Capture,
}

impl Trigger {
    const fn spec(self) -> &'static str {
        match self {
            Self::Mutation => "after:mutation:Order:insert",
            Self::Capture => "after:capture:Order:insert",
        }
    }

    const fn database(self) -> &'static str {
        match self {
            Self::Mutation => "fraiseql_dispatch_restart_mutation",
            Self::Capture => "fraiseql_dispatch_restart_capture",
        }
    }
}

fn schema_json(trigger: Trigger) -> serde_json::Value {
    json!({
        "types": [
            { "name": "Order", "sql_source": "app.v_order", "is_input": false,
              "fields": [{ "name": "id", "type": "ID", "nullable": false },
                         { "name": "label", "type": "String", "nullable": false }] },
            { "name": "Attempt", "sql_source": "app.v_attempt", "is_input": false,
              "fields": [{ "name": "id", "type": "ID", "nullable": false },
                         { "name": "token", "type": "String", "nullable": false },
                         { "name": "attempt", "type": "Int", "nullable": false }] }
        ],
        "queries": [{
            "name": "orders", "return_type": "Order", "returns_list": true, "nullable": false,
            "sql_source": "app.v_order", "arguments": []
        }],
        "mutations": [
            { "name": "createOrder", "return_type": "Order", "operation": "insert",
              "sql_source": "app.fn_create_order",
              "arguments": [{ "name": "label", "type": "String", "nullable": false }] },
            { "name": "recordAttempt", "return_type": "Attempt", "operation": "insert",
              "sql_source": "app.fn_record_attempt",
              "arguments": [{ "name": "token", "type": "String", "nullable": false }] }
        ],
        "functions": [{
            "name": "flaky", "trigger": trigger.spec(), "runtime": "Deno",
            "run_as": { "roles": ["attempts"] }
        }]
    })
}

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

/// A fresh database, prepared as a deployment prepares one: `fraiseql setup`, then the
/// application's SQL.
async fn prepare_database(admin_url: &str, db: &str) -> String {
    let admin = PgPool::connect(admin_url).await.unwrap();
    for statement in [
        format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"),
        format!("CREATE DATABASE {db}"),
    ] {
        // Reason: test SQL: names and values this test defines, or DDL the code under test
        // generated.
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&admin)
            .await
            .unwrap();
    }
    admin.close().await;
    let url = with_database(admin_url, db);
    fraiseql_cli::commands::setup::run(
        Some(&url),
        false,
        &fraiseql_cli::output::OutputFormatter::new(false, true),
    )
    .await
    .unwrap();
    let pool = PgPool::connect(&url).await.unwrap();
    sqlx::raw_sql(SETUP_SQL).execute(&pool).await.unwrap();
    pool.close().await;
    url
}

async fn drop_database(admin_url: &str, db: &str) {
    if let Ok(admin) = PgPool::connect(admin_url).await {
        // Reason: test SQL: names and values this test defines, or DDL the code under test
        // generated.
        let _ = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {db} WITH (FORCE)"
        )))
        .execute(&admin)
        .await;
        admin.close().await;
    }
}

/// Write the authoring inputs and the module into `scratch`, and compile them there, as
/// `fraiseql compile schema.json` does beside a `fraiseql.toml`.
async fn compile(scratch: &Path, trigger: Trigger) {
    let modules = scratch.join("modules");
    std::fs::create_dir_all(&modules).unwrap();
    std::fs::write(modules.join("flaky.ts"), FLAKY).unwrap();
    let schema = scratch.join("schema.json");
    std::fs::write(&schema, serde_json::to_vec_pretty(&schema_json(trigger)).unwrap()).unwrap();
    let project = scratch.join("fraiseql.toml");
    let observers = match trigger {
        Trigger::Mutation => "",
        Trigger::Capture => "\n[observers]\nenabled = true\n",
    };
    std::fs::write(&project, format!("[project]\nname = \"dispatch-restart\"\n{observers}"))
        .unwrap();
    fraiseql_cli::commands::compile::run(
        &schema.to_string_lossy(),
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &scratch.join("schema.compiled.json").to_string_lossy(),
        false,
        None,
        None,
        false,
        true,
        false,
        Some(&project.to_string_lossy()),
    )
    .await
    .unwrap();
}

/// One server process, killed when dropped.
struct Served {
    child:  Child,
    base:   String,
    stderr: PathBuf,
}

impl Served {
    fn stderr_tail(&self) -> String {
        let text = std::fs::read_to_string(&self.stderr).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(60)..].join("\n")
    }

    /// `SIGKILL`, as an OOM killer or a lost node ends a process: no shutdown hook runs.
    fn kill(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start a server process on `database_url`, serving what `scratch` holds, and wait for its
/// health endpoint. `generation` names its log.
async fn serve(scratch: &Path, database_url: &str, generation: u8) -> Served {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let stderr = scratch.join(format!("server-{generation}.stderr"));
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "serve_a_restart_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_SCRATCH, scratch)
        .env("DATABASE_URL", database_url)
        .env("FRAISEQL_BIND_ADDR", format!("127.0.0.1:{port}"))
        .env("FRAISEQL_ENV", "development")
        .env("FRAISEQL_FUNCTIONS_MODULE_DIR", scratch.join("modules"))
        // Durable dispatch, with a backoff long enough that the kill lands inside it.
        .env("FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS", "5")
        .env("FRAISEQL_FUNCTIONS_RETRY_INITIAL_DELAY_MS", "1500")
        .env("FRAISEQL_FUNCTIONS_RETRY_MAX_DELAY_MS", "60000")
        .env("RUST_LOG", "info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut served = Served {
        child,
        base: format!("http://127.0.0.1:{port}"),
        stderr,
    };
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = served.child.try_wait() {
            panic!(
                "server {generation} exited ({status}) before serving:\n{}",
                served.stderr_tail()
            );
        }
        if reqwest::get(format!("{}/health", served.base)).await.is_ok() {
            return served;
        }
        assert!(
            started.elapsed() < Duration::from_mins(1),
            "server {generation} never answered:\n{}",
            served.stderr_tail()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Every attempt recorded so far: (token, attempt number), oldest first.
async fn attempts(pool: &PgPool) -> Vec<(String, i64)> {
    sqlx::query("SELECT token, attempt::bigint AS attempt FROM app.tb_attempt ORDER BY recorded_at")
        .fetch_all(pool)
        .await
        .unwrap()
        .iter()
        .map(|row| (row.get("token"), row.get("attempt")))
        .collect()
}

/// Wait until at least `count` attempts are recorded, or `deadline` passes.
async fn wait_for_attempts(pool: &PgPool, count: usize, deadline: Duration) -> Vec<(String, i64)> {
    let started = Instant::now();
    loop {
        let seen = attempts(pool).await;
        if seen.len() >= count || started.elapsed() > deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Make the change that triggers the function.
async fn stimulate(trigger: Trigger, served: &Served, pool: &PgPool) {
    match trigger {
        Trigger::Mutation => {
            let response: serde_json::Value = reqwest::Client::new()
                .post(format!("{}/graphql", served.base))
                .json(&json!({ "query": r#"mutation { createOrder(label: "R-1") { id } }"# }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert!(response.get("errors").is_none(), "createOrder failed: {response}");
        },
        Trigger::Capture => {
            // The row the external-write capture trigger writes for an INSERT it saw.
            sqlx::query(
                "INSERT INTO core.tb_entity_change_log \
                     (object_type, modification_type, object_id, object_data, extra_metadata) \
                 VALUES ('Order', 'INSERT', gen_random_uuid(), \
                         jsonb_build_object('id', gen_random_uuid(), 'label', 'C-1'), \
                         '{\"cdc_source\": \"fallback_trigger\"}'::jsonb)",
            )
            .execute(pool)
            .await
            .unwrap();
        },
    }
}

async fn killed_during_retry_2_runs_again_after_restart(trigger: Trigger) {
    let Some(admin_url) = try_database_url() else {
        eprintln!("SKIP {trigger:?}: DATABASE_URL is not set");
        return;
    };
    let db = trigger.database();
    let database_url = prepare_database(&admin_url, db).await;
    let scratch = tempfile::tempdir().unwrap();
    compile(scratch.path(), trigger).await;
    let pool = PgPool::connect(&database_url).await.unwrap();

    let mut first = serve(scratch.path(), &database_url, 1).await;
    stimulate(trigger, &first, &pool).await;
    let before_kill = wait_for_attempts(&pool, 2, Duration::from_secs(45)).await;
    first.kill();
    assert_eq!(
        before_kill.len(),
        2,
        "the first server should be killed while the dispatch waits to retry a second time, \
         with two attempts recorded: {before_kill:?}\n{}",
        first.stderr_tail()
    );
    let after_kill = attempts(&pool).await;
    assert_eq!(after_kill.len(), 2, "an attempt ran after the kill: {after_kill:?}");

    let second = serve(scratch.path(), &database_url, 2).await;
    let after_restart = wait_for_attempts(&pool, 3, Duration::from_secs(45)).await;
    assert_eq!(
        after_restart.len(),
        3,
        "{trigger:?}: the dispatch the killed server was retrying never ran again after the \
         restart: {after_restart:?}\n--- server 2 ---\n{}",
        second.stderr_tail()
    );
    // Long enough for the lease to run out again and several sweeps to pass: the dispatch
    // succeeded on its third attempt, so nothing may run it a fourth time.
    tokio::time::sleep(Duration::from_secs(12)).await;
    let settled = attempts(&pool).await;
    assert_eq!(
        settled.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
        (1..=SUCCEEDS_ON).collect::<Vec<_>>(),
        "{trigger:?}: the dispatch ran again exactly once more and then stopped: {settled:?}"
    );
    assert!(
        settled.iter().all(|(token, _)| *token == settled[0].0),
        "{trigger:?}: every attempt, before and after the restart, carries one token: \
         {settled:?}"
    );

    drop(second);
    pool.close().await;
    drop_database(&admin_url, db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_after_mutation_dispatch_killed_during_retry_2_runs_again_after_restart() {
    killed_during_retry_2_runs_again_after_restart(Trigger::Mutation).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_after_capture_dispatch_killed_during_retry_2_runs_again_after_restart() {
    killed_during_retry_2_runs_again_after_restart(Trigger::Capture).await;
}

/// The server process the tests above start and kill: this binary, re-executed with
/// [`CHILD_SCRATCH`] set. Run any other way, it does nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the restart tests re-execute it as their server process"]
async fn serve_a_restart_child() {
    let Ok(scratch) = std::env::var(CHILD_SCRATCH) else {
        eprintln!("SKIP serve_a_restart_child: the restart tests set {CHILD_SCRATCH}");
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let scratch = PathBuf::from(scratch);
    let mut config: fraiseql_server::ServerConfig = toml::from_str(concat!(
        "cors_enabled = false\n",
        "[observers.runtime]\npoll_interval_ms = 200\n",
        // A dispatch's lease, and how often a server looks for one whose lease ran out.
        "[function_dispatch]\nlease_secs = 8\nsweep_interval_secs = 1\n",
    ))
    .unwrap();
    config.schema_path = scratch.join("schema.compiled.json");
    config.database_url = fraiseql_test_support::database_url();
    config.bind_addr = std::env::var("FRAISEQL_BIND_ADDR").unwrap().parse().unwrap();
    config.validate().unwrap();
    let loaded = fraiseql_server::CompiledSchemaLoader::new(&config.schema_path)
        .load_extended()
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await.unwrap();
    let server = Box::pin(fraiseql_server::boot::build_postgres_server(
        config,
        loaded.schema,
        loaded.functions,
    ))
    .await
    .unwrap();
    Box::pin(server.serve_on_listener(listener, std::future::pending::<()>()))
        .await
        .unwrap();
}
