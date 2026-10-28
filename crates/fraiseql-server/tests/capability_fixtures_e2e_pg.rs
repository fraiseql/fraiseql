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
//! - `listener` and `run`: this test binary, re-executed to run one ignored test,
//!   [`serve_a_fixture_through_an_in_process_entry_point`], with the fixture's environment. It
//!   serves as an embedder does (`build_postgres_server`, then `serve_on_listener`), or as
//!   `fraiseql run` does.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL`, `NATS_URL`
//! for a fixture that names NATS, `KAFKA_BOOTSTRAP` for one that names Kafka,
//! `GREENMAIL_{HOST,IMAPS_PORT,SMTP_PORT,CA}` for one that names the mail server · **Leg:**
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
/// MCP stdio refuses a schema without `[mcp]`, and a project that declares functions (an SDK
/// `schema.json`) cannot also declare `[mcp]` beside it (#1719), so no application can serve
/// its functions over stdio yet.
const NOT_DRIVEN_YET: &[(&str, Entry, &str)] = &[("cron", Entry::McpStdio, "#1719")];

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
    /// An HS256 JWT minted when the stimulus is sent, so no signed token is committed.
    Jwt { hs256: Hs256 },
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hs256 {
    /// The fixture `env` key holding the signing secret the server verifies with.
    secret_env: String,
    sub:        String,
    iss:        String,
    aud:        String,
}

