#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::await_holding_lock)] // Reason: the env lock must span the awaited getter — that is what serializes env access

use std::sync::Mutex;

use super::{host_port, normalize, postgres, redis};

/// Serializes the tests that mutate process env (env vars are process-global;
/// two parallel env tests would race each other).
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Locks [`ENV_LOCK`] even when a previous holder panicked — the guard's Drop
/// restores env state, so a poisoned lock is still consistent.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Sets an env var for the test's scope and restores the previous value on drop.
struct EnvGuard {
    name: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn set(name: &'static str, value: &str) -> Self {
        let prev = std::env::var(name).ok();
        std::env::set_var(name, value);
        Self { name, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.name, v),
            None => std::env::remove_var(self.name),
        }
    }
}

/// A 127.0.0.1 port that was just observed closed (bound then released).
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// #879: "available" must mean *reachable*, not merely *configured*. A
/// `DATABASE_URL` pointing at a closed port must read as no-database so the
/// documented skip path skips instead of hard-failing in test setup.
#[tokio::test]
async fn postgres_with_unreachable_url_reads_as_unavailable() {
    let _lock = env_lock();
    let url = format!("postgresql://u:p@127.0.0.1:{}/db", closed_port());
    let _guard = EnvGuard::set("DATABASE_URL", &url);
    assert!(
        postgres().await.is_none(),
        "postgres() returned Some for an unreachable DATABASE_URL"
    );
}

/// The positive direction: a URL whose host:port accepts TCP connections is
/// returned as-is (the probe checks reachability, not protocol).
#[tokio::test]
async fn postgres_with_reachable_url_is_returned() {
    let _lock = env_lock();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("postgresql://u:p@{}/db", listener.local_addr().unwrap());
    let _guard = EnvGuard::set("DATABASE_URL", &url);
    let svc = postgres().await.expect("reachable DATABASE_URL must resolve");
    assert_eq!(svc.url(), url);
}

/// The siblings share the policy: an unreachable `REDIS_URL` also skips.
#[tokio::test]
async fn redis_with_unreachable_url_reads_as_unavailable() {
    let _lock = env_lock();
    let url = format!("redis://127.0.0.1:{}", closed_port());
    let _guard = EnvGuard::set("REDIS_URL", &url);
    assert!(redis().await.is_none(), "redis() returned Some for an unreachable REDIS_URL");
}

#[test]
fn host_port_explicit_port() {
    assert_eq!(
        host_port("postgresql://u:p@db.example:5433/test_fraiseql"),
        Ok(("db.example".to_string(), 5433))
    );
}

#[test]
fn host_port_scheme_defaults() {
    assert_eq!(host_port("postgres://u@h/db"), Ok(("h".to_string(), 5432)));
    assert_eq!(host_port("redis://h"), Ok(("h".to_string(), 6379)));
    assert_eq!(host_port("nats://h"), Ok(("h".to_string(), 4222)));
    assert_eq!(host_port("http://minio"), Ok(("minio".to_string(), 80)));
    assert_eq!(host_port("https://gcs"), Ok(("gcs".to_string(), 443)));
}

#[test]
fn host_port_ipv6_literal() {
    assert_eq!(host_port("postgresql://u:p@[::1]:5433/db"), Ok(("::1".to_string(), 5433)));
    assert_eq!(host_port("redis://[::1]"), Ok(("::1".to_string(), 6379)));
}

#[test]
fn host_port_query_only_url() {
    assert_eq!(
        host_port("http://azurite:10000/devstoreaccount1?sslmode=disable"),
        Ok(("azurite".to_string(), 10000))
    );
}

#[test]
fn host_port_rejects_unparseable() {
    assert!(host_port("not-a-url").is_err(), "URL without scheme must be an error");
    assert!(host_port("postgresql://u:p@:5432/db").is_err(), "empty host must be an error");
    assert!(host_port("postgresql://h:notaport/db").is_err(), "bad port must be an error");
    assert!(
        host_port("weird://h/db").is_err(),
        "unknown scheme without port must be an error"
    );
}

#[test]
fn normalize_drops_empty() {
    assert_eq!(normalize(Some(String::new())), None);
}

#[test]
fn normalize_drops_whitespace_only() {
    assert_eq!(normalize(Some("   \t ".to_string())), None);
}

#[test]
fn normalize_keeps_real_value() {
    assert_eq!(
        normalize(Some("postgresql://x".to_string())),
        Some("postgresql://x".to_string())
    );
}

#[test]
fn normalize_passes_through_none() {
    assert_eq!(normalize(None), None);
}
