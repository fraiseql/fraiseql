//! Boot-to-effect fixtures: each capability, authored the way an application authors it,
//! compiled by the compiler `fraiseql compile` runs, served by the `fraiseql-server` binary,
//! and judged by what it does, not by what it reports.
//!
//! A fixture is a directory under `tests/capabilities/`:
//!
//! - `schema.json` and `fraiseql.toml`: the authoring inputs, compiled here. A fixture with no
//!   `schema.json` is TOML-only: `fraiseql.toml` declares the schema itself, as `fraiseql compile
//!   fraiseql.toml` reads it. `$FIXTURE` expands to the fixture's directory here too.
//! - `setup.sql`: the application's tables, views and functions.
//! - `server.toml`: the server's `ServerConfig`. `$FIXTURE` expands to the fixture's directory, and
//!   `$NATS_URL` to the broker this run was given; a fixture that names a service the run lacks
//!   fails, naming it. `$RECEIVER_URL` is the harness's receiver, which is also an OIDC issuer: it
//!   publishes a discovery document and a key set, and an `oidc` bearer is signed by its key.
//!   `$KAFKA_BOOTSTRAP` is the broker this run was given, and `$KAFKA_TOPIC` a topic created for
//!   this run alone, so a message an earlier run left cannot satisfy a `kafka` effect.
//!   `$GREENMAIL_HOST`, `"$GREENMAIL_IMAPS_PORT"` and `$GREENMAIL_CA` name the test mail server
//!   this run was given and the CA its certificate is signed by; the port is written quoted, so the
//!   file is valid TOML, and expands to a bare integer.
//! - `$RUN_ID` is unique to the run, in an `smtp` stimulus and a `row` effect: a mail server keeps
//!   what earlier runs delivered, and only this run's message may satisfy the effect.
//! - `fixture.toml`: the capability it exercises, the environment the server runs with, the stimuli
//!   it receives, and the effect that must follow.
//!
//! Each fixture gets a database of its own (`cap_<name>`), prepared as a deployment prepares
//! one (`fraiseql setup`, then the application's SQL), so no fixture leans on tables another
//! suite created. The server is a child process with an empty working directory: an
//! environment variable is set on the process that reads it, as in a deployment.
//!
//! A fixture is driven through every entry point its capability declares whose transport
//! carries its stimuli and its effect, each on a fresh database:
//!
//! - `binary`: the `fraiseql-server` binary.
//! - `mcp_stdio`: the same binary with `FRAISEQL_MCP_STDIO` set, judged as an MCP client judges it.
//!   It is ready when it answers `initialize` on stdout; every line it writes there must be a
//!   JSON-RPC message; an `mcp` stimulus is a `tools/call` on its stdin. It carries no HTTP and no
//!   bearer.
//! - `listener`: this test binary, re-executed to run one ignored test,
//!   [`serve_a_fixture_through_an_in_process_entry_point`], with the fixture's environment. It
//!   serves as an embedder does (`build_postgres_server`, then `serve_on_listener`).
//! - `run`: `fraiseql run`, from the `fraiseql-cli` binary built with `run-server` that [`RUN_BIN`]
//!   names. That binary is not built here: `run-server` builds this crate with its default
//!   features, which would then be on in every test build of it.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL`, `NATS_URL`
//! for a fixture that names NATS, `KAFKA_BOOTSTRAP` for one that names Kafka,
//! `GREENMAIL_{HOST,IMAPS_PORT,SMTP_PORT,CA}` for one that names the mail server,
//! `FRAISEQL_CAPABILITY_RUN_BIN` for one driven through `run` · **Leg:**
//! `integration (observers)`, which binds them · **Parallelism:** one scratch database per fixture,
//! fixtures run in turn.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
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

/// A serve path a fixture is driven through, one per field of the registry's `EntryPoints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Entry {
    /// The `fraiseql-server` binary.
    Binary,
    /// `build_postgres_server`, then `serve_on_listener`, as an embedder serves.
    Listener,
    /// The binary with `FRAISEQL_MCP_STDIO` set: MCP over stdin and stdout, and no HTTP.
    McpStdio,
    /// `fraiseql run`.
    Run,
}

impl Entry {
    const ALL: [Self; 4] = [Self::Binary, Self::Listener, Self::McpStdio, Self::Run];

    const fn name(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Listener => "listener",
            Self::McpStdio => "mcp_stdio",
            Self::Run => "run",
        }
    }

    /// Whether the capability named `capability` declares this entry point.
    fn declared_by(self, capability: &str) -> bool {
        REGISTRY
            .iter()
            .find(|entry| entry.meta.name == capability)
            .is_some_and(|entry| {
                let points = &entry.meta.entry_points;
                match self {
                    Self::Binary => points.binary,
                    Self::Listener => points.listener,
                    Self::McpStdio => points.mcp_stdio,
                    Self::Run => points.run,
                }
            })
    }
}

/// Fixtures not yet driven through an entry point their capability declares, each with the
/// issue that blocks it. Shrink-only: [`every_declared_entry_point_drives_a_fixture`] fails on
/// an entry the fixture would not be driven through anyway.
///
/// The `cron` fixture declares `[mcp]` beside its SDK `schema.json` (#1719), and its function
/// fires on the binary, a listener and `fraiseql run`. Served over MCP stdio it boots and
/// answers, and the function does not fire within the fixture's deadline.
const NOT_DRIVEN_YET: &[(&str, Entry, &str)] = &[];

/// Whether `fixture` is listed in [`NOT_DRIVEN_YET`] for `entry`.
fn waiting(fixture: &str, entry: Entry) -> bool {
    NOT_DRIVEN_YET
        .iter()
        .any(|(name, listed, _)| *name == fixture && *listed == entry)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    /// The registry entry this fixture exercises.
    capability:     String,
    /// How long the effect may take to appear once the stimuli are sent.
    #[serde(default = "default_deadline_secs")]
    deadline_secs:  u64,
    /// Whether the database is prepared with `fraiseql setup` before `setup.sql` (default
    /// `true`). Off for a fixture about what the server does without it.
    #[serde(default = "default_true")]
    fraiseql_setup: bool,
    /// Environment for the server process. `$FIXTURE` expands to the fixture's directory.
    #[serde(default)]
    env:            BTreeMap<String, String>,
    /// None for a fixture about what the server does once it is up.
    #[serde(default)]
    stimulus:       Vec<Stimulus>,
    effect:         Effect,
    /// What the fixture shows: the capability doing its work, or refusing an input.
    #[serde(default)]
    role:           Role,
}

/// What a fixture shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    /// The capability, given what it serves, produces its effect.
    #[default]
    Positive,
    /// The capability refuses an input it must refuse, and nothing follows from it.
    Hostile,
}

/// Why an effect is not observed: not there yet, or the probe could not look.
///
/// A positive fixture retries on both until its deadline. A `none` effect passes only on
/// [`Unmet::NotYet`]: a probe that could not look proves no absence.
#[derive(Debug)]
enum Unmet {
    /// The probe looked, and the effect is not there.
    NotYet(String),
    /// The probe failed: the service did not answer, the query did not run, a value did
    /// not parse.
    Broken(String),
}

impl std::fmt::Display for Unmet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotYet(why) => f.write_str(why),
            Self::Broken(why) => write!(f, "the probe failed: {why}"),
        }
    }
}

const fn default_deadline_secs() -> u64 {
    30
}

const fn default_true() -> bool {
    true
}

/// The bearer a stimulus presents.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Auth {
    /// Sent as written: an opaque token the server was configured with.
    Token(String),
    /// An RS256 JWT issued by the receiver (`iss` is `$RECEIVER_URL`) and signed with the key
    /// its key set publishes, as an OIDC provider issues one.
    Oidc { oidc: OidcClaims },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OidcClaims {
    sub: String,
    aud: String,
}

/// The receiver's signing key: generated per run, so no private key is committed.
struct Issuer {
    /// PKCS#1 PEM, for signing.
    pem:  Vec<u8>,
    /// The public key as a JWK set.
    jwks: Value,
}

/// The key id the receiver publishes and signs with.
const ISSUER_KID: &str = "capability-fixture";

impl Issuer {
    fn generate() -> Self {
        use base64::Engine as _;
        let rsa = openssl::rsa::Rsa::generate(2048).unwrap();
        let b64 = |bytes: Vec<u8>| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let jwks = serde_json::json!({ "keys": [{
            "kty": "RSA", "use": "sig", "alg": "RS256", "kid": ISSUER_KID,
            "n": b64(rsa.n().to_vec()), "e": b64(rsa.e().to_vec()),
        }]});
        Self {
            pem: rsa.private_key_to_pem().unwrap(),
            jwks,
        }
    }
}

