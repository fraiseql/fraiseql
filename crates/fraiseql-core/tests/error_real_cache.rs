#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Integration tests for `CachedDatabaseAdapter` with a real PostgreSQL backend.
//!
//! Validates cache behavior (hit/miss/invalidation) against a real database,
//! ensuring the cache layer works correctly with real query results.

mod common;

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache, ViewName},
    db::DatabaseAdapter,
};

#[tokio::test]
async fn cache_hit_avoids_second_real_query() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // First call — cache miss, hits real DB
    let r1 = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();

    // Second identical call — cache hit
    let r2 = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();

    assert_eq!(r1.len(), r2.len());

    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.hits, 1, "second call should be a cache hit");
    assert_eq!(metrics.misses, 1, "first call should be a cache miss");
}

#[tokio::test]
async fn cache_invalidation_forces_refetch() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Populate cache
    let _ = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();

    // Invalidate
    let evicted = cached.invalidate_views(&[ViewName::from("test.v_user")]).unwrap();
    assert!(evicted > 0, "should have evicted at least one entry");

    // Query again — should miss
    let _ = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();

    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.misses, 2, "post-invalidation call should miss");
    assert_eq!(metrics.hits, 0, "no hits expected after invalidation");
}

#[tokio::test]
async fn different_views_cache_independently() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Query two different views
    let _ = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();
    let _ = cached
        .execute_where_query("test.v_project", None, None, None, None)
        .await
        .unwrap();

    // Repeat both — should hit
    let _ = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();
    let _ = cached
        .execute_where_query("test.v_project", None, None, None, None)
        .await
        .unwrap();

    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.misses, 2);
    assert_eq!(metrics.hits, 2);
}

#[tokio::test]
async fn different_query_params_cache_independently() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Query with limit 1
    let r1 = cached
        .execute_where_query("test.v_project", None, Some(1), None, None)
        .await
        .unwrap();

    // Query with limit 2 — different cache key
    let r2 = cached
        .execute_where_query("test.v_project", None, Some(2), None, None)
        .await
        .unwrap();

    // Both should be misses (different params = different cache keys)
    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.misses, 2, "different limits should produce different cache keys");
    assert_eq!(metrics.hits, 0);

    assert!(r1.len() <= 1);
    assert!(r2.len() <= 2);
}

#[tokio::test]
async fn disabled_cache_always_hits_database() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::disabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Query twice
    let r1 = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();
    let r2 = cached.execute_where_query("test.v_user", None, None, None, None).await.unwrap();

    // Both should return data (from DB each time)
    assert_eq!(r1.len(), r2.len());

    let metrics = cached.cache().metrics().unwrap();
    // With caching disabled, no hits
    assert_eq!(metrics.hits, 0);
}

// ── #1584: a write fences only the cache fills of what it can reach ─────────────────────
//
// A read is get → miss → await the database → put. An invalidation landing inside the
// await must refuse the put when it can reach the read (#1079), and must not when it
// cannot: under steady writes some invalidation is always in flight, and a fence that
// refuses every fill caches nothing at all.
//
// The straddle is a sequence, not a sleep: the view blocks on an advisory lock the test
// holds, the test waits until the backend is seen waiting on it, invalidates, then lets
// the read finish. A sequence counts how often the view really ran, so "served from the
// cache" is a database fact, not the cache's own word.

/// One straddle fixture: `<schema>.v_gated` blocks on advisory lock `key` and counts its
/// executions in `<schema>.reads`. Its entity type is `Gated` (rows carry an `id`).
struct Gated {
    schema: &'static str,
    key:    i64,
    admin:  tokio_postgres::Client,
}

