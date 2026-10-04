//! The CLI's database connections negotiate TLS the way the server's do (#1429).
//!
//! Every CLI connection used to be `NoTls`. `?sslmode=require` then failed with "no TLS
//! implementation configured", so `compile --database` could not reach a database that
//! insists on TLS. The default `prefer` connected in cleartext even to a server that
//! offered TLS. The CLI now builds its pools through `fraiseql-db`'s connector:
//!
//! - the URL's `sslmode` applies (`disable`, `prefer`, `require`), as it does for the server;
//! - where a command loaded `fraiseql.toml`, its `[database] ssl_mode` applies too, including
//!   `verify-full`, which a URL cannot express.
//!
//! `pg_stat_ssl` reports whether a session is encrypted, as seen by the server, rather than
//! as the client believes.
//!
//! **Execution engine:** `PostgreSQL` with TLS · **Infrastructure:** `TLS_DATABASE_URL` and
//! `TLS_TEST_CA_CERT` (the `integration (tls)` leg; `make db-up` locally). The suite skips
//! when neither is set and panics when only one is: a half-provisioned rig is a broken rig.

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{io::Write, path::Path, process::Command};

use fraiseql_cli::connection::postgres_pool;
use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};
use fraiseql_db::postgres::{PostgresSslMode, PostgresTlsConfig};
use tempfile::TempDir;

/// The TLS rig's URL, or `None` when it was not provisioned for this run.
fn tls_url(test: &str) -> Option<String> {
    let url = std::env::var("TLS_DATABASE_URL").ok();
    let ca = std::env::var("TLS_TEST_CA_CERT").ok();
    match (url, ca) {
        (Some(url), Some(_)) => Some(url),
        (None, None) => {
            eprintln!("SKIP {test}: TLS rig not provisioned");
            None
        },
        (url, ca) => panic!(
            "the TLS rig is half-configured: TLS_DATABASE_URL is {}, TLS_TEST_CA_CERT is {}",
            if url.is_some() { "set" } else { "unset" },
            if ca.is_some() { "set" } else { "unset" },
        ),
    }
}

/// The handshake failed under `verify-full`: the rig's CA is not in the platform store,
/// so verification is what refused it (`prefer` would have connected).
fn assert_refused_by_verification(log: &str) {
    assert!(log.contains("ssl_mode = verify-full"), "names the mode in force: {log}");
    assert!(log.contains("TLS handshake"), "the handshake is what failed: {log}");
}

fn with_sslmode(url: &str, mode: &str) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}sslmode={mode}")
}

/// Whether the server sees the pool's session as encrypted.
async fn session_is_encrypted(url: &str, tls: &PostgresTlsConfig) -> bool {
    let pool = postgres_pool(url, "the TLS probe", tls).await.unwrap();
    let client = pool.get().await.unwrap();
    client
        .query_one("SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn sslmode_require_in_the_url_encrypts_the_session() {
    let Some(url) = tls_url("sslmode_require_in_the_url_encrypts_the_session") else {
        return;
    };
    let url = with_sslmode(&url, "require");
    assert!(session_is_encrypted(&url, &PostgresTlsConfig::default()).await);
}

#[tokio::test]
async fn the_default_negotiates_tls_with_a_server_that_offers_it() {
    let Some(url) = tls_url("the_default_negotiates_tls_with_a_server_that_offers_it") else {
        return;
    };
    assert!(
        session_is_encrypted(&url, &PostgresTlsConfig::default()).await,
        "libpq's default is `prefer`: encrypt when the server offers TLS"
    );
}

#[tokio::test]
async fn sslmode_disable_in_the_url_connects_in_plaintext() {
    let Some(url) = tls_url("sslmode_disable_in_the_url_connects_in_plaintext") else {
        return;
    };
    let url = with_sslmode(&url, "disable");
    assert!(!session_is_encrypted(&url, &PostgresTlsConfig::default()).await);
}

#[tokio::test]
async fn a_configured_mode_applies_to_the_pool() {
    let Some(url) = tls_url("a_configured_mode_applies_to_the_pool") else {
        return;
    };
    assert!(
        !session_is_encrypted(&url, &PostgresTlsConfig::new(PostgresSslMode::Disable)).await,
        "an explicit `disable` overrides the default"
    );
    let Err(err) =
        postgres_pool(&url, "the TLS probe", &PostgresTlsConfig::new(PostgresSslMode::VerifyFull))
            .await
    else {
        panic!("verify-full must refuse a certificate no trusted CA issued");
    };
    assert_refused_by_verification(&format!("{err:#}"));
}

fn empty_schema(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("schema.json");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(br#"{"types": [], "queries": []}"#)
        .unwrap();
    path
}

fn cli(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .current_dir(dir)
        .args(args)
        .env_remove("DATABASE_URL")
        .output()
        .expect("spawn fraiseql-cli");
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), log)
}

/// The issue's own reproduction: `compile --database "…?sslmode=require"`.
#[test]
fn compile_reaches_a_server_with_sslmode_require() {
    let Some(url) = tls_url("compile_reaches_a_server_with_sslmode_require") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let schema = empty_schema(dir.path());
    let (ok, log) = cli(
        dir.path(),
        &[
            "compile",
            schema.to_str().unwrap(),
            "--database",
            &with_sslmode(&url, "require"),
            "--skip-hash",
            "-o",
            "out.json",
        ],
    );
    assert!(ok, "{log}");
}

