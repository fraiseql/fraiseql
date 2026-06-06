//! Integration test for the multi-tenant runtime provisioning primitive (#330):
//! `FromPoolConfig for PostgresAdapter`, which lets the binary build a per-tenant
//! executor from a connection string at registration time. Requires PostgreSQL
//! (`DATABASE_URL`); skips gracefully when unset.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stderr)] // Reason: test code.

use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_server::tenancy::{FromPoolConfig, TenantPoolConfig};

#[tokio::test]
async fn postgres_adapter_builds_from_tenant_pool_config() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping postgres_adapter_builds_from_tenant_pool_config: DATABASE_URL unset");
        return;
    };

    let config = TenantPoolConfig {
        connection_string:    database_url,
        max_connections:      4,
        connect_timeout_secs: 5,
        idle_timeout_secs:    300,
    };

    // `from_pool_config` opens a connection (the startup health check inside
    // `with_pool_config`); `Ok` proves the production PostgresAdapter can be built
    // for a per-tenant pool — the enabler for runtime tenant provisioning (#330).
    PostgresAdapter::from_pool_config(&config)
        .await
        .expect("PostgresAdapter::from_pool_config should connect to the warm test PG");
}
