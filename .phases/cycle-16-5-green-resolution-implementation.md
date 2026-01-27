# Cycle 16-5: GREEN Phase - Resolution Strategies Implementation

**Cycle**: 5 of 8
**Phase**: GREEN (Implement minimal code to pass tests)
**Duration**: ~4-5 days
**Focus**: Direct DB resolution, HTTP fallback, connection pooling, batching

---

## Objective

Implement resolution strategies:
1. Direct database federation (same and cross-database)
2. HTTP fallback with retry logic
3. Connection pooling and management
4. Batch entity resolution across databases

---

## Implementation Plan

### Part 1: Direct Database Resolver

**File**: `crates/fraiseql-core/src/federation/resolution/direct_db.rs`

```rust
use crate::db::DatabaseAdapter;
use crate::federation::types::EntityRepresentation;
use async_trait::async_trait;
use std::sync::Arc;

pub struct DirectDatabaseResolver {
    remote_adapter: Arc<dyn DatabaseAdapter>,
    remote_db_type: String,
}

impl DirectDatabaseResolver {
    pub fn new(
        remote_adapter: Arc<dyn DatabaseAdapter>,
        remote_db_type: String,
    ) -> Self {
        Self {
            remote_adapter,
            remote_db_type,
        }
    }

    async fn resolve_local_batch(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        view_name: &str,
        key_columns: &[String],
    ) -> Result<Vec<serde_json::Value>, String> {
        if representations.is_empty() {
            return Ok(Vec::new());
        }

        // Build WHERE clause
        let mut conditions = Vec::new();
        for key_col in key_columns {
            let values: Vec<String> = representations
                .iter()
                .filter_map(|rep| rep.key_fields.get(key_col))
                .map(|v| format!("'{}'", v))
                .collect();

            if !values.is_empty() {
                conditions.push(format!("{} IN ({})", key_col, values.join(", ")));
            }
        }

        let where_clause = if !conditions.is_empty() {
            format!("WHERE {}", conditions.join(" AND "))
        } else {
            String::new()
        };

        let query = format!("SELECT * FROM {} {}", view_name, where_clause);

        // Execute on remote database
        self.remote_adapter.execute_raw_query(&query).await
            .map_err(|e| format!("Remote DB error: {}", e))
    }
}

#[async_trait]
impl EntityResolver for DirectDatabaseResolver {
    async fn resolve(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<serde_json::Value>, String> {
        // Determine view name from typename
        let view_name = format!("{}_federation_view", typename);

        // Get key columns from representations
        let key_columns: Vec<String> = representations[0]
            .key_fields
            .keys()
            .cloned()
            .collect();

        self.resolve_local_batch(
            typename,
            representations,
            &view_name,
            &key_columns,
        ).await
    }
}
```

### Part 2: HTTP Resolver with Retry Logic

**File**: `crates/fraiseql-core/src/federation/resolution/http.rs`

```rust
use crate::federation::types::EntityRepresentation;
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

pub struct HttpEntityResolver {
    http_client: Client,
    subgraph_url: String,
    max_retries: u32,
}

impl HttpEntityResolver {
    pub fn new(subgraph_url: String, max_retries: u32) -> Self {
        Self {
            http_client: Client::new(),
            subgraph_url,
            max_retries,
        }
    }

    async fn send_entities_query(
        &self,
        representations: &[EntityRepresentation],
    ) -> Result<serde_json::Value, String> {
        let query = serde_json::json!({
            "query": "query($representations: [_Any!]!) { _entities(representations: $representations) { ... } }",
            "variables": {
                "representations": representations.iter().map(|r| &r.all_fields).collect::<Vec<_>>()
            }
        });

        let response = self.http_client
            .post(&self.subgraph_url)
            .json(&query)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("HTTP error: {}", status));
        }

        response.json().await
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    async fn resolve_with_retry(
        &self,
        representations: &[EntityRepresentation],
    ) -> Result<serde_json::Value, String> {
        let mut last_error = String::new();

        for attempt in 0..=self.max_retries {
            match self.send_entities_query(representations).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = e;
                    if attempt < self.max_retries {
                        // Exponential backoff: 100ms, 200ms, 400ms, ...
                        let backoff = Duration::from_millis(100 * 2_u64.pow(attempt));
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        Err(format!("HTTP resolution failed after {} retries: {}", self.max_retries, last_error))
    }
}

#[async_trait]
impl EntityResolver for HttpEntityResolver {
    async fn resolve(
        &self,
        _typename: &str,
        representations: &[EntityRepresentation],
        _selection: &FieldSelection,
    ) -> Result<Vec<serde_json::Value>, String> {
        let result = self.resolve_with_retry(representations).await?;

        // Extract entities from response
        result.get("data")
            .and_then(|d| d.get("_entities"))
            .and_then(|e| e.as_array())
            .cloned()
            .ok_or_else(|| "Invalid _entities response".to_string())
    }
}
```

