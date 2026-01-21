# Phase 8: Advanced Features (Search, Cache, Queue)

## Objective

Implement integrations for full-text search (Meilisearch, Typesense, PostgreSQL FTS), caching (Redis, in-memory), and background job queues (Redis-based, PostgreSQL-based). These features enhance performance and enable complex data processing workflows.

## Dependencies

- Phase 1: Configuration system (TOML parsing)
- Phase 2: Core runtime (connection pooling, metrics)
- Phase 6: Observer runtime (triggers search index updates)

---

## Section 8.0: Testing Seams, SLO Tracking, and Architecture

### 8.0.1 Testing Architecture

All integration providers use trait-based design for testability and swappability:

```
┌─────────────────────────────────────────────────────────────────┐
│                    IntegrationService                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │SearchProvider│  │CacheProvider │  │QueueProvider │          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
└─────────┼─────────────────┼─────────────────┼──────────────────┘
          │                 │                 │
          ▼                 ▼                 ▼
    ┌───────────┐     ┌───────────┐     ┌───────────┐
    │HttpClient │     │Connection │     │Connection │
    │  (trait)  │     │   Pool    │     │   Pool    │
    └───────────┘     └───────────┘     └───────────┘
```

### 8.0.2 Service Level Objectives (SLO) Tracking

Each integration tracks latency against defined SLOs:

```rust
// src/slo.rs - SLO tracking for integrations
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use prometheus::{Counter, Histogram, HistogramOpts, IntCounter, register_counter, register_histogram, register_int_counter};

/// SLO configuration for an operation
#[derive(Debug, Clone)]
pub struct SloConfig {
    /// Target latency (p99)
    pub target_latency_ms: u64,
    /// Target availability (e.g., 0.999 for 99.9%)
    pub target_availability: f64,
    /// Error budget period (e.g., 30 days)
    pub budget_period_days: u32,
}

impl Default for SloConfig {
    fn default() -> Self {
        Self {
            target_latency_ms: 100,      // 100ms p99 target
            target_availability: 0.999,   // 99.9% availability
            budget_period_days: 30,
        }
    }
}

/// SLO presets for different integration types
impl SloConfig {
    /// Search: 200ms p99 (external service + network)
    pub fn search() -> Self {
        Self { target_latency_ms: 200, ..Default::default() }
    }

    /// Cache read: 10ms p99 (fast in-memory or Redis)
    pub fn cache_read() -> Self {
        Self { target_latency_ms: 10, ..Default::default() }
    }

    /// Cache write: 20ms p99
    pub fn cache_write() -> Self {
        Self { target_latency_ms: 20, ..Default::default() }
    }

    /// Queue enqueue: 50ms p99
    pub fn queue_enqueue() -> Self {
        Self { target_latency_ms: 50, ..Default::default() }
    }

    /// Queue process: job-specific, no default latency target
    pub fn queue_process() -> Self {
        Self { target_latency_ms: 30000, ..Default::default() } // 30s default
    }
}

/// SLO tracker for a specific operation type
pub struct SloTracker {
    name: String,
    config: SloConfig,

    // Prometheus metrics
    latency_histogram: Histogram,
    total_requests: IntCounter,
    slo_violations: IntCounter,
    errors: IntCounter,

    // In-memory tracking for quick access
    violation_count: AtomicU64,
    request_count: AtomicU64,
}

impl SloTracker {
    pub fn new(name: &str, config: SloConfig) -> Self {
        let latency_histogram = register_histogram!(
            HistogramOpts::new(
                format!("fraiseql_integration_{}_latency_seconds", name),
                format!("Latency for {} operations", name)
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])
        ).unwrap();

        let total_requests = register_int_counter!(
            format!("fraiseql_integration_{}_total", name),
            format!("Total {} requests", name)
        ).unwrap();

        let slo_violations = register_int_counter!(
            format!("fraiseql_integration_{}_slo_violations_total", name),
            format!("SLO violations for {}", name)
        ).unwrap();

        let errors = register_int_counter!(
            format!("fraiseql_integration_{}_errors_total", name),
            format!("Errors for {}", name)
        ).unwrap();

        Self {
            name: name.to_string(),
            config,
            latency_histogram,
            total_requests,
            slo_violations,
            errors,
            violation_count: AtomicU64::new(0),
            request_count: AtomicU64::new(0),
        }
    }

    /// Record a successful operation
    pub fn record_success(&self, duration: Duration) {
        let latency_ms = duration.as_millis() as u64;
        let latency_secs = duration.as_secs_f64();

        self.latency_histogram.observe(latency_secs);
        self.total_requests.inc();
        self.request_count.fetch_add(1, Ordering::Relaxed);

        if latency_ms > self.config.target_latency_ms {
            self.slo_violations.inc();
            self.violation_count.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                operation = %self.name,
                latency_ms = latency_ms,
                target_ms = self.config.target_latency_ms,
                "SLO violation: latency exceeded target"
            );
        }
    }

    /// Record an error
    pub fn record_error(&self) {
        self.errors.inc();
        self.total_requests.inc();
        self.request_count.fetch_add(1, Ordering::Relaxed);
        // Errors always count as SLO violations for availability
        self.slo_violations.inc();
        self.violation_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current error budget status
    pub fn error_budget_remaining(&self) -> f64 {
        let total = self.request_count.load(Ordering::Relaxed) as f64;
        let violations = self.violation_count.load(Ordering::Relaxed) as f64;

        if total == 0.0 {
            return 1.0; // 100% budget remaining
        }

        let actual_availability = 1.0 - (violations / total);
        let budget_used = (1.0 - actual_availability) / (1.0 - self.config.target_availability);

        (1.0 - budget_used).max(0.0) // Clamp to 0-1
    }

    /// Check if we're within SLO
    pub fn is_within_slo(&self) -> bool {
        self.error_budget_remaining() > 0.0
    }

    /// Timed operation helper
    pub fn time<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        let result = f();
        self.record_success(start.elapsed());
        result
    }

    /// Async timed operation helper
    pub async fn time_async<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        let start = Instant::now();
        let result = f.await;
        match &result {
            Ok(_) => self.record_success(start.elapsed()),
            Err(_) => self.record_error(),
        }
        result
    }
}

/// SLO registry for all integration operations
pub struct SloRegistry {
    trackers: std::collections::HashMap<String, SloTracker>,
}

impl SloRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            trackers: std::collections::HashMap::new(),
        };

        // Register default SLOs for integrations
        registry.register("search_query", SloConfig::search());
        registry.register("search_index", SloConfig::search());
        registry.register("cache_get", SloConfig::cache_read());
        registry.register("cache_set", SloConfig::cache_write());
        registry.register("cache_delete", SloConfig::cache_write());
        registry.register("queue_enqueue", SloConfig::queue_enqueue());
        registry.register("queue_process", SloConfig::queue_process());

        registry
    }

    pub fn register(&mut self, name: &str, config: SloConfig) {
        self.trackers.insert(name.to_string(), SloTracker::new(name, config));
    }

    pub fn get(&self, name: &str) -> Option<&SloTracker> {
        self.trackers.get(name)
    }

    /// Get overall SLO status
    pub fn overall_status(&self) -> SloStatus {
        let mut total_budget = 0.0;
        let mut count = 0;

        for tracker in self.trackers.values() {
            total_budget += tracker.error_budget_remaining();
            count += 1;
        }

        let avg_budget = if count > 0 { total_budget / count as f64 } else { 1.0 };

        SloStatus {
            within_slo: self.trackers.values().all(|t| t.is_within_slo()),
            average_budget_remaining: avg_budget,
            operations: self.trackers.iter().map(|(name, tracker)| {
                (name.clone(), tracker.error_budget_remaining())
            }).collect(),
        }
    }
}

impl Default for SloRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SloStatus {
    pub within_slo: bool,
    pub average_budget_remaining: f64,
    pub operations: std::collections::HashMap<String, f64>,
}
```

### 8.0.3 Mock Provider Implementations

```rust
// src/search/mock.rs - Mock search provider for testing
use super::{Document, SearchHit, SearchProvider, SearchQuery, SearchResult};
use crate::error::IntegrationError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

/// Mock search provider for testing
pub struct MockSearchProvider {
    /// Indexed documents by index name
    pub documents: Mutex<HashMap<String, Vec<Document>>>,
    /// Custom search results to return
    pub search_results: Mutex<Option<SearchResult>>,
    /// Whether to fail operations
    pub should_fail: Mutex<bool>,
    /// Recorded operations
    pub operations: Mutex<Vec<SearchOperation>>,
}

#[derive(Debug, Clone)]
pub enum SearchOperation {
    Search { index: String, query: String },
    Index { index: String, id: String },
    Delete { index: String, id: String },
    Clear { index: String },
}

impl MockSearchProvider {
    pub fn new() -> Self {
        Self {
            documents: Mutex::new(HashMap::new()),
            search_results: Mutex::new(None),
            should_fail: Mutex::new(false),
            operations: Mutex::new(Vec::new()),
        }
    }

    /// Set custom search results
    pub fn with_results(mut self, results: SearchResult) -> Self {
        *self.search_results.lock().unwrap() = Some(results);
        self
    }

    /// Configure to fail
    pub fn fail_with(&self, _message: &str) {
        *self.should_fail.lock().unwrap() = true;
    }

    /// Get all operations
    pub fn recorded_operations(&self) -> Vec<SearchOperation> {
        self.operations.lock().unwrap().clone()
    }

    /// Assert an operation was recorded
    pub fn assert_searched(&self, index: &str, query: &str) {
        let ops = self.operations.lock().unwrap();
        assert!(
            ops.iter().any(|op| matches!(op, SearchOperation::Search { index: i, query: q } if i == index && q == query)),
            "Expected search for '{}' in index '{}', operations: {:?}",
            query, index, ops
        );
    }
}

impl Default for MockSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchProvider for MockSearchProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn search(&self, query: SearchQuery) -> Result<SearchResult, IntegrationError> {
        if *self.should_fail.lock().unwrap() {
            return Err(IntegrationError::Provider("Mock failure".to_string()));
        }

        self.operations.lock().unwrap().push(SearchOperation::Search {
            index: query.index.clone(),
            query: query.query.clone(),
        });

        // Return custom results if set
        if let Some(results) = &*self.search_results.lock().unwrap() {
            return Ok(results.clone());
        }

        // Otherwise, search through indexed documents
        let documents = self.documents.lock().unwrap();
        let index_docs = documents.get(&query.index).cloned().unwrap_or_default();

        let hits: Vec<SearchHit> = index_docs
            .iter()
            .filter(|doc| {
                // Simple text matching
                doc.data.to_string().to_lowercase().contains(&query.query.to_lowercase())
            })
            .take(query.limit.unwrap_or(20) as usize)
            .map(|doc| SearchHit {
                id: doc.id.clone(),
                score: Some(1.0),
                document: doc.data.clone(),
                highlights: None,
            })
            .collect();

        Ok(SearchResult {
            hits: hits.clone(),
            total_hits: hits.len() as u64,
            processing_time_ms: 1,
            facets: None,
        })
    }

    async fn index_document(&self, doc: Document) -> Result<(), IntegrationError> {
        if *self.should_fail.lock().unwrap() {
            return Err(IntegrationError::Provider("Mock failure".to_string()));
        }

        self.operations.lock().unwrap().push(SearchOperation::Index {
            index: doc.index.clone(),
            id: doc.id.clone(),
        });

        let mut documents = self.documents.lock().unwrap();
        documents.entry(doc.index.clone()).or_default().push(doc);

        Ok(())
    }

    async fn index_documents(&self, docs: Vec<Document>) -> Result<(), IntegrationError> {
        for doc in docs {
            self.index_document(doc).await?;
        }
        Ok(())
    }

    async fn delete_document(&self, index: &str, id: &str) -> Result<(), IntegrationError> {
        self.operations.lock().unwrap().push(SearchOperation::Delete {
            index: index.to_string(),
            id: id.to_string(),
        });

        let mut documents = self.documents.lock().unwrap();
        if let Some(index_docs) = documents.get_mut(index) {
            index_docs.retain(|doc| doc.id != id);
        }

        Ok(())
    }

    async fn clear_index(&self, index: &str) -> Result<(), IntegrationError> {
        self.operations.lock().unwrap().push(SearchOperation::Clear {
            index: index.to_string(),
        });

        let mut documents = self.documents.lock().unwrap();
        documents.remove(index);

        Ok(())
    }

    async fn configure_index(&self, _index: &str, _config: &crate::config::IndexConfig) -> Result<(), IntegrationError> {
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        Ok(!*self.should_fail.lock().unwrap())
    }
}
```

