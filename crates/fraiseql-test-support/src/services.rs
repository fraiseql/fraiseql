//! Service provisioning getters — one policy, applied per service.
//!
//! Env var names are canonical and match the legacy CI job environment as well as
//! the URLs the Dagger module injects, so a test reads the same variable whether it
//! runs under `dagger call test-integration` locally or in CI.
//!
//! "Available" means **reachable**, not merely configured: every getter probes the
//! URL's host:port with a short-timeout TCP connect and treats an unreachable
//! service as absent, so the documented skip path skips instead of hard-failing in
//! test setup (#879).

use std::{
    any::Any,
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

/// `postgres()` env var.
const POSTGRES_URL_ENV: &str = "DATABASE_URL";
/// `redis()` env var.
const REDIS_URL_ENV: &str = "REDIS_URL";
/// `nats()` env var.
const NATS_URL_ENV: &str = "NATS_URL";
/// `minio()` endpoint env var.
const MINIO_ENDPOINT_ENV: &str = "MINIO_ENDPOINT";
/// `azure_blob()` endpoint env var.
const AZURE_BLOB_ENDPOINT_ENV: &str = "AZURE_BLOB_ENDPOINT";
/// `gcs()` endpoint env var.
const GCS_ENDPOINT_ENV: &str = "GCS_ENDPOINT";
/// `vault()` address env var.
const VAULT_ADDR_ENV: &str = "VAULT_ADDR";
/// `vault()` token env var.
const VAULT_TOKEN_ENV: &str = "VAULT_TOKEN";
/// `kafka()` bootstrap-servers env var.
const KAFKA_BOOTSTRAP_ENV: &str = "KAFKA_BOOTSTRAP";
/// `kinesis()` endpoint env var.
const KINESIS_ENDPOINT_ENV: &str = "KINESIS_ENDPOINT";

/// A provisioned service: a connection URL plus an optional liveness guard.
///
/// When the URL came from the environment the guard is `None`. When a local
/// container was spawned the guard owns it and tears it down on drop.
pub struct Service {
    url:   String,
    #[allow(dead_code)] // Reason: held only for its Drop — tears down the spawned local container
    guard: Option<Box<dyn Any + Send + Sync>>,
}

impl Service {
    /// Build from an environment-provided URL (no owned container).
    fn from_url(url: String) -> Self {
        Self { url, guard: None }
    }

    /// The connection URL for this service.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }
}

/// A provisioned `HashiCorp` Vault: address + root token.
pub struct Vault {
    addr:  String,
    token: String,
}

impl Vault {
    /// The Vault address (e.g. `http://vault:8200`).
    #[must_use]
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// The Vault root token.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Read an env var, treating empty / whitespace-only values as unset.
#[must_use]
pub(crate) fn env_url(name: &str) -> Option<String> {
    normalize(std::env::var(name).ok())
}

/// Drop empty / whitespace-only values to `None` (pure; unit-tested).
fn normalize(raw: Option<String>) -> Option<String> {
    raw.filter(|v| !v.trim().is_empty())
}

/// PostgreSQL. Env: `DATABASE_URL`. Local spawn: yes (with `local-testcontainers`).
pub async fn postgres() -> Option<Service> {
    if let Some(url) = env_url(POSTGRES_URL_ENV) {
        return reachable_service(POSTGRES_URL_ENV, url);
    }
    spawn_postgres().await
}

/// Redis. Env: `REDIS_URL`.
pub async fn redis() -> Option<Service> {
    resolve_env(REDIS_URL_ENV).await
}

/// NATS. Env: `NATS_URL`.
pub async fn nats() -> Option<Service> {
    resolve_env(NATS_URL_ENV).await
}

/// `MinIO` (`S3`-compatible). Env: `MINIO_ENDPOINT` (the endpoint URL, e.g.
/// `http://minio:9000`). Credentials are supplied separately by the caller.
pub async fn minio() -> Option<Service> {
    resolve_env(MINIO_ENDPOINT_ENV).await
}

/// Azure Blob (Azurite emulator). Env: `AZURE_BLOB_ENDPOINT` (the blob service URL,
/// including the account path, e.g. `http://azurite:10000/devstoreaccount1`).
pub async fn azure_blob() -> Option<Service> {
    resolve_env(AZURE_BLOB_ENDPOINT_ENV).await
}

/// Google Cloud Storage (`fake-gcs-server` emulator). Env: `GCS_ENDPOINT` (the base
/// URL, e.g. `http://fake-gcs:4443`).
pub async fn gcs() -> Option<Service> {
    resolve_env(GCS_ENDPOINT_ENV).await
}

/// Apache Kafka. Env: `KAFKA_BOOTSTRAP`.
///
/// Unlike every other service here the value is **scheme-less** — it is
/// librdkafka's `bootstrap.servers` shape, a comma-separated `host:port` list.
/// The first entry is probed for reachability, and the value returned by
/// [`Service::url`] is the list verbatim, ready to be prefixed with the sink's
/// own `kafka+ssl://` / `kafka://` scheme by the caller.
#[allow(clippy::unused_async)] // Reason: uniform async getter family; a local spawn path would land here
pub async fn kafka() -> Option<Service> {
    let bootstrap = env_url(KAFKA_BOOTSTRAP_ENV)?;
    // Probe through the shared, unit-tested `host_port` parser by giving it the
    // scheme it requires, rather than re-implementing host splitting here.
    let first = bootstrap.split(',').next().unwrap_or("").trim();
    match probe(&format!("kafka://{first}")) {
        Ok(()) => Some(Service::from_url(bootstrap)),
        Err(reason) => {
            announce_skip(KAFKA_BOOTSTRAP_ENV, &reason);
            None
        },
    }
}

/// AWS Kinesis, via a `LocalStack` endpoint. Env: `KINESIS_ENDPOINT` (the endpoint
/// URL, e.g. `http://localstack:4566`).
///
/// Returns the endpoint URL, which the caller passes to the sink through
/// `FRAISEQL_KINESIS_ENDPOINT_URL`; the sink's own configured endpoint carries the
/// region (`kinesis://<region>`), which `LocalStack` does not care about.
pub async fn kinesis() -> Option<Service> {
    resolve_env(KINESIS_ENDPOINT_ENV).await
}

/// `HashiCorp` Vault. Env: `VAULT_ADDR` + `VAULT_TOKEN` (both required).
#[must_use]
pub fn vault() -> Option<Vault> {
    let addr = env_url(VAULT_ADDR_ENV)?;
    let token = env_url(VAULT_TOKEN_ENV)?;
    Some(Vault { addr, token })
}

/// Env-only resolver for services in the spawnable family that do not yet have a
/// local spawn path. Kept `async` so wiring one up later is not a caller-facing
/// signature change. Every getter funnels through [`reachable_service`], so a new
/// service getter cannot reintroduce the presence-means-available shape (#879).
#[allow(clippy::unused_async)] // Reason: uniform async getter family; redis/nats gain local spawn in a later slice
async fn resolve_env(name: &str) -> Option<Service> {
    let url = env_url(name)?;
    reachable_service(name, url)
}

/// How long the reachability probe waits per resolved address before treating the
/// service as absent. Live services accept within milliseconds; a closed local
/// port refuses immediately, so the full wait applies only to blackholed hosts.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Wrap an env-provided URL as a [`Service`] only if its host:port accepts TCP
/// connections; otherwise announce the skip and return `None`. This is what makes
/// "available" mean *reachable* rather than *configured* (#879).
fn reachable_service(name: &str, url: String) -> Option<Service> {
    match probe(&url) {
        Ok(()) => Some(Service::from_url(url)),
        Err(reason) => {
            announce_skip(name, &reason);
            None
        },
    }
}

/// Announce an unreachable-service skip on stderr.
#[allow(clippy::print_stderr)] // Reason: a silent skip is the failure mode this crate exists to prevent; stderr is the test log
fn announce_skip(name: &str, reason: &str) {
    eprintln!(
        "SKIP: {name} is set but the service is unreachable ({reason}); \
         treating it as unavailable"
    );
}

/// Attempt one short-timeout TCP connect to the URL's host:port.
fn probe(url: &str) -> Result<(), String> {
    let (host, port) = host_port(url)?;
    let addrs = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {host}:{port}: {e}"))?;
    let mut last = format!("{host}:{port} did not resolve to any address");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, PROBE_TIMEOUT) {
            Ok(_) => return Ok(()),
            Err(e) => last = format!("connect to {addr} failed: {e}"),
        }
    }
    Err(last)
}