impl Gated {
    async fn new(schema: &'static str, key: i64) -> Self {
        let url = common::testcontainer::get_test_container().await.connection_string();
        let (admin, connection) =
            tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        admin
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {schema} CASCADE;
                 CREATE SCHEMA {schema};
                 CREATE SEQUENCE {schema}.reads;
                 CREATE FUNCTION {schema}.gate() RETURNS boolean LANGUAGE sql VOLATILE AS
                   $$ SELECT pg_advisory_lock_shared({key}) IS NOT NULL
                             AND pg_advisory_unlock_shared({key})
                             AND nextval('{schema}.reads') > 0 $$;
                 CREATE TABLE {schema}.tb_gated (data jsonb NOT NULL);
                 INSERT INTO {schema}.tb_gated VALUES ('{{\"id\": \"g-1\", \"n\": 1}}');
                 CREATE VIEW {schema}.v_gated AS
                   SELECT data FROM {schema}.tb_gated WHERE {schema}.gate();"
            ))
            .await
            .unwrap();
        Self { schema, key, admin }
    }

    fn view(&self) -> String {
        format!("{}.v_gated", self.schema)
    }

    async fn reads(&self) -> i64 {
        let row = self
            .admin
            .query_one(&format!("SELECT last_value FROM {}.reads", self.schema), &[])
            .await
            .unwrap();
        row.get(0)
    }

    /// Run one read of the view, and while it is blocked inside the database, run
    /// `during`. Returns once the read has finished (and put, or refused to put).
    async fn straddle<A>(
        &self,
        cached: &std::sync::Arc<CachedDatabaseAdapter<A>>,
        during: impl FnOnce(),
    ) where
        A: DatabaseAdapter + 'static,
    {
        self.admin.execute("SELECT pg_advisory_lock($1)", &[&self.key]).await.unwrap();
        let reader = std::sync::Arc::clone(cached);
        let view = self.view();
        let read = tokio::spawn(async move {
            reader.execute_where_query(&view, None, None, None, None).await.unwrap()
        });
        // Wait until the read is parked on the lock: from here on it is inside its round trip.
        loop {
            let waiting: i64 = self
                .admin
                .query_one(
                    "SELECT count(*) FROM pg_locks
                      WHERE locktype = 'advisory' AND NOT granted AND objid = $1::bigint::oid",
                    &[&self.key],
                )
                .await
                .unwrap()
                .get(0);
            if waiting > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        during();
        self.admin.execute("SELECT pg_advisory_unlock($1)", &[&self.key]).await.unwrap();
        let rows = read.await.unwrap();
        assert_eq!(rows.len(), 1, "precondition: the straddling read returns its row");
    }

    async fn drop(self) {
        self.admin
            .batch_execute(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .await
            .unwrap();
    }
}

async fn gated_cache()
-> std::sync::Arc<CachedDatabaseAdapter<fraiseql_core::db::postgres::PostgresAdapter>> {
    let adapter = common::testcontainer::get_test_adapter().await;
    std::sync::Arc::new(CachedDatabaseAdapter::new(
        adapter,
        QueryResultCache::new(CacheConfig::enabled()),
        "v1".to_string(),
    ))
}

/// Re-read after a straddle and report whether the database ran again.
async fn rereads(
    g: &Gated,
    cached: &CachedDatabaseAdapter<fraiseql_core::db::postgres::PostgresAdapter>,
) -> bool {
    let before = g.reads().await;
    cached.execute_where_query(&g.view(), None, None, None, None).await.unwrap();
    g.reads().await != before
}

/// The counterweight every refusal below rests on: without a straddle, the read is cached.
#[tokio::test(flavor = "multi_thread")]
async fn an_unstraddled_gated_read_is_served_from_the_cache() {
    let g = Gated::new("f1584a", 1_584_001).await;
    let cached = gated_cache().await;
    g.straddle(&cached, || {}).await;
    assert!(!rereads(&g, &cached).await, "an uncontended read must be cached");
    g.drop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_straddling_a_write_to_an_unrelated_view_is_cached() {
    let g = Gated::new("f1584b", 1_584_002).await;
    let cached = gated_cache().await;
    g.straddle(&cached, || {
        cached.invalidate_views(&[ViewName::from("f1584b.v_unrelated")]).unwrap();
    })
    .await;
    assert!(
        !rereads(&g, &cached).await,
        "a write to a view this read cannot see must not refuse its cache fill (#1584): \
         under steady writes the cache would store nothing"
    );
    g.drop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_straddling_a_write_to_its_own_view_is_not_cached() {
    let g = Gated::new("f1584c", 1_584_003).await;
    let cached = gated_cache().await;
    let view = g.view();
    g.straddle(&cached, || {
        cached.invalidate_views(&[ViewName::from(view.as_str())]).unwrap();
    })
    .await;
    assert!(
        rereads(&g, &cached).await,
        "a read whose round trip straddled a write to its own view must not be cached (#1079)"
    );
    g.drop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_straddling_an_entity_eviction_of_another_type_is_cached() {
    let g = Gated::new("f1584d", 1_584_004).await;
    let cached = gated_cache().await;
    g.straddle(&cached, || {
        cached.invalidate_by_entity("Unrelated", "x-1").unwrap();
    })
    .await;
    assert!(
        !rereads(&g, &cached).await,
        "an entity eviction of a type this read does not hold must not refuse its fill (#1584)"
    );
    g.drop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_straddling_an_entity_eviction_of_a_row_it_holds_is_not_cached() {
    let g = Gated::new("f1584e", 1_584_005).await;
    let cached = gated_cache().await;
    g.straddle(&cached, || {
        cached.invalidate_by_entity("Gated", "g-1").unwrap();
    })
    .await;
    assert!(
        rereads(&g, &cached).await,
        "a read straddling an eviction of a row it holds must not be cached (#1079)"
    );
    g.drop().await;
}

/// #1605: an eviction of another row of the type reaches no fill that does not hold it, so
/// it refuses none. A per-type fence refused this one, and under steady writes to other rows
/// of a type, every fill holding that type.
#[tokio::test(flavor = "multi_thread")]
async fn a_read_straddling_an_entity_eviction_of_another_row_of_its_type_is_cached() {
    let g = Gated::new("f1605a", 1_605_001).await;
    let cached = gated_cache().await;
    g.straddle(&cached, || {
        cached.invalidate_by_entity("Gated", "g-2").unwrap();
    })
    .await;
    assert!(
        !rereads(&g, &cached).await,
        "an eviction of Gated g-2 must not refuse the fill of a read holding only g-1 (#1605)"
    );
    g.drop().await;
}