### 8.0.4 Mock Cache Provider

```rust
// src/cache/mock.rs
use super::CacheProvider;
use crate::error::IntegrationError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Mock cache provider for testing
pub struct MockCacheProvider {
    entries: Mutex<HashMap<String, (Vec<u8>, Option<Instant>)>>,
    pub operations: Mutex<Vec<CacheOperation>>,
    pub should_fail: Mutex<bool>,
}

#[derive(Debug, Clone)]
pub enum CacheOperation {
    Get { key: String, hit: bool },
    Set { key: String, ttl_secs: Option<u64> },
    Delete { key: String },
    DeletePattern { pattern: String },
}

impl MockCacheProvider {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            operations: Mutex::new(Vec::new()),
            should_fail: Mutex::new(false),
        }
    }

    pub fn with_entry(self, key: &str, value: &[u8]) -> Self {
        self.entries.lock().unwrap().insert(key.to_string(), (value.to_vec(), None));
        self
    }

    pub fn recorded_operations(&self) -> Vec<CacheOperation> {
        self.operations.lock().unwrap().clone()
    }

    pub fn assert_cache_hit(&self, key: &str) {
        let ops = self.operations.lock().unwrap();
        assert!(
            ops.iter().any(|op| matches!(op, CacheOperation::Get { key: k, hit: true } if k == key)),
            "Expected cache hit for '{}', operations: {:?}",
            key, ops
        );
    }

    pub fn assert_cache_miss(&self, key: &str) {
        let ops = self.operations.lock().unwrap();
        assert!(
            ops.iter().any(|op| matches!(op, CacheOperation::Get { key: k, hit: false } if k == key)),
            "Expected cache miss for '{}', operations: {:?}",
            key, ops
        );
    }
}

impl Default for MockCacheProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CacheProvider for MockCacheProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IntegrationError> {
        if *self.should_fail.lock().unwrap() {
            return Err(IntegrationError::Provider("Mock failure".to_string()));
        }

        let entries = self.entries.lock().unwrap();
        let result = entries.get(key).map(|(v, exp)| {
            // Check expiration
            if exp.map(|e| e > Instant::now()).unwrap_or(true) {
                Some(v.clone())
            } else {
                None
            }
        }).flatten();

        self.operations.lock().unwrap().push(CacheOperation::Get {
            key: key.to_string(),
            hit: result.is_some(),
        });

        Ok(result)
    }

    async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<(), IntegrationError> {
        if *self.should_fail.lock().unwrap() {
            return Err(IntegrationError::Provider("Mock failure".to_string()));
        }

        let expires_at = ttl.map(|d| Instant::now() + d);
        self.entries.lock().unwrap().insert(key.to_string(), (value.to_vec(), expires_at));

        self.operations.lock().unwrap().push(CacheOperation::Set {
            key: key.to_string(),
            ttl_secs: ttl.map(|d| d.as_secs()),
        });

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, IntegrationError> {
        let existed = self.entries.lock().unwrap().remove(key).is_some();
        self.operations.lock().unwrap().push(CacheOperation::Delete {
            key: key.to_string(),
        });
        Ok(existed)
    }

    async fn delete_pattern(&self, pattern: &str) -> Result<u64, IntegrationError> {
        let prefix = pattern.trim_end_matches('*');
        let mut entries = self.entries.lock().unwrap();
        let keys_to_remove: Vec<_> = entries.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        let count = keys_to_remove.len() as u64;
        for key in keys_to_remove {
            entries.remove(&key);
        }
        self.operations.lock().unwrap().push(CacheOperation::DeletePattern {
            pattern: pattern.to_string(),
        });
        Ok(count)
    }

    async fn exists(&self, key: &str) -> Result<bool, IntegrationError> {
        Ok(self.entries.lock().unwrap().contains_key(key))
    }

    async fn ttl(&self, key: &str) -> Result<Option<Duration>, IntegrationError> {
        let entries = self.entries.lock().unwrap();
        Ok(entries.get(key).and_then(|(_, exp)| {
            exp.map(|e| e.duration_since(Instant::now()))
        }))
    }

    async fn expire(&self, key: &str, ttl: Duration) -> Result<bool, IntegrationError> {
        let mut entries = self.entries.lock().unwrap();
        if let Some((_, exp)) = entries.get_mut(key) {
            *exp = Some(Instant::now() + ttl);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn incr(&self, key: &str, delta: i64) -> Result<i64, IntegrationError> {
        let mut entries = self.entries.lock().unwrap();
        let current = entries.get(key)
            .and_then(|(v, _)| String::from_utf8(v.clone()).ok())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let new_value = current + delta;
        entries.insert(key.to_string(), (new_value.to_string().into_bytes(), None));
        Ok(new_value)
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        Ok(!*self.should_fail.lock().unwrap())
    }
}
```

### 8.0.5 Mock Queue Provider

```rust
// src/queue/mock.rs
use super::{EnqueueOptions, Job, JobResult, QueueProvider};
use crate::error::IntegrationError;
use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use uuid::Uuid;

/// Mock queue provider for testing
pub struct MockQueueProvider {
    /// Jobs by queue name
    pub queues: Mutex<HashMap<String, Vec<MockJob>>>,
    /// Recorded operations
    pub operations: Mutex<Vec<QueueOperation>>,
    /// Whether to fail
    pub should_fail: Mutex<bool>,
}

#[derive(Debug, Clone)]
pub struct MockJob {
    pub id: Uuid,
    pub data: serde_json::Value,
    pub state: String,
    pub delay: Option<Duration>,
}

#[derive(Debug, Clone)]
pub enum QueueOperation {
    Enqueue { queue: String, id: Uuid },
    EnqueueDelayed { queue: String, id: Uuid, delay_secs: u64 },
    Delete { queue: String, id: Uuid },
    Retry { queue: String, id: Uuid },
}

impl MockQueueProvider {
    pub fn new() -> Self {
        Self {
            queues: Mutex::new(HashMap::new()),
            operations: Mutex::new(Vec::new()),
            should_fail: Mutex::new(false),
        }
    }

    pub fn recorded_operations(&self) -> Vec<QueueOperation> {
        self.operations.lock().unwrap().clone()
    }

    pub fn jobs_in_queue(&self, queue: &str) -> Vec<MockJob> {
        self.queues.lock().unwrap()
            .get(queue)
            .cloned()
            .unwrap_or_default()
    }

    pub fn assert_enqueued(&self, queue: &str) {
        let ops = self.operations.lock().unwrap();
        assert!(
            ops.iter().any(|op| matches!(op, QueueOperation::Enqueue { queue: q, .. } if q == queue)),
            "Expected job enqueued to '{}', operations: {:?}",
            queue, ops
        );
    }
}

impl Default for MockQueueProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QueueProvider for MockQueueProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn enqueue<T: Serialize + Send + Sync>(
        &self,
        queue: &str,
        data: T,
        _options: EnqueueOptions,
    ) -> Result<Uuid, IntegrationError> {
        if *self.should_fail.lock().unwrap() {
            return Err(IntegrationError::Provider("Mock failure".to_string()));
        }

        let id = Uuid::new_v4();
        let job = MockJob {
            id,
            data: serde_json::to_value(&data)?,
            state: "pending".to_string(),
            delay: None,
        };

        self.queues.lock().unwrap()
            .entry(queue.to_string())
            .or_default()
            .push(job);

        self.operations.lock().unwrap().push(QueueOperation::Enqueue {
            queue: queue.to_string(),
            id,
        });

        Ok(id)
    }

    async fn enqueue_delayed<T: Serialize + Send + Sync>(
        &self,
        queue: &str,
        data: T,
        delay: Duration,
        _options: EnqueueOptions,
    ) -> Result<Uuid, IntegrationError> {
        let id = Uuid::new_v4();
        let job = MockJob {
            id,
            data: serde_json::to_value(&data)?,
            state: "pending".to_string(),
            delay: Some(delay),
        };

        self.queues.lock().unwrap()
            .entry(queue.to_string())
            .or_default()
            .push(job);

        self.operations.lock().unwrap().push(QueueOperation::EnqueueDelayed {
            queue: queue.to_string(),
            id,
            delay_secs: delay.as_secs(),
        });

        Ok(id)
    }

    async fn queue_length(&self, queue: &str) -> Result<u64, IntegrationError> {
        let queues = self.queues.lock().unwrap();
        Ok(queues.get(queue)
            .map(|q| q.iter().filter(|j| j.state == "pending").count())
            .unwrap_or(0) as u64)
    }

    async fn failed_count(&self, queue: &str) -> Result<u64, IntegrationError> {
        let queues = self.queues.lock().unwrap();
        Ok(queues.get(queue)
            .map(|q| q.iter().filter(|j| j.state == "failed").count())
            .unwrap_or(0) as u64)
    }

    async fn retry_job(&self, queue: &str, job_id: Uuid) -> Result<bool, IntegrationError> {
        let mut queues = self.queues.lock().unwrap();
        if let Some(jobs) = queues.get_mut(queue) {
            if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
                job.state = "pending".to_string();
                self.operations.lock().unwrap().push(QueueOperation::Retry {
                    queue: queue.to_string(),
                    id: job_id,
                });
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn delete_job(&self, queue: &str, job_id: Uuid) -> Result<bool, IntegrationError> {
        let mut queues = self.queues.lock().unwrap();
        if let Some(jobs) = queues.get_mut(queue) {
            let len_before = jobs.len();
            jobs.retain(|j| j.id != job_id);
            self.operations.lock().unwrap().push(QueueOperation::Delete {
                queue: queue.to_string(),
                id: job_id,
            });
            return Ok(jobs.len() < len_before);
        }
        Ok(false)
    }

    async fn pause(&self, _queue: &str) -> Result<(), IntegrationError> {
        Ok(())
    }

    async fn resume(&self, _queue: &str) -> Result<(), IntegrationError> {
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        Ok(!*self.should_fail.lock().unwrap())
    }
}
```

## Crate: `fraiseql-integrations`

```
crates/fraiseql-integrations/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs              # Integration configuration
│   ├── search/
│   │   ├── mod.rs             # SearchProvider trait
│   │   ├── meilisearch.rs     # Meilisearch provider
│   │   ├── typesense.rs       # Typesense provider
│   │   ├── algolia.rs         # Algolia provider
│   │   └── postgres.rs        # PostgreSQL full-text search
│   ├── cache/
│   │   ├── mod.rs             # CacheProvider trait
│   │   ├── redis.rs           # Redis cache
│   │   ├── memory.rs          # In-memory LRU cache
│   │   └── postgres.rs        # PostgreSQL-based cache
│   ├── queue/
│   │   ├── mod.rs             # QueueProvider trait
│   │   ├── redis.rs           # Redis (BullMQ-style) queue
│   │   ├── postgres.rs        # PostgreSQL (pg_boss-style) queue
│   │   └── rabbitmq.rs        # RabbitMQ queue
│   └── error.rs
└── tests/
    ├── search_test.rs
    ├── cache_test.rs
    └── queue_test.rs
```

---

## Step 1: Configuration Types