impl Auth {
    fn bearer(
        &self,
        env: &BTreeMap<String, String>,
        receiver: &Receiver,
    ) -> Result<String, String> {
        let now = || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .map_err(|e| e.to_string())
        };
        match self {
            Self::Token(token) => Ok(token.clone()),
            Self::Jwt { hs256 } => {
                let secret = env.get(&hs256.secret_env).ok_or_else(|| {
                    format!("auth.hs256: no `{}` in the fixture env", hs256.secret_env)
                })?;
                let now = now()?;
                let claims = serde_json::json!({
                    "sub": hs256.sub, "iss": hs256.iss, "aud": hs256.aud,
                    "iat": now, "exp": now + 3600,
                });
                jsonwebtoken::encode(
                    &jsonwebtoken::Header::default(),
                    &claims,
                    &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
                )
                .map_err(|e| format!("auth.hs256: {e}"))
            },
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
    /// `method path` (a `GET` unless named, with `body` when given) answers `status`, with
    /// `equals` at `json_pointer` when one is given.
    Http {
        #[serde(default = "default_method")]
        method:       String,
        path:         String,
        #[serde(default)]
        body:         Option<String>,
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
    const fn over_stdio(&self) -> bool {
        match self {
            Self::Row { .. }
            | Self::Nats { .. }
            | Self::Kafka { .. }
            | Self::Received { .. }
            | Self::Refused { .. } => true,
            Self::Http { .. } | Self::Flight { .. } | Self::Health { .. } => false,
        }
    }
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
        sqlx::raw_sql(&statement)
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

/// The environment variable naming the authoring input a re-executed child gives
/// `fraiseql run`.
const CHILD_RUN_INPUT: &str = "CAPABILITY_FIXTURE_CHILD_RUN_INPUT";

/// The process that serves through `entry`.
fn server_command(entry: Entry, scratch: &Path) -> Result<Command, String> {
    match entry {
        Entry::Binary => Ok(Command::new(env!("CARGO_BIN_EXE_fraiseql-server"))),
        Entry::McpStdio => {
            let mut command = Command::new(env!("CARGO_BIN_EXE_fraiseql-server"));
            command.env("FRAISEQL_MCP_STDIO", "1");
            Ok(command)
        },
        Entry::Listener | Entry::Run => {
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
            if entry == Entry::Run {
                // What `fraiseql run` is pointed at: the authoring input, with the
                // `fraiseql.toml` `compile` wrote beside it.
                let schema = scratch.join("schema.json");
                let input = if schema.is_file() {
                    schema
                } else {
                    scratch.join("fraiseql.toml")
                };
                command.env(CHILD_RUN_INPUT, input);
            }
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
    let mut command = server_command(entry, scratch)?;
    command
        .current_dir(&cwd)
        .env(
            "FRAISEQL_CONFIG",
            render_server_toml(dir, scratch, &receiver, &kafka_topic).await?,
        )
        .env("FRAISEQL_SCHEMA_PATH", schema)
        .env("DATABASE_URL", database_url)
        .env("FRAISEQL_BIND_ADDR", format!("127.0.0.1:{port}"))
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

async fn send(
    served: &Served,
    database_url: &str,
    env: &BTreeMap<String, String>,
    stimulus: &Stimulus,
) -> Result<(), String> {
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
            if let Some(auth) = auth {
                request = request.bearer_auth(auth.bearer(env, &served.receiver)?);
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
            Ok(())
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
            let bearer =
                auth.as_ref().map(|auth| auth.bearer(env, &served.receiver)).transpose()?;
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
            sqlx::raw_sql(&served.receiver.expand(statement))
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
async fn observe(
    served: &Served,
    database_url: &str,
    env: &BTreeMap<String, String>,
    effect: &Effect,
) -> Result<(), String> {
    match effect {
        Effect::Http {
            method,
            path,
            body,
            status,
            json_pointer,
            equals,
        } => {
            let verb = reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|e| format!("method {method}: {e}"))?;
            let mut request = reqwest::Client::new()
                .request(verb, format!("{}{path}", served.base))
                .header("content-type", "application/json");
            if let Some(body) = body {
                request = request.body(body.clone());
            }
            let response = request.send().await.map_err(|e| format!("{method} {path}: {e}"))?;
            let got = response.status().as_u16();
            let body: Value = response.json().await.unwrap_or(Value::Null);
            if got != *status {
                return Err(format!("{method} {path} answered {got}, not {status}: {body}"));
            }
            match (json_pointer, equals) {
                (Some(pointer), Some(want)) if body.pointer(pointer) != Some(want) => Err(format!(
                    "{method} {path}: {pointer} is {:?}, not {want}: {body}",
                    body.pointer(pointer)
                )),
                _ => Ok(()),
            }
        },
        Effect::Row { query, expect } => {
            let pool = PgPool::connect(database_url).await.map_err(|e| format!("connect: {e}"))?;
            let query = query.replace("$RUN_ID", &served.run_id);
            let rows = sqlx::query(&query).fetch_all(&pool).await;
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
        Effect::Nats {
            stream,
            subject_prefix,
            count,
        } => {
            let subjects = stream_subjects(stream).await?;
            let matching =
                subjects.iter().filter(|s| s.starts_with(subject_prefix.as_str())).count();
            if matching >= *count {
                Ok(())
            } else {
                Err(format!(
                    "{matching} message(s) under `{subject_prefix}` on {stream}, not {count}: \
                     {subjects:?}"
                ))
            }
        },
        Effect::Kafka {
            subscription,
            count,
            data,
        } => {
            let messages = topic_messages(&served.kafka_topic).await?;
            let hits: Vec<&Value> =
                messages.iter().filter(|m| m["subscription_name"] == **subscription).collect();
            if hits.len() < *count {
                return Err(format!(
                    "{} message(s) for {subscription} on {}, not {count}: {messages:?}",
                    hits.len(),
                    served.kafka_topic
                ));
            }
            let Some(data) = data else { return Ok(()) };
            let want: Value =
                serde_json::from_str(data).map_err(|e| format!("kafka.data is not JSON: {e}"))?;
            if hits.iter().any(|m| m["data"] == want) {
                Ok(())
            } else {
                let got: Vec<&Value> = hits.iter().map(|m| &m["data"]).collect();
                Err(format!("no {subscription} message carries data {want}: {got:?}"))
            }
        },
        Effect::Received { path, count } => {
            let paths = served.receiver.paths.lock().unwrap().clone();
            let hits = paths.iter().filter(|p| *p == path).count();
            if hits >= *count {
                Ok(())
            } else {
                Err(format!(
                    "the receiver was sent {hits} request(s) for {path}, not {count}: {paths:?}"
                ))
            }
        },
        Effect::Refused { .. } => Err("a refusal is judged before the server serves".to_owned()),
        Effect::Flight { ticket, auth, rows } => {
            let bearer = auth.bearer(env, &served.receiver)?;
            let got = flight_rows(&served.flight, &bearer, ticket).await?;
            if got == *rows {
                Ok(())
            } else {
                Err(format!("Flight do_get {ticket} streamed {got} row(s), not {rows}"))
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

    if let Effect::Nats { stream, .. } = &fixture.effect {
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
            send(&served, &database_url, &fixture.env, stimulus).await?;
        }
        let deadline = Instant::now() + Duration::from_secs(fixture.deadline_secs);
        loop {
            served
                .still_running()
                .map_err(|exited| format!("{exited}:\n{}", served.log_tail()))?;
            match observe(&served, &database_url, &fixture.env, &fixture.effect).await {
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

/// The `listener` and `run` entry points, for one fixture: the fixture runner re-executes
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
        "run" => Box::pin(serve_through_run()).await,
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

/// Serve as `fraiseql run` serves, pointed at the fixture's authoring input.
async fn serve_through_run() -> anyhow::Result<()> {
    let input = std::env::var(CHILD_RUN_INPUT)?;
    let bind: std::net::SocketAddr = std::env::var("FRAISEQL_BIND_ADDR")?.parse()?;
    Box::pin(fraiseql_cli::commands::run::run(
        Some(&input),
        None,
        Some(bind.port()),
        Some(bind.ip().to_string()),
        false,
        false,
    ))
    .await
}
