<!-- Skip to main content -->
---

title: Saga API Reference
description: Complete API documentation for FraiseQL saga orchestration.
keywords: ["directives", "types", "scalars", "schema", "api"]
tags: ["documentation", "reference"]
---

# Saga API Reference

Complete API documentation for FraiseQL saga orchestration.

---

## Table of Contents

1. [SagaCoordinator](#sagacoordinator)
2. [SagaStep](#sagastep)
3. [SagaStore](#sagastore)
4. [RecoveryManager](#recoverymanager)
5. [Data Types](#data-types)
6. [Error Types](#error-types)

---

## SagaCoordinator

The central orchestrator for saga execution.

### `new(metadata: FederationMetadata, store: Arc<dyn SagaStore>) -> Self`

Create a new saga coordinator.

```rust
<!-- Code example in RUST -->
let coordinator = SagaCoordinator::new(metadata, store);
```text
<!-- Code example in TEXT -->

**Parameters:**

- `metadata`: Federation metadata with type and directive information
- `store`: Saga persistence store (PostgreSQL, MySQL, SQLite)

**Returns:** SagaCoordinator instance

---

### `execute(steps: Vec<SagaStep>) -> Result<SagaResult>`

Execute a saga synchronously (waits for completion).

```rust
<!-- Code example in RUST -->
let result = coordinator.execute(steps).await?;
println!("Status: {:?}", result.status);
```text
<!-- Code example in TEXT -->

**Parameters:**

- `steps`: Vector of saga steps to execute in order

**Returns:** `Result<SagaResult>` with execution results

**Errors:**

- `SagaError::StepFailed` - A step returned an error
- `SagaError::CompensationFailed` - Compensation step failed
- `SagaError::Timeout` - Saga exceeded timeout duration
- `SagaError::StoreError` - Cannot persist saga state

---

### `execute_async(steps: Vec<SagaStep>) -> Result<String>`

Execute a saga asynchronously (returns immediately with saga ID).

```rust
<!-- Code example in RUST -->
let saga_id = coordinator.execute_async(steps).await?;
// Saga runs in background
```text
<!-- Code example in TEXT -->

**Parameters:**

- `steps`: Vector of saga steps to execute

**Returns:** `Result<String>` containing saga ID for status tracking

---

### `execute_parallel(steps: Vec<SagaStep>, config: ParallelConfig) -> Result<SagaResult>`

Execute independent steps in parallel.

```rust
<!-- Code example in RUST -->
let result = coordinator.execute_parallel(
    steps,
    ParallelConfig {
        max_concurrent: 4,
        fail_fast: true,
    }
).await?;
```text
<!-- Code example in TEXT -->

**Parameters:**

- `steps`: Vector of independent saga steps
- `config`: Parallel execution configuration

**Returns:** `Result<SagaResult>`

---

### `with_timeout(duration: Duration) -> Self`

Set timeout for entire saga.

```rust
<!-- Code example in RUST -->
let coordinator = coordinator.with_timeout(Duration::from_secs(300));
```text
<!-- Code example in TEXT -->

**Default:** 5 minutes

---

### `with_step_timeout(duration: Duration) -> Self`

Set timeout for individual steps.

```rust
<!-- Code example in RUST -->
let coordinator = coordinator.with_step_timeout(Duration::from_secs(30));
```text
<!-- Code example in TEXT -->

**Default:** 30 seconds

---

### `with_max_retries(count: u32) -> Self`

Set maximum retry attempts for failed steps.

```rust
<!-- Code example in RUST -->
let coordinator = coordinator.with_max_retries(3);
```text
<!-- Code example in TEXT -->

**Default:** 3 retries

---

### `with_trace_id(id: &str) -> Self`

Set distributed trace ID for observability.

```rust
<!-- Code example in RUST -->
let coordinator = coordinator.with_trace_id("trace-abc-123");
```text
<!-- Code example in TEXT -->

---

### `get_saga(saga_id: &str) -> Result<Option<SagaState>>`

Retrieve saga state by ID.

```rust
<!-- Code example in RUST -->
if let Some(saga) = coordinator.get_saga("saga-123").await? {
    println!("Status: {:?}", saga.status);
}
```text
<!-- Code example in TEXT -->

**Parameters:**

- `saga_id`: Unique saga identifier

**Returns:** `Result<Option<SagaState>>`

---

### `recover_failed_sagas() -> Result<Vec<String>>`

Manually trigger recovery of all failed sagas.

```rust
<!-- Code example in RUST -->
let recovered = coordinator.recover_failed_sagas().await?;
println!("Recovered {} sagas", recovered.len());
```text
<!-- Code example in TEXT -->

**Returns:** `Result<Vec<String>>` with IDs of recovered sagas

---

## SagaStep

A single step in a saga.

### Structure

```rust
<!-- Code example in RUST -->
pub struct SagaStep {
    pub name: String,
    pub forward: Mutation,
    pub compensation: Option<Mutation>,
}
```text
<!-- Code example in TEXT -->

### `new(name: String, forward: Mutation) -> Self`

Create a saga step without compensation.

```rust
<!-- Code example in RUST -->
let step = SagaStep::new(
    "charge_payment".to_string(),
    Mutation { /* ... */ }
);
```text
<!-- Code example in TEXT -->

---

### `with_compensation(compensation: Mutation) -> Self`

Add compensation to a step.

```rust
<!-- Code example in RUST -->
let step = step.with_compensation(Mutation { /* ... */ });
```text
<!-- Code example in TEXT -->

---

## SagaStore

Persistence layer for saga state.

### `execute_raw_query(sql: &str) -> Result<Vec<HashMap<String, Value>>>`

Execute raw SQL query (for testing/debugging).

```rust
<!-- Code example in RUST -->
let rows = store.execute_raw_query(
    "SELECT * FROM sagas WHERE status = 'PENDING'"
).await?;
```text
<!-- Code example in TEXT -->

---

### `create_saga(saga: SagaState) -> Result<String>`

Create new saga in store.

**Returns:** Saga ID

---

### `update_saga(saga: SagaState) -> Result<()>`

Update saga state in store.

---

### `get_saga(saga_id: &str) -> Result<Option<SagaState>>`

Retrieve saga by ID.

---

### `get_pending_sagas() -> Result<Vec<SagaState>>`

Get all pending sagas (useful for recovery).

```rust
<!-- Code example in RUST -->
let pending = store.get_pending_sagas().await?;
for saga in pending {
    println!("Pending: {}", saga.id);
}
```text
<!-- Code example in TEXT -->

---

### `count_sagas_by_status(status: SagaStatus) -> Result<u64>`

Count sagas in given status.

```rust
<!-- Code example in RUST -->
let failed = store.count_sagas_by_status(SagaStatus::Failed).await?;
```text
<!-- Code example in TEXT -->

---

### `get_stuck_sagas(duration: Duration) -> Result<Vec<SagaState>>`

Get sagas stuck for longer than specified duration.

```rust
<!-- Code example in RUST -->
let stuck = store.get_stuck_sagas(Duration::from_secs(3600)).await?;
```text
<!-- Code example in TEXT -->

---

## RecoveryManager

Handles automatic saga recovery.

### `new(store: Arc<dyn SagaStore>) -> Self`

Create recovery manager.

```rust
<!-- Code example in RUST -->
let manager = RecoveryManager::new(store);
```text
<!-- Code example in TEXT -->

---

### `with_max_retries(count: u32) -> Self`

Set max retries for recovery attempts.

```rust
<!-- Code example in RUST -->
let manager = manager.with_max_retries(3);
```text
<!-- Code example in TEXT -->

**Default:** 3

---

### `with_retry_delay(duration: Duration) -> Self`

Set delay between retry attempts.

```rust
<!-- Code example in RUST -->
let manager = manager.with_retry_delay(Duration::from_secs(5));
```text
<!-- Code example in TEXT -->

**Default:** 5 seconds

---

### `with_exponential_backoff(multiplier: f64) -> Self`

Enable exponential backoff for retries.

```rust
<!-- Code example in RUST -->
// Delays: 5s, 10s, 20s, 40s, ...
let manager = manager.with_exponential_backoff(2.0);
```text
<!-- Code example in TEXT -->

**Default:** No backoff (fixed delays)

---

### `with_crash_recovery() -> Self`

Enable automatic recovery on system startup.

```rust
<!-- Code example in RUST -->
let manager = manager.with_crash_recovery();
```text
<!-- Code example in TEXT -->

---

### `start_recovery_loop() -> Result<()>`

Start background recovery loop.

```rust
<!-- Code example in RUST -->
manager.start_recovery_loop().await?;
// Recovery runs continuously in background
```text
<!-- Code example in TEXT -->

---

### `recover_saga(saga: &SagaState) -> Result<SagaState>`

Manually recover a specific saga.

```rust
<!-- Code example in RUST -->
let recovered_saga = manager.recover_saga(&failed_saga).await?;
```text
<!-- Code example in TEXT -->

---

### `is_running() -> bool`

Check if recovery loop is running.

```rust
<!-- Code example in RUST -->
if manager.is_running() {
    println!("Recovery manager active");
}
```text
<!-- Code example in TEXT -->

---

## Data Types

### `SagaStatus`

```rust
<!-- Code example in RUST -->
pub enum SagaStatus {
    Pending,          // Not yet started
    Executing,        // In progress
    Completed,        // All steps succeeded
    Compensating,     // Undoing failed steps
    RolledBack,       // Compensation succeeded
    Failed,           // Compensation failed (manual intervention needed)
    Recovering,       // Being recovered from failure
}
```text
<!-- Code example in TEXT -->

### `StepStatus`

```rust
<!-- Code example in RUST -->
pub enum StepStatus {
    Pending,          // Not yet started
    Executing,        // Currently running
    Succeeded,        // Completed successfully
    Failed,           // Returned error
    Retrying,         // Being retried
    Compensating,     // Compensation running
    CompensationFailed, // Compensation failed
}
```text
<!-- Code example in TEXT -->

### `SagaResult`

```rust
<!-- Code example in RUST -->
pub struct SagaResult {
    pub saga_id: String,
    pub status: SagaStatus,
    pub completed_steps: u32,
    pub data: Value,          // Result data from last step
    pub duration_ms: u64,
    pub error: Option<String>,
}
```text
<!-- Code example in TEXT -->

### `SagaState`

```rust
<!-- Code example in RUST -->
pub struct SagaState {
    pub id: String,
    pub saga_type: String,
    pub status: SagaStatus,
    pub steps: Vec<StepState>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub data: Value,
    pub error: Option<String>,
}
```text
<!-- Code example in TEXT -->

### `StepState`

```rust
<!-- Code example in RUST -->
pub struct StepState {
    pub index: usize,
    pub name: String,
    pub status: StepStatus,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub attempts: u32,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
```text
<!-- Code example in TEXT -->

### `Mutation`

```rust
<!-- Code example in RUST -->
pub struct Mutation {
    pub subgraph: String,         // Which subgraph to target
    pub operation: String,        // GraphQL mutation name
    pub variables: Value,         // JSON variables
    pub request_id: Option<String>, // For idempotency
}
```text
<!-- Code example in TEXT -->

### `ParallelConfig`

```rust
<!-- Code example in RUST -->
pub struct ParallelConfig {
    pub max_concurrent: usize,    // How many steps in parallel
    pub fail_fast: bool,          // Stop on first failure?
}
```text
<!-- Code example in TEXT -->

---

## Error Types

### `SagaError`

```rust
<!-- Code example in RUST -->
pub enum SagaError {
    StepFailed {
        step_index: usize,
        step_name: String,
        error: String,
    },
    CompensationFailed {
        step_index: usize,
        step_name: String,
        error: String,
    },
    Timeout {
        saga_id: String,
        duration_ms: u64,
    },
    StoreError {
        message: String,
    },
    ValidationError {
        message: String,
    },
    RecoveryFailed {
        saga_id: String,
        reason: String,
    },
}
```text
<!-- Code example in TEXT -->

### Common Error Handling

```rust
<!-- Code example in RUST -->
match coordinator.execute(steps).await {
    Ok(result) => {
        println!("Saga succeeded: {:?}", result.data);
    },
    Err(SagaError::StepFailed { step_index, error, .. }) => {
        eprintln!("Step {} failed: {}", step_index, error);
        // Compensation already ran
    },
    Err(SagaError::Timeout { duration_ms, .. }) => {
        eprintln!("Saga timed out after {}ms", duration_ms);
    },
    Err(e) => {
        eprintln!("Saga error: {:?}", e);
    },
}
```text
<!-- Code example in TEXT -->

---

## Configuration

### Environment Variables

```bash
<!-- Code example in BASH -->
FRAISEQL_SAGA_ENABLED=true
FRAISEQL_SAGA_STORE_TYPE=postgres
FRAISEQL_SAGA_MAX_RETRIES=3
FRAISEQL_SAGA_STEP_TIMEOUT_SECONDS=30
FRAISEQL_SAGA_TIMEOUT_SECONDS=300
FRAISEQL_SAGA_RECOVERY_ENABLED=true
FRAISEQL_SAGA_RECOVERY_POLL_INTERVAL_SECONDS=60

# Store-specific
FRAISEQL_SAGA_STORE_CONNECTION_STRING=postgres://...
FRAISEQL_SAGA_STORE_MAX_POOL_SIZE=20
```text
<!-- Code example in TEXT -->

### TOML Configuration

```toml
<!-- Code example in TOML -->
[saga]
enabled = true
store_type = "postgres"
max_retries = 3
step_timeout_seconds = 30
saga_timeout_seconds = 300
recovery_enabled = true
recovery_poll_interval_seconds = 60

[saga.store.postgres]
connection_string = "postgres://user:pass@localhost/FraiseQL"
max_pool_size = 20
```text
<!-- Code example in TEXT -->

---

## Examples

### Complete Saga Example

```rust
<!-- Code example in RUST -->
async fn example_saga() -> Result<()> {
    // Setup
    let store = PostgresSagaStore::new(config).await?;
    let coordinator = SagaCoordinator::new(metadata, Arc::new(store))
        .with_timeout(Duration::from_secs(300))
        .with_max_retries(3);

    // Define steps
    let steps = vec![
        SagaStep {
            name: "step1".to_string(),
            forward: Mutation {
                subgraph: "service1".to_string(),
                operation: "doSomething".to_string(),
                variables: json!({"input": "value"}),
                request_id: Some("req-123".to_string()),
            },
            compensation: Some(Mutation {
                subgraph: "service1".to_string(),
                operation: "undoSomething".to_string(),
                variables: json!({"id": "{result_id}"}),
                request_id: Some("comp-123".to_string()),
            }),
        },
    ];

    // Execute
    match coordinator.execute(steps).await {
        Ok(result) => {
            println!("Success! Data: {:?}", result.data);
            Ok(())
        },
        Err(e) => {
            eprintln!("Failed: {:?}", e);
            Err(e.into())
        },
    }
}
```text
<!-- Code example in TEXT -->

---

**Last Updated:** 2026-01-29
**Version:** 2.0
**Maintainer:** FraiseQL Federation Team