### 1.1 Integration Configuration

```rust
// src/config.rs
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level integrations configuration
#[derive(Debug, Clone, Deserialize)]
pub struct IntegrationsConfig {
    #[serde(default)]
    pub search: Option<SearchConfig>,

    #[serde(default)]
    pub cache: Option<CacheConfig>,

    #[serde(default)]
    pub queues: Option<QueueConfig>,
}

// ============================================================
// SEARCH CONFIGURATION
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum SearchConfig {
    Meilisearch(MeilisearchConfig),
    Typesense(TypesenseConfig),
    Algolia(AlgoliaConfig),
    Postgresql(PostgresSearchConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeilisearchConfig {
    pub host_env: String,
    pub api_key_env: String,
    #[serde(default)]
    pub indexes: HashMap<String, IndexConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TypesenseConfig {
    pub host_env: String,
    pub api_key_env: String,
    #[serde(default = "default_typesense_port")]
    pub port: u16,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub collections: HashMap<String, CollectionConfig>,
}

fn default_typesense_port() -> u16 { 8108 }

#[derive(Debug, Clone, Deserialize)]
pub struct AlgoliaConfig {
    pub app_id_env: String,
    pub api_key_env: String,
    #[serde(default)]
    pub indexes: HashMap<String, IndexConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresSearchConfig {
    /// Column to search in tsvector format
    #[serde(default)]
    pub default_language: String,
    /// Tables to enable FTS on
    #[serde(default)]
    pub tables: HashMap<String, FtsTableConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexConfig {
    /// Searchable fields
    pub searchable: Vec<String>,
    /// Filterable fields
    #[serde(default)]
    pub filterable: Vec<String>,
    /// Sortable fields
    #[serde(default)]
    pub sortable: Vec<String>,
    /// Primary key field
    #[serde(default = "default_primary_key")]
    pub primary_key: String,
}

fn default_primary_key() -> String { "id".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct CollectionConfig {
    /// Schema fields for Typesense
    pub fields: Vec<TypesenseField>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TypesenseField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub facet: bool,
    #[serde(default)]
    pub index: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FtsTableConfig {
    /// Columns to include in full-text search
    pub columns: Vec<String>,
    /// Weight for each column (A, B, C, D)
    #[serde(default)]
    pub weights: HashMap<String, String>,
    /// Language for text search
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String { "english".to_string() }

// ============================================================
// CACHE CONFIGURATION
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase")]
pub enum CacheConfig {
    Redis(RedisCacheConfig),
    Memory(MemoryCacheConfig),
    Postgresql(PostgresCacheConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisCacheConfig {
    pub url_env: String,
    #[serde(default = "default_cache_prefix")]
    pub prefix: String,
    #[serde(default = "default_default_ttl")]
    pub default_ttl: u64,  // seconds
    #[serde(default)]
    pub cluster: bool,
}

fn default_cache_prefix() -> String { "fraiseql:cache:".to_string() }
fn default_default_ttl() -> u64 { 3600 }

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryCacheConfig {
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_default_ttl")]
    pub default_ttl: u64,
}

fn default_max_entries() -> usize { 10_000 }

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresCacheConfig {
    /// Table name for cache entries
    #[serde(default = "default_cache_table")]
    pub table: String,
    #[serde(default = "default_default_ttl")]
    pub default_ttl: u64,
}

fn default_cache_table() -> String { "_system.cache".to_string() }

// ============================================================
// QUEUE CONFIGURATION
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase")]
pub enum QueueConfig {
    Redis(RedisQueueConfig),
    Postgresql(PostgresQueueConfig),
    Rabbitmq(RabbitMQConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisQueueConfig {
    pub url_env: String,
    #[serde(default = "default_queue_prefix")]
    pub prefix: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default)]
    pub queues: HashMap<String, QueueOptions>,
}

fn default_queue_prefix() -> String { "fraiseql:queue:".to_string() }
fn default_concurrency() -> u32 { 5 }

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresQueueConfig {
    #[serde(default = "default_queue_schema")]
    pub schema: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub queues: HashMap<String, QueueOptions>,
}

fn default_queue_schema() -> String { "_system".to_string() }
fn default_poll_interval() -> u64 { 1000 }

#[derive(Debug, Clone, Deserialize)]
pub struct RabbitMQConfig {
    pub url_env: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default)]
    pub queues: HashMap<String, QueueOptions>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueOptions {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay")]
    pub retry_delay_ms: u64,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
}

fn default_max_retries() -> u32 { 3 }
fn default_retry_delay() -> u64 { 5000 }

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimit {
    pub max: u32,
    pub per_seconds: u64,
}
```

---

## Step 2: Search Provider Trait and Implementations

### 2.1 Search Provider Trait

```rust
// src/search/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::error::IntegrationError;

pub mod meilisearch;
pub mod typesense;
pub mod algolia;
pub mod postgres;

/// Search query parameters
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub query: String,
    pub index: String,
    pub filters: Option<String>,
    pub sort: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub facets: Option<Vec<String>>,
    pub highlight_fields: Option<Vec<String>>,
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total_hits: u64,
    pub processing_time_ms: u64,
    pub facets: Option<HashMap<String, Vec<FacetValue>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub score: Option<f64>,
    pub document: Value,
    pub highlights: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetValue {
    pub value: String,
    pub count: u64,
}

/// Document to index
#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub index: String,
    pub data: Value,
}

/// Trait for search providers
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Search for documents
    async fn search(&self, query: SearchQuery) -> Result<SearchResult, IntegrationError>;

    /// Index a single document
    async fn index_document(&self, doc: Document) -> Result<(), IntegrationError>;

    /// Index multiple documents
    async fn index_documents(&self, docs: Vec<Document>) -> Result<(), IntegrationError>;

    /// Delete a document
    async fn delete_document(&self, index: &str, id: &str) -> Result<(), IntegrationError>;

    /// Delete all documents in an index
    async fn clear_index(&self, index: &str) -> Result<(), IntegrationError>;

    /// Create or update index settings
    async fn configure_index(&self, index: &str, config: &crate::config::IndexConfig) -> Result<(), IntegrationError>;

    /// Health check
    async fn health_check(&self) -> Result<bool, IntegrationError>;
}
```

### 2.2 Meilisearch Provider

```rust
// src/search/meilisearch.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::debug;

use crate::config::MeilisearchConfig;
use crate::error::IntegrationError;

use super::{Document, FacetValue, SearchHit, SearchProvider, SearchQuery, SearchResult};

pub struct MeilisearchProvider {
    client: Client,
    host: String,
    api_key: String,
    indexes: HashMap<String, crate::config::IndexConfig>,
}

impl MeilisearchProvider {
    pub fn new(config: &MeilisearchConfig) -> Result<Self, IntegrationError> {
        let host = std::env::var(&config.host_env).map_err(|_| {
            IntegrationError::Configuration(format!("Missing env var: {}", config.host_env))
        })?;

        let api_key = std::env::var(&config.api_key_env).map_err(|_| {
            IntegrationError::Configuration(format!("Missing env var: {}", config.api_key_env))
        })?;

        Ok(Self {
            client: Client::new(),
            host: host.trim_end_matches('/').to_string(),
            api_key,
            indexes: config.indexes.clone(),
        })
    }
}

#[async_trait]
impl SearchProvider for MeilisearchProvider {
    fn name(&self) -> &'static str {
        "meilisearch"
    }

    async fn search(&self, query: SearchQuery) -> Result<SearchResult, IntegrationError> {
        let url = format!("{}/indexes/{}/search", self.host, query.index);

        let mut body = json!({
            "q": query.query,
        });

        if let Some(limit) = query.limit {
            body["limit"] = json!(limit);
        }
        if let Some(offset) = query.offset {
            body["offset"] = json!(offset);
        }
        if let Some(filters) = &query.filters {
            body["filter"] = json!(filters);
        }
        if let Some(sort) = &query.sort {
            body["sort"] = json!(sort);
        }
        if let Some(facets) = &query.facets {
            body["facets"] = json!(facets);
        }
        if let Some(highlight) = &query.highlight_fields {
            body["attributesToHighlight"] = json!(highlight);
        }

        debug!(index = %query.index, query = %query.query, "Searching Meilisearch");

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| IntegrationError::Provider(format!("Meilisearch request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(IntegrationError::Provider(format!(
                "Meilisearch error: HTTP {} - {}",
                status, error
            )));
        }

        let result: Value = response.json().await?;

        let hits: Vec<SearchHit> = result.get("hits")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter().map(|hit| {
                    let id = hit.get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    let highlights = hit.get("_formatted").map(|f| {
                        let mut h = HashMap::new();
                        if let Some(obj) = f.as_object() {
                            for (k, v) in obj {
                                if let Some(s) = v.as_str() {
                                    h.insert(k.clone(), s.to_string());
                                }
                            }
                        }
                        h
                    });

                    SearchHit {
                        id,
                        score: None,
                        document: hit.clone(),
                        highlights,
                    }
                }).collect()
            })
            .unwrap_or_default();

        let total_hits = result.get("estimatedTotalHits")
            .or_else(|| result.get("totalHits"))
            .and_then(|v| v.as_u64())
            .unwrap_or(hits.len() as u64);

        let processing_time_ms = result.get("processingTimeMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let facets = result.get("facetDistribution").map(|fd| {
            let mut facet_map = HashMap::new();
            if let Some(obj) = fd.as_object() {
                for (field, values) in obj {
                    if let Some(values_obj) = values.as_object() {
                        let facet_values: Vec<FacetValue> = values_obj
                            .iter()
                            .map(|(value, count)| FacetValue {
                                value: value.clone(),
                                count: count.as_u64().unwrap_or(0),
                            })
                            .collect();
                        facet_map.insert(field.clone(), facet_values);
                    }
                }
            }
            facet_map
        });

        Ok(SearchResult {
            hits,
            total_hits,
            processing_time_ms,
            facets,
        })
    }

    async fn index_document(&self, doc: Document) -> Result<(), IntegrationError> {
        self.index_documents(vec![doc]).await
    }

    async fn index_documents(&self, docs: Vec<Document>) -> Result<(), IntegrationError> {
        if docs.is_empty() {
            return Ok(());
        }

        // Group by index
        let mut by_index: HashMap<String, Vec<Value>> = HashMap::new();
        for doc in docs {
            by_index
                .entry(doc.index)
                .or_default()
                .push(doc.data);
        }

        for (index, documents) in by_index {
            let url = format!("{}/indexes/{}/documents", self.host, index);

            debug!(index = %index, count = documents.len(), "Indexing documents to Meilisearch");

            let response = self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&documents)
                .send()
                .await?;

            if !response.status().is_success() {
                let error = response.text().await.unwrap_or_default();
                return Err(IntegrationError::Provider(format!(
                    "Failed to index documents: {}",
                    error
                )));
            }
        }

        Ok(())
    }

    async fn delete_document(&self, index: &str, id: &str) -> Result<(), IntegrationError> {
        let url = format!("{}/indexes/{}/documents/{}", self.host, index, id);

        let response = self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(IntegrationError::Provider(format!(
                "Failed to delete document: {}",
                error
            )));
        }

        Ok(())
    }

    async fn clear_index(&self, index: &str) -> Result<(), IntegrationError> {
        let url = format!("{}/indexes/{}/documents", self.host, index);

        let response = self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(IntegrationError::Provider(format!(
                "Failed to clear index: {}",
                error
            )));
        }

        Ok(())
    }

    async fn configure_index(
        &self,
        index: &str,
        config: &crate::config::IndexConfig,
    ) -> Result<(), IntegrationError> {
        // Create index if it doesn't exist
        let create_url = format!("{}/indexes", self.host);
        let _ = self.client
            .post(&create_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "uid": index,
                "primaryKey": config.primary_key,
            }))
            .send()
            .await;

        // Update settings
        let settings_url = format!("{}/indexes/{}/settings", self.host, index);

        let settings = json!({
            "searchableAttributes": config.searchable,
            "filterableAttributes": config.filterable,
            "sortableAttributes": config.sortable,
        });

        let response = self.client
            .patch(&settings_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&settings)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(IntegrationError::Provider(format!(
                "Failed to configure index: {}",
                error
            )));
        }

        Ok(())
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        let url = format!("{}/health", self.host);

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        Ok(response.status().is_success())
    }
}
```

