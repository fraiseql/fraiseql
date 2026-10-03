#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #1390 — with read replicas configured, a source that depends on an UNLOGGED
//! table is refused at boot instead of failing every query on every replica.
//!
//! `pg_tviews` creates its `tv_*` tables UNLOGGED by default, and PostgreSQL will not read
//! an UNLOGGED relation during recovery. Driven against the real primary and its
//! streaming standby (`STANDBY_DATABASE_URL`, as the read-replica suite is): the test
//! first shows the standby refusing the read — the defect — then that the check names
//! the view and the table behind it, follows a view to its table, and passes logged
//! sources.

use std::time::Duration;

use fraiseql_core::{
    db::postgres::{
        PoolPrewarmConfig, PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy, VectorScanConfig,
    },
    schema::{CompiledSchema, refuse_standby_unreadable_sources, standby_unreadable_sources},
};
use serde_json::json;
use tokio_postgres::NoTls;

const SCHEMA: &str = "issue_1390";

async fn admin(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

/// One query reading `view`.
fn schema_reading(view: &str) -> CompiledSchema {
    serde_json::from_value(json!({
        "types": [{
            "name": "Post",
            "sql_source": format!("{SCHEMA}.{view}"),
            "fields": [{ "name": "id", "field_type": "Int" }]
        }],
        "queries": [{
            "name": "posts",
            "return_type": "Post",
            "returns_list": true,
            "nullable": false,
            "sql_source": format!("{SCHEMA}.{view}")
        }]
    }))
    .unwrap()
}

async fn adapter(with_replica: bool) -> PostgresAdapter {
    let read_replicas = with_replica.then(|| {
        ReadReplicaPolicy::default()
            .with_urls(vec![fraiseql_test_support::standby_database_url()])
            .unwrap()
    });
    PostgresAdapter::with_pool_config(
        &fraiseql_test_support::database_url(),
        PoolPrewarmConfig {
            min_size: 0,
            max_size: 2,
            timeout_secs: None,
            search_path: None,
            max_streaming_reads: None,
            vector_scan: VectorScanConfig::default(),
            tls: PostgresTlsConfig::default(),
            read_replicas,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn an_unlogged_source_is_refused_when_reads_go_to_replicas() {
    let primary = admin(&fraiseql_test_support::database_url()).await;
    primary
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE UNLOGGED TABLE {SCHEMA}.tv_post (id int PRIMARY KEY, data jsonb NOT NULL);
             CREATE VIEW {SCHEMA}.v_post_unlogged AS SELECT id, data FROM {SCHEMA}.tv_post;
             CREATE TABLE {SCHEMA}.tb_post (id int PRIMARY KEY, data jsonb NOT NULL);
             CREATE VIEW {SCHEMA}.v_post AS SELECT id, data FROM {SCHEMA}.tb_post;"
        ))
        .await
        .unwrap();

    // The defect: the standby refuses the read once it has replayed the catalog.
    let standby = admin(&fraiseql_test_support::standby_database_url()).await;
    let mut refused = None;
    for _ in 0..50 {
        match standby
            .simple_query(&format!("SELECT data FROM {SCHEMA}.v_post_unlogged"))
            .await
        {
            Err(e) if e.to_string().contains("does not exist") => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            },
            other => {
                refused = Some(other);
                break;
            },
        }
    }
    let err = refused
        .expect("standby replayed the DDL")
        .expect_err("a standby cannot read it");
    assert!(
        format!("{err:?}").contains("unlogged"),
        "the premise: a hot standby refuses an unlogged relation: {err:?}"
    );

    let with_replica = adapter(true).await;

    // The view is followed to the unlogged table it reads.
    let found = standby_unreadable_sources(&with_replica, &schema_reading("v_post_unlogged"))
        .await
        .unwrap();
    assert!(
        found.iter().any(|f| f.contains("v_post_unlogged") && f.contains("tv_post")),
        "{found:?}"
    );
    let message =
        refuse_standby_unreadable_sources(&with_replica, &schema_reading("v_post_unlogged"))
            .await
            .unwrap_err()
            .to_string();
    assert!(
        message.contains("SET LOGGED") && message.contains("read_replica_urls"),
        "{message}"
    );

    // The server boots through the cache wrapper; it must not switch the check off.
    let cached = fraiseql_core::cache::CachedDatabaseAdapter::new(
        adapter(true).await,
        fraiseql_core::cache::QueryResultCache::new(fraiseql_core::cache::CacheConfig::enabled()),
        "1".to_string(),
    );
    refuse_standby_unreadable_sources(&cached, &schema_reading("v_post_unlogged"))
        .await
        .expect_err("the cached adapter must report its replicas");

    // A logged source is served by replicas: nothing to refuse.
    refuse_standby_unreadable_sources(&with_replica, &schema_reading("v_post"))
        .await
        .unwrap();

    // Without replicas the unlogged source is fine: every read goes to the primary.
    refuse_standby_unreadable_sources(&adapter(false).await, &schema_reading("v_post_unlogged"))
        .await
        .unwrap();
}
