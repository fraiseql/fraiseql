//! Service provisioning getters — one policy, applied per service.
//!
//! Env var names are canonical and match the legacy CI job environment as well as
//! the URLs the Dagger module injects, so a test reads the same variable whether it
//! runs under `dagger call test-integration` locally or in CI.

use std::any::Any;

/// `postgres()` env var.
const POSTGRES_URL_ENV: &str = "DATABASE_URL";
/// `mysql()` env var.
const MYSQL_URL_ENV: &str = "MYSQL_URL";
/// `redis()` env var.
const REDIS_URL_ENV: &str = "REDIS_URL";
/// `sqlserver()` env var.
const SQLSERVER_URL_ENV: &str = "SQLSERVER_URL";
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
        return Some(Service::from_url(url));
    }
    spawn_postgres().await
}

/// MySQL. Env: `MYSQL_URL`.
pub async fn mysql() -> Option<Service> {
    resolve_env(MYSQL_URL_ENV).await
}

/// Redis. Env: `REDIS_URL`.
pub async fn redis() -> Option<Service> {
    resolve_env(REDIS_URL_ENV).await
}

/// SQL Server. Env: `SQLSERVER_URL`.
pub async fn sqlserver() -> Option<Service> {
    resolve_env(SQLSERVER_URL_ENV).await
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

/// `HashiCorp` Vault. Env: `VAULT_ADDR` + `VAULT_TOKEN` (both required).
#[must_use]
pub fn vault() -> Option<Vault> {
    let addr = env_url(VAULT_ADDR_ENV)?;
    let token = env_url(VAULT_TOKEN_ENV)?;
    Some(Vault { addr, token })
}

/// Env-only resolver for services in the spawnable family that do not yet have a
/// local spawn path. Kept `async` so wiring one up later is not a caller-facing
/// signature change.
#[allow(clippy::unused_async)] // Reason: uniform async getter family; mysql/redis/sqlserver/nats gain local spawn in a later Phase-04 slice
async fn resolve_env(name: &str) -> Option<Service> {
    env_url(name).map(Service::from_url)
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