### 2.3 PostgreSQL Full-Text Search Provider

```rust
// src/search/postgres.rs
use async_trait::async_trait;
use sqlx::PgPool;
use tracing::debug;

use crate::config::PostgresSearchConfig;
use crate::error::IntegrationError;

use super::{Document, SearchHit, SearchProvider, SearchQuery, SearchResult};

/// PostgreSQL full-text search provider using tsvector/tsquery
pub struct PostgresSearchProvider {
    pool: PgPool,
    config: PostgresSearchConfig,
}

impl PostgresSearchProvider {
    pub fn new(pool: PgPool, config: &PostgresSearchConfig) -> Self {
        Self {
            pool,
            config: config.clone(),
        }
    }

    /// Initialize FTS indexes for configured tables
    pub async fn initialize(&self) -> Result<(), IntegrationError> {
        for (table, table_config) in &self.config.tables {
            // Create tsvector column if it doesn't exist
            let column_name = format!("{}_search", table);
            let columns = table_config.columns.join(", ");
            let language = &table_config.language;

            // Build weighted tsvector expression
            let mut weighted_parts = Vec::new();
            for col in &table_config.columns {
                let weight = table_config.weights.get(col).map(|w| w.as_str()).unwrap_or("D");
                weighted_parts.push(format!(
                    "setweight(to_tsvector('{}', COALESCE({}, '')), '{}')",
                    language, col, weight
                ));
            }
            let tsvector_expr = weighted_parts.join(" || ");

            // Create or replace the trigger function
            let function_sql = format!(
                r#"
                CREATE OR REPLACE FUNCTION {table}_search_trigger()
                RETURNS TRIGGER AS $$
                BEGIN
                    NEW.{column_name} := {tsvector_expr};
                    RETURN NEW;
                END;
                $$ LANGUAGE plpgsql;
                "#,
                table = table,
                column_name = column_name,
                tsvector_expr = tsvector_expr,
            );

            sqlx::query(&function_sql).execute(&self.pool).await?;

            // Create trigger if not exists
            let trigger_sql = format!(
                r#"
                DROP TRIGGER IF EXISTS {table}_search_update ON {table};
                CREATE TRIGGER {table}_search_update
                BEFORE INSERT OR UPDATE ON {table}
                FOR EACH ROW
                EXECUTE FUNCTION {table}_search_trigger();
                "#,
                table = table,
            );

            sqlx::query(&trigger_sql).execute(&self.pool).await?;

            // Create GIN index
            let index_sql = format!(
                "CREATE INDEX IF NOT EXISTS idx_{table}_search ON {table} USING GIN ({column_name})",
                table = table,
                column_name = column_name,
            );

            sqlx::query(&index_sql).execute(&self.pool).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl SearchProvider for PostgresSearchProvider {
    fn name(&self) -> &'static str {
        "postgresql"
    }

    async fn search(&self, query: SearchQuery) -> Result<SearchResult, IntegrationError> {
        let table = &query.index;
        let search_column = format!("{}_search", table);
        let language = self.config.tables.get(table)
            .map(|c| c.language.as_str())
            .unwrap_or("english");

        // Build the query
        let sql = format!(
            r#"
            SELECT
                id,
                ts_rank({search_column}, plainto_tsquery($1, $2)) AS score,
                to_jsonb({table}.*) AS document
            FROM {table}
            WHERE {search_column} @@ plainto_tsquery($1, $2)
            ORDER BY score DESC
            LIMIT $3 OFFSET $4
            "#,
            table = table,
            search_column = search_column,
        );

        debug!(table = %table, query = %query.query, "Searching PostgreSQL FTS");

        let limit = query.limit.unwrap_or(20) as i64;
        let offset = query.offset.unwrap_or(0) as i64;

        let rows = sqlx::query_as::<_, (String, f64, serde_json::Value)>(&sql)
            .bind(language)
            .bind(&query.query)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        // Get total count
        let count_sql = format!(
            r#"
            SELECT COUNT(*) as count
            FROM {table}
            WHERE {search_column} @@ plainto_tsquery($1, $2)
            "#,
            table = table,
            search_column = search_column,
        );

        let (total_hits,): (i64,) = sqlx::query_as(&count_sql)
            .bind(language)
            .bind(&query.query)
            .fetch_one(&self.pool)
            .await?;

        let hits: Vec<SearchHit> = rows
            .into_iter()
            .map(|(id, score, document)| SearchHit {
                id,
                score: Some(score),
                document,
                highlights: None,  // PostgreSQL FTS doesn't provide highlighting by default
            })
            .collect();

        Ok(SearchResult {
            hits,
            total_hits: total_hits as u64,
            processing_time_ms: 0,  // Would need to measure
            facets: None,
        })
    }

    async fn index_document(&self, _doc: Document) -> Result<(), IntegrationError> {
        // PostgreSQL FTS is automatically updated via triggers
        // Documents are indexed when inserted/updated to the table
        Ok(())
    }

    async fn index_documents(&self, _docs: Vec<Document>) -> Result<(), IntegrationError> {
        Ok(())
    }

    async fn delete_document(&self, _index: &str, _id: &str) -> Result<(), IntegrationError> {
        // Handled by normal DELETE operations
        Ok(())
    }

    async fn clear_index(&self, _index: &str) -> Result<(), IntegrationError> {
        // Would require TRUNCATE
        Ok(())
    }

    async fn configure_index(
        &self,
        _index: &str,
        _config: &crate::config::IndexConfig,
    ) -> Result<(), IntegrationError> {
        // Configuration is done via initialize()
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        let result: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(result.0 == 1)
    }
}
```

---

## Step 3: Cache Provider Trait and Implementations

### 3.1 Cache Provider Trait

```rust
// src/cache/mod.rs
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::error::IntegrationError;

pub mod redis;
pub mod memory;
pub mod postgres;

/// Trait for cache providers
#[async_trait]
pub trait CacheProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Get a value by key
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IntegrationError>;

    /// Get a typed value
    async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, IntegrationError> {
        match self.get(key).await? {
            Some(bytes) => {
                let value = serde_json::from_slice(&bytes)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Set a value with optional TTL
    async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<(), IntegrationError>;

    /// Set a typed value
    async fn set_json<T: Serialize + Sync>(
        &self,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
    ) -> Result<(), IntegrationError> {
        let bytes = serde_json::to_vec(value)?;
        self.set(key, &bytes, ttl).await
    }

    /// Delete a key
    async fn delete(&self, key: &str) -> Result<bool, IntegrationError>;

    /// Delete multiple keys by pattern
    async fn delete_pattern(&self, pattern: &str) -> Result<u64, IntegrationError>;

    /// Check if key exists
    async fn exists(&self, key: &str) -> Result<bool, IntegrationError>;

    /// Get TTL remaining
    async fn ttl(&self, key: &str) -> Result<Option<Duration>, IntegrationError>;

    /// Set TTL on existing key
    async fn expire(&self, key: &str, ttl: Duration) -> Result<bool, IntegrationError>;

    /// Increment a counter
    async fn incr(&self, key: &str, delta: i64) -> Result<i64, IntegrationError>;

    /// Health check
    async fn health_check(&self) -> Result<bool, IntegrationError>;
}
```

### 3.2 Redis Cache Provider

```rust
// src/cache/redis.rs
use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client};
use std::time::Duration;
use tracing::debug;

use crate::config::RedisCacheConfig;
use crate::error::IntegrationError;

use super::CacheProvider;

pub struct RedisCache {
    connection: MultiplexedConnection,
    prefix: String,
    default_ttl: u64,
}

impl RedisCache {
    pub async fn new(config: &RedisCacheConfig) -> Result<Self, IntegrationError> {
        let url = std::env::var(&config.url_env).map_err(|_| {
            IntegrationError::Configuration(format!("Missing env var: {}", config.url_env))
        })?;

        let client = Client::open(url.as_str()).map_err(|e| {
            IntegrationError::Configuration(format!("Invalid Redis URL: {}", e))
        })?;

        let connection = client.get_multiplexed_async_connection().await.map_err(|e| {
            IntegrationError::Provider(format!("Failed to connect to Redis: {}", e))
        })?;

        Ok(Self {
            connection,
            prefix: config.prefix.clone(),
            default_ttl: config.default_ttl,
        })
    }

    fn prefixed_key(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }
}

#[async_trait]
impl CacheProvider for RedisCache {
    fn name(&self) -> &'static str {
        "redis"
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IntegrationError> {
        let prefixed = self.prefixed_key(key);
        let mut conn = self.connection.clone();

        let result: Option<Vec<u8>> = conn.get(&prefixed).await.map_err(|e| {
            IntegrationError::Provider(format!("Redis GET failed: {}", e))
        })?;

        debug!(key = %key, hit = result.is_some(), "Cache get");
        Ok(result)
    }

    async fn set(
        &self,
        key: &str,
        value: &[u8],
        ttl: Option<Duration>,
    ) -> Result<(), IntegrationError> {
        let prefixed = self.prefixed_key(key);
        let mut conn = self.connection.clone();

        let ttl_secs = ttl.map(|d| d.as_secs()).unwrap_or(self.default_ttl);

        if ttl_secs > 0 {
            conn.set_ex::<_, _, ()>(&prefixed, value, ttl_secs).await
        } else {
            conn.set::<_, _, ()>(&prefixed, value).await
        }.map_err(|e| IntegrationError::Provider(format!("Redis SET failed: {}", e)))?;

        debug!(key = %key, ttl = ttl_secs, "Cache set");
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, IntegrationError> {
        let prefixed = self.prefixed_key(key);
        let mut conn = self.connection.clone();

        let deleted: i32 = conn.del(&prefixed).await.map_err(|e| {
            IntegrationError::Provider(format!("Redis DEL failed: {}", e))
        })?;

        debug!(key = %key, deleted = deleted > 0, "Cache delete");
        Ok(deleted > 0)
    }

    async fn delete_pattern(&self, pattern: &str) -> Result<u64, IntegrationError> {
        let prefixed_pattern = self.prefixed_key(pattern);
        let mut conn = self.connection.clone();

        // SCAN for keys matching pattern
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&prefixed_pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| IntegrationError::Provider(format!("Redis KEYS failed: {}", e)))?;

        if keys.is_empty() {
            return Ok(0);
        }

        let deleted: i64 = conn.del(&keys).await.map_err(|e| {
            IntegrationError::Provider(format!("Redis DEL failed: {}", e))
        })?;

        debug!(pattern = %pattern, deleted = deleted, "Cache delete pattern");
        Ok(deleted as u64)
    }

    async fn exists(&self, key: &str) -> Result<bool, IntegrationError> {
        let prefixed = self.prefixed_key(key);
        let mut conn = self.connection.clone();

        let exists: bool = conn.exists(&prefixed).await.map_err(|e| {
            IntegrationError::Provider(format!("Redis EXISTS failed: {}", e))
        })?;

        Ok(exists)
    }

    async fn ttl(&self, key: &str) -> Result<Option<Duration>, IntegrationError> {
        let prefixed = self.prefixed_key(key);
        let mut conn = self.connection.clone();

        let ttl: i64 = conn.ttl(&prefixed).await.map_err(|e| {
            IntegrationError::Provider(format!("Redis TTL failed: {}", e))
        })?;

        Ok(match ttl {
            -2 => None,  // Key doesn't exist
            -1 => None,  // Key has no TTL
            n => Some(Duration::from_secs(n as u64)),
        })
    }

    async fn expire(&self, key: &str, ttl: Duration) -> Result<bool, IntegrationError> {
        let prefixed = self.prefixed_key(key);
        let mut conn = self.connection.clone();

        let result: bool = conn.expire(&prefixed, ttl.as_secs() as i64).await.map_err(|e| {
            IntegrationError::Provider(format!("Redis EXPIRE failed: {}", e))
        })?;

        Ok(result)
    }

    async fn incr(&self, key: &str, delta: i64) -> Result<i64, IntegrationError> {
        let prefixed = self.prefixed_key(key);
        let mut conn = self.connection.clone();

        let result: i64 = conn.incr(&prefixed, delta).await.map_err(|e| {
            IntegrationError::Provider(format!("Redis INCR failed: {}", e))
        })?;

        Ok(result)
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        let mut conn = self.connection.clone();
        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| IntegrationError::Provider(format!("Redis PING failed: {}", e)))?;

        Ok(pong == "PONG")
    }
}
```

