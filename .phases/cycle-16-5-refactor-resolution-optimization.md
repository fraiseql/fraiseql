# Cycle 16-5: REFACTOR Phase - Resolution Strategies Optimization

**Cycle**: 5 of 8
**Phase**: REFACTOR (Improve design without changing behavior)
**Duration**: ~2-3 days
**Focus**: Extract traits, optimize performance, improve error handling

---

## Refactoring Tasks

### Task 1: Extract Resolution Trait

Improve entity resolver abstraction:

```rust
#[async_trait]
pub trait EntityResolver: Send + Sync {
    async fn resolve(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<serde_json::Value>, String>;

    fn supports_batching(&self) -> bool {
        true
    }

    fn max_batch_size(&self) -> usize {
        1000
    }

    async fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
```

### Task 2: Connection Pool Optimization

Implement efficient connection pooling:

```rust
pub struct OptimizedConnectionPool {
    active_connections: Arc<Semaphore>,
    idle_connections: Arc<Mutex<Vec<Connection>>>,
    stats: Arc<RwLock<PoolStats>>,
}

struct PoolStats {
    total_connections: usize,
    active_connections: usize,
    idle_connections: usize,
    total_waits: usize,
    avg_wait_time_ms: f64,
}
```

### Task 3: Retry Strategy Pattern

Extract retry logic into reusable strategy:

```rust
#[async_trait]
pub trait RetryStrategy: Send + Sync {
    async fn execute<F, T>(&self, f: F) -> Result<T, String>
    where
        F: Fn() -> BoxFuture<'static, Result<T, String>> + Send;
}

pub struct ExponentialBackoffRetry {
    max_retries: u32,
    initial_backoff_ms: u64,
}
```

### Task 4: Performance Monitoring

Add observability to resolution:

```rust
pub struct ResolutionMetrics {
    latency_ms: f64,
    entities_resolved: usize,
    cache_hits: usize,
    cache_misses: usize,
    errors: usize,
}

impl ResolutionMetrics {
    pub fn throughput_per_sec(&self) -> f64 {
        1000.0 / self.latency_ms
    }
}
```

### Task 5: Strategy Selection Caching

Improve strategy selection performance:

```rust
pub struct StrategyCache {
    cache: Arc<RwLock<HashMap<String, ResolutionStrategy>>>,
    ttl: Duration,
}

impl StrategyCache {
    pub async fn get_or_compute(
        &self,
        typename: &str,
        compute_fn: impl Fn(&str) -> ResolutionStrategy,
    ) -> ResolutionStrategy {
        if let Some(strategy) = self.cache.read().await.get(typename) {
            return strategy.clone();
        }

        let strategy = compute_fn(typename);
        self.cache.write().await.insert(typename.to_string(), strategy.clone());
        strategy
    }
}
```

### Task 6: Error Context & Observability

Improve error handling:

```rust
#[derive(Debug)]
pub enum ResolutionError {
    DirectDatabase {
        typename: String,
        keys: Vec<String>,
        source: String,
    },
    Http {
        typename: String,
        url: String,
        status: Option<u16>,
        source: String,
    },
    ConnectionPoolExhausted {
        remote: String,
        timeout_ms: u64,
    },
}

impl Display for ResolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionError::DirectDatabase { typename, keys, source } => {
                write!(f, "Failed to resolve {} with keys {:?}: {}", typename, keys, source)
            }
            // ... other cases
        }
    }
}
```

### Task 7: Batch Parallelization

Optimize parallel batch execution:

```rust
pub async fn execute_batches_optimized(
    batches: Vec<Batch>,
    max_concurrent: usize,
) -> Result<Vec<serde_json::Value>, Vec<ResolutionError>> {
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = vec![];

    for batch in batches {
        let sem = semaphore.clone();
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await;
            // Execute batch
        });

        handles.push(handle);
    }

    // Collect results with partial failure handling
    let mut results = vec![];
    let mut errors = vec![];

    for handle in handles {
        match handle.await {
            Ok(Ok(batch_results)) => results.extend(batch_results),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(ResolutionError::Internal(e.to_string())),
        }
    }

    if errors.is_empty() {
        Ok(results)
    } else {
        Err(errors)
    }
}
```

---

## Refactoring Checklist

- [ ] Resolution trait extracted and documented
- [ ] Connection pool optimized with stats
- [ ] Retry strategy pattern implemented
- [ ] Performance monitoring added
- [ ] Strategy selection caching optimized
- [ ] Error handling improved with context
- [ ] Batch parallelization optimized
- [ ] No performance regression
- [ ] All tests still passing
- [ ] No clippy warnings

---

**Status**: [~] In Progress (Refactoring)
**Next**: CLEANUP Phase - Final verification and commit