impl Auth {
    fn bearer(&self, receiver: &Receiver) -> Result<String, String> {
        let now = || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .map_err(|e| e.to_string())
        };
        match self {
            Self::Token(token) => Ok(token.clone()),
            Self::Oidc { oidc } => {
                let now = now()?;
                let claims = serde_json::json!({
                    "sub": oidc.sub, "iss": receiver.url, "aud": oidc.aud,
                    "iat": now, "exp": now + 3600,
                });
                let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
                header.kid = Some(ISSUER_KID.to_owned());
                let key = jsonwebtoken::EncodingKey::from_rsa_pem(&receiver.issuer.pem)
                    .map_err(|e| format!("auth.oidc: {e}"))?;
                jsonwebtoken::encode(&header, &claims, &key).map_err(|e| format!("auth.oidc: {e}"))
            },
        }
    }
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
        auth:   Option<Auth>,
        /// An HMAC-SHA256 signature of `body`, sent in a header, as a webhook provider
        /// signs a delivery.
        #[serde(default)]
        sign:   Option<Signature>,
        /// The status the server must answer it with. Unset, any answer will do: the effect
        /// judges the fixture.
        #[serde(default)]
        status: Option<u16>,
    },
    /// One MCP `tools/call` over streamable HTTP, after the session handshake an agent
    /// client performs (`initialize`, then `notifications/initialized`).
    Mcp {
        path:      String,
        tool:      String,
        #[serde(default)]
        arguments: Value,
        /// A bearer token sent as `Authorization` on every request of the session.
        #[serde(default)]
        auth:      Option<Auth>,
    },
    /// A GraphQL subscription over `graphql-transport-ws`, held open until the fixture ends:
    /// `connection_init`, its `connection_ack`, then `subscribe` with `query`.
    Subscribe {
        path:  String,
        query: String,
    },
    /// One message submitted to the test mail server over SMTP (`GREENMAIL_SMTP_PORT`).
    /// `$RUN_ID` expands in `subject`.
    Smtp {
        from:    String,
        to:      String,
        subject: String,
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
    /// `method path` (a `GET` unless named, with `body` and the bearer `auth` when given)
    /// answers `status`, with `equals` at `json_pointer` when one is given.
    Http {
        #[serde(default = "default_method")]
        method:       String,
        path:         String,
        #[serde(default)]
        body:         Option<String>,
        #[serde(default)]
        auth:         Option<Auth>,
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
    /// At least `count` messages whose subject starts with `subject_prefix` are on the
    /// NATS `JetStream` stream `stream`. The harness deletes the stream before booting, so
    /// a message an earlier run left cannot satisfy it.
    Nats {
        stream:         String,
        subject_prefix: String,
        count:          usize,
    },
    /// At least `count` messages naming `subscription` are on the run's `$KAFKA_TOPIC`; with
    /// `data`, JSON text, one of them carries exactly that as its `data`.
    Kafka {
        subscription: String,
        count:        usize,
        #[serde(default)]
        data:         Option<String>,
    },
    /// The harness's receiver (`$RECEIVER_URL`) was sent at least `count` requests for
    /// `path`, as an observer's webhook action sends them.
    Received { path: String, count: usize },
    /// The server refuses to serve: it exits before answering, saying `message`.
    Refused { message: String },
    /// Over Arrow Flight, a handshake presenting `auth`, then a `do_get` of `ticket` with the
    /// session it returned, streams exactly `rows` rows.
    Flight {
        ticket: String,
        auth:   Auth,
        rows:   usize,
    },
    /// `of` does not appear within the deadline: a hostile fixture's "and nothing followed".
    ///
    /// Judged by [`absence_verdict`]: a probe that fails, rather than finding `of` not there
    /// yet, fails the fixture, and so does a server that exits. The capability must also have
    /// a positive fixture whose effect is of `of`'s kind ([`missing_twins`]), which shows the
    /// probe can see it.
    #[serde(rename = "none")]
    Never { of: Box<Effect> },
    /// `/health` reports the capability `started` and `serving` as given.
    Health {
        capability: String,
        #[serde(default)]
        started:    Option<bool>,
        #[serde(default)]
        serving:    Option<bool>,
    },
}

fn default_method() -> String {
    "GET".to_owned()
}

impl Stimulus {
    /// Whether MCP over stdio carries it: it has no HTTP, and no header to carry a bearer in.
    const fn over_stdio(&self) -> bool {
        match self {
            Self::Mcp { auth, .. } => auth.is_none(),
            Self::Smtp { .. } | Self::Sql { .. } | Self::Wait { .. } => true,
            Self::Http { .. } | Self::Subscribe { .. } => false,
        }
    }
}

impl Effect {
    /// Whether it can be observed of a server that serves MCP over stdio and nothing else.
    fn over_stdio(&self) -> bool {
        match self {
            Self::Row { .. }
            | Self::Nats { .. }
            | Self::Kafka { .. }
            | Self::Received { .. }
            | Self::Refused { .. } => true,
            Self::Never { of } => of.over_stdio(),
            Self::Http { .. } | Self::Flight { .. } | Self::Health { .. } => false,
        }
    }

    /// The effect a probe looks for: `of` for a `none`, the effect itself otherwise.
    fn probed(&self) -> &Self {
        match self {
            Self::Never { of } => of,
            other => other,
        }
    }

    /// Its key in `fixture.toml`.
    const fn kind(&self) -> &'static str {
        match self {
            Self::Http { .. } => "http",
            Self::Row { .. } => "row",
            Self::Nats { .. } => "nats",
            Self::Kafka { .. } => "kafka",
            Self::Received { .. } => "received",
            Self::Refused { .. } => "refused",
            Self::Flight { .. } => "flight",
            Self::Never { .. } => "none",
            Self::Health { .. } => "health",
        }
    }
}

/// Whether a stimulus answered with `got` was answered as its fixture says.
fn answered(path: &str, got: u16, want: Option<u16>) -> Result<(), String> {
    match want {
        Some(want) if got != want => {
            Err(format!("stimulus {path} answered {got}, not the {want} the fixture expects"))
        },
        _ => Ok(()),
    }
}

/// The verdict on a `none` effect after one probe of the effect it names, or `None` to
/// probe again. `past_deadline` is read before the probe, so the last probe ran after it.
fn absence_verdict(probe: Result<(), Unmet>, past_deadline: bool) -> Option<Result<(), String>> {
    match probe {
        Ok(()) => Some(Err("the effect the fixture refuses appeared".to_owned())),
        Err(Unmet::Broken(why)) => {
            Some(Err(format!("the probe failed, so finding nothing proves nothing: {why}")))
        },
        Err(Unmet::NotYet(_)) if past_deadline => Some(Ok(())),
        Err(Unmet::NotYet(_)) => None,
    }
}

/// Each `none` effect that cannot be trusted: one of a kind no probe looks for, or one whose
/// capability has no positive fixture with an effect of that kind to show the probe sees it.
fn missing_twins(fixtures: &[(String, Fixture)]) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, fixture) in fixtures {
        let Effect::Never { of } = &fixture.effect else {
            continue;
        };
        if matches!(**of, Effect::Never { .. } | Effect::Refused { .. }) {
            failures.push(format!(
                "fixture {name}: `none` of `{}` names nothing a probe looks for",
                of.kind()
            ));
            continue;
        }
        let twinned = fixtures.iter().any(|(_, other)| {
            other.capability == fixture.capability
                && other.role == Role::Positive
                && other.effect.kind() == of.kind()
        });
        if !twinned {
            failures.push(format!(
                "fixture {name}: `none` of `{}`, and no positive {} fixture has a `{}` effect \
                 to show that probe sees one",
                of.kind(),
                fixture.capability,
                of.kind()
            ));
        }
    }
    failures
}

impl Fixture {
    /// The entry points this fixture is driven through: each one its capability declares
    /// whose transport carries its stimuli and its effect. A capability that declares none
    /// is judged on the binary, which must say it did not start it.
    fn entries(&self) -> Vec<Entry> {
        let declared: Vec<Entry> = Entry::ALL
            .into_iter()
            .filter(|entry| entry.declared_by(&self.capability))
            .collect();
        if declared.is_empty() {
            return vec![Entry::Binary];
        }
        declared
            .into_iter()
            .filter(|entry| {
                *entry != Entry::McpStdio
                    || (self.stimulus.iter().all(Stimulus::over_stdio) && self.effect.over_stdio())
            })
            .collect()
    }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/capabilities")
}

/// Every fixture directory, by name.
fn fixtures(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(dir).map_or_default(|entries| {
        entries
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.join("fixture.toml").is_file())
            .map(|path| (path.file_name().unwrap().to_string_lossy().into_owned(), path))
            .collect()
    });
    found.sort();
    found
}

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

/// A fresh `db`, prepared as a deployment prepares one: `fraiseql setup`, then `setup.sql`.
async fn prepare_database(
    admin_url: &str,
    db: &str,
    setup_sql: &str,
    fraiseql_setup: bool,
) -> Result<String, String> {
    let admin = PgPool::connect(admin_url).await.map_err(|e| format!("connect: {e}"))?;
    for statement in [
        format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"),
        format!("CREATE DATABASE {db}"),
    ] {
        // Reason: test SQL: names and values this test defines, or DDL the code under test
        // generated.
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&admin)
            .await
            .map_err(|e| format!("{statement}: {e}"))?;
    }
    admin.close().await;
    let url = with_database(admin_url, db);
    if fraiseql_setup {
        fraiseql_cli::commands::setup::run(
            Some(&url),
            false,
            &fraiseql_cli::output::OutputFormatter::new(false, true),
        )
        .await
        .map_err(|e| format!("fraiseql setup: {e:#}"))?;
    }
    let pool = PgPool::connect(&url).await.map_err(|e| format!("connect to {db}: {e}"))?;
    // Reason: the fixture's own setup script.
    sqlx::raw_sql(sqlx::AssertSqlSafe(setup_sql))
        .execute(&pool)
        .await
        .map_err(|e| format!("setup.sql: {e}"))?;
    pool.close().await;
    Ok(url)
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

