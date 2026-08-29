//! #1173 — the in-memory rate limiter's bucket sweep must be SCHEDULED, not merely
//! callable.
//!
//! **Execution engine:** none (`NoopAdapter`)
//! **Infrastructure:** none — in-process HTTP over an ephemeral port
//! **Parallelism:** safe (no process-global state, no fixed port)
//!
//! #1080's defect was that `InMemoryRateLimiter::cleanup()` and the
//! `cleanup_interval_secs` knob both existed while **nothing in the server ever called
//! it** — a function with no caller. The fix spawns a ticker in
//! `builder.rs::spawn_rate_limit_cleanup`, reached from `from_executor`.
//!
//! The regression guard for that wiring was lost. `rate_limit_bucket_eviction_test.rs`
//! was deleted because #1143 made its premise unreachable — it filled the map with 40
//! distinct `X-Tenant-ID` values and asserted a newcomer was then refused with 429, and
//! #1143 (correctly) removed the tenant from the IP bucket key, so 40 values now mint
//! **one** bucket, and made a full map evict LRU rather than deny, so there is no
//! capacity refusal to recover from. From a single host over HTTP a test can no longer
//! mint a second IP bucket at all, which is exactly the property #1143 bought.
//!
//! The 76 limiter unit tests call `cleanup()` **directly**, so they prove the sweep
//! WORKS and never that it is SCHEDULED: deleting the spawn leaves every one of them
//! green. This test asserts the schedule instead, by observing the effect.
//!
//! ## Why it is shaped like this
//!
//! It drives `serve_on_listener` — the shipped in-process entry point — rather than a
//! hand-built `Router`, for the same reason the deleted test did: the defect is in what
//! the server **spawns**, and a router assembled inside the test spawns nothing either
//! way, so it would pass whether or not the wiring exists.
//!
//! It observes `live_bucket_count()` rather than a 429, because under #1143's
//! evict-never-deny there is no denial to observe. The count is the only remaining
//! signal that distinguishes "the sweep ran" from "the sweep does not exist".
//!
//! Staleness is `burst_size / rps_per_ip` seconds (see `InMemoryRateLimiter::cleanup`),
//! so the config below makes a bucket stale after 1s and the ticker fire every 1s, and
//! the rate limit itself never fires — a denial here would make the test ambiguous.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        DatabaseAdapter, DatabaseType, SupportsMutations, WhereClause,
        types::{JsonbValue, OrderByClause, PoolMetrics},
    },
    error::Result as FraiseQLResult,
    schema::{CompiledSchema, SqlProjectionHint},
};
use fraiseql_server::{Server, middleware::RateLimitConfig, server_config::ServerConfig};

#[derive(Debug, Clone)]
struct NoopAdapter;

#[async_trait]
impl DatabaseAdapter for NoopAdapter {
    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> FraiseQLResult<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl SupportsMutations for NoopAdapter {}

/// A bucket goes stale after `burst_size / rps_per_ip` seconds — 10_000/10_000 = **1s**
/// — and the ticker fires every `cleanup_interval_secs` = **1s**.
///
/// `rps_per_ip`/`burst_size` are equal and high so the *rate* limit never fires: every
/// request here must be allowed, or a denial would confound the count. `max_buckets` is
/// left generous for the same reason — this test is about the sweep, not the cap.
const fn rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        enabled:               true,
        rps_per_ip:            10_000,
        rps_per_user:          10_000,
        burst_size:            10_000,
        cleanup_interval_secs: 1,
        trust_proxy_headers:   false,
        trusted_proxy_cidrs:   Vec::new(),
        max_buckets:           1_024,
    }
}

fn config() -> ServerConfig {
    ServerConfig {
        schema_path: "/nonexistent/schema.compiled.json".into(),
        cors_enabled: false,
        cache_enabled: false,
        rate_limiting: Some(rate_limit_config()),
        ..ServerConfig::default()
    }
}

#[tokio::test]
async fn the_bucket_sweep_is_scheduled_by_the_server_that_configures_it() {
    let server = Server::new(config(), CompiledSchema::default(), Arc::new(NoopAdapter), None)
        .await
        .expect("Server::new");

    // Cloned BEFORE the server moves into serve_on_listener: this Arc is the only way to
    // observe the sweep's effect from outside the `rate_limit` module.
    let limiter = Arc::clone(
        server
            .rate_limiter()
            .expect("a config with [rate_limiting] enabled must resolve a limiter"),
    );
    assert_eq!(
        limiter.live_bucket_count(),
        Some(0),
        "precondition: a freshly built limiter holds no buckets"
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });

    let http = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/health");
    for _ in 0..4 {
        let status = http.get(&url).send().await.expect("request").status();
        assert!(
            status.is_success(),
            "the rate limit must not fire in this test — a denial would make the bucket \
             count ambiguous; got {status}"
        );
    }

    // Precondition: requests minted at least one bucket. Without this the assertion
    // below would be satisfied by a limiter that never had anything to sweep, which is
    // the way this test could most easily pass for the wrong reason.
    let minted = limiter.live_bucket_count().expect("in-memory limiter reports a count");
    assert!(
        minted > 0,
        "precondition: {minted} buckets after 4 requests — the requests must mint a \
         bucket, or the sweep has nothing to evict and the assertion below is vacuous"
    );

    // THE ASSERTION. Poll rather than sleeping a fixed span: the sweep is a ticker, so
    // the effect is eventually-consistent, and a bare sleep flakes on a loaded runner.
    // Bounded, so a failure is a failure and not a hang.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut swept = false;
    let mut last = minted;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        last = limiter.live_bucket_count().expect("in-memory limiter reports a count");
        if last == 0 {
            swept = true;
            break;
        }
    }

    let _ = tx.send(());
    let _ = handle.await;

    assert!(
        swept,
        "the bucket map must be swept by a ticker the SERVER spawns \
         (cleanup_interval_secs = 1, staleness 1s, waited up to 20s); \
         still holding {last} bucket(s) of the {minted} minted.\n\n\
         If this fails, `spawn_rate_limit_cleanup` is no longer reached from \
         `from_executor` — `cleanup()` is back to being a function with no caller, which \
         is exactly #1080. Note the 76 limiter unit tests call `cleanup()` directly and \
         stay green in that state, so this is the only assertion that would notice."
    );
}