/// Extract `(host, port)` from a service URL, defaulting the port by scheme.
/// Pure; unit-tested. Errors name what is missing so the skip line is actionable.
fn host_port(url: &str) -> Result<(String, u16), String> {
    let (scheme, rest) =
        url.split_once("://").ok_or_else(|| format!("no scheme in URL {url:?}"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let hostport = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
    let (host, explicit_port) = if let Some(bracketed) = hostport.strip_prefix('[') {
        // IPv6 literal: [::1] or [::1]:5433
        let (inside, after) = bracketed
            .split_once(']')
            .ok_or_else(|| format!("unclosed IPv6 bracket in URL {url:?}"))?;
        (inside.to_string(), after.strip_prefix(':'))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p)),
            None => (hostport.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err(format!("no host in URL {url:?}"));
    }
    let port = match explicit_port {
        Some(p) => p.parse::<u16>().map_err(|_| format!("invalid port {p:?} in URL {url:?}"))?,
        None => default_port(scheme).ok_or_else(|| {
            format!("no port in URL {url:?} and no default for scheme {scheme:?}")
        })?,
    };
    Ok((host, port))
}

/// Default ports for the schemes this harness provisions.
fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "postgres" | "postgresql" => Some(5432),
        "redis" | "rediss" => Some(6379),
        "nats" => Some(4222),
        "kafka" => Some(9092),
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }
}

#[cfg(feature = "local-testcontainers")]
async fn spawn_postgres() -> Option<Service> {
    use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

    let user = "fraiseql_test";
    let password = "fraiseql_test_password";
    let database = "test_fraiseql";

    let container = Postgres::default()
        .with_user(user)
        .with_password(password)
        .with_db_name(database)
        .start()
        .await
        .expect("failed to start local postgres testcontainer");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("failed to get local postgres container port");

    let url = format!("postgresql://{user}:{password}@127.0.0.1:{port}/{database}");
    Some(Service {
        url,
        guard: Some(Box::new(container)),
    })
}

#[cfg(not(feature = "local-testcontainers"))]
#[allow(clippy::unused_async)] // Reason: mirrors the feature-gated spawn signature so postgres() awaits uniformly; this build has no local Docker
async fn spawn_postgres() -> Option<Service> {
    None
}

#[cfg(test)]
mod tests;
