# Configuration Examples

This guide shows configurations for different deployment scenarios.

`ObserverRuntimeConfig` (`src/config/runtime.rs`) derives `Deserialize` and has no `Default`
and no loader of its own, so the examples below are TOML that you parse with
`toml::from_str::<ObserverRuntimeConfig>(..)` (add the `toml` crate to your own
`Cargo.toml`). Its fields are:

| Field | Type | Default |
|-------|------|---------|
| `transport` | `TransportConfig` (table) | Postgres, executors on, bridge off |
| `redis` | `Option<RedisConfig>` | absent |
| `clickhouse` | `Option<ClickHouseConfig>` | absent |
| `job_queue` | `Option<JobQueueConfig>` | absent |
| `performance` | `PerformanceConfig` | dedup off, caching off, concurrent on |
| `channel_capacity` | `usize` | `1000` |
| `max_concurrency` | `usize` | `50` |
| `backlog_alert_threshold` | `usize` | `500` |
| `shutdown_timeout` | `String` | `"30s"` |
| `max_dlq_size` | `Option<usize>` | unbounded |
| `observers` | map of name → `ObserverDefinition` | empty |

Top-level scalar keys must come **before** the first `[table]` in a TOML file.

Other pieces of the crate are **not** fields of this struct and are built separately: the
checkpoint store, the circuit breaker, the multi-listener coordinator, the search sink and
the metrics registry. Each has its own section below.

## Table of Contents

