//! The server binary's boot output names its database without the password in the URL.
//!
//! The binary is started with a `DATABASE_URL` that carries a password and points at a port
//! nothing listens on, so it logs its configuration and then fails to connect. Everything it
//! writes to stdout and stderr up to that exit is read back.

use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The password in the URL the binary is given. Nothing it writes may contain it.
const PASSWORD: &str = "pw-boot-7c41e9";

/// Everything the binary writes before it exits (or is stopped after `limit`), with `extra`
/// added to its environment.
fn boot_output(database_url: &str, extra: &[(&str, &str)], limit: Duration) -> String {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_fraiseql-server"))
        .current_dir(scratch.path())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("DATABASE_URL", database_url)
        .env("RUST_LOG", "info")
        // Development mode: the production-only refusals (CORS and the like) would stop
        // the binary before it logs its configuration.
        .env("FRAISEQL_ENV", "development")
        .env("FRAISEQL_SCHEMA_PATH", scratch.path().join("schema.compiled.json"))
        .envs(extra.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fraiseql-server");
    let started = Instant::now();
    while child.try_wait().expect("poll fraiseql-server").is_none() {
        if started.elapsed() > limit {
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.wait();
    let mut output = String::new();
    child
        .stdout
        .take()
        .expect("stdout")
        .read_to_string(&mut output)
        .expect("read stdout");
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut output)
        .expect("read stderr");
    output
}

#[test]
fn the_boot_log_names_the_database_without_its_password() {
    let output = boot_output(
        &format!("postgres://fraiseql_app:{PASSWORD}@127.0.0.1:1/appdb"),
        &[],
        Duration::from_mins(1),
    );
    assert!(
        output.contains("Server configuration loaded"),
        "the binary never reached its configuration log line, so this proves nothing:\n{output}"
    );
    assert!(
        output.contains("fraiseql_app") && output.contains("127.0.0.1:1/appdb"),
        "the boot log no longer names the user, host and database:\n{output}"
    );
    assert!(
        !output.contains(PASSWORD),
        "the boot output contains the database password:\n{output}"
    );
}

#[cfg(feature = "tracing-opentelemetry")]
#[test]
fn the_otlp_startup_line_names_the_collector_without_its_password() {
    let endpoint = format!("https://otel:{PASSWORD}@otel.example.com:4318");
    let output = boot_output(
        "postgres://fraiseql_app@127.0.0.1:1/appdb",
        &[("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint)],
        Duration::from_mins(1),
    );
    assert!(
        output.contains(
            "OTLP tracing export enabled: endpoint=https://otel:***@otel.example.com:4318"
        ),
        "the binary never reported its OTLP collector, so this proves nothing:\n{output}"
    );
    assert!(
        !output.contains(PASSWORD),
        "the boot output wrote the collector password:\n{output}"
    );
}