### 3.3 In-Memory LRU Cache

```rust
// src/cache/memory.rs
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::debug;

use crate::config::MemoryCacheConfig;
use crate::error::IntegrationError;

use super::CacheProvider;

struct CacheEntry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
    last_accessed: Instant,
}

pub struct MemoryCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    max_entries: usize,
    default_ttl: Duration,
}

impl MemoryCache {
    pub fn new(config: &MemoryCacheConfig) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries: config.max_entries,
            default_ttl: Duration::from_secs(config.default_ttl),
        }
    }

    fn evict_expired(&self) {
        let now = Instant::now();
        let mut entries = self.entries.write();
        entries.retain(|_, entry| {
            entry.expires_at.map(|e| e > now).unwrap_or(true)
        });
    }

    fn evict_lru(&self) {
        let mut entries = self.entries.write();
        if entries.len() <= self.max_entries {
            return;
        }

        // Find LRU entry
        let lru_key = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(k, _)| k.clone());

        if let Some(key) = lru_key {
            entries.remove(&key);
        }
    }
}

#[async_trait]
impl CacheProvider for MemoryCache {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IntegrationError> {
        let now = Instant::now();
        let mut entries = self.entries.write();

        if let Some(entry) = entries.get_mut(key) {
            // Check expiration
            if entry.expires_at.map(|e| e > now).unwrap_or(true) {
                entry.last_accessed = now;
                debug!(key = %key, hit = true, "Memory cache get");
                return Ok(Some(entry.value.clone()));
            } else {
                // Expired, remove it
                entries.remove(key);
            }
        }

        debug!(key = %key, hit = false, "Memory cache get");
        Ok(None)
    }

    async fn set(
        &self,
        key: &str,
        value: &[u8],
        ttl: Option<Duration>,
    ) -> Result<(), IntegrationError> {
        let now = Instant::now();
        let ttl = ttl.unwrap_or(self.default_ttl);
        let expires_at = if ttl.as_secs() > 0 {
            Some(now + ttl)
        } else {
            None
        };

        {
            let mut entries = self.entries.write();
            entries.insert(key.to_string(), CacheEntry {
                value: value.to_vec(),
                expires_at,
                last_accessed: now,
            });
        }

        // Evict if necessary
        self.evict_expired();
        self.evict_lru();

        debug!(key = %key, "Memory cache set");
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, IntegrationError> {
        let mut entries = self.entries.write();
        let existed = entries.remove(key).is_some();
        debug!(key = %key, deleted = existed, "Memory cache delete");
        Ok(existed)
    }

    async fn delete_pattern(&self, pattern: &str) -> Result<u64, IntegrationError> {
        // Simple glob-style pattern matching (* only)
        let pattern = pattern.replace('*', "");
        let mut entries = self.entries.write();

        let keys_to_delete: Vec<String> = entries
            .keys()
            .filter(|k| k.starts_with(&pattern))
            .cloned()
            .collect();

        let count = keys_to_delete.len() as u64;
        for key in keys_to_delete {
            entries.remove(&key);
        }

        debug!(pattern = %pattern, deleted = count, "Memory cache delete pattern");
        Ok(count)
    }

    async fn exists(&self, key: &str) -> Result<bool, IntegrationError> {
        let now = Instant::now();
        let entries = self.entries.read();

        Ok(entries.get(key).map(|e| {
            e.expires_at.map(|exp| exp > now).unwrap_or(true)
        }).unwrap_or(false))
    }

    async fn ttl(&self, key: &str) -> Result<Option<Duration>, IntegrationError> {
        let now = Instant::now();
        let entries = self.entries.read();

        Ok(entries.get(key).and_then(|e| {
            e.expires_at.map(|exp| {
                if exp > now {
                    exp.duration_since(now)
                } else {
                    Duration::ZERO
                }
            })
        }))
    }

    async fn expire(&self, key: &str, ttl: Duration) -> Result<bool, IntegrationError> {
        let now = Instant::now();
        let mut entries = self.entries.write();

        if let Some(entry) = entries.get_mut(key) {
            entry.expires_at = Some(now + ttl);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn incr(&self, key: &str, delta: i64) -> Result<i64, IntegrationError> {
        let mut entries = self.entries.write();

        let current = entries
            .get(key)
            .and_then(|e| String::from_utf8(e.value.clone()).ok())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        let new_value = current + delta;

        entries.insert(key.to_string(), CacheEntry {
            value: new_value.to_string().into_bytes(),
            expires_at: None,
            last_accessed: Instant::now(),
        });

        Ok(new_value)
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        Ok(true)
    }
}
```

---

## Step 4: Queue Provider Trait and Implementations

### 4.1 Queue Provider Trait

```rust
// src/queue/mod.rs
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

use crate::error::IntegrationError;

pub mod redis;
pub mod postgres;
pub mod rabbitmq;

/// Job to be processed
#[derive(Debug, Clone)]
pub struct Job<T> {
    pub id: Uuid,
    pub queue: String,
    pub data: T,
    pub attempts: u32,
    pub max_retries: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Result of job processing
#[derive(Debug)]
pub enum JobResult {
    /// Job completed successfully
    Complete,
    /// Job failed, should retry
    Retry { reason: String, delay: Option<Duration> },
    /// Job failed permanently, don't retry
    Failed { reason: String },
}

/// Job handler function type
pub type JobHandler<T> = Box<
    dyn Fn(Job<T>) -> Pin<Box<dyn Future<Output = JobResult> + Send>> + Send + Sync
>;

/// Trait for queue providers
#[async_trait]
pub trait QueueProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Enqueue a job
    async fn enqueue<T: Serialize + Send + Sync>(
        &self,
        queue: &str,
        data: T,
        options: EnqueueOptions,
    ) -> Result<Uuid, IntegrationError>;

    /// Enqueue a job with delay
    async fn enqueue_delayed<T: Serialize + Send + Sync>(
        &self,
        queue: &str,
        data: T,
        delay: Duration,
        options: EnqueueOptions,
    ) -> Result<Uuid, IntegrationError>;

    /// Get job count in queue
    async fn queue_length(&self, queue: &str) -> Result<u64, IntegrationError>;

    /// Get failed job count
    async fn failed_count(&self, queue: &str) -> Result<u64, IntegrationError>;

    /// Retry a failed job
    async fn retry_job(&self, queue: &str, job_id: Uuid) -> Result<bool, IntegrationError>;

    /// Delete a job
    async fn delete_job(&self, queue: &str, job_id: Uuid) -> Result<bool, IntegrationError>;

    /// Pause queue processing
    async fn pause(&self, queue: &str) -> Result<(), IntegrationError>;

    /// Resume queue processing
    async fn resume(&self, queue: &str) -> Result<(), IntegrationError>;

    /// Health check
    async fn health_check(&self) -> Result<bool, IntegrationError>;
}

#[derive(Debug, Clone, Default)]
pub struct EnqueueOptions {
    pub priority: Option<i32>,
    pub dedup_key: Option<String>,
    pub timeout: Option<Duration>,
}
```

### 4.2 PostgreSQL Queue (pg_boss-style)

