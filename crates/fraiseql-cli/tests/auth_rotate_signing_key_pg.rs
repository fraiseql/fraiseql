//! `fraiseql auth rotate-signing-key`: the first-party issuer's active key is retired and a
//! new one activated, from what the server itself reads.
//!
//! - without a server configuration, without `[auth_issuer]` in it, or without `[auth.database]` in
//!   the project, it is refused, naming what is missing;
//! - it retires the active key and activates a new one, naming both, and the retired key stays
//!   published;
//! - under a key-encryption key that does not open the active key, nothing changes;
//! - where no server has created the ring, it says a server creates it;
//! - a running ring signs with the new key after a re-read, and a session signed before the
//!   rotation still validates;
//! - nothing it prints carries the database password or the key-encryption key.
//!
//! Self-skips the database cases when no `DATABASE_URL` is set. The URL must be able to create
//! databases.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` · **Parallelism:**
//! creates and drops its own `fraiseql-rotate-key-*` databases → run `--test-threads=1`.
#![cfg(all(feature = "test-postgres", feature = "run-server"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{
    fmt::Write as _,
    path::Path,
    process::{Command, Output},
    sync::Arc,
    time::Duration,
};

use fraiseql_server::{
    auth::{AccessTokenSigner as _, Claims, PgSigningKeyStore, RingSigner},
    first_party::{SealedWithFieldEncryption, key_ring, unix_now},
    sqlx::{self, PgPool},
};
use tempfile::TempDir;
use tokio_postgres::NoTls;

/// The servers' key-encryption key: 32 bytes, as 64 hex digits.
const KEK: &str = "0707070707070707070707070707070707070707070707070707070707070707";
/// Another key-encryption key, which opens nothing the servers sealed.
const OTHER_KEK: &str = "0808080808080808080808080808080808080808080808080808080808080808";
const KEK_ENV: &str = "FRAISEQL_TEST_ROTATE_SIGNING_KEK";
const AUTH_URL_ENV: &str = "FRAISEQL_TEST_ROTATE_AUTH_URL";

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

/// `url` with its database replaced by `db`. The suite's URLs are
/// `scheme://user[:password]@host[:port]/database[?query]`.
fn with_database(url: &str, db: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("DATABASE_URL is a URL");
    let (userinfo, rest) = rest.rsplit_once('@').expect("DATABASE_URL names a user");
    let (host, rest) = rest.split_once('/').expect("DATABASE_URL names a database");
    let query = rest.split_once('?').map_or("", |(_, query)| query);
    let query = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };
    format!("{scheme}://{userinfo}@{host}/{db}{query}")
}

/// The password in `url`, if it carries one.
fn password_of(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let (userinfo, _) = rest.rsplit_once('@')?;
    userinfo.split_once(':').map(|(_, password)| password.to_string())
}