### Part 3: Connection Manager

**File**: `crates/fraiseql-core/src/federation/resolution/connection_manager.rs`

```rust
use crate::db::DatabaseAdapter;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ConnectionManager {
    local_adapter: Arc<dyn DatabaseAdapter>,
    remote_adapters: Arc<RwLock<HashMap<String, Arc<dyn DatabaseAdapter>>>>,
    http_clients: Arc<RwLock<HashMap<String, HttpEntityResolver>>>,
}

impl ConnectionManager {
    pub fn new(local_adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self {
            local_adapter,
            remote_adapters: Arc::new(RwLock::new(HashMap::new())),
            http_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_remote_database(
        &self,
        name: String,
        adapter: Arc<dyn DatabaseAdapter>,
    ) {
        let mut remotes = self.remote_adapters.write().await;
        remotes.insert(name, adapter);
    }

    pub async fn register_http_subgraph(
        &self,
        name: String,
        url: String,
        max_retries: u32,
    ) {
        let mut clients = self.http_clients.write().await;
        let resolver = HttpEntityResolver::new(url, max_retries);
        clients.insert(name, resolver);
    }

    pub async fn get_remote_adapter(
        &self,
        name: &str,
    ) -> Option<Arc<dyn DatabaseAdapter>> {
        let remotes = self.remote_adapters.read().await;
        remotes.get(name).cloned()
    }

    pub async fn get_http_resolver(
        &self,
        name: &str,
    ) -> Option<Arc<HttpEntityResolver>> {
        let clients = self.http_clients.read().await;
        clients.get(name).map(|r| Arc::new(r.clone()))
    }
}
```

### Part 4: Batching Orchestrator

**File**: `crates/fraiseql-core/src/federation/resolution/batch_orchestrator.rs`

```rust
use crate::federation::types::EntityRepresentation;
use std::collections::HashMap;

pub struct BatchOrchestrator;

#[derive(Debug, Clone)]
pub struct Batch {
    pub typename: String,
    pub strategy: ResolutionStrategy,
    pub representations: Vec<EntityRepresentation>,
}

impl BatchOrchestrator {
    pub fn create_batches(
        representations: &[EntityRepresentation],
        strategy_cache: &HashMap<String, ResolutionStrategy>,
    ) -> Vec<Batch> {
        let mut batches: HashMap<(String, String), Vec<EntityRepresentation>> = HashMap::new();

        for rep in representations {
            // Determine strategy
            let strategy = strategy_cache
                .get(&rep.typename)
                .cloned()
                .unwrap_or(ResolutionStrategy::Http); // Default

            let key = (rep.typename.clone(), format!("{:?}", strategy));
            batches.entry(key).or_insert_with(Vec::new).push(rep.clone());
        }

        batches
            .into_iter()
            .map(|((typename, _), reps)| Batch {
                typename,
                strategy: strategy_cache
                    .get(&typename)
                    .cloned()
                    .unwrap_or(ResolutionStrategy::Http),
                representations: reps,
            })
            .collect()
    }

    pub async fn execute_batches_parallel(
        batches: Vec<Batch>,
        connection_mgr: &ConnectionManager,
    ) -> Result<Vec<serde_json::Value>, String> {
        let mut handles = vec![];

        for batch in batches {
            let conn_mgr = connection_mgr.clone();
            let handle = tokio::spawn(async move {
                // Execute batch based on strategy
                match batch.strategy {
                    ResolutionStrategy::Local => {
                        // Local resolution
                    }
                    ResolutionStrategy::DirectDatabase { .. } => {
                        // Direct DB resolution
                    }
                    ResolutionStrategy::Http { url } => {
                        // HTTP resolution
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all batches
        let mut results = vec![];
        for handle in handles {
            let result = handle.await.map_err(|e| format!("Batch failed: {}", e))?;
            results.extend(result);
        }

        Ok(results)
    }
}
```