```rust
// src/queue/postgres.rs
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::config::PostgresQueueConfig;
use crate::error::IntegrationError;

use super::{EnqueueOptions, Job, JobResult, QueueProvider};

/// PostgreSQL-based queue (similar to pg_boss)
pub struct PostgresQueue {
    pool: PgPool,
    schema: String,
    poll_interval: Duration,
    concurrency: u32,
}

impl PostgresQueue {
    pub async fn new(pool: PgPool, config: &PostgresQueueConfig) -> Result<Self, IntegrationError> {
        let queue = Self {
            pool,
            schema: config.schema.clone(),
            poll_interval: Duration::from_millis(config.poll_interval_ms),
            concurrency: config.concurrency,
        };

        // Initialize schema
        queue.initialize().await?;

        Ok(queue)
    }

    async fn initialize(&self) -> Result<(), IntegrationError> {
        let sql = format!(
            r#"
            CREATE SCHEMA IF NOT EXISTS {schema};

            CREATE TABLE IF NOT EXISTS {schema}.jobs (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                queue TEXT NOT NULL,
                data JSONB NOT NULL,
                state TEXT NOT NULL DEFAULT 'pending',
                priority INTEGER NOT NULL DEFAULT 0,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 3,
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                failed_at TIMESTAMPTZ,
                error TEXT,
                run_after TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                dedup_key TEXT,
                timeout_seconds INTEGER,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_queue_state
                ON {schema}.jobs(queue, state, run_after, priority DESC)
                WHERE state = 'pending';

            CREATE INDEX IF NOT EXISTS idx_jobs_dedup
                ON {schema}.jobs(queue, dedup_key)
                WHERE dedup_key IS NOT NULL AND state IN ('pending', 'running');

            CREATE INDEX IF NOT EXISTS idx_jobs_stale
                ON {schema}.jobs(state, started_at)
                WHERE state = 'running';
            "#,
            schema = self.schema,
        );

        sqlx::raw_sql(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Start processing jobs for a queue
    pub async fn process<T, H>(&self, queue: &str, handler: H) -> Result<(), IntegrationError>
    where
        T: DeserializeOwned + Send + 'static,
        H: Fn(Job<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = JobResult> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let handler = std::sync::Arc::new(handler);
        let pool = self.pool.clone();
        let schema = self.schema.clone();
        let queue = queue.to_string();
        let poll_interval = self.poll_interval;
        let concurrency = self.concurrency;

        tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel::<Job<T>>(concurrency as usize);

            // Spawner task: poll for jobs and send to workers
            let poll_pool = pool.clone();
            let poll_schema = schema.clone();
            let poll_queue = queue.clone();
            tokio::spawn(async move {
                loop {
                    match Self::fetch_job::<T>(&poll_pool, &poll_schema, &poll_queue).await {
                        Ok(Some(job)) => {
                            if tx.send(job).await.is_err() {
                                break;  // Channel closed
                            }
                        }
                        Ok(None) => {
                            tokio::time::sleep(poll_interval).await;
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to fetch job");
                            tokio::time::sleep(poll_interval).await;
                        }
                    }
                }
            });

            // Worker tasks
            while let Some(job) = rx.recv().await {
                let handler = handler.clone();
                let pool = pool.clone();
                let schema = schema.clone();

                tokio::spawn(async move {
                    let job_id = job.id;
                    let result = handler(job).await;

                    if let Err(e) = Self::complete_job(&pool, &schema, job_id, result).await {
                        error!(job_id = %job_id, error = %e, "Failed to complete job");
                    }
                });
            }
        });

        Ok(())
    }

    async fn fetch_job<T: DeserializeOwned>(
        pool: &PgPool,
        schema: &str,
        queue: &str,
    ) -> Result<Option<Job<T>>, IntegrationError> {
        let sql = format!(
            r#"
            UPDATE {schema}.jobs
            SET state = 'running',
                started_at = NOW(),
                attempts = attempts + 1,
                updated_at = NOW()
            WHERE id = (
                SELECT id FROM {schema}.jobs
                WHERE queue = $1
                    AND state = 'pending'
                    AND run_after <= NOW()
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, queue, data, attempts, max_retries, created_at
            "#,
            schema = schema,
        );

        let row: Option<(Uuid, String, serde_json::Value, i32, i32, DateTime<Utc>)> =
            sqlx::query_as(&sql)
                .bind(queue)
                .fetch_optional(pool)
                .await?;

        match row {
            Some((id, queue, data, attempts, max_retries, created_at)) => {
                let data: T = serde_json::from_value(data)?;
                Ok(Some(Job {
                    id,
                    queue,
                    data,
                    attempts: attempts as u32,
                    max_retries: max_retries as u32,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    async fn complete_job(
        pool: &PgPool,
        schema: &str,
        job_id: Uuid,
        result: JobResult,
    ) -> Result<(), IntegrationError> {
        match result {
            JobResult::Complete => {
                let sql = format!(
                    r#"
                    UPDATE {schema}.jobs
                    SET state = 'completed',
                        completed_at = NOW(),
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                    schema = schema,
                );
                sqlx::query(&sql).bind(job_id).execute(pool).await?;
                debug!(job_id = %job_id, "Job completed");
            }
            JobResult::Retry { reason, delay } => {
                let delay_secs = delay.map(|d| d.as_secs() as i64).unwrap_or(5);
                let sql = format!(
                    r#"
                    UPDATE {schema}.jobs
                    SET state = CASE
                            WHEN attempts >= max_retries THEN 'failed'
                            ELSE 'pending'
                        END,
                        run_after = NOW() + ($2 * INTERVAL '1 second'),
                        error = $3,
                        failed_at = CASE
                            WHEN attempts >= max_retries THEN NOW()
                            ELSE NULL
                        END,
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                    schema = schema,
                );
                sqlx::query(&sql)
                    .bind(job_id)
                    .bind(delay_secs)
                    .bind(&reason)
                    .execute(pool)
                    .await?;
                debug!(job_id = %job_id, reason = %reason, "Job retry scheduled");
            }
            JobResult::Failed { reason } => {
                let sql = format!(
                    r#"
                    UPDATE {schema}.jobs
                    SET state = 'failed',
                        failed_at = NOW(),
                        error = $2,
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                    schema = schema,
                );
                sqlx::query(&sql)
                    .bind(job_id)
                    .bind(&reason)
                    .execute(pool)
                    .await?;
                debug!(job_id = %job_id, reason = %reason, "Job failed permanently");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl QueueProvider for PostgresQueue {
    fn name(&self) -> &'static str {
        "postgresql"
    }

    async fn enqueue<T: Serialize + Send + Sync>(
        &self,
        queue: &str,
        data: T,
        options: EnqueueOptions,
    ) -> Result<Uuid, IntegrationError> {
        // Check for duplicate
        if let Some(dedup_key) = &options.dedup_key {
            let check_sql = format!(
                r#"
                SELECT id FROM {schema}.jobs
                WHERE queue = $1 AND dedup_key = $2
                    AND state IN ('pending', 'running')
                "#,
                schema = self.schema,
            );

            let existing: Option<(Uuid,)> = sqlx::query_as(&check_sql)
                .bind(queue)
                .bind(dedup_key)
                .fetch_optional(&self.pool)
                .await?;

            if let Some((id,)) = existing {
                debug!(queue = %queue, dedup_key = %dedup_key, "Duplicate job skipped");
                return Ok(id);
            }
        }

        let sql = format!(
            r#"
            INSERT INTO {schema}.jobs (queue, data, priority, dedup_key, timeout_seconds, max_retries)
            VALUES ($1, $2, $3, $4, $5, 3)
            RETURNING id
            "#,
            schema = self.schema,
        );

        let data_json = serde_json::to_value(&data)?;
        let priority = options.priority.unwrap_or(0);
        let timeout = options.timeout.map(|d| d.as_secs() as i32);

        let (id,): (Uuid,) = sqlx::query_as(&sql)
            .bind(queue)
            .bind(&data_json)
            .bind(priority)
            .bind(&options.dedup_key)
            .bind(timeout)
            .fetch_one(&self.pool)
            .await?;

        debug!(queue = %queue, job_id = %id, "Job enqueued");
        Ok(id)
    }

    async fn enqueue_delayed<T: Serialize + Send + Sync>(
        &self,
        queue: &str,
        data: T,
        delay: Duration,
        options: EnqueueOptions,
    ) -> Result<Uuid, IntegrationError> {
        let sql = format!(
            r#"
            INSERT INTO {schema}.jobs (queue, data, priority, run_after, dedup_key, timeout_seconds, max_retries)
            VALUES ($1, $2, $3, NOW() + ($4 * INTERVAL '1 second'), $5, $6, 3)
            RETURNING id
            "#,
            schema = self.schema,
        );

        let data_json = serde_json::to_value(&data)?;
        let priority = options.priority.unwrap_or(0);
        let delay_secs = delay.as_secs() as i64;
        let timeout = options.timeout.map(|d| d.as_secs() as i32);

        let (id,): (Uuid,) = sqlx::query_as(&sql)
            .bind(queue)
            .bind(&data_json)
            .bind(priority)
            .bind(delay_secs)
            .bind(&options.dedup_key)
            .bind(timeout)
            .fetch_one(&self.pool)
            .await?;

        debug!(queue = %queue, job_id = %id, delay_secs = delay_secs, "Delayed job enqueued");
        Ok(id)
    }

    async fn queue_length(&self, queue: &str) -> Result<u64, IntegrationError> {
        let sql = format!(
            "SELECT COUNT(*) FROM {schema}.jobs WHERE queue = $1 AND state = 'pending'",
            schema = self.schema,
        );

        let (count,): (i64,) = sqlx::query_as(&sql)
            .bind(queue)
            .fetch_one(&self.pool)
            .await?;

        Ok(count as u64)
    }

    async fn failed_count(&self, queue: &str) -> Result<u64, IntegrationError> {
        let sql = format!(
            "SELECT COUNT(*) FROM {schema}.jobs WHERE queue = $1 AND state = 'failed'",
            schema = self.schema,
        );

        let (count,): (i64,) = sqlx::query_as(&sql)
            .bind(queue)
            .fetch_one(&self.pool)
            .await?;

        Ok(count as u64)
    }

    async fn retry_job(&self, queue: &str, job_id: Uuid) -> Result<bool, IntegrationError> {
        let sql = format!(
            r#"
            UPDATE {schema}.jobs
            SET state = 'pending',
                run_after = NOW(),
                attempts = 0,
                failed_at = NULL,
                error = NULL,
                updated_at = NOW()
            WHERE id = $1 AND queue = $2 AND state = 'failed'
            "#,
            schema = self.schema,
        );

        let result = sqlx::query(&sql)
            .bind(job_id)
            .bind(queue)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_job(&self, queue: &str, job_id: Uuid) -> Result<bool, IntegrationError> {
        let sql = format!(
            "DELETE FROM {schema}.jobs WHERE id = $1 AND queue = $2",
            schema = self.schema,
        );

        let result = sqlx::query(&sql)
            .bind(job_id)
            .bind(queue)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn pause(&self, queue: &str) -> Result<(), IntegrationError> {
        // Store pause state in a separate table
        let sql = format!(
            r#"
            INSERT INTO {schema}.queue_state (queue, paused)
            VALUES ($1, true)
            ON CONFLICT (queue) DO UPDATE SET paused = true
            "#,
            schema = self.schema,
        );

        sqlx::query(&sql).bind(queue).execute(&self.pool).await?;
        Ok(())
    }

    async fn resume(&self, queue: &str) -> Result<(), IntegrationError> {
        let sql = format!(
            "UPDATE {schema}.queue_state SET paused = false WHERE queue = $1",
            schema = self.schema,
        );

        sqlx::query(&sql).bind(queue).execute(&self.pool).await?;
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, IntegrationError> {
        let result: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(result.0 == 1)
    }
}
```

---

## Step 5: Comprehensive Error Types

### 5.1 Error Codes

```rust
// src/error.rs
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

/// Integration error codes for structured error responses
/// Format: IN### where ### is a numeric code
///
/// Ranges:
/// - IN001-IN099: Configuration errors
/// - IN100-IN199: Search errors
/// - IN200-IN299: Cache errors
/// - IN300-IN399: Queue errors
/// - IN400-IN499: Provider-specific errors
/// - IN500-IN599: Network/transport errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IntegrationErrorCode {
    // Configuration errors (IN001-IN099)
    /// Missing configuration
    #[serde(rename = "IN001")]
    MissingConfiguration,
    /// Invalid configuration
    #[serde(rename = "IN002")]
    InvalidConfiguration,
    /// Missing environment variable
    #[serde(rename = "IN003")]
    MissingEnvVar,
    /// Provider not configured
    #[serde(rename = "IN004")]
    ProviderNotConfigured,

    // Search errors (IN100-IN199)
    /// Index not found
    #[serde(rename = "IN100")]
    IndexNotFound,
    /// Search query invalid
    #[serde(rename = "IN101")]
    InvalidSearchQuery,
    /// Search timeout
    #[serde(rename = "IN102")]
    SearchTimeout,
    /// Index already exists
    #[serde(rename = "IN103")]
    IndexAlreadyExists,
    /// Document not found
    #[serde(rename = "IN104")]
    DocumentNotFound,
    /// Indexing failed
    #[serde(rename = "IN105")]
    IndexingFailed,

    // Cache errors (IN200-IN299)
    /// Cache key not found
    #[serde(rename = "IN200")]
    CacheKeyNotFound,
    /// Cache connection failed
    #[serde(rename = "IN201")]
    CacheConnectionFailed,
    /// Cache serialization error
    #[serde(rename = "IN202")]
    CacheSerializationError,
    /// Cache capacity exceeded
    #[serde(rename = "IN203")]
    CacheCapacityExceeded,
    /// Invalid TTL
    #[serde(rename = "IN204")]
    InvalidTtl,

    // Queue errors (IN300-IN399)
    /// Job not found
    #[serde(rename = "IN300")]
    JobNotFound,
    /// Queue not found
    #[serde(rename = "IN301")]
    QueueNotFound,
    /// Job already exists (dedup)
    #[serde(rename = "IN302")]
    DuplicateJob,
    /// Job processing failed
    #[serde(rename = "IN303")]
    JobProcessingFailed,
    /// Queue paused
    #[serde(rename = "IN304")]
    QueuePaused,
    /// Job timeout
    #[serde(rename = "IN305")]
    JobTimeout,
    /// Max retries exceeded
    #[serde(rename = "IN306")]
    MaxRetriesExceeded,

    // Provider-specific errors (IN400-IN499)
    /// Meilisearch error
    #[serde(rename = "IN400")]
    MeilisearchError,
    /// Typesense error
    #[serde(rename = "IN401")]
    TypesenseError,
    /// Algolia error
    #[serde(rename = "IN402")]
    AlgoliaError,
    /// Redis error
    #[serde(rename = "IN403")]
    RedisError,
    /// RabbitMQ error
    #[serde(rename = "IN404")]
    RabbitMQError,

    // Network errors (IN500-IN599)
    /// Connection timeout
    #[serde(rename = "IN500")]
    ConnectionTimeout,
    /// Connection refused
    #[serde(rename = "IN501")]
    ConnectionRefused,
    /// Provider unavailable
    #[serde(rename = "IN502")]
    ProviderUnavailable,
    /// Circuit breaker open
    #[serde(rename = "IN503")]
    CircuitBreakerOpen,
}

impl IntegrationErrorCode {
    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::MissingConfiguration => "https://fraiseql.dev/docs/errors/IN001",
            Self::InvalidConfiguration => "https://fraiseql.dev/docs/errors/IN002",
            Self::MissingEnvVar => "https://fraiseql.dev/docs/errors/IN003",
            Self::ProviderNotConfigured => "https://fraiseql.dev/docs/errors/IN004",
            Self::IndexNotFound => "https://fraiseql.dev/docs/errors/IN100",
            Self::InvalidSearchQuery => "https://fraiseql.dev/docs/errors/IN101",
            Self::SearchTimeout => "https://fraiseql.dev/docs/errors/IN102",
            Self::IndexAlreadyExists => "https://fraiseql.dev/docs/errors/IN103",
            Self::DocumentNotFound => "https://fraiseql.dev/docs/errors/IN104",
            Self::IndexingFailed => "https://fraiseql.dev/docs/errors/IN105",
            Self::CacheKeyNotFound => "https://fraiseql.dev/docs/errors/IN200",
            Self::CacheConnectionFailed => "https://fraiseql.dev/docs/errors/IN201",
            Self::CacheSerializationError => "https://fraiseql.dev/docs/errors/IN202",
            Self::CacheCapacityExceeded => "https://fraiseql.dev/docs/errors/IN203",
            Self::InvalidTtl => "https://fraiseql.dev/docs/errors/IN204",
            Self::JobNotFound => "https://fraiseql.dev/docs/errors/IN300",
            Self::QueueNotFound => "https://fraiseql.dev/docs/errors/IN301",
            Self::DuplicateJob => "https://fraiseql.dev/docs/errors/IN302",
            Self::JobProcessingFailed => "https://fraiseql.dev/docs/errors/IN303",
            Self::QueuePaused => "https://fraiseql.dev/docs/errors/IN304",
            Self::JobTimeout => "https://fraiseql.dev/docs/errors/IN305",
            Self::MaxRetriesExceeded => "https://fraiseql.dev/docs/errors/IN306",
            Self::MeilisearchError => "https://fraiseql.dev/docs/errors/IN400",
            Self::TypesenseError => "https://fraiseql.dev/docs/errors/IN401",
            Self::AlgoliaError => "https://fraiseql.dev/docs/errors/IN402",
            Self::RedisError => "https://fraiseql.dev/docs/errors/IN403",
            Self::RabbitMQError => "https://fraiseql.dev/docs/errors/IN404",
            Self::ConnectionTimeout => "https://fraiseql.dev/docs/errors/IN500",
            Self::ConnectionRefused => "https://fraiseql.dev/docs/errors/IN501",
            Self::ProviderUnavailable => "https://fraiseql.dev/docs/errors/IN502",
            Self::CircuitBreakerOpen => "https://fraiseql.dev/docs/errors/IN503",
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::SearchTimeout
                | Self::CacheConnectionFailed
                | Self::ConnectionTimeout
                | Self::ConnectionRefused
                | Self::ProviderUnavailable
                | Self::CircuitBreakerOpen
        )
    }

    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidSearchQuery
                | Self::InvalidTtl
                | Self::DuplicateJob
        )
    }
}
```

