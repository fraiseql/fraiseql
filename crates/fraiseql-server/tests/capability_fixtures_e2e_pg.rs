//! Boot-to-effect fixtures: each capability, authored the way an application authors it,
//! compiled by the compiler `fraiseql compile` runs, served by the `fraiseql-server` binary,
//! and judged by what it does, not by what it reports.
//!
//! A fixture is a directory under `tests/capabilities/`:
//!
//! - `schema.json` and `fraiseql.toml`: the authoring inputs, compiled here. A fixture with no
//!   `schema.json` is TOML-only: `fraiseql.toml` declares the schema itself, as `fraiseql compile
//!   fraiseql.toml` reads it.
//! - `setup.sql`: the application's tables, views and functions.
//! - `server.toml`: the server's `ServerConfig`.
//! - `fixture.toml`: the capability it exercises, the environment the server runs with, the stimuli
//!   it receives, and the effect that must follow.
//!
//! Each fixture gets a database of its own (`cap_<name>`), prepared as a deployment prepares
//! one (`fraiseql setup`, then the application's SQL), so no fixture leans on tables another
//! suite created. The server is a child process with an empty working directory: an
//! environment variable is set on the process that reads it, as in a deployment.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** one scratch database per fixture, fixtures run in turn.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use fraiseql_server::capability::REGISTRY;
use fraiseql_test_support::try_database_url;
use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgPool, Row as _};

/// The files every fixture carries besides `fixture.toml` (and `schema.json`, unless the
/// fixture is TOML-only).
const FIXTURE_FILES: [&str; 3] = ["fraiseql.toml", "setup.sql", "server.toml"];

/// How long a server may take to answer its first request.
const BOOT_DEADLINE: Duration = Duration::from_mins(1);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    /// The registry entry this fixture exercises.
    capability:    String,
    /// How long the effect may take to appear once the stimuli are sent.
    #[serde(default = "default_deadline_secs")]
    deadline_secs: u64,
    /// Environment for the server process. `$FIXTURE` expands to the fixture's directory.
    #[serde(default)]
    env:           BTreeMap<String, String>,
    stimulus:      Vec<Stimulus>,
    effect:        Effect,
}

const fn default_deadline_secs() -> u64 {
    30
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Stimulus {
    Http {
        method: String,
        path:   String,
        #[serde(default)]
        body:   Option<String>,
        /// A bearer token sent as `Authorization`.
        #[serde(default)]
        auth:   Option<String>,
        /// An HMAC-SHA256 signature of `body`, sent in a header, as a webhook provider
        /// signs a delivery.
        #[serde(default)]
        sign:   Option<Signature>,
    },
    /// One MCP `tools/call` over streamable HTTP, after the session handshake an agent
    /// client performs (`initialize`, then `notifications/initialized`).
    Mcp {
        path:      String,
        tool:      String,
        #[serde(default)]
        arguments: Value,
    },
    Sql {
        statement: String,
    },
    Wait {
        secs: u64,
    },
}

/// How an `http` stimulus signs its body.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Signature {
    /// The header carrying the signature.
    header: String,
    /// The shared secret.
    secret: String,
    /// Written before the hex digest, such as `sha256=`.
    #[serde(default)]
    prefix: String,
}