/// `[database] ssl_mode` reaches compile's connection when compile loaded the file.
#[test]
fn compile_applies_ssl_mode_from_its_config() {
    let Some(url) = tls_url("compile_applies_ssl_mode_from_its_config") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let schema = empty_schema(dir.path());
    std::fs::write(dir.path().join("fraiseql.toml"), "[database]\nssl_mode = \"verify-full\"\n")
        .unwrap();
    let (ok, log) = cli(
        dir.path(),
        &[
            "compile",
            schema.to_str().unwrap(),
            "--config",
            "fraiseql.toml",
            "--database",
            &url,
            "--skip-hash",
            "-o",
            "out.json",
        ],
    );
    assert!(!ok, "verify-full from fraiseql.toml must refuse the rig's untrusted CA:\n{log}");
    assert_refused_by_verification(&log);
}

/// A TOML input carries its own `[database]` section; compile reads `ssl_mode` from it.
#[test]
fn compile_applies_ssl_mode_from_a_toml_input() {
    let Some(url) = tls_url("compile_applies_ssl_mode_from_a_toml_input") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("fraiseql.toml"), "[database]\nssl_mode = \"verify-full\"\n")
        .unwrap();
    let (ok, log) = cli(
        dir.path(),
        &[
            "compile",
            "fraiseql.toml",
            "--database",
            &url,
            "--skip-hash",
            "-o",
            "out.json",
        ],
    );
    assert!(
        !ok,
        "verify-full from the TOML input must refuse the rig's untrusted CA:\n{log}"
    );
    assert_refused_by_verification(&log);
}

/// Each command that takes its URL from `fraiseql.toml` takes `ssl_mode` from the same
/// section.
#[test]
fn a_url_from_fraiseql_toml_brings_its_ssl_mode() {
    let Some(url) = tls_url("a_url_from_fraiseql_toml_brings_its_ssl_mode") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let schema = empty_schema(dir.path());
    let (ok, log) = cli(
        dir.path(),
        &[
            "compile",
            schema.to_str().unwrap(),
            "--skip-hash",
            "-o",
            "c.json",
        ],
    );
    assert!(ok, "{log}");
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[database]\nurl = \"{url}\"\nssl_mode = \"verify-full\"\n"),
    )
    .unwrap();
    for args in [
        &["sources", "--schema", "c.json"][..],
        &["setup"][..],
        &["perf", "regression-scan"][..],
        &["perf", "explore", "summary"][..],
    ] {
        let (ok, log) = cli(dir.path(), args);
        assert!(!ok, "{args:?}: verify-full must refuse the rig's untrusted CA:\n{log}");
        assert_refused_by_verification(&log);
    }
}

/// `doctor --against-db` connects with `[database] ssl_mode` from its `--config`.
#[test]
fn doctor_applies_ssl_mode_from_its_config() {
    let Some(url) = tls_url("doctor_applies_ssl_mode_from_its_config") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("fraiseql.toml"), "[database]\nssl_mode = \"verify-full\"\n")
        .unwrap();
    let (_, log) = cli(dir.path(), &["doctor", "--config", "fraiseql.toml", "--against-db", &url]);
    assert!(
        log.contains("cannot connect") && log.contains("ssl_mode = verify-full"),
        "the database checks connect under verify-full and are refused:\n{log}"
    );

    // `--runtime` executes through an adapter rather than the catalogue pool.
    let schema = empty_schema(dir.path());
    let (ok, log) = cli(
        dir.path(),
        &[
            "compile",
            schema.to_str().unwrap(),
            "--skip-hash",
            "-o",
            "c.json",
        ],
    );
    assert!(ok, "{log}");
    let (_, log) = cli(
        dir.path(),
        &[
            "doctor",
            "--config",
            "fraiseql.toml",
            "--schema",
            "c.json",
            "--runtime",
            "--db-url",
            &url,
        ],
    );
    assert!(
        log.contains("cannot connect")
            && log.contains("the runtime smoke (ssl_mode = verify-full)"),
        "the runtime smoke connects under verify-full and is refused:\n{log}"
    );
}

/// A `ssl_mode` the connector cannot honour stops doctor's database checks instead of
/// letting them connect under a weaker mode.
#[test]
fn doctor_refuses_an_ssl_mode_it_cannot_honour() {
    let Some(url) = tls_url("doctor_refuses_an_ssl_mode_it_cannot_honour") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("fraiseql.toml"), "[database]\nssl_mode = \"verify-ca\"\n")
        .unwrap();
    let (ok, log) = cli(dir.path(), &["doctor", "--config", "fraiseql.toml", "--against-db", &url]);
    assert!(!ok, "{log}");
    assert!(log.contains("Database TLS settings"), "{log}");
    assert!(!log.contains("Change-log contract"), "no database check ran: {log}");
}

#[test]
fn generate_views_validate_reaches_a_server_with_sslmode_require() {
    let Some(url) = tls_url("generate_views_validate_reaches_a_server_with_sslmode_require") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let mut schema = CompiledSchema::new();
    let mut thing = TypeDefinition::new("Thing", "v_tls_thing_absent");
    thing.jsonb_column = "data".to_string();
    thing.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(thing);
    std::fs::write(dir.path().join("schema.json"), schema.to_json().unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .current_dir(dir.path())
        .args([
            "generate-views",
            "-s",
            "schema.json",
            "--entity",
            "Thing",
            "--view",
            "va_tls_thing",
            "--validate",
        ])
        .env("DATABASE_URL", with_sslmode(&url, "require"))
        .output()
        .unwrap();
    // The source relation does not exist, so the server rejecting the DDL is the proof
    // that the request reached it over TLS.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("relation \"v_tls_thing_absent\" does not exist"),
        "--validate must reach the server over TLS: {stderr}"
    );
}