### 5.2 Error Type with HTTP Response Mapping

```rust
// src/error.rs (continued)

#[derive(Error, Debug)]
pub enum IntegrationError {
    #[error("Configuration error: {message}")]
    Configuration {
        code: IntegrationErrorCode,
        message: String,
    },

    #[error("Provider error ({provider}): {message}")]
    Provider {
        code: IntegrationErrorCode,
        provider: String,
        message: String,
    },

    #[error("Not found: {message}")]
    NotFound {
        code: IntegrationErrorCode,
        message: String,
        resource_type: String,
    },

    #[error("Invalid input: {message}")]
    InvalidInput {
        code: IntegrationErrorCode,
        message: String,
        field: Option<String>,
    },

    #[error("Circuit breaker open for {provider}")]
    CircuitOpen { provider: String },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

impl IntegrationError {
    pub fn code(&self) -> IntegrationErrorCode {
        match self {
            Self::Configuration { code, .. } => *code,
            Self::Provider { code, .. } => *code,
            Self::NotFound { code, .. } => *code,
            Self::InvalidInput { code, .. } => *code,
            Self::CircuitOpen { .. } => IntegrationErrorCode::CircuitBreakerOpen,
            Self::Database(_) => IntegrationErrorCode::ProviderUnavailable,
            Self::Serialization(_) => IntegrationErrorCode::CacheSerializationError,
            Self::Http(_) => IntegrationErrorCode::ConnectionRefused,
        }
    }

    // Convenience constructors
    pub fn index_not_found(index: &str) -> Self {
        Self::NotFound {
            code: IntegrationErrorCode::IndexNotFound,
            message: format!("Index not found: {}", index),
            resource_type: "index".to_string(),
        }
    }

    pub fn job_not_found(job_id: &str) -> Self {
        Self::NotFound {
            code: IntegrationErrorCode::JobNotFound,
            message: format!("Job not found: {}", job_id),
            resource_type: "job".to_string(),
        }
    }

    pub fn cache_key_not_found(key: &str) -> Self {
        Self::NotFound {
            code: IntegrationErrorCode::CacheKeyNotFound,
            message: format!("Cache key not found: {}", key),
            resource_type: "cache_key".to_string(),
        }
    }

    pub fn duplicate_job(dedup_key: &str) -> Self {
        Self::InvalidInput {
            code: IntegrationErrorCode::DuplicateJob,
            message: format!("Job with dedup key already exists: {}", dedup_key),
            field: Some("dedup_key".to_string()),
        }
    }
}

/// JSON error response body
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: IntegrationErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    pub docs_url: &'static str,
}

impl IntoResponse for IntegrationError {
    fn into_response(self) -> Response {
        let code = self.code();

        let (status, body) = match &self {
            IntegrationError::Configuration { message, .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: None,
                    resource_type: None,
                    docs_url: code.docs_url(),
                },
            ),
            IntegrationError::Provider { provider, message, .. } => (
                StatusCode::BAD_GATEWAY,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: Some(provider.clone()),
                    resource_type: None,
                    docs_url: code.docs_url(),
                },
            ),
            IntegrationError::NotFound { message, resource_type, .. } => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: None,
                    resource_type: Some(resource_type.clone()),
                    docs_url: code.docs_url(),
                },
            ),
            IntegrationError::InvalidInput { message, .. } => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: None,
                    resource_type: None,
                    docs_url: code.docs_url(),
                },
            ),
            IntegrationError::CircuitOpen { provider } => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code,
                    message: format!("Service temporarily unavailable: {}", provider),
                    provider: Some(provider.clone()),
                    resource_type: None,
                    docs_url: code.docs_url(),
                },
            ),
            IntegrationError::Database(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: e.to_string(),
                    provider: Some("postgresql".to_string()),
                    resource_type: None,
                    docs_url: code.docs_url(),
                },
            ),
            IntegrationError::Serialization(e) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code,
                    message: e.to_string(),
                    provider: None,
                    resource_type: None,
                    docs_url: code.docs_url(),
                },
            ),
            IntegrationError::Http(e) => (
                StatusCode::BAD_GATEWAY,
                ErrorBody {
                    code,
                    message: e.to_string(),
                    provider: None,
                    resource_type: None,
                    docs_url: code.docs_url(),
                },
            ),
        };

        (status, Json(ErrorResponse { error: body })).into_response()
    }
}
```
```

---

## Step 6: Comprehensive Unit Tests

### 6.1 Search Provider Tests

```rust
// tests/search_test.rs
use fraiseql_integrations::{
    search::{Document, MockSearchProvider, SearchHit, SearchProvider, SearchQuery, SearchResult},
    error::IntegrationErrorCode,
};
use serde_json::json;

#[tokio::test]
async fn test_mock_search_indexes_and_retrieves_documents() {
    let provider = MockSearchProvider::new();

    // Index some documents
    provider.index_document(Document {
        id: "1".to_string(),
        index: "products".to_string(),
        data: json!({"name": "Widget", "price": 9.99}),
    }).await.unwrap();

    provider.index_document(Document {
        id: "2".to_string(),
        index: "products".to_string(),
        data: json!({"name": "Gadget", "price": 19.99}),
    }).await.unwrap();

    // Search
    let result = provider.search(SearchQuery {
        query: "widget".to_string(),
        index: "products".to_string(),
        ..Default::default()
    }).await.unwrap();

    assert_eq!(result.total_hits, 1);
    assert_eq!(result.hits[0].id, "1");
}

#[tokio::test]
async fn test_mock_search_with_custom_results() {
    let custom_result = SearchResult {
        hits: vec![SearchHit {
            id: "custom-1".to_string(),
            score: Some(0.95),
            document: json!({"title": "Custom Result"}),
            highlights: None,
        }],
        total_hits: 100,
        processing_time_ms: 5,
        facets: None,
    };

    let provider = MockSearchProvider::new().with_results(custom_result);

    let result = provider.search(SearchQuery {
        query: "anything".to_string(),
        index: "any".to_string(),
        ..Default::default()
    }).await.unwrap();

    assert_eq!(result.total_hits, 100);
    assert_eq!(result.hits[0].id, "custom-1");
}

#[tokio::test]
async fn test_search_operations_are_recorded() {
    let provider = MockSearchProvider::new();

    provider.search(SearchQuery {
        query: "test".to_string(),
        index: "products".to_string(),
        ..Default::default()
    }).await.unwrap();

    provider.index_document(Document {
        id: "1".to_string(),
        index: "products".to_string(),
        data: json!({}),
    }).await.unwrap();

    provider.delete_document("products", "1").await.unwrap();

    let ops = provider.recorded_operations();
    assert_eq!(ops.len(), 3);
    provider.assert_searched("products", "test");
}

#[tokio::test]
async fn test_search_provider_failure() {
    let provider = MockSearchProvider::new();
    provider.fail_with("Connection refused");

    let result = provider.search(SearchQuery {
        query: "test".to_string(),
        index: "products".to_string(),
        ..Default::default()
    }).await;

    assert!(result.is_err());
}
```

### 6.2 Cache Provider Tests

```rust
// tests/cache_test.rs
use fraiseql_integrations::cache::{CacheProvider, MockCacheProvider};
use std::time::Duration;

#[tokio::test]
async fn test_mock_cache_get_set() {
    let cache = MockCacheProvider::new();

    cache.set("key1", b"value1", None).await.unwrap();

    let result = cache.get("key1").await.unwrap();
    assert_eq!(result, Some(b"value1".to_vec()));

    cache.assert_cache_hit("key1");
}

#[tokio::test]
async fn test_mock_cache_miss() {
    let cache = MockCacheProvider::new();

    let result = cache.get("nonexistent").await.unwrap();
    assert!(result.is_none());

    cache.assert_cache_miss("nonexistent");
}

#[tokio::test]
async fn test_mock_cache_with_ttl() {
    let cache = MockCacheProvider::new();

    cache.set("key1", b"value1", Some(Duration::from_secs(60))).await.unwrap();

    let ops = cache.recorded_operations();
    assert!(ops.iter().any(|op| matches!(op,
        fraiseql_integrations::cache::CacheOperation::Set { key, ttl_secs: Some(60) }
        if key == "key1"
    )));
}

#[tokio::test]
async fn test_mock_cache_delete_pattern() {
    let cache = MockCacheProvider::new()
        .with_entry("user:1:profile", b"data1")
        .with_entry("user:1:settings", b"data2")
        .with_entry("user:2:profile", b"data3");

    let deleted = cache.delete_pattern("user:1:*").await.unwrap();
    assert_eq!(deleted, 2);

    assert!(cache.get("user:1:profile").await.unwrap().is_none());
    assert!(cache.get("user:2:profile").await.unwrap().is_some());
}

