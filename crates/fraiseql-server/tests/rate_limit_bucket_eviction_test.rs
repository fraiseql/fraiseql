//! #1080 — the in-memory rate limiter's bucket map must be swept, or `max_buckets`
//! becomes a permanent lockout.
//!
//! **Execution engine:** none (`NoopAdapter`)
//! **Infrastructure:** none — in-process HTTP over an ephemeral port
//! **Parallelism:** safe (no process-global state, no fixed port)
//!
//! `InMemoryRateLimiter::cleanup()` evicts stale buckets, and `RateLimitConfig` carries a
//! `cleanup_interval_secs` knob documented as the cadence it runs on. Until this test,
//! **nothing in the server ever called it**: `builder.rs` spawned tickers for PKCE state,
//! the local-auth sweeps and session-state eviction, and none for the rate limiter. Once
//! `ip_buckets` reached `max_buckets`, every previously-unseen key was denied — and stayed
//! denied for the life of the process, because the only thing that could have freed a slot
//! was a function with no caller.
//!
//! The exposure is narrower than "rate limiting is broken": rate limiting is **off unless
//! configured** (`resolve_rate_limiter_in` returns `Ok(None)` with no `[rate_limiting]`
//! table, no compiled `security.rate_limiting`, and no override), so a default deployment
//! is unaffected. But an operator who turned it on got a permanent availability failure
//! against *new unauthenticated* clients — which is every login and registration attempt,
//! so nobody new could onboard.
//!
//! The test drives `serve_on_listener` — the shipped in-process entry point — rather than a
//! hand-built `Router`, because the defect was in what the server *spawns*, and a router
//! assembled by the test would spawn nothing either way and pass for the wrong reason.

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

/// `max_buckets` small enough to fill in a handful of requests, and a cleanup cadence
/// short enough that a test need not wait minutes for it.
///
/// `rps_per_ip`/`burst_size` are set high so the *rate* limit never fires — the only thing
/// that may deny a request here is the bucket-capacity branch. Otherwise a denial would be
/// ambiguous between "the map is full" (the defect) and "you sent too many requests"
/// (working as intended), and the test would pass whether or not eviction ran.
const fn rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        enabled:               true,
        rps_per_ip:            10_000,
        rps_per_user:          10_000,
        burst_size:            10_000,
        cleanup_interval_secs: 1,
        trust_proxy_headers:   false,
        trusted_proxy_cidrs:   Vec::new(),
        max_buckets:           8,
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

struct Client {
    port: u16,
    http: reqwest::Client,
}

impl Client {
    /// One request carrying a distinct `X-Tenant-ID`. The in-memory limiter keys its IP
    /// bucket `"{tenant}:{ip}"`, so each distinct value mints a distinct bucket — the
    /// cheapest way to fill the map, and the reason the fill is trivially attacker-driven
    /// (that amplification is a separate defect, filed separately).
    async fn get_health_as_tenant(&self, tenant: &str) -> reqwest::StatusCode {
        self.http
            .get(format!("http://127.0.0.1:{}/health", self.port))
            .header("X-Tenant-ID", tenant)
            .send()
            .await
            .expect("request")
            .status()
    }
}

#[tokio::test]
async fn a_full_bucket_map_recovers_after_one_cleanup_interval() {
    let server = Server::new(config(), CompiledSchema::default(), Arc::new(NoopAdapter), None)
        .await
        .expect("Server::new");

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

    let client = Client {
        port,
        http: reqwest::Client::new(),
    };

    // Fill the map. `max_buckets` is 8, so well past it.
    for i in 0..40 {
        let _ = client.get_health_as_tenant(&format!("fill-{i}")).await;
    }

    // Precondition: the map is now full, so a previously-unseen client is refused.
    // If this does not hold the rest of the test proves nothing, so it is asserted
    // rather than assumed.
    let denied = client.get_health_as_tenant("newcomer-before").await;
    assert_eq!(
        denied,
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        "precondition: with max_buckets exhausted, a new client must be refused — \
         if this is not 429 the fill did not work and the recovery assertion below is vacuous"
    );

    // THE ASSERTION. Poll rather than sleeping a fixed span: the sweep is a ticker, so
    // the recovery is eventually-consistent, and a bare sleep is a flake on a loaded
    // self-hosted runner. Bounded so a failure is a failure and not a hang.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut last = reqwest::StatusCode::TOO_MANY_REQUESTS;
    let mut recovered = false;
    let mut attempt = 0;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        attempt += 1;
        last = client.get_health_as_tenant(&format!("newcomer-after-{attempt}")).await;
        if last != reqwest::StatusCode::TOO_MANY_REQUESTS {
            recovered = true;
            break;
        }
    }

    let _ = tx.send(());
    let _ = handle.await;

    assert!(
        recovered,
        "a new client must be served again once stale buckets are swept \
         (cleanup_interval_secs = 1, waited up to 15s); still got {last}. \
         Without a spawned sweep the denial is permanent for the life of the process — \
         `max_buckets` stops being a cap and becomes a lockout (#1080)."
    );
}