/// Compile the fixture's authoring inputs to `out`, as `fraiseql compile` does:
/// `schema.json` with `fraiseql.toml` beside it, or `fraiseql.toml` alone.
///
/// `$FIXTURE` in `fraiseql.toml` expands to the fixture's directory, for a path the
/// compiled schema carries to the server, which runs in an empty directory of its own. The
/// inputs are written beside `out`, where `fraiseql run` is pointed at them.
async fn compile(dir: &Path, out: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(dir.join("fraiseql.toml"))
        .map_err(|e| format!("fraiseql.toml: {e}"))?;
    let project = out.with_file_name("fraiseql.toml");
    std::fs::write(&project, source.replace("$FIXTURE", &dir.to_string_lossy()))
        .map_err(|e| format!("{}: {e}", project.display()))?;
    let schema = out.with_file_name("schema.json");
    let (input, config) = if dir.join("schema.json").is_file() {
        std::fs::copy(dir.join("schema.json"), &schema).map_err(|e| format!("schema.json: {e}"))?;
        (schema, Some(project.to_string_lossy().into_owned()))
    } else {
        (project, None)
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

/// The broker a fixture that needs NATS is given (`NATS_URL`).
fn nats_url() -> Result<String, String> {
    std::env::var("NATS_URL")
        .map_err(|_| "this fixture needs NATS, and NATS_URL is not set".to_owned())
}

/// The broker a fixture that needs Kafka is given (`KAFKA_BOOTSTRAP`, `host:port`).
fn kafka_bootstrap() -> Result<String, String> {
    std::env::var("KAFKA_BOOTSTRAP")
        .map_err(|_| "this fixture needs Kafka, and KAFKA_BOOTSTRAP is not set".to_owned())
}

/// Create `topic` before the server boots: a consumer of a topic that does not exist yet
/// fails, which would read as "nothing was published".
async fn create_topic(topic: &str) -> Result<(), String> {
    use fraiseql_kafka::rdkafka::{
        ClientConfig,
        admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
        client::DefaultClientContext,
    };
    let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", kafka_bootstrap()?)
        .create()
        .map_err(|e| format!("Kafka admin client: {e}"))?;
    let created = admin
        .create_topics(&[NewTopic::new(topic, 1, TopicReplication::Fixed(1))], &AdminOptions::new())
        .await
        .map_err(|e| format!("create topic {topic}: {e}"))?;
    for result in created {
        result.map_err(|(topic, code)| format!("create topic {topic}: {code}"))?;
    }
    Ok(())
}

/// Every message on `topic`, as JSON, read from the start by a consumer of its own.
async fn topic_messages(topic: &str) -> Result<Vec<Value>, String> {
    use fraiseql_kafka::rdkafka::{
        ClientConfig, Message as _,
        consumer::{Consumer as _, StreamConsumer},
    };
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", kafka_bootstrap()?)
        .set("group.id", format!("capability-fixture-{}", uuid::Uuid::new_v4()))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()
        .map_err(|e| format!("Kafka consumer: {e}"))?;
    consumer.subscribe(&[topic]).map_err(|e| format!("subscribe to {topic}: {e}"))?;
    let mut messages = Vec::new();
    // Until the topic has been quiet for a while: the first poll also waits out the group join.
    while let Ok(received) = tokio::time::timeout(Duration::from_secs(3), consumer.recv()).await {
        let Ok(message) = received else { continue };
        messages.push(
            message
                .payload()
                .map_or(Value::Null, |bytes| serde_json::from_slice(bytes).unwrap_or(Value::Null)),
        );
    }
    Ok(messages)
}

/// The test mail server a fixture that needs one is given, by `GREENMAIL_<key>`.
fn greenmail(key: &str) -> Result<String, String> {
    std::env::var(format!("GREENMAIL_{key}")).map_err(|_| {
        format!("this fixture needs the test mail server, and GREENMAIL_{key} is not set")
    })
}

/// Submit one message over SMTP, as a sending server delivers it: no TLS and no
/// authentication, which a test mail server accepts.
async fn submit_smtp(from: &str, to: &str, subject: &str) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    let address = format!("{}:{}", greenmail("HOST")?, greenmail("SMTP_PORT")?);
    let stream = tokio::net::TcpStream::connect(&address)
        .await
        .map_err(|e| format!("SMTP connect {address}: {e}"))?;
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let message = format!(
        "From: <{from}>\r\nTo: <{to}>\r\nSubject: {subject}\r\nMessage-ID: <{}@capability-fixture>\r\n\
         \r\nDelivered by the capability fixture harness.\r\n.\r\n",
        uuid::Uuid::new_v4()
    );
    // The greeting, then each command's reply, multi-line or not, by its code.
    for (command, code) in [
        (None, "220"),
        (Some("EHLO capability-fixture\r\n".to_owned()), "250"),
        (Some(format!("MAIL FROM:<{from}>\r\n")), "250"),
        (Some(format!("RCPT TO:<{to}>\r\n")), "250"),
        (Some("DATA\r\n".to_owned()), "354"),
        (Some(message), "250"),
        (Some("QUIT\r\n".to_owned()), "221"),
    ] {
        if let Some(command) = command {
            write
                .write_all(command.as_bytes())
                .await
                .map_err(|e| format!("SMTP write: {e}"))?;
        }
        loop {
            let line = lines
                .next_line()
                .await
                .map_err(|e| format!("SMTP read: {e}"))?
                .ok_or("the SMTP server closed the connection")?;
            if !line.starts_with(code) {
                return Err(format!("SMTP answered {line:?}, not {code}"));
            }
            if line.as_bytes().get(3) != Some(&b'-') {
                break;
            }
        }
    }
    Ok(())
}

/// An HTTP endpoint of the harness's own, at `$RECEIVER_URL`, recording the path of every
/// request it is sent: where a fixture points an observer's webhook.
///
/// It is also an OIDC issuer at its own URL: `/.well-known/openid-configuration` names
/// `/jwks`, which publishes the key an `oidc` bearer is signed with.
struct Receiver {
    url:    String,
    paths:  Arc<Mutex<Vec<String>>>,
    issuer: Issuer,
}

impl Receiver {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let paths = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&paths);
        let issuer = Issuer::generate();
        let discovery = serde_json::json!({
            "issuer": url,
            "jwks_uri": format!("{url}/jwks"),
            "authorization_endpoint": format!("{url}/authorize"),
            "token_endpoint": format!("{url}/token"),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
        });
        let jwks = issuer.jwks.clone();
        let app = axum::Router::new()
            .route(
                "/.well-known/openid-configuration",
                axum::routing::get(move || async move { axum::Json(discovery) }),
            )
            .route("/jwks", axum::routing::get(move || async move { axum::Json(jwks) }))
            .fallback(move |uri: axum::http::Uri| {
                let seen = Arc::clone(&seen);
                async move {
                    seen.lock().unwrap().push(uri.path().to_owned());
                    axum::http::StatusCode::OK
                }
            });
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self { url, paths, issuer }
    }

    /// `text` with `$RECEIVER_URL` expanded.
    fn expand(&self, text: &str) -> String {
        text.replace("$RECEIVER_URL", &self.url)
    }
}

/// The fixture's `server.toml` with `$FIXTURE`, `$NATS_URL`, `$RECEIVER_URL`,
/// `$KAFKA_BOOTSTRAP`, `$KAFKA_TOPIC` and `$GREENMAIL_*` expanded, written to `scratch`.
async fn render_server_toml(
    dir: &Path,
    scratch: &Path,
    receiver: &Receiver,
    kafka_topic: &str,
) -> Result<PathBuf, String> {
    let mut text = std::fs::read_to_string(dir.join("server.toml")).map_err(|e| e.to_string())?;
    text = receiver.expand(&text.replace("$FIXTURE", &dir.to_string_lossy()));
    if text.contains("$NATS_URL") {
        text = text.replace("$NATS_URL", &nats_url()?);
    }
    if text.contains("$KAFKA_BOOTSTRAP") {
        text = text.replace("$KAFKA_BOOTSTRAP", &kafka_bootstrap()?);
    }
    // The port is a TOML integer: it is written quoted, so `server.toml` stays valid TOML, and
    // it expands with its quotes.
    for (key, placeholder) in [
        ("HOST", "$GREENMAIL_HOST"),
        ("IMAPS_PORT", "\"$GREENMAIL_IMAPS_PORT\""),
        ("CA", "$GREENMAIL_CA"),
    ] {
        if text.contains(placeholder) {
            text = text.replace(placeholder, &greenmail(key)?);
        }
    }
    if text.contains("$KAFKA_TOPIC") {
        create_topic(kafka_topic).await?;
        text = text.replace("$KAFKA_TOPIC", kafka_topic);
    }
    let rendered = scratch.join("server.toml");
    std::fs::write(&rendered, text).map_err(|e| e.to_string())?;
    Ok(rendered)
}

/// Delete the stream a `nats` effect reads, so it starts empty.
async fn reset_stream(stream: &str) -> Result<(), String> {
    let client = async_nats::connect(nats_url()?)
        .await
        .map_err(|e| format!("connect to NATS: {e}"))?;
    let _ = async_nats::jetstream::new(client).delete_stream(stream).await;
    Ok(())
}

/// The subjects of every message on `stream`, read through a fresh consumer.
async fn stream_subjects(stream: &str) -> Result<Vec<String>, String> {
    use futures::StreamExt as _;
    let client = async_nats::connect(nats_url()?)
        .await
        .map_err(|e| format!("connect to NATS: {e}"))?;
    let consumer = async_nats::jetstream::new(client)
        .create_consumer_on_stream(
            async_nats::jetstream::consumer::pull::Config::default(),
            stream.to_owned(),
        )
        .await
        .map_err(|e| format!("stream {stream}: {e}"))?;
    let mut messages = consumer.messages().await.map_err(|e| e.to_string())?;
    let mut subjects = Vec::new();
    while let Ok(Some(Ok(message))) =
        tokio::time::timeout(Duration::from_millis(500), messages.next()).await
    {
        subjects.push(message.subject.to_string());
    }
    Ok(subjects)
}

/// MCP over a server's stdin and stdout, read as an MCP client reads it: one JSON-RPC
/// message per line.
struct StdioChannel {
    stdin:   Mutex<ChildStdin>,
    /// Every line the server wrote to stdout, in order.
    lines:   Arc<Mutex<Vec<String>>>,
    next_id: AtomicU64,
}

impl StdioChannel {
    /// Read `stdout` on a thread of its own, copying each line to `log`.
    fn open(stdin: ChildStdin, stdout: std::process::ChildStdout, log: &Path) -> Self {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&lines);
        let mut copy = std::fs::File::create(log).unwrap();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = writeln!(copy, "{line}");
                seen.lock().unwrap().push(line);
            }
        });
        Self {
            stdin: Mutex::new(stdin),
            lines,
            next_id: AtomicU64::new(1),
        }
    }

    fn send(&self, message: &Value) -> Result<(), String> {
        let mut stdin = self.stdin.lock().unwrap();
        writeln!(stdin, "{message}")
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("write to the server's stdin: {e}"))
    }

    /// Send a request, and return its id.
    fn request(&self, method: &str, params: &Value) -> Result<u64, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.send(
            &serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )?;
        Ok(id)
    }

    /// The answer to request `id`, if it has come. Fails on any line that is not a JSON-RPC
    /// message: a client cannot read past one.
    fn answer(&self, id: u64) -> Result<Option<Value>, String> {
        let mut found = None;
        for line in self.lines.lock().unwrap().iter() {
            let message: Value = serde_json::from_str(line).unwrap_or(Value::Null);
            if message["jsonrpc"] != "2.0" {
                return Err(format!(
                    "the server wrote a line to stdout that is not a JSON-RPC message, which \
                     an MCP client cannot read: {line:?}"
                ));
            }
            if message["id"] == id {
                found = Some(message);
            }
        }
        Ok(found)
    }

    /// `initialize`, as a client opens a session; the server is ready once it answers.
    fn initialize(&self) -> Result<u64, String> {
        self.request(
            "initialize",
            &serde_json::json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "capability-fixture", "version": "0" }
            }),
        )
    }
}