1. [Production Setup](#production-setup)
2. [Development Setup](#development-setup)
3. [High-Performance Setup](#high-performance-setup)
4. [Budget Setup](#budget-setup)
5. [Feature-Specific Examples](#feature-specific-examples)
6. [Environment Overrides](#environment-overrides)
7. [Checklist: Configuration Review](#checklist-configuration-review)

---

## Production Setup

**Recommended for**: Mission-critical production systems

**Characteristics**:

- PostgreSQL transport, with durable checkpoints (`PostgresCheckpointStore`)
- Redis for deduplication and caching
- A Redis-backed job queue for asynchronous actions
- A bounded dead letter queue
- Multiple listeners for failover

### Cargo.toml

```toml
[dependencies]
fraiseql-observers = { version = "2", features = ["checkpoint", "dedup", "caching", "queue"] }
```

The `enterprise` feature is the bundle `checkpoint`, `dedup`, `caching`, `queue`, `search`
and `metrics`.

### Runtime Configuration

```toml
channel_capacity = 5000
max_concurrency = 100
backlog_alert_threshold = 2000
shutdown_timeout = "30s"
# When the dead letter queue reaches this size the newest entry is dropped, with a warning.
max_dlq_size = 10000

[transport]
transport = "postgres"

[redis]
url = "redis://redis:6379/0"
pool_size = 20
connect_timeout_secs = 5
command_timeout_secs = 2
dedup_window_secs = 600   # 10 minutes, to catch retries (1..=3600)
cache_ttl_secs = 300      # 5 minutes (1..=3600)

[job_queue]
url = "redis://redis:6379/0"
batch_size = 100
worker_concurrency = 50
max_retries = 5
initial_delay_ms = 100
max_delay_ms = 30000

[performance]
enable_dedup = true       # requires [redis]
enable_caching = true     # requires [redis]
enable_concurrent = true
max_concurrent_actions = 50
concurrent_timeout_ms = 30000

[observers.order_created]
event_type = "INSERT"
entity = "Order"
on_failure = "dlq"

[observers.order_created.retry]
max_attempts = 5
initial_delay_ms = 100
max_delay_ms = 30000
backoff_strategy = "exponential"

[[observers.order_created.actions]]
type = "webhook"
url_env = "ORDER_WEBHOOK_URL"
signing_secret_env = "ORDER_WEBHOOK_SECRET"
```

Load it, then build the executor stack from it:

```rust
use std::sync::Arc;

use fraiseql_observers::{
    ObserverError, ObserverRuntimeConfig, factory::ExecutorFactory, traits::DeadLetterQueue,
};

async fn start(
    toml_text: &str,
    dlq: Arc<dyn DeadLetterQueue>,
) -> fraiseql_observers::Result<()> {
    let config: ObserverRuntimeConfig = toml::from_str(toml_text)
        .map_err(|e| ObserverError::InvalidConfig { message: e.to_string() })?;
    config.validate()?;

    // Wraps the base executor with deduplication and caching according to `config.performance`.
    let _executor = ExecutorFactory::build(&config, dlq).await?;
    Ok(())
}
```

### Checkpoints and listener identity

Checkpoints are written by the listener, not configured in `ObserverRuntimeConfig`.
`PostgresCheckpointStore` takes a `PgPool`; its `observer_checkpoints` table comes from
`migrations/02_create_observer_checkpoints.sql`:

```rust
use fraiseql_observers::{ChangeLogListenerConfig, PostgresCheckpointStore};
use sqlx::PgPool;

fn listener(pool: PgPool) -> (ChangeLogListenerConfig, PostgresCheckpointStore) {
    let config =
        ChangeLogListenerConfig::new(pool.clone()).with_listener_id("orders-listener-1");
    (config, PostgresCheckpointStore::new(pool))
}
```

Use the **same** listener id for the listener and for the checkpoint store, so the cursor and
the dispatch ledger describe one listener.

### Environment Setup

Each config section has `with_env_overrides`; see [Environment Overrides](#environment-overrides)
for the variable names.

```bash
FRAISEQL_REDIS_URL=redis://:secure_password@redis:6379/0
FRAISEQL_JOB_QUEUE_URL=redis://:secure_password@redis:6379/0
ORDER_WEBHOOK_URL=https://example.com/hooks/orders
ORDER_WEBHOOK_SECRET=change-me
```

---

## Development Setup

**Recommended for**: Local development and testing

**Characteristics**:

- Minimal external dependencies
- No Redis: no deduplication, no caching, no job queue
- In-memory checkpoints (`InMemoryCheckpointStore`, **not durable**)
- Short retry delays

### Cargo.toml

```toml
[dependencies]
fraiseql-observers = { version = "2", features = ["checkpoint"] }
```

### Runtime Configuration

```toml
[transport]
transport = "in_memory"

[performance]
enable_dedup = false
enable_caching = false

[observers.order_created]
event_type = "INSERT"
entity = "Order"

[observers.order_created.retry]
max_attempts = 2
initial_delay_ms = 10
backoff_strategy = "fixed"

[[observers.order_created.actions]]
type = "webhook"
url = "https://example.test/hook"
```

Outbound action URLs go through the crate's SSRF check, which rejects loopback and private
addresses such as `http://localhost:8080`. For local development only, the
`FRAISEQL_OBSERVERS_ALLOW_INSECURE` bypass exists; it is refused when any production marker
is set (see `src/insecure_guard.rs`).

### In-memory checkpoints

```rust
use fraiseql_observers::InMemoryCheckpointStore;

let store = InMemoryCheckpointStore::new(); // state is lost on every restart
```

Postgres is the only durable checkpoint store. `check_checkpoint_requirement` and
`CheckpointMode::DevOnly` exist so that running without one is an explicit choice.

---

## High-Performance Setup

**Recommended for**: High-throughput systems

**Characteristics**:

- NATS transport (JetStream), so that executors can scale out
- Larger channel and concurrency limits
- Redis deduplication and caching, with a larger connection pool
- Many job-queue workers

### Cargo.toml

```toml
[dependencies]
fraiseql-observers = { version = "2", features = ["checkpoint", "dedup", "caching", "queue", "nats"] }
```

### Runtime Configuration

```toml
channel_capacity = 20000
max_concurrency = 200
backlog_alert_threshold = 10000
max_dlq_size = 50000

[transport]
transport = "nats"
run_executors = true

[transport.nats]
url = "nats://nats:4222"
stream_name = "fraiseql_events"
consumer_name = "fraiseql_observer_worker"

[transport.nats.jetstream]
dedup_window_minutes = 5   # 1..=60
ack_wait_secs = 30
max_deliver = 3

[redis]
url = "redis://redis:6379/0"
pool_size = 20
dedup_window_secs = 300
cache_ttl_secs = 600

[job_queue]
url = "redis://redis:6379/0"
batch_size = 500
worker_concurrency = 100
poll_interval_ms = 200

[performance]
enable_dedup = true
enable_caching = true
enable_concurrent = true
max_concurrent_actions = 100
concurrent_timeout_ms = 15000
```

`run_bridge = true` (the Postgres-to-NATS bridge, configured under `[transport.bridge]`)
requires `transport = "nats"`; `TransportConfig::validate` rejects it otherwise.

---

## Budget Setup

**Recommended for**: Cost-conscious deployments, non-critical systems

**Characteristics**:

- PostgreSQL only: no Redis, no NATS
- A single node
- Durable checkpoints, so that a restart does not lose events

### Cargo.toml

```toml
[dependencies]
fraiseql-observers = { version = "2", features = ["checkpoint"] }
```

### Runtime Configuration

```toml
channel_capacity = 1000
max_concurrency = 20

[transport]
transport = "postgres"

[performance]
enable_dedup = false      # both need [redis]; PerformanceConfig::validate rejects them without it
enable_caching = false
max_concurrent_actions = 5
```

`ExecutorFactory::build_postgres_only` builds this shape and returns
`ObserverError::InvalidConfig` if deduplication or caching is enabled.

---

## Feature-Specific Examples

### Example 1: Checkpoint Configuration

The listener's batch size and poll interval live on `ChangeLogListenerConfig`:

```rust
use fraiseql_observers::ChangeLogListenerConfig;

// `pool` is your `sqlx::PgPool`.

// Safest: small batches, frequent polls.
let mut careful = ChangeLogListenerConfig::new(pool.clone());
careful.batch_size = 10;
careful.poll_interval_ms = 50;

// Throughput: larger batches.
let mut fast = ChangeLogListenerConfig::new(pool.clone());
fast.batch_size = 1000;
```

Delivery is at-least-once by default (`CheckpointStrategy::AtLeastOnce`): a crash between the
side effect and the checkpoint write redelivers the event. Use
`CheckpointStrategy::EffectivelyOnce { idempotency_table }` when side effects are not
idempotent; it costs one extra database round trip per event.

---

### Example 2: Retry Configuration

Retries are configured **per observer**, in `[observers.<name>.retry]`. `RetryConfig` rejects
unknown keys. The delay before the retry that follows failed attempt *n* is:

| `backoff_strategy` | Delay | Example (`initial_delay_ms = 100`, `max_delay_ms = 10000`) |
|--------------------|-------|-----------------------------------------------------------|
| `exponential` (default) | `2^(n-1) × initial`, capped at `max`, with ±25% jitter | about 100, 200, 400, 800 ms … |
| `linear` | `n × initial`, capped at `max` | 100, 200, 300, 400 ms … |
| `fixed` | `initial` | 100, 100, 100 ms … |

```toml
[observers.payment_failed.retry]
max_attempts = 5
initial_delay_ms = 100
max_delay_ms = 10000
backoff_strategy = "linear"
```

`on_failure` decides what happens after the last attempt: `"log"` (default), `"alert"` or
`"dlq"`.

---

### Example 3: Circuit Breaker Configuration

`CircuitBreakerConfig` is a plain struct, passed to `CircuitBreaker::new`:

```rust
use fraiseql_observers::{CircuitBreaker, CircuitBreakerConfig};

// Aggressive: fail fast to protect an expensive downstream.
let aggressive = CircuitBreaker::new(CircuitBreakerConfig {
    failure_threshold: 0.2,       // open at a 20% failure rate
    sample_size: 50,
    open_timeout_ms: 10_000,      // probe again after 10 s
    half_open_max_requests: 3,
});

// Conservative: tolerate brief outages.
let conservative = CircuitBreaker::new(CircuitBreakerConfig {
    failure_threshold: 0.7,
    sample_size: 1000,
    open_timeout_ms: 300_000,
    half_open_max_requests: 10,
});
```

The default is a 50% failure rate over the last 100 requests, 30 s before half-open, and up to
5 half-open requests.

---

### Example 4: Cache and Deduplication Windows

Both live in `[redis]` and are switched on in `[performance]`:

```toml
[redis]
url = "redis://redis:6379"
cache_ttl_secs = 60       # 1..=3600
dedup_window_secs = 300   # 1..=3600

[performance]
enable_caching = true
enable_dedup = true
```

A longer `cache_ttl_secs` serves staler results; a shorter one re-runs the action more often.
A value of `0`, or above `3600`, fails `RedisConfig::validate`.

---

### Example 5: Multi-Listener Configuration

`MultiListenerConfig` is its own struct; it is not a field of `ObserverRuntimeConfig`.
Standard failover:

```rust
use fraiseql_observers::MultiListenerConfig;

let config: MultiListenerConfig = toml::from_str(
    r#"
    enabled = true
    listener_id = "orders-listener-1"
    lease_duration_ms = 30000
    health_check_interval_ms = 5000
    failover_threshold_ms = 60000
    max_listeners = 3
    "#,
)
.expect("valid MultiListenerConfig TOML");
```

A tighter failover, for mission-critical systems, lowers `health_check_interval_ms` and
`failover_threshold_ms` (for example to `2000` and `10000`). With `enabled = false` (the
default) the instance runs alone. The coordinator is `MultiListenerCoordinator`;
`CheckpointLease::redis` (feature `redis-lease`) is the lease for multi-process setups.

---

### Example 6: Search Sink

With the `search` feature, `ElasticsearchSink` bulk-indexes events and `HttpSearchBackend`
(`HttpSearchBackend::new(url)`) is the Elasticsearch implementation of `SearchBackend`. The
sink is configured by `ElasticsearchSinkConfig`:

```rust
use fraiseql_observers::{ElasticsearchSink, ElasticsearchSinkConfig};

let sink = ElasticsearchSink::new(ElasticsearchSinkConfig {
    url: "http://elasticsearch:9200".to_string(),
    index_prefix: "fraiseql-events".to_string(),
    bulk_size: 1000,
    flush_interval_secs: 5,
    max_retries: 3,
})?;
```

The URL goes through the same SSRF check as action URLs, so a loopback or private IP literal
is rejected.

---

## Environment Overrides

`RedisConfig`, `JobQueueConfig`, `ClickHouseConfig`, `PerformanceConfig` and `TransportConfig`
each have `with_env_overrides()`, which replaces a field when its variable is set and parses.
`ObserverRuntimeConfig` itself has no such method, so apply it to the sections you use:

```rust
use fraiseql_observers::ObserverRuntimeConfig;

fn load(toml_text: &str) -> Result<ObserverRuntimeConfig, Box<dyn std::error::Error>> {
    let mut config: ObserverRuntimeConfig = toml::from_str(toml_text)?;
    config.transport = config.transport.with_env_overrides();
    config.performance = config.performance.with_env_overrides();
    config.redis = config.redis.map(|r| r.with_env_overrides());
    config.job_queue = config.job_queue.map(|q| q.with_env_overrides());
    config.validate()?;
    Ok(config)
}
```

| Section | Variables |
|---------|-----------|
| `redis` | `FRAISEQL_REDIS_URL`, `FRAISEQL_REDIS_POOL_SIZE`, `FRAISEQL_REDIS_CONNECT_TIMEOUT_SECS`, `FRAISEQL_REDIS_COMMAND_TIMEOUT_SECS`, `FRAISEQL_REDIS_DEDUP_WINDOW_SECS`, `FRAISEQL_REDIS_CACHE_TTL_SECS` |
| `job_queue` | `FRAISEQL_JOB_QUEUE_URL`, `FRAISEQL_JOB_QUEUE_BATCH_SIZE`, `FRAISEQL_JOB_QUEUE_BATCH_TIMEOUT_SECS`, `FRAISEQL_JOB_QUEUE_MAX_RETRIES`, `FRAISEQL_JOB_QUEUE_WORKER_CONCURRENCY`, `FRAISEQL_JOB_QUEUE_POLL_INTERVAL_MS`, `FRAISEQL_JOB_QUEUE_INITIAL_DELAY_MS`, `FRAISEQL_JOB_QUEUE_MAX_DELAY_MS` |
| `performance` | `FRAISEQL_ENABLE_DEDUP`, `FRAISEQL_ENABLE_CACHING`, `FRAISEQL_ENABLE_CONCURRENT`, `FRAISEQL_MAX_CONCURRENT_ACTIONS`, `FRAISEQL_CONCURRENT_TIMEOUT_MS` |
| `transport` | `FRAISEQL_OBSERVER_TRANSPORT` (`postgres`, `nats`, `in_memory`), `FRAISEQL_NATS_URL`, `FRAISEQL_NATS_ENABLE_BRIDGE`, `FRAISEQL_NATS_RUN_EXECUTORS`, plus the other `FRAISEQL_NATS_*` and `FRAISEQL_BRIDGE_*` variables read in `src/config/transport.rs` |
| `clickhouse` | `FRAISEQL_CLICKHOUSE_URL`, `FRAISEQL_CLICKHOUSE_DATABASE`, `FRAISEQL_CLICKHOUSE_TABLE`, `FRAISEQL_CLICKHOUSE_BATCH_SIZE`, `FRAISEQL_CLICKHOUSE_BATCH_TIMEOUT_SECS`, `FRAISEQL_CLICKHOUSE_MAX_RETRIES` |

To keep one configuration per environment, keep one TOML file per environment (for example
`observers.production.toml` and `observers.staging.toml`) and choose between them in your own
code; nothing in the crate selects one.

---

## Migration Path

Add capabilities one at a time; each is a Cargo feature plus a config section.

| Step | Features | Config | Adds |
|------|----------|--------|------|
| 1 | `checkpoint` | `[transport]` | No event loss across restarts; PostgreSQL only |
| 2 | `+ caching` | `[redis]`, `performance.enable_caching` | Redis dependency; fewer repeated action calls |
| 3 | `+ dedup` | `performance.enable_dedup` | Suppresses duplicate side effects; reuses the same Redis |
| 4 | `+ metrics` | | Prometheus metrics (`MetricsRegistry`) |
| 5 | `+ search` | | `ElasticsearchSink` audit trail |

---

## Performance Tuning

### Increase Throughput

1. **Larger listener batches**: `ChangeLogListenerConfig.batch_size = 1000`. Fewer round trips;
   more events are in flight if the process crashes.
2. **Longer cache TTL**: `redis.cache_ttl_secs = 600`. Staler results.
3. **More concurrency**: raise `max_concurrency`, `performance.max_concurrent_actions` and
   `job_queue.worker_concurrency`. More resource use.

### Reduce Latency

1. **Shorter cache TTL**: `redis.cache_ttl_secs = 10`.
2. **Shorter retry delays**: `initial_delay_ms = 10` with `backoff_strategy = "fixed"`.
3. **Smaller listener batches and a shorter `poll_interval_ms`**: more frequent database reads.

---

## Checklist: Configuration Review

- [ ] Selected the Cargo features for the use case
- [ ] Used one listener id for the `ChangeLogListenerConfig` and its checkpoint store
- [ ] Set `retry` and `on_failure` on every observer
- [ ] Set `max_dlq_size` if any observer uses `on_failure = "dlq"`
- [ ] Set the `redis` TTLs and windows, and the `[performance]` flags that depend on them
- [ ] Configured `MultiListenerConfig` if running more than one listener
- [ ] Set `channel_capacity` and `backlog_alert_threshold`
- [ ] Supplied secrets through `*_env` keys or environment overrides, not literals
- [ ] Ran `ObserverRuntimeConfig::validate()` on the loaded config
- [ ] Tested the configuration with sample events