impl Signature {
    fn of(&self, body: &str) -> String {
        use hmac::{KeyInit as _, Mac as _};
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(self.secret.as_bytes()).unwrap();
        mac.update(body.as_bytes());
        format!("{}{}", self.prefix, hex::encode(mac.finalize().into_bytes()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Effect {
    /// `GET path` answers `status`, with `equals` at `json_pointer` when one is given.
    Http {
        path:         String,
        status:       u16,
        #[serde(default)]
        json_pointer: Option<String>,
        #[serde(default)]
        equals:       Option<Value>,
    },
    /// `query` returns exactly `expect`, every column read as text.
    Row {
        query:  String,
        expect: Vec<Vec<String>>,
    },
    /// `/health` reports the capability `started` and `serving` as given.
    Health {
        capability: String,
        #[serde(default)]
        started:    Option<bool>,
        #[serde(default)]
        serving:    Option<bool>,
    },
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/capabilities")
}

/// Every fixture directory, by name.
fn fixtures(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.join("fixture.toml").is_file())
                .map(|path| (path.file_name().unwrap().to_string_lossy().into_owned(), path))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

/// A fresh `db`, prepared as a deployment prepares one: `fraiseql setup`, then `setup.sql`.
async fn prepare_database(admin_url: &str, db: &str, setup_sql: &str) -> Result<String, String> {
    let admin = PgPool::connect(admin_url).await.map_err(|e| format!("connect: {e}"))?;
    for statement in [
        format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"),
        format!("CREATE DATABASE {db}"),
    ] {
        sqlx::raw_sql(&statement)
            .execute(&admin)
            .await
            .map_err(|e| format!("{statement}: {e}"))?;
    }
    admin.close().await;
    let url = with_database(admin_url, db);
    fraiseql_cli::commands::setup::run(
        Some(&url),
        false,
        &fraiseql_cli::output::OutputFormatter::new(false, true),
    )
    .await
    .map_err(|e| format!("fraiseql setup: {e:#}"))?;
    let pool = PgPool::connect(&url).await.map_err(|e| format!("connect to {db}: {e}"))?;
    sqlx::raw_sql(setup_sql)
        .execute(&pool)
        .await
        .map_err(|e| format!("setup.sql: {e}"))?;
    pool.close().await;
    Ok(url)
}

async fn drop_database(admin_url: &str, db: &str) {
    if let Ok(admin) = PgPool::connect(admin_url).await {
        let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
            .execute(&admin)
            .await;
        admin.close().await;
    }
}

/// Compile the fixture's authoring inputs to `out`, as `fraiseql compile` does:
/// `schema.json` with `fraiseql.toml` beside it, or `fraiseql.toml` alone.
async fn compile(dir: &Path, out: &Path) -> Result<(), String> {
    let schema = dir.join("schema.json");
    let (input, config) = if schema.is_file() {
        (schema, Some(dir.join("fraiseql.toml").to_string_lossy().into_owned()))
    } else {
        (dir.join("fraiseql.toml"), None)
    };
    fraiseql_cli::commands::compile::run(
        &input.to_string_lossy(),
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &out.to_string_lossy(),
        false,
        None,
        None,
        false,
        true,
        false,
        config.as_deref(),
    )
    .await
    .map_err(|e| format!("fraiseql compile: {e:#}"))
}

/// A served fixture: the server process, and where its output goes.
struct Served {
    child: Child,
    base:  String,
    log:   PathBuf,
}

impl Served {
    fn log_tail(&self) -> String {
        let log = std::fs::read_to_string(&self.log).unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        lines[lines.len().saturating_sub(40)..].join("\n")
    }

    /// Fails when the server has exited, with its status.
    fn still_running(&mut self) -> Result<(), String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Err(format!("the server exited ({status})")),
            Ok(None) => Ok(()),
            Err(e) => Err(format!("the server cannot be waited on: {e}")),
        }
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Start the `fraiseql-server` binary over the fixture, and wait for its first answer.
async fn serve(
    dir: &Path,
    fixture: &Fixture,
    schema: &Path,
    database_url: &str,
    scratch: &Path,
) -> Result<Served, String> {
    let port = free_port();
    let log = scratch.join("server.log");
    let cwd = scratch.join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fraiseql-server"));
    command
        .current_dir(&cwd)
        .env("FRAISEQL_CONFIG", dir.join("server.toml"))
        .env("FRAISEQL_SCHEMA_PATH", schema)
        .env("DATABASE_URL", database_url)
        .env("FRAISEQL_BIND_ADDR", format!("127.0.0.1:{port}"))
        .env("RUST_LOG", "info")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&log).unwrap())
        .stderr(std::fs::File::create(scratch.join("server.stderr")).unwrap());
    for (key, value) in &fixture.env {
        command.env(key, value.replace("$FIXTURE", &dir.to_string_lossy()));
    }
    let child = command.spawn().map_err(|e| format!("spawn fraiseql-server: {e}"))?;
    let mut served = Served {
        child,
        base: format!("http://127.0.0.1:{port}"),
        log,
    };
    let started = Instant::now();
    loop {
        if let Err(exited) = served.still_running() {
            let stderr = std::fs::read_to_string(scratch.join("server.stderr")).unwrap_or_default();
            return Err(format!("{exited} before serving:\n{}\n{stderr}", served.log_tail()));
        }
        if reqwest::get(format!("{}/health", served.base)).await.is_ok() {
            return Ok(served);
        }
        if started.elapsed() > BOOT_DEADLINE {
            return Err(format!("the server never answered:\n{}", served.log_tail()));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn send(served: &Served, database_url: &str, stimulus: &Stimulus) -> Result<(), String> {
    match stimulus {
        Stimulus::Http {
            method,
            path,
            body,
            auth,
            sign,
        } => {
            let method = reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|e| format!("method {method}: {e}"))?;
            let mut request = reqwest::Client::new()
                .request(method, format!("{}{path}", served.base))
                .header("content-type", "application/json");
            if let Some(token) = auth {
                request = request.bearer_auth(token);
            }
            if let Some(signature) = sign {
                request = request
                    .header(signature.header.as_str(), signature.of(body.as_deref().unwrap_or("")));
            }
            if let Some(body) = body {
                request = request.body(body.clone());
            }
            let response = request.send().await.map_err(|e| format!("{path}: {e}"))?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            eprintln!("stimulus {path}: {status} {text}");
            Ok(())
        },
        Stimulus::Mcp {
            path,
            tool,
            arguments,
        } => {
            let client = reqwest::Client::new();
            let url = format!("{}{path}", served.base);
            let post = |body: Value, session: Option<&str>| {
                let request = client
                    .post(&url)
                    .header("accept", "application/json, text/event-stream")
                    .json(&body);
                match session {
                    Some(id) => request.header("mcp-session-id", id),
                    None => request,
                }
            };
            let initialize = post(
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": {},
                        "clientInfo": { "name": "capability-fixture", "version": "0" }
                    }
                }),
                None,
            )
            .send()
            .await
            .map_err(|e| format!("mcp initialize: {e}"))?;
            let session = initialize
                .headers()
                .get("mcp-session-id")
                .and_then(|id| id.to_str().ok())
                .map(str::to_owned);
            let status = initialize.status();
            let text = initialize.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(format!("mcp initialize answered {status}: {text}"));
            }
            post(
                serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                session.as_deref(),
            )
            .send()
            .await
            .map_err(|e| format!("mcp initialized: {e}"))?;
            let call = post(
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                    "params": { "name": tool, "arguments": arguments }
                }),
                session.as_deref(),
            )
            .send()
            .await
            .map_err(|e| format!("mcp tools/call: {e}"))?;
            let status = call.status();
            eprintln!("stimulus mcp {tool}: {status} {}", call.text().await.unwrap_or_default());
            Ok(())
        },
        Stimulus::Sql { statement } => {
            let pool = PgPool::connect(database_url).await.map_err(|e| format!("connect: {e}"))?;
            sqlx::raw_sql(statement)
                .execute(&pool)
                .await
                .map_err(|e| format!("stimulus `{statement}`: {e}"))?;
            pool.close().await;
            Ok(())
        },
        Stimulus::Wait { secs } => {
            tokio::time::sleep(Duration::from_secs(*secs)).await;
            Ok(())
        },
    }
}