async fn admin(url: &str) -> tokio_postgres::Client {
    let (client, conn) = tokio_postgres::connect(url, NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

/// A database of its own, dropped on [`Scratch::drop_all`].
struct Scratch {
    admin_url: String,
    db:        String,
    url:       String,
}

impl Scratch {
    async fn new(admin_url: &str, tag: &str) -> Self {
        let db = format!("fraiseql-rotate-key-{tag}");
        let scratch = Self {
            admin_url: admin_url.to_string(),
            url: with_database(admin_url, &db),
            db,
        };
        scratch.drop_all().await;
        admin(admin_url)
            .await
            .batch_execute(&format!("CREATE DATABASE \"{}\"", scratch.db))
            .await
            .expect("create scratch database");
        scratch
    }

    async fn drop_all(&self) {
        admin(&self.admin_url)
            .await
            .batch_execute(&format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.db))
            .await
            .expect("drop scratch database");
    }

    async fn pool(&self) -> PgPool {
        PgPool::connect(&self.url).await.expect("connect to the scratch database")
    }
}

/// The servers' store, sealing under [`KEK`].
fn server_store(pool: &PgPool) -> PgSigningKeyStore {
    let wrap = SealedWithFieldEncryption::new(&[7; 32]).expect("a 32-byte key-encryption key");
    PgSigningKeyStore::new(pool.clone(), Arc::new(wrap))
}

/// The ring as a server boots it: the table and its first key.
async fn booted_ring(pool: &PgPool) -> String {
    let store = server_store(pool);
    store.init().await.expect("create auth.signing_key");
    store
        .ensure_active(unix_now().expect("the clock"))
        .await
        .expect("the first key")
        .expect("an empty ring gets a key")
}

/// Every key's `kid` and whether it is retired, in a fixed order.
async fn keys(pool: &PgPool) -> Vec<(String, bool)> {
    sqlx::query_as("SELECT kid, retired_at IS NOT NULL FROM auth.signing_key ORDER BY kid")
        .fetch_all(pool)
        .await
        .expect("read the keys")
}

/// A project directory: `fraiseql.toml` (with `[auth.database]` when `auth_database`) and
/// `server.toml` (with `[auth_issuer]` when `issuer`).
fn project(auth_database: bool, issuer: bool) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut toml = String::from(
        r#"[schema]
name = "rotate-signing-key"

[types.Note]
sql_source = "app.v_note"
fields.id = { type = "ID" }

[queries.notes]
return_type = "Note"
return_array = true
sql_source = "app.v_note"
"#,
    );
    if auth_database {
        write!(
            toml,
            "\n[auth.session]\nttl_secs = 900\n\n[auth.database]\nurl_env = \"{AUTH_URL_ENV}\"\n"
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    let mut server = String::from("cors_enabled = false\n");
    if issuer {
        write!(
            server,
            "\n[auth_issuer]\nurl = \"https://api.example.com\"\naudience = \"fraiseql\"\n\
             key_encryption_key_env = \"{KEK_ENV}\"\n"
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("server.toml"), server).unwrap();
    dir
}

/// Run `fraiseql auth rotate-signing-key` in `dir`, with `config` as `--config` (else
/// `FRAISEQL_CONFIG` unset), the auth database URL and the key-encryption key.
fn rotate(dir: &Path, config: Option<&str>, auth_url: Option<&str>, kek: &str) -> Output {
    let mut command = Command::new(cli_bin());
    command
        .current_dir(dir)
        .args(["auth", "rotate-signing-key", "fraiseql.toml"])
        .env_remove("FRAISEQL_CONFIG")
        .env_remove("DATABASE_URL")
        .env(KEK_ENV, kek);
    if let Some(config) = config {
        command.args(["--config", config]);
    }
    match auth_url {
        Some(url) => command.env(AUTH_URL_ENV, url),
        None => command.env_remove(AUTH_URL_ENV),
    };
    command.output().expect("run fraiseql-cli")
}

fn printed(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Nothing printed carries the URL's password or either key-encryption key.
fn assert_quotes_no_secret(output: &Output, url: &str) {
    let printed = printed(output);
    if let Some(password) = password_of(url).filter(|password| !password.is_empty()) {
        assert!(!printed.contains(&password), "the database password was printed:\n{printed}");
    }
    for kek in [KEK, OTHER_KEK] {
        assert!(!printed.contains(kek), "a key-encryption key was printed:\n{printed}");
    }
}

#[test]
fn the_command_is_offered_with_its_help() {
    let output = Command::new(cli_bin())
        .args(["auth", "rotate-signing-key", "--help"])
        .output()
        .expect("run fraiseql-cli");
    assert!(output.status.success(), "{}", printed(&output));
    assert!(printed(&output).contains("FRAISEQL_CONFIG"));
}

#[test]
fn without_a_server_configuration_it_says_where_to_name_one() {
    let dir = project(true, true);
    let output = rotate(dir.path(), None, None, KEK);
    assert!(!output.status.success());
    let printed = printed(&output);
    assert!(printed.contains("FRAISEQL_CONFIG") && printed.contains("--config"), "{printed}");
}

#[test]
fn a_server_configuration_without_auth_issuer_is_refused() {
    let dir = project(true, false);
    let output = rotate(dir.path(), Some("server.toml"), None, KEK);
    assert!(!output.status.success());
    assert!(printed(&output).contains("[auth_issuer]"), "{}", printed(&output));
}

#[test]
fn a_project_without_auth_database_is_refused() {
    let dir = project(false, true);
    let output = rotate(dir.path(), Some("server.toml"), None, KEK);
    assert!(!output.status.success());
    assert!(printed(&output).contains("[auth.database]"), "{}", printed(&output));
}

#[tokio::test]
async fn it_retires_the_active_key_and_activates_a_new_one() {
    let Some(admin_url) = database_url_or_skip("it_retires_the_active_key") else {
        return;
    };
    let scratch = Scratch::new(&admin_url, "retires").await;
    let pool = scratch.pool().await;
    let first = booted_ring(&pool).await;
    let dir = project(true, true);

    let output = rotate(dir.path(), Some("server.toml"), Some(&scratch.url), KEK);
    assert!(output.status.success(), "{}", printed(&output));
    assert_quotes_no_secret(&output, &scratch.url);

    let keys = keys(&pool).await;
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(keys.contains(&(first.clone(), true)), "the old key is retired: {keys:?}");
    let (active, _) = keys.iter().find(|(_, retired)| !retired).expect("one active key");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&first) && stdout.contains(active.as_str()), "{stdout}");
    pool.close().await;
    scratch.drop_all().await;
}

#[tokio::test]
async fn under_another_key_encryption_key_nothing_changes() {
    let Some(admin_url) = database_url_or_skip("under_another_key_encryption_key") else {
        return;
    };
    let scratch = Scratch::new(&admin_url, "other-kek").await;
    let pool = scratch.pool().await;
    booted_ring(&pool).await;
    let before = keys(&pool).await;
    let dir = project(true, true);

    let output = rotate(dir.path(), Some("server.toml"), Some(&scratch.url), OTHER_KEK);
    assert!(!output.status.success(), "{}", printed(&output));
    assert_quotes_no_secret(&output, &scratch.url);
    assert_eq!(keys(&pool).await, before);
    pool.close().await;
    scratch.drop_all().await;
}

#[tokio::test]
async fn where_no_server_created_the_ring_it_says_a_server_creates_it() {
    let Some(admin_url) = database_url_or_skip("where_no_server_created_the_ring") else {
        return;
    };
    let scratch = Scratch::new(&admin_url, "no-ring").await;
    let dir = project(true, true);

    let output = rotate(dir.path(), Some("server.toml"), Some(&scratch.url), KEK);
    assert!(!output.status.success());
    assert!(printed(&output).contains("creates it when it boots"), "{}", printed(&output));
    assert_quotes_no_secret(&output, &scratch.url);
    scratch.drop_all().await;
}

#[tokio::test]
async fn a_running_ring_signs_with_the_new_key_and_older_sessions_still_validate() {
    let Some(admin_url) = database_url_or_skip("a_running_ring_signs_with_the_new_key") else {
        return;
    };
    let scratch = Scratch::new(&admin_url, "running").await;
    let pool = scratch.pool().await;
    let first = booted_ring(&pool).await;
    let ring = Arc::new(key_ring(server_store(&pool), 900, Duration::from_secs(30)));
    let now = unix_now().expect("the clock");
    ring.tick(now).await.expect("read the ring");
    let claims = Claims {
        sub:   "u".to_string(),
        iat:   now,
        exp:   now + 900,
        nbf:   Some(now),
        iss:   "https://api.example.com".to_string(),
        aud:   vec!["fraiseql".to_string()],
        extra: std::collections::HashMap::from([(
            "jti".to_string(),
            fraiseql_server::serde_json::json!(fraiseql_server::uuid::Uuid::new_v4().to_string()),
        )]),
    };
    let before = RingSigner::new(Arc::clone(&ring), 900).sign_access_token(&claims).unwrap();
    let dir = project(true, true);

    let output = rotate(dir.path(), Some("server.toml"), Some(&scratch.url), KEK);
    assert!(output.status.success(), "{}", printed(&output));

    ring.reload(now).await.expect("re-read, as refresh-jwks does");
    let current = ring.current().unwrap();
    assert_ne!(current.active().kid(), first, "it signs with the new key");
    assert!(current.published_key(&first).is_some(), "the old key stays published");
    let issuer = fraiseql_server::server_config::IssuerConfig {
        url:                    "https://api.example.com".to_string(),
        audience:               "fraiseql".to_string(),
        key_encryption_key_env: KEK_ENV.to_string(),
    };
    let validator = fraiseql_core::security::OidcValidator::from_issuer_keys(
        fraiseql_server::first_party::issuer_oidc_config(&issuer),
        Arc::new(fraiseql_server::first_party::RingKeys::new(Arc::clone(&ring))),
    )
    .unwrap();
    validator
        .validate_token(&before)
        .await
        .expect("a session signed before still validates");
    pool.close().await;
    scratch.drop_all().await;
}
