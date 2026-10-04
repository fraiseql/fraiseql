//! PostgreSQL 18 is the floor, and a pool refuses a server below it at the first
//! connection.
//!
//! Every other suite runs against a supported server, where a connection that never
//! asked the server its version passes just as well as one that did. These tests
//! need a server that is actually older: the rigs bind PostgreSQL 17 as
//! `BELOW_FLOOR_DATABASE_URL` for that purpose alone.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` (the
//! supported primary) and `BELOW_FLOOR_DATABASE_URL` (PostgreSQL 17).

#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::time::Duration;

use fraiseql_db::postgres::{
    MINIMUM_SERVER_VERSION_NUM, PoolPrewarmConfig, PostgresAdapter, PostgresTlsConfig,
    ReadReplicaConfig, VectorScanConfig, require_supported_server,
};
use fraiseql_error::FraiseQLError;
use fraiseql_test_support::{below_floor_database_url, database_url};

fn config(read_replicas: Option<ReadReplicaConfig>) -> PoolPrewarmConfig {
    PoolPrewarmConfig {
        min_size: 0,
        max_size: 1,
        timeout_secs: Some(10),
        search_path: None,
        tls: PostgresTlsConfig::default(),
        read_replicas,
        max_streaming_reads: None,
        vector_scan: VectorScanConfig::default(),
    }
}

/// The message names the server's own version string, so `17` in it is that.
/// Returns the message for further assertions.
fn assert_refuses_17(err: &FraiseQLError) -> &str {
    let FraiseQLError::Unsupported { message } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    for needle in ["PostgreSQL 17", "PostgreSQL 18 or newer", "pg_upgrade"] {
        assert!(message.contains(needle), "missing {needle:?}: {message}");
    }
    message
}

#[tokio::test]
async fn the_below_floor_rig_is_postgresql_17() {
    // Non-vacuity: if the rig ever moved to a supported version, every refusal
    // test below would fail for the wrong reason or, worse, be rewritten to pass.
    let (client, connection) =
        tokio_postgres::connect(&below_floor_database_url(), tokio_postgres::NoTls)
            .await
            .unwrap();
    tokio::spawn(connection);
    let num: i32 = client
        .query_one("SELECT current_setting('server_version_num')::int", &[])
        .await
        .unwrap()
        .get(0);
    assert!(
        (170_000..MINIMUM_SERVER_VERSION_NUM).contains(&num),
        "server_version_num = {num}"
    );
}

#[tokio::test]
async fn the_check_refuses_postgresql_17() {
    let (client, connection) =
        tokio_postgres::connect(&below_floor_database_url(), tokio_postgres::NoTls)
            .await
            .unwrap();
    tokio::spawn(connection);
    let err = require_supported_server(&client).await.expect_err("17 is below the floor");
    assert_refuses_17(&err);
}

#[tokio::test]
async fn the_check_accepts_the_supported_primary() {
    let (client, connection) =
        tokio_postgres::connect(&database_url(), tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    require_supported_server(&client).await.expect("the rig's primary is supported");
}

#[tokio::test]
async fn a_pool_refuses_a_primary_below_the_floor() {
    let err = PostgresAdapter::with_pool_config(&below_floor_database_url(), config(None))
        .await
        .expect_err("the adapter must not start on PostgreSQL 17");
    assert_refuses_17(&err);
}

#[tokio::test]
async fn the_default_pool_refuses_a_primary_below_the_floor() {
    let err = PostgresAdapter::new(&below_floor_database_url())
        .await
        .expect_err("the adapter must not start on PostgreSQL 17");
    assert_refuses_17(&err);
}

#[tokio::test]
async fn a_pool_refuses_a_read_replica_below_the_floor() {
    let replicas = ReadReplicaConfig {
        urls:                  vec![below_floor_database_url()],
        pin_after_write:       Duration::from_secs(5),
        max_lag:               None,
        health_probe_interval: Duration::from_secs(1),
    };
    let err = PostgresAdapter::with_pool_config(&database_url(), config(Some(replicas)))
        .await
        .expect_err("a replica below the floor refuses boot");
    let message = assert_refuses_17(&err);
    assert!(message.starts_with("Read replica 0: "), "names the replica: {message}");
}