/// `Ok` when the effect holds now, `Err` with what was seen instead.
async fn observe(served: &Served, database_url: &str, effect: &Effect) -> Result<(), String> {
    match effect {
        Effect::Http {
            path,
            status,
            json_pointer,
            equals,
        } => {
            let response = reqwest::get(format!("{}{path}", served.base))
                .await
                .map_err(|e| format!("GET {path}: {e}"))?;
            let got = response.status().as_u16();
            let body: Value = response.json().await.unwrap_or(Value::Null);
            if got != *status {
                return Err(format!("GET {path} answered {got}, not {status}: {body}"));
            }
            match (json_pointer, equals) {
                (Some(pointer), Some(want)) if body.pointer(pointer) != Some(want) => {
                    Err(format!("GET {path}: {pointer} is {:?}, not {want}", body.pointer(pointer)))
                },
                _ => Ok(()),
            }
        },
        Effect::Row { query, expect } => {
            let pool = PgPool::connect(database_url).await.map_err(|e| format!("connect: {e}"))?;
            let rows = sqlx::query(query).fetch_all(&pool).await;
            pool.close().await;
            let rows = rows.map_err(|e| format!("`{query}`: {e}"))?;
            let got: Vec<Vec<String>> = rows
                .iter()
                .map(|row| {
                    (0..row.len())
                        .map(|i| row.try_get::<String, _>(i).unwrap_or_else(|e| format!("<{e}>")))
                        .collect()
                })
                .collect();
            if got == *expect {
                Ok(())
            } else {
                Err(format!("`{query}` returned {got:?}, not {expect:?}"))
            }
        },
        Effect::Health {
            capability,
            started,
            serving,
        } => {
            let health: Value = reqwest::get(format!("{}/health", served.base))
                .await
                .map_err(|e| format!("/health: {e}"))?
                .json()
                .await
                .map_err(|e| format!("/health: {e}"))?;
            let reported = &health["capabilities"][capability];
            let started_ok = started.is_none_or(|want| reported["started"] == want);
            let serving_ok = serving
                .is_none_or(|want| reported.get("serving").and_then(Value::as_bool) == Some(want));
            if started_ok && serving_ok {
                Ok(())
            } else {
                Err(format!("/health reports {capability}: {reported}"))
            }
        },
    }
}