### Part 5: Integrated Resolution Orchestrator

**File**: `crates/fraiseql-core/src/federation/resolution/mod.rs`

```rust
pub mod trait_def;
pub mod local;
pub mod direct_db;
pub mod http;
pub mod connection_manager;
pub mod batch_orchestrator;

pub use trait_def::*;
pub use local::*;
pub use direct_db::*;
pub use http::*;
pub use connection_manager::*;
pub use batch_orchestrator::*;

pub struct UnifiedEntityResolver {
    connection_manager: Arc<ConnectionManager>,
    strategy_cache: Arc<RwLock<HashMap<String, ResolutionStrategy>>>,
}

impl UnifiedEntityResolver {
    pub async fn resolve(
        &self,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<serde_json::Value>, String> {
        // Create batches
        let batches = BatchOrchestrator::create_batches(
            representations,
            &self.strategy_cache.read().await,
        );

        // Execute batches in parallel
        let mut all_results = vec![];
        for batch in batches {
            let resolver = self.get_resolver_for_batch(&batch).await?;
            let results = resolver.resolve(
                &batch.typename,
                &batch.representations,
                selection,
            ).await?;
            all_results.extend(results);
        }

        Ok(all_results)
    }

    async fn get_resolver_for_batch(
        &self,
        batch: &Batch,
    ) -> Result<Box<dyn EntityResolver>, String> {
        match &batch.strategy {
            ResolutionStrategy::Local { view_name, key_columns } => {
                Ok(Box::new(LocalResolver::new(
                    self.connection_manager.get_local_adapter(),
                    view_name.clone(),
                    key_columns.clone(),
                )))
            }
            ResolutionStrategy::DirectDatabase { connection_pool, .. } => {
                let adapter = self.connection_manager
                    .get_remote_adapter("remote_db")
                    .await
                    .ok_or_else(|| "Remote adapter not configured".to_string())?;

                Ok(Box::new(DirectDatabaseResolver::new(adapter, "postgresql".to_string())))
            }
            ResolutionStrategy::Http { subgraph_url } => {
                Ok(Box::new(HttpEntityResolver::new(subgraph_url.clone(), 3)))
            }
        }
    }
}
```

---

## Compilation & Testing

```bash
# Check compilation
cargo check -p fraiseql-core

# Run resolution tests
cargo test --test federation test_direct_db_resolution
cargo test --test federation test_http_resolution
cargo test --test federation test_connection_management
cargo test --test federation test_batching

# Run benchmarks
cargo bench --bench federation_resolution_benchmarks

# Expected: All tests pass, latency targets met
```

---

## Implementation Checklist

- [ ] Direct DB resolver implemented
- [ ] HTTP resolver with retry logic implemented
- [ ] Connection manager implemented
- [ ] Batch orchestrator implemented
- [ ] All direct DB tests pass
- [ ] All HTTP tests pass
- [ ] All connection tests pass
- [ ] All batching tests pass
- [ ] Performance targets met
- [ ] No clippy warnings

---

**Status**: [~] In Progress (Implementing)
**Next**: REFACTOR Phase - Optimize and improve design