#[tokio::test]
async fn test_mock_cache_incr() {
    let cache = MockCacheProvider::new();

    let val1 = cache.incr("counter", 1).await.unwrap();
    assert_eq!(val1, 1);

    let val2 = cache.incr("counter", 5).await.unwrap();
    assert_eq!(val2, 6);

    let val3 = cache.incr("counter", -3).await.unwrap();
    assert_eq!(val3, 3);
}

#[tokio::test]
async fn test_cache_json_round_trip() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct User {
        id: u64,
        name: String,
    }

    let cache = MockCacheProvider::new();

    let user = User { id: 1, name: "Alice".to_string() };

    cache.set_json("user:1", &user, None).await.unwrap();
    let retrieved: User = cache.get_json("user:1").await.unwrap().unwrap();

    assert_eq!(user, retrieved);
}
```

### 6.3 Queue Provider Tests

```rust
// tests/queue_test.rs
use fraiseql_integrations::queue::{EnqueueOptions, MockQueueProvider, QueueProvider};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn test_mock_queue_enqueue_and_query() {
    let queue = MockQueueProvider::new();

    let job_data = json!({"action": "send_email", "to": "user@example.com"});
    let job_id = queue.enqueue("emails", job_data, EnqueueOptions::default()).await.unwrap();

    assert!(!job_id.is_nil());

    let length = queue.queue_length("emails").await.unwrap();
    assert_eq!(length, 1);

    queue.assert_enqueued("emails");
}

#[tokio::test]
async fn test_mock_queue_delayed_job() {
    let queue = MockQueueProvider::new();

    let job_id = queue.enqueue_delayed(
        "scheduled",
        json!({"task": "cleanup"}),
        Duration::from_secs(3600),
        EnqueueOptions::default(),
    ).await.unwrap();

    let ops = queue.recorded_operations();
    assert!(ops.iter().any(|op| matches!(op,
        fraiseql_integrations::queue::QueueOperation::EnqueueDelayed { queue, delay_secs: 3600, .. }
        if queue == "scheduled"
    )));

    let jobs = queue.jobs_in_queue("scheduled");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].delay, Some(Duration::from_secs(3600)));
}

#[tokio::test]
async fn test_mock_queue_retry_job() {
    let queue = MockQueueProvider::new();

    let job_id = queue.enqueue("tasks", json!({"task": "process"}), EnqueueOptions::default()).await.unwrap();

    // Simulate job failure by modifying state directly
    {
        let mut queues = queue.queues.lock().unwrap();
        if let Some(jobs) = queues.get_mut("tasks") {
            jobs[0].state = "failed".to_string();
        }
    }

    let failed = queue.failed_count("tasks").await.unwrap();
    assert_eq!(failed, 1);

    let retried = queue.retry_job("tasks", job_id).await.unwrap();
    assert!(retried);

    let failed_after = queue.failed_count("tasks").await.unwrap();
    assert_eq!(failed_after, 0);
}

#[tokio::test]
async fn test_mock_queue_delete_job() {
    let queue = MockQueueProvider::new();

    let job_id = queue.enqueue("tasks", json!({}), EnqueueOptions::default()).await.unwrap();

    let deleted = queue.delete_job("tasks", job_id).await.unwrap();
    assert!(deleted);

    let length = queue.queue_length("tasks").await.unwrap();
    assert_eq!(length, 0);
}
```

### 6.4 SLO Tracking Tests

```rust
// tests/slo_test.rs
use fraiseql_integrations::slo::{SloConfig, SloRegistry, SloTracker};
use std::time::Duration;

#[test]
fn test_slo_tracker_records_success() {
    let tracker = SloTracker::new("test_operation", SloConfig::default());

    tracker.record_success(Duration::from_millis(50));
    tracker.record_success(Duration::from_millis(80));

    assert!(tracker.is_within_slo());
    assert!(tracker.error_budget_remaining() > 0.99);
}

#[test]
fn test_slo_tracker_detects_violations() {
    let config = SloConfig {
        target_latency_ms: 100,
        target_availability: 0.99, // 99% SLA
        budget_period_days: 30,
    };
    let tracker = SloTracker::new("test_operation", config);

    // 99 fast operations
    for _ in 0..99 {
        tracker.record_success(Duration::from_millis(50));
    }

    // 1 slow operation (1% error rate - at budget limit)
    tracker.record_success(Duration::from_millis(150));

    // Should be at or near 0% budget remaining
    let budget = tracker.error_budget_remaining();
    assert!(budget >= 0.0 && budget <= 0.1, "Budget should be near 0, got: {}", budget);
}

#[test]
fn test_slo_tracker_errors_count_as_violations() {
    let tracker = SloTracker::new("test_operation", SloConfig::default());

    tracker.record_error();
    tracker.record_success(Duration::from_millis(50));
    tracker.record_success(Duration::from_millis(50));

    // 1 out of 3 = 33% error rate
    let budget = tracker.error_budget_remaining();
    assert!(budget < 1.0, "Budget should be reduced after errors");
}

#[test]
fn test_slo_registry_tracks_multiple_operations() {
    let registry = SloRegistry::new();

    // Record some operations
    if let Some(tracker) = registry.get("search_query") {
        tracker.record_success(Duration::from_millis(100));
        tracker.record_success(Duration::from_millis(150));
    }

    if let Some(tracker) = registry.get("cache_get") {
        tracker.record_success(Duration::from_millis(5));
        tracker.record_success(Duration::from_millis(8));
    }

    let status = registry.overall_status();
    assert!(status.within_slo);
    assert!(status.average_budget_remaining > 0.9);
}

#[test]
fn test_slo_presets() {
    assert_eq!(SloConfig::search().target_latency_ms, 200);
    assert_eq!(SloConfig::cache_read().target_latency_ms, 10);
    assert_eq!(SloConfig::cache_write().target_latency_ms, 20);
    assert_eq!(SloConfig::queue_enqueue().target_latency_ms, 50);
}
```

### 6.5 Error Handling Tests

```rust
// tests/error_test.rs
use fraiseql_integrations::error::{IntegrationError, IntegrationErrorCode};
use axum::response::IntoResponse;
use axum::http::StatusCode;

#[tokio::test]
async fn test_error_code_to_http_status_mapping() {
    // Not found -> 404
    let error = IntegrationError::index_not_found("products");
    assert_eq!(error.into_response().status(), StatusCode::NOT_FOUND);

    // Configuration -> 500
    let error = IntegrationError::Configuration {
        code: IntegrationErrorCode::MissingEnvVar,
        message: "Missing REDIS_URL".to_string(),
    };
    assert_eq!(error.into_response().status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Provider error -> 502
    let error = IntegrationError::Provider {
        code: IntegrationErrorCode::MeilisearchError,
        provider: "meilisearch".to_string(),
        message: "Connection refused".to_string(),
    };
    assert_eq!(error.into_response().status(), StatusCode::BAD_GATEWAY);

    // Circuit open -> 503
    let error = IntegrationError::CircuitOpen {
        provider: "redis".to_string(),
    };
    assert_eq!(error.into_response().status(), StatusCode::SERVICE_UNAVAILABLE);

    // Invalid input -> 400
    let error = IntegrationError::duplicate_job("unique-key-123");
    assert_eq!(error.into_response().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_transient_error_classification() {
    assert!(IntegrationErrorCode::SearchTimeout.is_transient());
    assert!(IntegrationErrorCode::CacheConnectionFailed.is_transient());
    assert!(IntegrationErrorCode::ConnectionTimeout.is_transient());
    assert!(IntegrationErrorCode::CircuitBreakerOpen.is_transient());

    assert!(!IntegrationErrorCode::IndexNotFound.is_transient());
    assert!(!IntegrationErrorCode::DuplicateJob.is_transient());
    assert!(!IntegrationErrorCode::InvalidSearchQuery.is_transient());
}

#[test]
fn test_error_docs_url() {
    assert_eq!(
        IntegrationErrorCode::IndexNotFound.docs_url(),
        "https://fraiseql.dev/docs/errors/IN100"
    );
    assert_eq!(
        IntegrationErrorCode::CircuitBreakerOpen.docs_url(),
        "https://fraiseql.dev/docs/errors/IN503"
    );
}
```

### 6.6 Integration Service Tests

```rust
// tests/service_test.rs
use fraiseql_integrations::{
    service::IntegrationService,
    search::MockSearchProvider,
    cache::MockCacheProvider,
    queue::MockQueueProvider,
};
use std::sync::Arc;

#[tokio::test]
async fn test_integration_service_with_all_mocks() {
    let search = Arc::new(MockSearchProvider::new());
    let cache = Arc::new(MockCacheProvider::new());
    let queue = Arc::new(MockQueueProvider::new());

    let service = IntegrationService::new(
        Some(search.clone()),
        Some(cache.clone()),
        Some(queue.clone()),
    );

    // Test search
    service.search("products", "test query").await.unwrap();
    search.assert_searched("products", "test query");

    // Test cache
    service.cache_set("key", b"value", None).await.unwrap();
    let cached = service.cache_get("key").await.unwrap();
    assert_eq!(cached, Some(b"value".to_vec()));

    // Test queue
    service.enqueue("tasks", serde_json::json!({"task": "test"})).await.unwrap();
    queue.assert_enqueued("tasks");
}

#[tokio::test]
async fn test_health_check_with_all_providers() {
    let search = Arc::new(MockSearchProvider::new());
    let cache = Arc::new(MockCacheProvider::new());
    let queue = Arc::new(MockQueueProvider::new());

    let service = IntegrationService::new(
        Some(search),
        Some(cache),
        Some(queue),
    );

    let health = service.health_check().await;

    assert!(health.contains_key("search:mock"));
    assert!(health.contains_key("cache:mock"));
    assert!(health.contains_key("queue:mock"));
    assert!(health.values().all(|&v| v));
}

#[tokio::test]
async fn test_health_check_with_failing_provider() {
    let search = Arc::new(MockSearchProvider::new());
    let cache = Arc::new(MockCacheProvider::new());
    let queue = Arc::new(MockQueueProvider::new());

    // Make cache fail
    *cache.should_fail.lock().unwrap() = true;

    let service = IntegrationService::new(
        Some(search),
        Some(cache),
        Some(queue),
    );

    let health = service.health_check().await;

    assert_eq!(health.get("cache:mock"), Some(&false));
    assert_eq!(health.get("search:mock"), Some(&true));
    assert_eq!(health.get("queue:mock"), Some(&true));
}
```

## Verification Commands

```bash
# Build the crate
cargo build -p fraiseql-integrations

# Run tests
cargo nextest run -p fraiseql-integrations

# Lint
cargo clippy -p fraiseql-integrations -- -D warnings

# Integration tests (requires services)
MEILISEARCH_HOST=http://localhost:7700 \
MEILISEARCH_API_KEY=xxx \
REDIS_URL=redis://localhost:6379 \
cargo nextest run -p fraiseql-integrations --features integration
```

---

## Acceptance Criteria

- [ ] Search providers: Meilisearch, Typesense, PostgreSQL FTS all work
- [ ] [PLACEHOLDER] Search: Algolia provider - needs `algolia` crate evaluation
- [ ] Cache providers: Redis, in-memory, PostgreSQL all work
- [ ] Queue providers: Redis, PostgreSQL all work
- [ ] [PLACEHOLDER] Queue: RabbitMQ provider - needs `lapin` crate integration
- [ ] Search supports: query, filters, facets, pagination, highlighting
- [ ] Cache supports: get/set with TTL, pattern deletion, atomic increment
- [ ] Queue supports: enqueue, delayed jobs, retry, priority
- [ ] All providers have circuit breakers for external calls
- [ ] All providers have health checks
- [ ] Proper error handling for all failure modes

---

## DO NOT

- Block worker threads with synchronous I/O
- Lose jobs on crash (ensure at-least-once delivery)
- Cache without TTL (prevent memory leaks)
- Ignore queue priority (process high-priority first)
- Skip deduplication checks (prevent duplicate processing)