/// Boot the fixture in `dir`, send its stimuli, and wait for its effect.
async fn run_fixture(admin_url: &str, name: &str, dir: &Path) -> Result<(), String> {
    let fixture: Fixture = toml::from_str(
        &std::fs::read_to_string(dir.join("fixture.toml")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("fixture.toml: {e}"))?;
    for file in FIXTURE_FILES {
        if !dir.join(file).is_file() {
            return Err(format!("the fixture has no `{file}`"));
        }
    }
    let Some(entry) = REGISTRY.iter().find(|entry| entry.meta.name == fixture.capability) else {
        return Err(format!("no registered capability is named {:?}", fixture.capability));
    };
    if !entry.compiled_in {
        return Err(format!(
            "{} is not compiled into this build: the fixture leg builds every capability",
            fixture.capability
        ));
    }

    let scratch = tempfile::tempdir().unwrap();
    let schema = scratch.path().join("schema.compiled.json");
    compile(dir, &schema).await?;
    let db = format!("cap_{name}");
    let setup_sql = std::fs::read_to_string(dir.join("setup.sql")).unwrap();
    let database_url = prepare_database(admin_url, &db, &setup_sql).await?;

    let outcome = async {
        let mut served = serve(dir, &fixture, &schema, &database_url, scratch.path()).await?;
        for stimulus in &fixture.stimulus {
            send(&served, &database_url, stimulus).await?;
        }
        let deadline = Instant::now() + Duration::from_secs(fixture.deadline_secs);
        loop {
            served
                .still_running()
                .map_err(|exited| format!("{exited}:\n{}", served.log_tail()))?;
            match observe(&served, &database_url, &fixture.effect).await {
                Ok(()) => return Ok(()),
                Err(seen) if Instant::now() > deadline => {
                    return Err(format!(
                        "the effect never appeared within {}s: {seen}\n--- server log ---\n{}",
                        fixture.deadline_secs,
                        served.log_tail()
                    ));
                },
                Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
    }
    .await;
    drop_database(admin_url, &db).await;
    outcome
}

#[tokio::test]
async fn every_capability_fixture_reaches_its_effect() {
    let Some(admin_url) = try_database_url() else {
        eprintln!("SKIP every_capability_fixture_reaches_its_effect: DATABASE_URL not set");
        return;
    };
    let fixtures = fixtures(&fixtures_dir());
    assert!(!fixtures.is_empty(), "no fixture under {}", fixtures_dir().display());
    // `CAPABILITY_FIXTURE=<name>` runs one, while working on it. Unset, as in CI, all run.
    let only = std::env::var("CAPABILITY_FIXTURE").ok();
    let fixtures: Vec<_> = fixtures
        .into_iter()
        .filter(|(name, _)| only.as_ref().is_none_or(|o| o == name))
        .collect();
    assert!(!fixtures.is_empty(), "no fixture is named {only:?}");

    let mut failures = Vec::new();
    for (name, dir) in &fixtures {
        match run_fixture(&admin_url, name, dir).await {
            Ok(()) => eprintln!("fixture {name}: effect observed"),
            Err(failure) => failures.push(format!("fixture {name}: {failure}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
