# Configuration Examples - Phase 8

This guide provides real-world configuration examples for different scenarios.

## Table of Contents

1. [Production Setup](#production-setup)
2. [Development Setup](#development-setup)
3. [High-Performance Setup](#high-performance-setup)
4. [Budget Setup](#budget-setup)
5. [Feature-Specific Examples](#feature-specific-examples)

---

## Production Setup

**Recommended for**: Mission-critical production systems

**Characteristics**:
- All Phase 8 features enabled
- PostgreSQL for checkpoints
- Redis for cache and dedup
- Elasticsearch for audit trail
- Prometheus for monitoring
- Multiple listeners for HA

### Cargo.toml

```toml
[features]
production = ["checkpoint", "dedup", "caching", "search", "metrics", "queue"]
```

### Runtime Configuration

```rust
use fraiseql_observers::*;
use std::time::Duration;

pub async fn production_config() -> ObserverRuntimeConfig {
    ObserverRuntimeConfig {
        // Checkpoints: Save every 100 events (balance safety vs performance)
        checkpoint_batch_size: 100,
        checkpoint_store: Arc::new(
            PostgresCheckpointStore::new(
                "postgresql://user:pass@localhost/observers",
                "observer_checkpoints"
            )
            .await
            .expect("Failed to initialize checkpoint store")
        ),

        // Deduplication: 10-minute window to catch retries
        dedup_window: Duration::from_secs(600),
        dedup_store: Arc::new(
            RedisDeduplicationStore::new(
                "redis://localhost:6379",
                600  // 10 minutes TTL
            )
            .await
            .expect("Failed to initialize dedup store")
        ),

        // Caching: 5-minute TTL for frequent lookups
        cache_backend: Arc::new(
            RedisCacheBackend::new(
                "redis://localhost:6379",
                Duration::from_secs(300)  // 5 minutes
            )
            .await
            .expect("Failed to initialize cache")
        ),

        // Search: Index all events in Elasticsearch
        search_backend: Arc::new(
            HttpSearchBackend::new(
                "http://localhost:9200",
                Duration::from_secs(30)
            )
        ),

        // Metrics: Export to Prometheus
        metrics: Some(Arc::new(ObserverMetrics::new())),

        // Job Queue: Redis-backed with 50 workers
        job_queue: Arc::new(
            RedisJobQueue::new(
                "redis://localhost:6379",
                50  // workers
            )
            .await
            .expect("Failed to initialize job queue")
        ),

        // Retry: Exponential backoff (100ms -> 30s)
        retry_strategy: BackoffStrategy::Exponential {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(30),
        },
        max_retry_attempts: 5,

        // Circuit Breaker: 50% failure threshold
        circuit_breaker: CircuitBreakerConfig {
            failure_threshold: 0.5,
            success_threshold: 0.8,
            timeout: Duration::from_secs(60),
            sample_size: 100,
        },

        // Multi-Listener: 3 listeners for HA
        multi_listener_config: Some(MultiListenerConfig {
            num_listeners: 3,
            health_check_interval: Duration::from_secs(5),
            failover_threshold: Duration::from_secs(60),
        }),

        // Backpressure: Drop oldest events if queue fills
        overflow_policy: OverflowPolicy::DropOldest,
        max_queue_size: 10000,

        // Logging
        log_level: "info".to_string(),
    }
}
```

### Environment Setup

```bash
# PostgreSQL for checkpoints
DATABASE_URL=postgresql://observer:secure_password@postgres:5432/fraiseql_observers

# Redis for cache and dedup
REDIS_URL=redis://:secure_password@redis:6379/0

# Elasticsearch for search
ELASTICSEARCH_URL=http://elasticsearch:9200

# Prometheus metrics (push gateway)
PROMETHEUS_PUSHGATEWAY=http://prometheus-pushgateway:9091

# Observer configuration
OBSERVER_LOG_LEVEL=info
OBSERVER_MAX_RETRIES=5
OBSERVER_CACHE_TTL=300
```

---

## Development Setup

**Recommended for**: Local development and testing

**Characteristics**:
- Minimal external dependencies
- SQLite for checkpoints (local file)
- In-memory caching
- No Elasticsearch
- Immediate retries (no backoff)
- Single listener

### Cargo.toml

```toml
[features]
development = ["checkpoint"]  # Only checkpoints, nothing else
```

### Runtime Configuration

```rust
pub async fn development_config() -> ObserverRuntimeConfig {
    ObserverRuntimeConfig {
        // Checkpoints: SQLite for local development
        checkpoint_batch_size: 1,  // Save immediately
        checkpoint_store: Arc::new(
            SqliteCheckpointStore::new("./observer_checkpoints.db")
                .await
                .expect("Failed to initialize checkpoint store")
        ),

        // No deduplication (faster testing)
        dedup_store: Arc::new(NullDeduplicationStore::new()),

        // No caching (test real execution paths)
        cache_backend: Arc::new(NullCacheBackend::new()),

        // No search (no external dependencies)
        search_backend: Arc::new(NullSearchBackend::new()),

        // No metrics in dev
        metrics: None,

        // No job queue (execute synchronously)
        job_queue: Arc::new(NullJobQueue::new()),

        // Immediate retries for fast testing
        retry_strategy: BackoffStrategy::Fixed {
            delay: Duration::from_millis(10),
        },
        max_retry_attempts: 2,  // Fast failure

        // No circuit breaker in dev
        circuit_breaker: CircuitBreakerConfig::default(),

        // Single listener
        multi_listener_config: None,

        // No backpressure limits in dev
        overflow_policy: OverflowPolicy::Block,
        max_queue_size: 1000,

        log_level: "debug".to_string(),
    }
}
```

### Mock Implementations

For testing, use provided mocks:

```rust
use fraiseql_observers::testing::mocks::*;

pub fn test_config() -> ObserverRuntimeConfig {
    ObserverRuntimeConfig {
        checkpoint_store: Arc::new(MockCheckpointStore::new()),
        dedup_store: Arc::new(MockDeduplicationStore::new()),
        cache_backend: Arc::new(MockCacheBackend::new()),
        search_backend: Arc::new(MockSearchBackend::new()),
        job_queue: Arc::new(MockJobQueue::new()),
        // ... rest of config
    }
}
```

---

## High-Performance Setup

**Recommended for**: High-throughput systems (1000+ events/second)

**Characteristics**:
- Caching enabled for performance
- Concurrent execution
- Dedup for quality
- Batched checkpoints
- Large queue
- Many worker threads

### Cargo.toml

```toml
[features]
performance = ["checkpoint", "dedup", "caching", "queue"]
```

### Runtime Configuration

```rust
pub async fn performance_config() -> ObserverRuntimeConfig {
    ObserverRuntimeConfig {
        // Checkpoints: Large batches (performance priority)
        checkpoint_batch_size: 1000,  // Write every 1000 events
        checkpoint_store: Arc::new(
            PostgresCheckpointStore::with_pool_config(
                "postgresql://localhost/observers",
                PoolConfig {
                    min_connections: 5,
                    max_connections: 20,
                    ..Default::default()
                }
            )
            .await
            .expect("Failed to create checkpoint store")
        ),

        // Deduplication: Shorter window (fast duplicate detection)
        dedup_window: Duration::from_secs(300),  // 5 minutes
        dedup_store: Arc::new(
            RedisDeduplicationStore::with_config(
                "redis://localhost",
                RedisConfig {
                    connection_pool_size: 20,
                    ttl: 300,
                }
            )
            .await
            .expect("Failed to create dedup store")
        ),

        // Caching: Aggressive caching
        cache_backend: Arc::new(
            RedisCacheBackend::with_config(
                "redis://localhost",
                CacheConfig {
                    ttl: Duration::from_secs(600),  // 10 minutes
                    max_size: 100_000,  // 100k entries
                    eviction: EvictionPolicy::LRU,
                }
            )
            .await
            .expect("Failed to create cache")
        ),

        // Job Queue: Many workers for parallelism
        job_queue: Arc::new(
            RedisJobQueue::with_workers(
                "redis://localhost",
                200  // 200 worker threads!
            )
            .await
            .expect("Failed to create job queue")
        ),

        // Retry: Fast backoff for quick recovery
        retry_strategy: BackoffStrategy::Linear {
            initial: Duration::from_millis(50),
            increment: Duration::from_millis(50),
            max: Duration::from_secs(10),
        },
        max_retry_attempts: 3,  // Fast failure

        // Circuit Breaker: Aggressive thresholds
        circuit_breaker: CircuitBreakerConfig {
            failure_threshold: 0.3,  // Open at 30% failures
            success_threshold: 0.9,  // Close at 90% success
            timeout: Duration::from_secs(30),  // Quick recovery probing
            sample_size: 50,  // Small sample for responsiveness
        },

        // Backpressure: Large queue for throughput
        overflow_policy: OverflowPolicy::DropOldest,
        max_queue_size: 50000,  // Large buffer

        log_level: "warn".to_string(),  // Reduce logging overhead
    }
}
```

---

## Budget Setup

**Recommended for**: Cost-conscious deployments, non-critical systems

**Characteristics**:
- Only essentials enabled
- Shared Redis instance
- No Elasticsearch
- Batched processing
- Single node

### Cargo.toml

```toml
[features]
budget = ["checkpoint"]  # Checkpoint only, for safety
```

### Runtime Configuration

```rust
pub async fn budget_config() -> ObserverRuntimeConfig {
    ObserverRuntimeConfig {
        // Checkpoints: Only feature enabled (safety essential)
        checkpoint_batch_size: 500,
        checkpoint_store: Arc::new(
            PostgresCheckpointStore::new(
                "postgresql://localhost/observers",
                "observer_checkpoints"
            )
            .await
            .expect("Failed to initialize checkpoint store")
        ),

        // No extras (keep costs down)
        dedup_store: Arc::new(NullDeduplicationStore::new()),
        cache_backend: Arc::new(NullCacheBackend::new()),
        search_backend: Arc::new(NullSearchBackend::new()),
        metrics: None,
        job_queue: Arc::new(NullJobQueue::new()),

        // Conservative retry (limited costs)
        retry_strategy: BackoffStrategy::Fixed {
            delay: Duration::from_secs(1),
        },
        max_retry_attempts: 3,

        // Single listener (no HA complexity)
        multi_listener_config: None,

        // Moderate queue
        overflow_policy: OverflowPolicy::DropOldest,
        max_queue_size: 5000,

        log_level: "warn".to_string(),
    }
}
```

**Cost Estimate**:
- PostgreSQL: ~$15/month (managed service)
- Compute: ~$50/month (single instance)
- **Total: ~$65/month**

---

## Feature-Specific Examples

### Example 1: Checkpoint Configuration

```rust
// Save checkpoint after every 100 events (balance)
checkpoint_batch_size: 100,

// Or: Save immediately (safest but slowest)
checkpoint_batch_size: 1,

// Or: Save every 10000 events (fast but riskier)
checkpoint_batch_size: 10000,
```

**When to use**:
- `1`: Financial transactions, healthcare (safety critical)
- `100`: Most production systems
- `10000`: High-throughput, lower-criticality

---

### Example 2: Retry Strategy Configuration

#### Exponential Backoff (Standard)
```rust
retry_strategy: BackoffStrategy::Exponential {
    initial: Duration::from_millis(100),
    max: Duration::from_secs(30),
},
```
Delays: 100ms, 200ms, 400ms, 800ms, 1.6s, 3.2s, 6.4s, 12.8s, 25.6s, 30s

**Use case**: Transient failures (network glitches, temporary overload)

#### Linear Backoff (Predictable)
```rust
retry_strategy: BackoffStrategy::Linear {
    initial: Duration::from_millis(100),
    increment: Duration::from_millis(100),
    max: Duration::from_secs(10),
},
```
Delays: 100ms, 200ms, 300ms, 400ms, ..., 10s

**Use case**: More predictable, uniform retry pattern

#### Fixed Backoff (Simple)
```rust
retry_strategy: BackoffStrategy::Fixed {
    delay: Duration::from_millis(100),
},
```
Delays: 100ms, 100ms, 100ms, ...

**Use case**: Testing, or when service recovers quickly

---

### Example 3: Circuit Breaker Configuration

#### Aggressive (Fail Fast)
```rust
CircuitBreakerConfig {
    failure_threshold: 0.2,      // Open at 20% failures
    success_threshold: 0.9,      // Close at 90% success
    timeout: Duration::from_secs(10),
    sample_size: 50,
}
```
**Use case**: Expensive operations (protect system from runaway)

#### Conservative (High Tolerance)
```rust
CircuitBreakerConfig {
    failure_threshold: 0.7,      // Open at 70% failures
    success_threshold: 0.5,      // Close at 50% success
    timeout: Duration::from_secs(300),
    sample_size: 1000,
}
```
**Use case**: Resilient to brief outages, don't want false alarms

---

### Example 4: Cache Configuration

#### Aggressive Caching (Speed Priority)
```rust
cache_backend: Arc::new(
    RedisCacheBackend::with_config(
        "redis://localhost",
        CacheConfig {
            ttl: Duration::from_secs(3600),  // 1 hour
            max_size: 1_000_000,  // 1M entries
        }
    )
    .await?
),
```
**Result**: 95%+ hit rate, extreme performance

#### Conservative Caching (Correctness Priority)
```rust
cache_backend: Arc::new(
    RedisCacheBackend::with_config(
        "redis://localhost",
        CacheConfig {
            ttl: Duration::from_secs(60),  // 1 minute
            max_size: 10_000,  // 10k entries
        }
    )
    .await?
),
```
**Result**: ~50-60% hit rate, fresher data

---

### Example 5: Multi-Listener Configuration

#### 1 Listener (No HA)
```rust
multi_listener_config: None,
```
**Use case**: Development, non-critical systems

#### 3 Listeners (Standard HA)
```rust
multi_listener_config: Some(MultiListenerConfig {
    num_listeners: 3,
    health_check_interval: Duration::from_secs(5),
    failover_threshold: Duration::from_secs(60),
}),
```
**Use case**: Production with acceptable downtime (seconds)

#### 5 Listeners (High Availability)
```rust
multi_listener_config: Some(MultiListenerConfig {
    num_listeners: 5,
    health_check_interval: Duration::from_secs(2),
    failover_threshold: Duration::from_secs(10),
}),
```
**Use case**: Mission-critical with minimal acceptable downtime

---

## Environment-Based Configuration

### Using Config from Environment

```rust
use std::env;
use std::time::Duration;

pub async fn config_from_env() -> ObserverRuntimeConfig {
    let environment = env::var("ENVIRONMENT").unwrap_or("development".to_string());

    match environment.as_str() {
        "production" => production_config().await,
        "staging" => staging_config().await,
        "development" => development_config().await,
        _ => panic!("Unknown environment: {}", environment),
    }
}
```

### Staging Configuration

```rust
pub async fn staging_config() -> ObserverRuntimeConfig {
    // Similar to production but with:
    // - Shorter retry delays (test faster)
    // - Smaller batches (detect issues quicker)
    // - Same features (test production setup)

    let mut config = production_config().await;
    config.checkpoint_batch_size = 10;  // More frequent saves
    config.retry_strategy = BackoffStrategy::Fixed {
        delay: Duration::from_millis(100),
    };
    config
}
```

---

## Migration Path

### Phase 1: Deploy with Checkpoints Only
```toml
[features]
phase1 = ["checkpoint"]
```
- Enables zero-event-loss guarantee
- No external dependencies (except PostgreSQL)
- Safe foundation

### Phase 2: Add Caching
```toml
[features]
phase2 = ["checkpoint", "caching"]
```
- Reduces external API load
- Improves latency
- Adds Redis dependency

### Phase 3: Add Deduplication
```toml
[features]
phase3 = ["checkpoint", "caching", "dedup"]
```
- Prevents duplicate side effects
- Uses existing Redis

### Phase 4: Add Monitoring
```toml
[features]
phase4 = ["checkpoint", "caching", "dedup", "metrics"]
```
- Production observability
- Enables alerting

### Phase 5: Add Search
```toml
[features]
final = ["checkpoint", "caching", "dedup", "metrics", "search"]
```
- Compliance-ready audit trail
- Full debugging capability

---

## Performance Tuning

### Increase Throughput

1. **Increase checkpoint batch size**
   ```rust
   checkpoint_batch_size: 1000,  // Was 100
   ```
   Trade-off: Higher data loss risk on crash

2. **Enable aggressive caching**
   ```rust
   cache_ttl: Duration::from_secs(600),  // Was 60
   ```
   Trade-off: Staler data

3. **Increase worker pool**
   ```rust
   job_queue: Arc::new(RedisJobQueue::with_workers("redis://...", 500))
   ```
   Trade-off: Higher resource usage

### Reduce Latency

1. **Reduce cache TTL** (for fresher data)
   ```rust
   cache_ttl: Duration::from_secs(10),  // Was 60
   ```

2. **Reduce retry delays**
   ```rust
   retry_strategy: BackoffStrategy::Fixed {
       delay: Duration::from_millis(10),  // Was 100
   },
   ```

3. **Decrease checkpoint batch size**
   ```rust
   checkpoint_batch_size: 10,  // Was 100
   ```
   Trade-off: More frequent writes to database

---

## Checklist: Configuration Review

- [ ] Selected appropriate feature set for use case
- [ ] Configured checkpoint batch size
- [ ] Set retry strategy and max attempts
- [ ] Configured circuit breaker thresholds
- [ ] Set cache TTL and size
- [ ] Configured multi-listener if using HA
- [ ] Set overflow policy and queue size
- [ ] Configured external service URLs
- [ ] Set log level
- [ ] Tested configuration with sample events
- [ ] Documented configuration choices
- [ ] Set up monitoring and alerting