/// A served fixture: the server process, and where its output goes.
struct Served {
    child:         Child,
    base:          String,
    /// Where Arrow Flight listens.
    flight:        String,
    log:           PathBuf,
    stderr:        PathBuf,
    /// MCP over the server's stdin and stdout, on the `mcp_stdio` entry point.
    stdio:         Option<StdioChannel>,
    receiver:      Receiver,
    /// This run's `$KAFKA_TOPIC`.
    kafka_topic:   String,
    /// This run's `$RUN_ID`.
    run_id:        String,
    /// The `subscribe` stimuli's connections, held open until the fixture ends.
    subscriptions: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Served {
    fn log_tail(&self) -> String {
        let tail = |path: &Path| {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(40)..].join("\n")
        };
        format!("{}\n--- stderr ---\n{}", tail(&self.log), tail(&self.stderr))
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
        for subscription in self.subscriptions.lock().unwrap().drain(..) {
            subscription.abort();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Two ports free now, and distinct: both listeners are held until both are known, so the
/// second cannot be handed the port the first just released.
fn free_ports() -> (u16, u16) {
    let bind = || std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let (first, second) = (bind(), bind());
    (first.local_addr().unwrap().port(), second.local_addr().unwrap().port())
}

/// The environment variable naming the entry point a re-executed child serves through.
const CHILD_ENTRY: &str = "CAPABILITY_FIXTURE_CHILD_ENTRY";

/// The environment variable naming the `fraiseql-cli` binary, built with `run-server`, that the
/// `run` entry point serves through. Unset, a fixture driven through `run` fails, naming it.
const RUN_BIN: &str = "FRAISEQL_CAPABILITY_RUN_BIN";

/// The process that serves through `entry`, on `bind`.
fn server_command(entry: Entry, scratch: &Path, bind: &str) -> Result<Command, String> {
    match entry {
        Entry::Binary => Ok(Command::new(env!("CARGO_BIN_EXE_fraiseql-server"))),
        Entry::McpStdio => {
            let mut command = Command::new(env!("CARGO_BIN_EXE_fraiseql-server"));
            command.env("FRAISEQL_MCP_STDIO", "1");
            Ok(command)
        },
        Entry::Listener => {
            let mut command = Command::new(
                std::env::current_exe().map_err(|e| format!("this test binary's path: {e}"))?,
            );
            command
                .args([
                    "serve_a_fixture_through_an_in_process_entry_point",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD_ENTRY, entry.name());
            Ok(command)
        },
        Entry::Run => {
            let binary = std::env::var_os(RUN_BIN).ok_or_else(|| {
                format!(
                    "{RUN_BIN} is not set: it names the `fraiseql-cli` binary built with \
                     `run-server` (`cargo build -p fraiseql-cli --features run-server`) that the \
                     `run` entry point serves through"
                )
            })?;
            // What `fraiseql run` is pointed at: the authoring input, with the
            // `fraiseql.toml` `compile` wrote beside it.
            let schema = scratch.join("schema.json");
            let input = if schema.is_file() {
                schema
            } else {
                scratch.join("fraiseql.toml")
            };
            let (host, port) =
                bind.rsplit_once(':').ok_or_else(|| format!("bind address {bind}"))?;
            let mut command = Command::new(binary);
            command.arg("run").arg(input).args(["--port", port, "--bind", host]);
            Ok(command)
        },
    }
}

/// Serve the fixture through `entry`, and wait for its first answer.
#[allow(clippy::too_many_arguments)] // Reason: one fixture run's inputs, each read once
async fn serve(
    entry: Entry,
    dir: &Path,
    fixture: &Fixture,
    schema: &Path,
    database_url: &str,
    scratch: &Path,
    receiver: Receiver,
) -> Result<Served, String> {
    let (port, flight_port) = free_ports();
    let kafka_topic = format!("capability-fixture-{}", uuid::Uuid::new_v4());
    let log = scratch.join("server.log");
    let stderr = scratch.join("server.stderr");
    let cwd = scratch.join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    let bind = format!("127.0.0.1:{port}");
    let mut command = server_command(entry, scratch, &bind)?;
    command
        .current_dir(&cwd)
        .env(
            "FRAISEQL_CONFIG",
            render_server_toml(dir, scratch, &receiver, &kafka_topic).await?,
        )
        .env("FRAISEQL_SCHEMA_PATH", schema)
        .env("DATABASE_URL", database_url)
        .env("FRAISEQL_BIND_ADDR", &bind)
        // Flight binds a port of its own, by default the one every fixture's server would
        // share, so a server already on it fails a fixture about something else.
        .env("FRAISEQL_FLIGHT_BIND_ADDR", format!("127.0.0.1:{flight_port}"))
        .env("RUST_LOG", "info")
        .stderr(std::fs::File::create(&stderr).unwrap());
    if entry == Entry::McpStdio {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
    } else {
        command.stdin(Stdio::null()).stdout(std::fs::File::create(&log).unwrap());
    }
    for (key, value) in &fixture.env {
        command.env(key, value.replace("$FIXTURE", &dir.to_string_lossy()));
    }
    let mut child =
        command.spawn().map_err(|e| format!("spawn the {} server: {e}", entry.name()))?;
    let stdio = match (child.stdin.take(), child.stdout.take()) {
        (Some(stdin), Some(stdout)) => Some(StdioChannel::open(stdin, stdout, &log)),
        _ => None,
    };
    let mut served = Served {
        child,
        base: format!("http://127.0.0.1:{port}"),
        flight: format!("http://127.0.0.1:{flight_port}"),
        log,
        stderr,
        stdio,
        receiver,
        kafka_topic,
        run_id: uuid::Uuid::new_v4().simple().to_string(),
        subscriptions: Mutex::new(Vec::new()),
    };
    let initialize = served.stdio.as_ref().map(StdioChannel::initialize).transpose()?;
    let started = Instant::now();
    loop {
        if let Err(exited) = served.still_running() {
            // All of stderr: a refusal is judged by its message, which a backtrace
            // (`RUST_BACKTRACE`, set in CI) follows.
            let stderr = std::fs::read_to_string(&served.stderr).unwrap_or_default();
            let log = std::fs::read_to_string(&served.log).unwrap_or_default();
            let lines: Vec<&str> = log.lines().collect();
            return Err(format!(
                "{exited} before serving:\n{}\n--- stderr ---\n{stderr}",
                lines[lines.len().saturating_sub(40)..].join("\n")
            ));
        }
        let ready = match (&served.stdio, initialize) {
            (Some(stdio), Some(id)) => stdio.answer(id)?.is_some(),
            _ => reqwest::get(format!("{}/health", served.base)).await.is_ok(),
        };
        if ready {
            if let Some(stdio) = &served.stdio {
                stdio.send(
                    &serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                )?;
            }
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
            status: want,
        } => {
            let method = reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|e| format!("method {method}: {e}"))?;
            let mut request = reqwest::Client::new()
                .request(method, format!("{}{path}", served.base))
                .header("content-type", "application/json");
            if let Some(auth) = auth {
                request = request.bearer_auth(auth.bearer(&served.receiver)?);
            }
            if let Some(signature) = sign {
                request = request
                    .header(signature.header.as_str(), signature.of(body.as_deref().unwrap_or("")));
            }
            if let Some(body) = body {
                request = request.body(served.receiver.expand(body));
            }
            let response = request.send().await.map_err(|e| format!("{path}: {e}"))?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            eprintln!("stimulus {path}: {status} {text}");
            answered(path, status.as_u16(), *want)
        },
        Stimulus::Mcp {
            tool, arguments, ..
        } if served.stdio.is_some() => {
            let stdio = served.stdio.as_ref().unwrap();
            let id = stdio.request(
                "tools/call",
                &serde_json::json!({ "name": tool, "arguments": arguments }),
            )?;
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                if let Some(answer) = stdio.answer(id)? {
                    eprintln!("stimulus mcp {tool} over stdio: {answer}");
                    return Ok(());
                }
                if Instant::now() > deadline {
                    return Err(format!("mcp tools/call {tool}: no answer on stdout within 30s"));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        },
        Stimulus::Mcp {
            path,
            tool,
            arguments,
            auth,
        } => {
            let bearer = auth.as_ref().map(|auth| auth.bearer(&served.receiver)).transpose()?;
            let client = reqwest::Client::new();
            let url = format!("{}{path}", served.base);
            let post = |body: Value, session: Option<&str>| {
                let mut request = client
                    .post(&url)
                    .header("accept", "application/json, text/event-stream")
                    .json(&body);
                if let Some(token) = &bearer {
                    request = request.bearer_auth(token);
                }
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
        Stimulus::Subscribe { path, query } => {
            use futures::{SinkExt as _, StreamExt as _};
            use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest as _};

            let url = format!("{}{path}", served.base.replacen("http://", "ws://", 1));
            let mut request = url.as_str().into_client_request().map_err(|e| e.to_string())?;
            request.headers_mut().insert(
                "sec-websocket-protocol",
                axum::http::HeaderValue::from_static("graphql-transport-ws"),
            );
            let (mut socket, _) = tokio_tungstenite::connect_async(request)
                .await
                .map_err(|e| format!("subscribe {path}: {e}"))?;
            let text = |value: Value| Message::Text(value.to_string().into());
            socket
                .send(text(serde_json::json!({ "type": "connection_init", "payload": {} })))
                .await
                .map_err(|e| format!("connection_init: {e}"))?;
            loop {
                let reply = tokio::time::timeout(Duration::from_secs(10), socket.next())
                    .await
                    .map_err(|_| "no connection_ack within 10s".to_owned())?
                    .ok_or("the subscription socket closed before connection_ack")?
                    .map_err(|e| format!("connection_init: {e}"))?;
                if let Message::Text(reply) = reply {
                    let reply: Value = serde_json::from_str(&reply).unwrap_or(Value::Null);
                    if reply["type"] == "connection_ack" {
                        break;
                    }
                    return Err(format!("connection_init answered {reply}"));
                }
            }
            socket
                .send(text(serde_json::json!({
                    "type": "subscribe", "id": "fixture",
                    "payload": { "query": query, "variables": {} },
                })))
                .await
                .map_err(|e| format!("subscribe: {e}"))?;
            // Read what arrives, so the connection stays open and its frames are logged.
            served.subscriptions.lock().unwrap().push(tokio::spawn(async move {
                while let Some(Ok(frame)) = socket.next().await {
                    eprintln!("subscription frame: {frame}");
                }
            }));
            Ok(())
        },
        Stimulus::Smtp { from, to, subject } => {
            submit_smtp(from, to, &subject.replace("$RUN_ID", &served.run_id)).await
        },
        Stimulus::Sql { statement } => {
            let pool = PgPool::connect(database_url).await.map_err(|e| format!("connect: {e}"))?;
            // Reason: test SQL: names and values this test defines, or DDL the code under test
            // generated.
            sqlx::raw_sql(sqlx::AssertSqlSafe(served.receiver.expand(statement)))
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
async fn observe(served: &Served, database_url: &str, effect: &Effect) -> Result<(), Unmet> {
    use Unmet::{Broken, NotYet};
    match effect {
        Effect::Http {
            method,
            path,
            body,
            auth,
            status,
            json_pointer,
            equals,
        } => {
            let verb = reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|e| Broken(format!("method {method}: {e}")))?;
            let mut request = reqwest::Client::new()
                .request(verb, format!("{}{path}", served.base))
                .header("content-type", "application/json");
            if let Some(auth) = auth {
                request = request.bearer_auth(auth.bearer(&served.receiver).map_err(Broken)?);
            }
            if let Some(body) = body {
                request = request.body(body.clone());
            }
            let response =
                request.send().await.map_err(|e| Broken(format!("{method} {path}: {e}")))?;
            let got = response.status().as_u16();
            let body: Value = response.json().await.unwrap_or(Value::Null);
            if got != *status {
                return Err(NotYet(format!(
                    "{method} {path} answered {got}, not {status}: {body}"
                )));
            }
            match (json_pointer, equals) {
                (Some(pointer), Some(want)) if body.pointer(pointer) != Some(want) => {
                    Err(NotYet(format!(
                        "{method} {path}: {pointer} is {:?}, not {want}: {body}",
                        body.pointer(pointer)
                    )))
                },
                _ => Ok(()),
            }
        },
        Effect::Row { query, expect } => {
            observe_row(database_url, &served.run_id, query, expect).await
        },
        Effect::Nats {
            stream,
            subject_prefix,
            count,
        } => {
            let subjects = stream_subjects(stream).await.map_err(Broken)?;
            let matching =
                subjects.iter().filter(|s| s.starts_with(subject_prefix.as_str())).count();
            if matching >= *count {
                Ok(())
            } else {
                Err(NotYet(format!(
                    "{matching} message(s) under `{subject_prefix}` on {stream}, not {count}: \
                     {subjects:?}"
                )))
            }
        },
        Effect::Kafka {
            subscription,
            count,
            data,
        } => {
            let messages = topic_messages(&served.kafka_topic).await.map_err(Broken)?;
            let hits: Vec<&Value> =
                messages.iter().filter(|m| m["subscription_name"] == **subscription).collect();
            if hits.len() < *count {
                return Err(NotYet(format!(
                    "{} message(s) for {subscription} on {}, not {count}: {messages:?}",
                    hits.len(),
                    served.kafka_topic
                )));
            }
            let Some(data) = data else { return Ok(()) };
            let want: Value = serde_json::from_str(data)
                .map_err(|e| Broken(format!("kafka.data is not JSON: {e}")))?;
            if hits.iter().any(|m| m["data"] == want) {
                Ok(())
            } else {
                let got: Vec<&Value> = hits.iter().map(|m| &m["data"]).collect();
                Err(NotYet(format!("no {subscription} message carries data {want}: {got:?}")))
            }
        },
        Effect::Received { path, count } => {
            let paths = served.receiver.paths.lock().unwrap().clone();
            let hits = paths.iter().filter(|p| *p == path).count();
            if hits >= *count {
                Ok(())
            } else {
                Err(NotYet(format!(
                    "the receiver was sent {hits} request(s) for {path}, not {count}: {paths:?}"
                )))
            }
        },
        Effect::Refused { .. } => {
            Err(Broken("a refusal is judged before the server serves".to_owned()))
        },
        Effect::Never { .. } => Err(Broken("a `none` is judged by the runner".to_owned())),
        Effect::Flight { ticket, auth, rows } => {
            let bearer = auth.bearer(&served.receiver).map_err(Broken)?;
            let got = flight_rows(&served.flight, &bearer, ticket).await.map_err(Broken)?;
            if got == *rows {
                Ok(())
            } else {
                Err(NotYet(format!("Flight do_get {ticket} streamed {got} row(s), not {rows}")))
            }
        },
        Effect::Health {
            capability,
            started,
            serving,
        } => {
            let health: Value = reqwest::get(format!("{}/health", served.base))
                .await
                .map_err(|e| Broken(format!("/health: {e}")))?
                .json()
                .await
                .map_err(|e| Broken(format!("/health: {e}")))?;
            let reported = &health["capabilities"][capability];
            let started_ok = started.is_none_or(|want| reported["started"] == want);
            let serving_ok = serving
                .is_none_or(|want| reported.get("serving").and_then(Value::as_bool) == Some(want));
            if started_ok && serving_ok {
                Ok(())
            } else {
                Err(NotYet(format!("/health reports {capability}: {reported}")))
            }
        },
    }
}

/// `query`, with `$RUN_ID` expanded, returns exactly `expect`. A query that does not run, on
/// a database that does not answer, is [`Unmet::Broken`]: an empty `expect` must not pass on
/// a misspelt table.
async fn observe_row(
    database_url: &str,
    run_id: &str,
    query: &str,
    expect: &[Vec<String>],
) -> Result<(), Unmet> {
    let pool = PgPool::connect(database_url)
        .await
        .map_err(|e| Unmet::Broken(format!("connect: {e}")))?;
    let query = query.replace("$RUN_ID", run_id);
    // Reason: test SQL: names and values this test defines, or DDL the code under test
    // generated.
    let rows = sqlx::query(sqlx::AssertSqlSafe(query.as_str())).fetch_all(&pool).await;
    pool.close().await;
    let rows = rows.map_err(|e| Unmet::Broken(format!("`{query}`: {e}")))?;
    let got: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            (0..row.len())
                .map(|i| row.try_get::<String, _>(i).unwrap_or_else(|e| format!("<{e}>")))
                .collect()
        })
        .collect();
    if got == expect {
        Ok(())
    } else {
        Err(Unmet::NotYet(format!("`{query}` returned {got:?}, not {expect:?}")))
    }
}

/// The rows a Flight `do_get` of `ticket` streams, after a handshake presenting `bearer`, as
/// a Flight client reads them: the handshake answers a session token, which the `do_get`
/// presents.
#[cfg(feature = "arrow")]
async fn flight_rows(address: &str, bearer: &str, ticket: &str) -> Result<usize, String> {
    use fraiseql_arrow::{
        arrow_flight::{
            HandshakeRequest, Ticket, decode::FlightRecordBatchStream, error::FlightError,
            flight_service_client::FlightServiceClient,
        },
        tonic::{Request, transport::Endpoint},
    };
    use futures::{StreamExt as _, TryStreamExt as _};

    let channel = Endpoint::from_shared(address.to_owned())
        .map_err(|e| e.to_string())?
        .connect()
        .await
        .map_err(|e| format!("connect to Flight at {address}: {e}"))?;
    let mut client = FlightServiceClient::new(channel);
    let handshake = HandshakeRequest {
        protocol_version: 0,
        payload:          format!("Bearer {bearer}").into_bytes().into(),
    };
    let mut answers = client
        .handshake(futures::stream::iter([handshake]))
        .await
        .map_err(|status| format!("Flight handshake: {status}"))?
        .into_inner();
    let session = answers
        .next()
        .await
        .ok_or("the Flight handshake answered nothing")?
        .map_err(|status| format!("Flight handshake: {status}"))?
        .payload;
    let session = String::from_utf8(session.to_vec()).map_err(|e| e.to_string())?;

    let mut request = Request::new(Ticket {
        ticket: ticket.as_bytes().to_vec().into(),
    });
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {session}").parse().map_err(|e| format!("session token: {e}"))?,
    );
    let data = client
        .do_get(request)
        .await
        .map_err(|status| format!("Flight do_get: {status}"))?
        .into_inner()
        .map_err(FlightError::from);
    let batches: Vec<_> = FlightRecordBatchStream::new_from_flight_data(data)
        .try_collect()
        .await
        .map_err(|e| format!("Flight do_get stream: {e}"))?;
    Ok(batches.iter().map(|batch| batch.num_rows()).sum())
}

#[cfg(not(feature = "arrow"))]
async fn flight_rows(_address: &str, _bearer: &str, _ticket: &str) -> Result<usize, String> {
    Err("a `flight` effect needs the `arrow` feature".to_owned())
}

/// A fixture's `fixture.toml`.
fn read_fixture(dir: &Path) -> Result<Fixture, String> {
    toml::from_str(&std::fs::read_to_string(dir.join("fixture.toml")).map_err(|e| e.to_string())?)
        .map_err(|e| format!("fixture.toml: {e}"))
}

/// Boot the fixture in `dir` through each entry point it is driven through (or only
/// `only_entry`), and judge each by its effect.
async fn run_fixture(
    admin_url: &str,
    name: &str,
    dir: &Path,
    only_entry: Option<&str>,
) -> Result<Vec<(Entry, Result<(), String>)>, String> {
    let fixture = read_fixture(dir)?;
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
    let mut outcomes = Vec::new();
    for entry in fixture.entries() {
        if waiting(name, entry) || only_entry.is_some_and(|only| only != entry.name()) {
            continue;
        }
        let outcome = run_fixture_on(entry, admin_url, name, dir, &fixture, &schema).await;
        outcomes.push((entry, outcome));
    }
    Ok(outcomes)
}

/// Boot the fixture through `entry` on a fresh database, send its stimuli, and wait for its
/// effect.
async fn run_fixture_on(
    entry: Entry,
    admin_url: &str,
    name: &str,
    dir: &Path,
    fixture: &Fixture,
    schema: &Path,
) -> Result<(), String> {
    let scratch = tempfile::tempdir().unwrap();
    let db = format!("cap_{name}");
    let setup_sql = std::fs::read_to_string(dir.join("setup.sql")).unwrap();
    let database_url = prepare_database(admin_url, &db, &setup_sql, fixture.fraiseql_setup).await?;
    // `fraiseql run` is pointed at the authoring inputs `compile` wrote beside the schema.
    for input in ["fraiseql.toml", "schema.json"] {
        let source = schema.with_file_name(input);
        if source.is_file() {
            std::fs::copy(&source, scratch.path().join(input)).map_err(|e| e.to_string())?;
        }
    }

    if let Effect::Nats { stream, .. } = fixture.effect.probed() {
        reset_stream(stream).await?;
    }
    if let Effect::Refused { message } = &fixture.effect {
        let outcome = match serve(
            entry,
            dir,
            fixture,
            schema,
            &database_url,
            scratch.path(),
            Receiver::start().await,
        )
        .await
        {
            Ok(_) => Err(format!("the server served; it should have refused: {message}")),
            Err(refusal) if refusal.contains(message.as_str()) => Ok(()),
            Err(other) => Err(format!("the server failed without saying {message:?}: {other}")),
        };
        drop_database(admin_url, &db).await;
        return outcome;
    }
    let outcome = async {
        let mut served = serve(
            entry,
            dir,
            fixture,
            schema,
            &database_url,
            scratch.path(),
            Receiver::start().await,
        )
        .await?;
        for stimulus in &fixture.stimulus {
            send(&served, &database_url, stimulus).await?;
        }
        let deadline = Instant::now() + Duration::from_secs(fixture.deadline_secs);
        if let Effect::Never { of } = &fixture.effect {
            loop {
                served
                    .still_running()
                    .map_err(|exited| format!("{exited}:\n{}", served.log_tail()))?;
                let past_deadline = Instant::now() > deadline;
                let probe = observe(&served, &database_url, of).await;
                match absence_verdict(probe, past_deadline) {
                    Some(Ok(())) => break,
                    Some(Err(failure)) => {
                        return Err(format!(
                            "{failure}\n--- server log ---\n{}",
                            served.log_tail()
                        ));
                    },
                    None => tokio::time::sleep(Duration::from_millis(250)).await,
                }
            }
        } else {
            loop {
                served
                    .still_running()
                    .map_err(|exited| format!("{exited}:\n{}", served.log_tail()))?;
                match observe(&served, &database_url, &fixture.effect).await {
                    Ok(()) => break,
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
        // Everything the server wrote to stdout by now must still read as JSON-RPC.
        if let Some(stdio) = &served.stdio {
            stdio.answer(0)?;
        }
        Ok(())
    }
    .await;
    drop_database(admin_url, &db).await;
    outcome
}

/// Registrations with no fixture yet, each with the issue that blocks it. Shrink-only: the
/// gate below also fails when one of these gains a fixture, so the entry goes with it.
const NO_FIXTURE_YET: &[(&str, &str)] = &[];

/// The capability each fixture exercises, read from its `fixture.toml`.
fn exercised_capabilities() -> Vec<String> {
    fixtures(&fixtures_dir())
        .into_iter()
        .map(|(name, dir)| {
            read_fixture(&dir).unwrap_or_else(|e| panic!("fixture {name}: {e}")).capability
        })
        .collect()
}

/// Every registration is served by the binary and judged by its effect in at least one
/// fixture, so a capability cannot be registered and never booted. Needs no database.
#[test]
fn every_registration_has_a_boot_to_effect_fixture() {
    let exercised = exercised_capabilities();
    let mut failures = Vec::new();
    for entry in REGISTRY {
        let name = entry.meta.name;
        let has_fixture = exercised.iter().any(|capability| capability == name);
        match NO_FIXTURE_YET.iter().find(|(allowed, _)| *allowed == name) {
            None if !has_fixture => failures.push(format!(
                "{name} has no fixture under tests/capabilities/: add one whose fixture.toml \
                 names `capability = \"{name}\"`"
            )),
            Some((_, issue)) if has_fixture => failures.push(format!(
                "{name} has a fixture now: remove its NO_FIXTURE_YET entry ({issue})"
            )),
            _ => {},
        }
    }
    for (name, issue) in NO_FIXTURE_YET {
        if !REGISTRY.iter().any(|entry| entry.meta.name == *name) {
            failures
                .push(format!("NO_FIXTURE_YET names {name} ({issue}), which is not registered"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every entry point a capability declares drives at least one of its fixtures, or each of
/// its fixtures is listed in [`NOT_DRIVEN_YET`] for it, so a capability cannot declare a
/// serve path no fixture is served through. Needs no database.
#[test]
fn every_declared_entry_point_drives_a_fixture() {
    let fixtures: Vec<(String, Fixture)> = fixtures(&fixtures_dir())
        .into_iter()
        .map(|(name, dir)| {
            let fixture = read_fixture(&dir).unwrap_or_else(|e| panic!("fixture {name}: {e}"));
            (name, fixture)
        })
        .collect();
    let mut failures = Vec::new();
    for registration in REGISTRY {
        let capability = registration.meta.name;
        for entry in Entry::ALL.into_iter().filter(|entry| entry.declared_by(capability)) {
            let carried = fixtures
                .iter()
                .any(|(_, f)| f.capability == capability && f.entries().contains(&entry));
            if !carried {
                failures.push(format!(
                    "{capability} declares the {} entry point, and none of its fixtures can be \
                     driven through it: add one whose stimuli and effect it carries",
                    entry.name()
                ));
            }
        }
    }
    for (name, entry, issue) in NOT_DRIVEN_YET {
        match fixtures.iter().find(|(fixture, _)| fixture == name) {
            None => {
                failures.push(format!("NOT_DRIVEN_YET names {name} ({issue}): no such fixture"));
            },
            Some((_, fixture)) if !fixture.entries().contains(entry) => failures.push(format!(
                "NOT_DRIVEN_YET lists {name} on {} ({issue}), which it is not driven through \
                 anyway: remove the entry",
                entry.name()
            )),
            Some(_) => {},
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every `none` effect is one a probe can be trusted on: its capability has a positive
/// fixture with an effect of the same kind, which shows the probe sees one. Needs no
/// database.
#[test]
fn every_none_effect_has_a_positive_twin() {
    let fixtures: Vec<(String, Fixture)> = fixtures(&fixtures_dir())
        .into_iter()
        .map(|(name, dir)| {
            let fixture = read_fixture(&dir).unwrap_or_else(|e| panic!("fixture {name}: {e}"));
            (name, fixture)
        })
        .collect();
    let failures = missing_twins(&fixtures);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The rules [`absence_verdict`], [`missing_twins`], [`answered`] and [`observe_row`]
/// apply, on inputs written here: no fixture under `tests/capabilities/` exercises each
/// case.
mod harness_rules {
    use super::*;

    fn fixture(toml: &str) -> Fixture {
        toml::from_str(toml).unwrap_or_else(|e| panic!("{toml}: {e}"))
    }

    const ROW: &str = r#"row = { query = "SELECT 1", expect = [] }"#;

    #[test]
    fn an_effect_that_appears_fails_a_none() {
        assert!(matches!(absence_verdict(Ok(()), false), Some(Err(_))));
        assert!(matches!(absence_verdict(Ok(()), true), Some(Err(_))));
    }

    #[test]
    fn a_probe_that_fails_fails_a_none_before_its_deadline() {
        let probe = || Err(Unmet::Broken("connection refused".to_owned()));
        let verdict = absence_verdict(probe(), false);
        assert!(
            matches!(&verdict, Some(Err(why)) if why.contains("connection refused")),
            "{verdict:?}"
        );
        assert!(matches!(absence_verdict(probe(), true), Some(Err(_))));
    }

    #[test]
    fn a_none_waits_out_its_deadline_and_then_passes() {
        let probe = || Err(Unmet::NotYet("no rows".to_owned()));
        assert!(absence_verdict(probe(), false).is_none());
        assert!(matches!(absence_verdict(probe(), true), Some(Ok(()))));
    }

    #[test]
    fn a_none_needs_a_positive_fixture_of_its_kind_for_the_same_capability() {
        let hostile = fixture(&format!(
            "capability = \"rest\"\nrole = \"hostile\"\n[effect]\nnone = {{ of = {{ {ROW} }} }}"
        ));
        let positive_row = fixture(&format!("capability = \"rest\"\n[effect]\n{ROW}"));
        let positive_http =
            fixture("capability = \"rest\"\n[effect]\nhttp = { path = \"/health\", status = 200 }");
        let other_capability = fixture(&format!("capability = \"mcp\"\n[effect]\n{ROW}"));
        let hostile_row =
            fixture(&format!("capability = \"rest\"\nrole = \"hostile\"\n[effect]\n{ROW}"));
        let named = |list: Vec<Fixture>| -> Vec<(String, Fixture)> {
            list.into_iter().enumerate().map(|(i, f)| (format!("f{i}"), f)).collect()
        };

        let missing = missing_twins(&named(vec![hostile, positive_row]));
        assert!(missing.is_empty(), "{missing:?}");
        for unproven in [positive_http, other_capability, hostile_row] {
            let hostile = fixture(&format!(
                "capability = \"rest\"\nrole = \"hostile\"\n[effect]\nnone = {{ of = {{ {ROW} }} }}"
            ));
            let failures = missing_twins(&named(vec![hostile, unproven]));
            assert_eq!(failures.len(), 1, "{failures:?}");
        }
    }

    #[test]
    fn a_none_of_a_refusal_or_of_a_none_is_refused() {
        for of in [
            r#"refused = { message = "x" }"#,
            &format!("none = {{ of = {{ {ROW} }} }}"),
        ] {
            let hostile = fixture(&format!(
                "capability = \"rest\"\nrole = \"hostile\"\n[effect]\nnone = {{ of = {{ {of} }} }}"
            ));
            // A twin of `of`'s own kind, so only the shape rule can fail it.
            let twin = fixture(&format!("capability = \"rest\"\n[effect]\n{of}"));
            let failures: Vec<String> =
                missing_twins(&[("h".to_owned(), hostile), ("t".to_owned(), twin)])
                    .into_iter()
                    .filter(|failure| failure.starts_with("fixture h:"))
                    .collect();
            assert_eq!(failures.len(), 1, "{of}: {failures:?}");
            assert!(failures[0].contains("names nothing a probe looks for"), "{failures:?}");
        }
    }

    #[test]
    fn a_stimulus_answered_otherwise_than_stated_fails() {
        assert!(answered("/rest/v1/notes", 401, Some(401)).is_ok());
        assert!(answered("/rest/v1/notes", 201, Some(401)).is_err());
        assert!(answered("/rest/v1/notes", 201, None).is_ok());
    }

    #[tokio::test]
    async fn a_row_probe_on_a_database_that_does_not_answer_is_broken() {
        let probe =
            observe_row("postgres://postgres:pw@127.0.0.1:1/none", "", "SELECT 1", &[]).await;
        assert!(matches!(probe, Err(Unmet::Broken(_))), "{probe:?}");
    }

    /// The case a hostile fixture would pass on if a failed query read as no rows.
    #[tokio::test]
    async fn a_row_probe_whose_query_does_not_run_is_broken() {
        let Some(url) = try_database_url() else {
            eprintln!("SKIP a_row_probe_whose_query_does_not_run_is_broken: DATABASE_URL not set");
            return;
        };
        let probe = observe_row(&url, "", "SELECT title FROM no_such_table", &[]).await;
        assert!(matches!(probe, Err(Unmet::Broken(_))), "{probe:?}");
        let probe = observe_row(&url, "", "SELECT 1 WHERE false", &[]).await;
        assert!(probe.is_ok(), "{probe:?}");
        let probe = observe_row(&url, "", "SELECT 'a'", &[]).await;
        assert!(matches!(probe, Err(Unmet::NotYet(_))), "{probe:?}");
    }
}

/// The declared pairs (capability, entry point) some fixture is driven through.
fn pairs_to_drive() -> BTreeSet<(String, Entry)> {
    fixtures(&fixtures_dir())
        .into_iter()
        .flat_map(|(name, dir)| {
            let fixture = read_fixture(&dir).unwrap_or_else(|e| panic!("fixture {name}: {e}"));
            fixture
                .entries()
                .into_iter()
                .filter(|entry| !waiting(&name, *entry))
                .map(|entry| (fixture.capability.clone(), entry))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[tokio::test]
async fn every_capability_fixture_reaches_its_effect() {
    let Some(admin_url) = try_database_url() else {
        eprintln!("SKIP every_capability_fixture_reaches_its_effect: DATABASE_URL not set");
        return;
    };
    let fixtures = fixtures(&fixtures_dir());
    assert!(!fixtures.is_empty(), "no fixture under {}", fixtures_dir().display());
    // `CAPABILITY_FIXTURE=<name>` and `CAPABILITY_ENTRY=<entry point>` narrow the run, while
    // working on one. Unset, as in CI, every fixture runs on every entry point it is driven
    // through.
    let only = std::env::var("CAPABILITY_FIXTURE").ok();
    let only_entry = std::env::var("CAPABILITY_ENTRY").ok();
    let fixtures: Vec<_> = fixtures
        .into_iter()
        .filter(|(name, _)| only.as_ref().is_none_or(|o| o == name))
        .collect();
    assert!(!fixtures.is_empty(), "no fixture is named {only:?}");

    let mut failures = Vec::new();
    let mut driven = BTreeSet::new();
    for (name, dir) in &fixtures {
        let capability = read_fixture(dir).map(|fixture| fixture.capability);
        match run_fixture(&admin_url, name, dir, only_entry.as_deref()).await {
            Ok(outcomes) => {
                for (entry, outcome) in outcomes {
                    match outcome {
                        Ok(()) => {
                            eprintln!("fixture {name} on {}: effect observed", entry.name());
                            if let Ok(capability) = &capability {
                                driven.insert((capability.clone(), entry));
                            }
                        },
                        Err(failure) => {
                            failures.push(format!("fixture {name} on {}: {failure}", entry.name()));
                        },
                    }
                }
            },
            Err(failure) => failures.push(format!("fixture {name}: {failure}")),
        }
    }
    // The runner itself drove every pair the gate above expects, so a pair cannot be declared,
    // counted by the gate, and skipped here.
    if only.is_none() && only_entry.is_none() && failures.is_empty() {
        let missed: Vec<String> = pairs_to_drive()
            .difference(&driven)
            .map(|(capability, entry)| format!("{capability} on {}", entry.name()))
            .collect();
        assert!(missed.is_empty(), "no fixture reached its effect through: {missed:?}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The `listener` entry point, for one fixture: the fixture runner re-executes
/// this test binary to run this test alone, with the fixture's environment set on the
/// process, as a deployment sets it. Run any other way, it does nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the fixture runner re-executes it with a fixture's environment"]
async fn serve_a_fixture_through_an_in_process_entry_point() {
    let Ok(entry) = std::env::var(CHILD_ENTRY) else {
        eprintln!(
            "SKIP serve_a_fixture_through_an_in_process_entry_point: the fixture runner sets \
             {CHILD_ENTRY}"
        );
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let served = match entry.as_str() {
        "listener" => Box::pin(serve_on_a_listener()).await,
        other => Err(anyhow::anyhow!("{CHILD_ENTRY} names no in-process entry point: {other}")),
    };
    if let Err(e) = served {
        panic!("Error: {e:#}");
    }
}

/// Serve as an embedder serves: the server configuration from `FRAISEQL_CONFIG` and the
/// environment, the compiled schema from `FRAISEQL_SCHEMA_PATH`, then
/// `build_postgres_server` and `serve_on_listener` on a listener of its own.
async fn serve_on_a_listener() -> anyhow::Result<()> {
    let args = fraiseql_server::cli::ServerArgs::from_env().map_err(anyhow::Error::msg)?;
    let mut config = match &args.config {
        Some(path) => fraiseql_server::ServerConfig::from_file(path).map_err(anyhow::Error::msg)?,
        None => fraiseql_server::ServerConfig::default(),
    };
    args.apply_to_config(&mut config);
    config.validate().map_err(|e| anyhow::anyhow!("{e}"))?;
    let loaded = fraiseql_server::CompiledSchemaLoader::new(&config.schema_path)
        .load_extended()
        .await?;
    fraiseql_core::utils::casing::set_runtime_acronyms(&loaded.schema.naming_acronyms);
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let server = Box::pin(fraiseql_server::boot::build_postgres_server(
        config,
        loaded.schema,
        loaded.functions,
    ))
    .await?;
    Box::pin(server.serve_on_listener(listener, std::future::pending::<()>())).await?;
    Ok(())
}

/// Call `tool` over the server's stdin with no arguments, and return its answer.
async fn call_over_stdio(stdio: &StdioChannel, tool: &str) -> Result<Value, String> {
    let id = stdio.request("tools/call", &serde_json::json!({ "name": tool, "arguments": {} }))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(answer) = stdio.answer(id)? {
            return Ok(answer);
        }
        if Instant::now() > deadline {
            return Err(format!("mcp tools/call {tool}: no answer on stdout within 30s"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// What the `mcp` fixture's `notes` query answers, through the entry point `served` serves.
async fn read_notes(served: &Served) -> Result<String, String> {
    match &served.stdio {
        Some(stdio) => Ok(call_over_stdio(stdio, "notes").await?.to_string()),
        None => reqwest::Client::new()
            .post(format!("{}/graphql", served.base))
            .json(&serde_json::json!({ "query": "{ notes { id title } }" }))
            .send()
            .await
            .map_err(|e| format!("/graphql: {e}"))?
            .text()
            .await
            .map_err(|e| format!("/graphql: {e}")),
    }
}

/// Serve the `mcp` fixture through `entry`, give its `notes` query a role in the compiled
/// schema, signal `SIGUSR1`, and wait for `notes` to be hidden from the anonymous caller, as
/// the reloaded schema hides a query whose role the caller lacks.
#[cfg(unix)]
async fn reload_on_sigusr1(entry: Entry) {
    let Some(admin_url) = try_database_url() else {
        eprintln!("SKIP reload on {}: DATABASE_URL not set", entry.name());
        return;
    };
    let dir = fixtures_dir().join("mcp");
    let fixture = read_fixture(&dir).unwrap();
    let compiled = tempfile::tempdir().unwrap();
    let schema = compiled.path().join("schema.compiled.json");
    compile(&dir, &schema).await.unwrap();
    let db = format!("cap_reload_{}", entry.name());
    let setup_sql = format!(
        "{}\nINSERT INTO app.tb_note VALUES (gen_random_uuid(), 'as booted');",
        std::fs::read_to_string(dir.join("setup.sql")).unwrap()
    );
    let database_url = prepare_database(&admin_url, &db, &setup_sql, fixture.fraiseql_setup)
        .await
        .unwrap();
    let scratch = tempfile::tempdir().unwrap();

    let outcome = async {
        let mut served = serve(
            entry,
            &dir,
            &fixture,
            &schema,
            &database_url,
            scratch.path(),
            Receiver::start().await,
        )
        .await?;
        let before = read_notes(&served).await?;
        if !before.contains("as booted") {
            return Err(format!("the booted schema serves `notes`: {before}"));
        }

        let mut document: Value =
            serde_json::from_str(&std::fs::read_to_string(&schema).unwrap()).unwrap();
        let notes = document["queries"]
            .as_array_mut()
            .and_then(|queries| queries.iter_mut().find(|q| q["name"] == "notes"))
            .ok_or("the compiled schema has a `notes` query")?;
        notes["requires_role"] = Value::from("auditor");
        let staged = schema.with_extension("staged");
        std::fs::write(&staged, document.to_string()).unwrap();
        std::fs::rename(&staged, &schema).unwrap();
        let signalled = Command::new("kill")
            .args(["-USR1", &served.child.id().to_string()])
            .status()
            .map_err(|e| format!("kill -USR1: {e}"))?;
        if !signalled.success() {
            return Err(format!("kill -USR1 exited {signalled}"));
        }

        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            // Without a handler, the signal's default action ends the process.
            served.still_running()?;
            let after = read_notes(&served).await?;
            if after.contains("Query 'notes' not found in schema") {
                return Ok(());
            }
            if Instant::now() > deadline {
                let stderr = std::fs::read_to_string(&served.stderr).unwrap_or_default();
                let reload: Vec<&str> =
                    stderr.lines().filter(|line| line.to_lowercase().contains("reload")).collect();
                return Err(format!(
                    "`notes` is still served as booted 20s after SIGUSR1: {after}\n\
                     --- what the server logged about the reload ---\n{}",
                    reload.join("\n")
                ));
            }
        }
    }
    .await;
    drop_database(&admin_url, &db).await;
    if let Err(failure) = outcome {
        panic!("reload on {}: {failure}", entry.name());
    }
}

/// A server serving MCP over stdio reloads its schema on `SIGUSR1`, as the binary serving
/// HTTP does. The tool list stays the boot one (a stdio session lasts the process); the
/// reloaded schema answers the query a tool runs.
#[cfg(unix)]
#[tokio::test]
async fn a_stdio_server_reloads_its_schema_on_sigusr1() {
    reload_on_sigusr1(Entry::McpStdio).await;
}

/// The binary serving HTTP reloads its schema on `SIGUSR1`: the reference the stdio server is
/// held to.
#[cfg(unix)]
#[tokio::test]
async fn the_binary_reloads_its_schema_on_sigusr1() {
    reload_on_sigusr1(Entry::Binary).await;
}

/// Write `observers` into the compiled schema at `schema`, replacing the file in one rename.
fn declare_observers(schema: &Path, observers: Value) {
    let mut document: Value =
        serde_json::from_str(&std::fs::read_to_string(schema).unwrap()).unwrap();
    document["observers"] = observers;
    let staged = schema.with_extension("staged");
    std::fs::write(&staged, document.to_string()).unwrap();
    std::fs::rename(&staged, schema).unwrap();
}

/// `order_seen`, declared to post each new `Order` to `url`.
fn order_seen(url: &str) -> Value {
    serde_json::json!([{
        "name": "order_seen",
        "entity": "Order",
        "event": "INSERT",
        "actions": [{ "type": "webhook", "url": url }],
        "retry": {
            "max_attempts": 1,
            "backoff_strategy": "fixed",
            "initial_delay_ms": 100,
            "max_delay_ms": 100
        }
    }])
}

async fn insert_order_change(database_url: &str) -> Result<(), String> {
    let pool = PgPool::connect(database_url).await.map_err(|e| format!("connect: {e}"))?;
    sqlx::query(
        "INSERT INTO core.tb_entity_change_log (object_type, modification_type, object_id, \
         tenant_id, object_data) VALUES ('Order', 'INSERT', gen_random_uuid(), \
         gen_random_uuid(), '{\"id\": 1}')",
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("insert a change: {e}"))?;
    pool.close().await;
    Ok(())
}

/// Wait until the receiver has seen `path` `count` times.
async fn received(served: &mut Served, path: &str, count: usize) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        served.still_running()?;
        let seen = served.receiver.paths.lock().unwrap().iter().filter(|p| *p == path).count();
        if seen >= count {
            return Ok(());
        }
        if Instant::now() > deadline {
            let paths = served.receiver.paths.lock().unwrap().clone();
            return Err(format!("{path} was received {seen} of {count} times in 20s: {paths:?}"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Serve the `observers` fixture with a declared observer, edit the declaration in the
/// compiled schema, signal `SIGUSR1`: the reload rewrites its row in `core.tb_observer`, and
/// the next change is delivered by the edited declaration, with no restart.
#[cfg(unix)]
async fn reload_edits_a_declared_observer(entry: Entry) {
    let Some(admin_url) = try_database_url() else {
        eprintln!("SKIP observer reload on {}: DATABASE_URL not set", entry.name());
        return;
    };
    let dir = fixtures_dir().join("observers");
    let fixture = read_fixture(&dir).unwrap();
    let compiled = tempfile::tempdir().unwrap();
    let schema = compiled.path().join("schema.compiled.json");
    compile(&dir, &schema).await.unwrap();
    let db = format!("cap_observer_reload_{}", entry.name());
    let setup_sql = std::fs::read_to_string(dir.join("setup.sql")).unwrap();
    let database_url = prepare_database(&admin_url, &db, &setup_sql, fixture.fraiseql_setup)
        .await
        .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let receiver = Receiver::start().await;
    let at = |path: &str| format!("{}{path}", receiver.url);
    let (booted, reloaded) = (at("/as-booted"), at("/as-reloaded"));
    declare_observers(&schema, order_seen(&booted));

    let outcome = async {
        let mut served =
            serve(entry, &dir, &fixture, &schema, &database_url, scratch.path(), receiver).await?;
        insert_order_change(&database_url).await?;
        received(&mut served, "/as-booted", 1).await?;

        declare_observers(&schema, order_seen(&reloaded));
        let signalled = Command::new("kill")
            .args(["-USR1", &served.child.id().to_string()])
            .status()
            .map_err(|e| format!("kill -USR1: {e}"))?;
        if !signalled.success() {
            return Err(format!("kill -USR1 exited {signalled}"));
        }

        let pool = PgPool::connect(&database_url).await.map_err(|e| format!("connect: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            served.still_running()?;
            let stored: Option<String> = sqlx::query_scalar(
                "SELECT actions -> 0 ->> 'url' FROM core.tb_observer \
                 WHERE name = 'order_seen' AND deleted_at IS NULL",
            )
            .fetch_optional(&pool)
            .await
            .map_err(|e| format!("read the row: {e}"))?;
            if stored.as_deref() == Some(reloaded.as_str()) {
                break;
            }
            if Instant::now() > deadline {
                let stderr = std::fs::read_to_string(&served.stderr).unwrap_or_default();
                let reload: Vec<&str> =
                    stderr.lines().filter(|line| line.to_lowercase().contains("reload")).collect();
                return Err(format!(
                    "the row still holds {stored:?} 20s after SIGUSR1\n\
                     --- what the server logged about the reload ---\n{}",
                    reload.join("\n")
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        pool.close().await;

        insert_order_change(&database_url).await?;
        received(&mut served, "/as-reloaded", 1).await?;
        let booted_count = served
            .receiver
            .paths
            .lock()
            .unwrap()
            .iter()
            .filter(|p| *p == "/as-booted")
            .count();
        if booted_count != 1 {
            return Err(format!("the booted declaration delivered {booted_count} times"));
        }
        Ok(())
    }
    .await;
    drop_database(&admin_url, &db).await;
    if let Err(failure) = outcome {
        panic!("observer reload on {}: {failure}", entry.name());
    }
}

/// The binary reloads an edited declared observer on `SIGUSR1`.
#[cfg(unix)]
#[tokio::test]
async fn the_binary_reloads_an_edited_declared_observer() {
    reload_edits_a_declared_observer(Entry::Binary).await;
}

/// A server serving MCP over stdio reloads an edited declared observer on `SIGUSR1`, as the
/// binary does: its `AppState` carries the observer runtime the reload rebuilds.
#[cfg(unix)]
#[tokio::test]
async fn a_stdio_server_reloads_an_edited_declared_observer() {
    reload_edits_a_declared_observer(Entry::McpStdio).await;
}

/// Every row of `core.tb_observer`, whole, in order.
async fn observer_rows(pool: &PgPool) -> Result<Vec<String>, String> {
    sqlx::query_scalar("SELECT row_to_json(o)::text FROM core.tb_observer o ORDER BY pk_observer")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("read core.tb_observer: {e}"))
}

/// Wait until the server has logged `count` refused reloads.
async fn refused_reloads(served: &mut Served, count: usize) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        served.still_running()?;
        let stderr = std::fs::read_to_string(&served.stderr).unwrap_or_default();
        let log = std::fs::read_to_string(&served.log).unwrap_or_default();
        let refused = format!("{stderr}{log}")
            .lines()
            .filter(|line| line.contains("Schema reload failed via SIGUSR1"))
            .count();
        if refused >= count {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!("{refused} of {count} reloads were refused in 20s"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A reload that changes a declared observer and is refused, for another section that
/// needs a restart or for an observer name another project holds, changes nothing: the
/// rows of `core.tb_observer` are as they were, byte for byte, and the next change is
/// delivered by the booted declaration.
#[cfg(unix)]
#[tokio::test]
async fn a_refused_reload_leaves_the_declared_observers_as_they_were() {
    let Some(admin_url) = try_database_url() else {
        eprintln!("SKIP refused observer reload: DATABASE_URL not set");
        return;
    };
    let dir = fixtures_dir().join("observers");
    let fixture = read_fixture(&dir).unwrap();
    let compiled = tempfile::tempdir().unwrap();
    let schema = compiled.path().join("schema.compiled.json");
    compile(&dir, &schema).await.unwrap();
    let db = "cap_observer_reload_refused";
    let setup_sql = std::fs::read_to_string(dir.join("setup.sql")).unwrap();
    let database_url = prepare_database(&admin_url, db, &setup_sql, fixture.fraiseql_setup)
        .await
        .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let receiver = Receiver::start().await;
    let at = |path: &str| format!("{}{path}", receiver.url);
    let (booted, reloaded) = (at("/as-booted"), at("/as-reloaded"));
    declare_observers(&schema, order_seen(&booted));

    let outcome = async {
        let mut served =
            serve(Entry::Binary, &dir, &fixture, &schema, &database_url, scratch.path(), receiver)
                .await?;
        let pool = PgPool::connect(&database_url).await.map_err(|e| format!("connect: {e}"))?;
        sqlx::query(
            "INSERT INTO core.tb_observer (name, entity_type, event_type, actions, managed_by) \
             VALUES ('order_held', 'Order', 'DELETE', '[]', 'schema:another-project')",
        )
        .execute(&pool)
        .await
        .map_err(|e| format!("seed another project's observer: {e}"))?;
        let before = observer_rows(&pool).await?;
        let signal = |served: &Served| {
            let signalled = Command::new("kill")
                .args(["-USR1", &served.child.id().to_string()])
                .status()
                .map_err(|e| format!("kill -USR1: {e}"))?;
            if signalled.success() {
                Ok(())
            } else {
                Err(format!("kill -USR1 exited {signalled}"))
            }
        };

        // The edited observer, and a section only a restart applies.
        let mut document: Value =
            serde_json::from_str(&std::fs::read_to_string(&schema).unwrap()).unwrap();
        document["observers"] = order_seen(&reloaded);
        document["naming_acronyms"] = serde_json::json!(["db2"]);
        let staged = schema.with_extension("staged");
        std::fs::write(&staged, document.to_string()).unwrap();
        std::fs::rename(&staged, &schema).unwrap();
        signal(&served)?;
        refused_reloads(&mut served, 1).await?;
        if observer_rows(&pool).await? != before {
            return Err("a reload refused for `naming_acronyms` rewrote core.tb_observer".into());
        }

        // The edited observer first, then a name another project holds.
        let mut both = order_seen(&reloaded);
        let mut held = order_seen(&reloaded)[0].clone();
        held["name"] = Value::from("order_held");
        both.as_array_mut().unwrap().push(held);
        declare_observers(&schema, both);
        signal(&served)?;
        refused_reloads(&mut served, 2).await?;
        if observer_rows(&pool).await? != before {
            return Err("a reload refused for a name another project holds rewrote \
                        core.tb_observer"
                .into());
        }
        pool.close().await;

        insert_order_change(&database_url).await?;
        received(&mut served, "/as-booted", 1).await?;
        let reloaded_count = served
            .receiver
            .paths
            .lock()
            .unwrap()
            .iter()
            .filter(|p| *p == "/as-reloaded")
            .count();
        if reloaded_count != 0 {
            return Err(format!("a refused declaration delivered {reloaded_count} times"));
        }
        Ok(())
    }
    .await;
    drop_database(&admin_url, db).await;
    if let Err(failure) = outcome {
        panic!("refused observer reload: {failure}");
    }
}
