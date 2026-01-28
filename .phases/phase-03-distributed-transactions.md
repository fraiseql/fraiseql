# Phase 3: Distributed Transactions (Saga Pattern)

**Duration**: 8 weeks (Weeks 12-19)
**Objective**: Implement saga coordinator with state persistence, compensation logic, and recovery
**Test Target**: 165+ tests
**Status**: [~] In Progress

---

## Executive Summary

Phase 3 implements **distributed transaction support** via the saga pattern, enabling multi-subgraph mutations with ACID-like guarantees and automatic recovery from failures. This is critical for enterprise federation scenarios where mutations span multiple backend systems.

### Key Deliverables
- ✅ Saga coordinator with persistent state (PostgreSQL)
- ✅ Compensation logic (forward recovery + backward compensation)
- ✅ Recovery manager with background crash recovery
- ✅ API integration (@transaction directive)
- ✅ Observability (metrics, tracing, dashboards)
- ✅ 165+ tests covering all scenarios

### Success Criteria
- All sagas complete successfully (>99% success rate)
- Automatic recovery from subgraph failures
- <100ms overhead for 3-step saga
- Crash recovery validates consistency
- Production-ready error handling

---

## Phase Dependencies

**Requires**: Phase 1 ✅ + Phase 2 ✅
- Entity resolution working
- Schema validation in place
- Mutation execution foundation

**Blocks**: Phase 4 (Apollo Router Integration)
- Phase 4 tests sagas through Apollo Router

---

## Architecture Overview

```
┌──────────────────────────────────────┐
│  GraphQL Mutation (@transaction)     │
└──────────────┬───────────────────────┘
               │
┌──────────────▼───────────────────────┐
│  Saga Coordinator                    │
│  - Parse mutation into steps         │
│  - Build compensation chain          │
│  - Orchestrate execution             │
└──────────────┬───────────────────────┘
               │
      ┌────────┴──────────┐
      ▼                   ▼
┌──────────────┐  ┌──────────────────┐
│ Saga Store   │  │ Saga Executor    │
│ PostgreSQL   │  │ - Step 1: users  │
│ - saga_id    │  │ - Step 2: orders │
│ - state      │  │ - Step 3: products
│ - steps      │  │                  │
│ - result     │  └──────────────────┘
└──────────────┘         │
                         │
        ┌────────────────┴────────────────┐
        ▼                                 ▼
   ┌─────────┐                      ┌──────────┐
   │ Success │                      │ Failure  │
   │ Record  │                      │ Trigger  │
   │ result  │                      │ Compensation
   └─────────┘                      └──────────┘
        │                                │
        └────────────────┬───────────────┘
                         ▼
                  ┌──────────────┐
                  │Recovery Loop │
                  │Every 5 min   │
                  │ - Find stuck │
                  │ - Retry OR   │
                  │ - Compensate│
                  └──────────────┘
```

---

## Week-by-Week Breakdown

### Week 12: Foundation & Saga Coordinator
**Objective**: Saga coordinator core + dependency graph

#### Cycle 1: Saga Coordinator Foundation (RED → GREEN → REFACTOR → CLEANUP)

**RED Phase** (Days 1-2):
```rust
#[test]
fn test_saga_coordinator_creation() {
    let coordinator = SagaCoordinator::new();
    assert!(coordinator.is_ok());
}

#[test]
fn test_saga_parse_steps() {
    let mutation = r#"
        mutation CreateOrderWithInventory {
            createOrder(...) { id, total }
        }
    "#;
    let saga = coordinator.parse_mutation(mutation)?;
    assert_eq!(saga.steps.len(), 3); // users, orders, products
}

#[test]
fn test_saga_execution_success() {
    let saga = create_test_saga();
    let result = coordinator.execute_saga(saga)?;
    assert_eq!(result.state, SagaState::Completed);
}

#[test]
fn test_saga_execution_failure_triggers_compensation() {
    let saga = create_test_saga_with_failure_on_step_2();
    let result = coordinator.execute_saga(saga)?;
    assert_eq!(result.state, SagaState::Compensated);
}
```

**Test Count**: 25+ tests
- Saga creation and parsing
- Step execution ordering
- Success path validation
- Failure detection and handling
- Compensation triggering

**GREEN Phase** (Days 3-4):
```rust
pub struct SagaCoordinator {
    store: Arc<dyn SagaStore>,
    execution_engine: Arc<SagaExecutionEngine>,
}

pub struct Saga {
    pub id: Uuid,
    pub state: SagaState,
    pub steps: Vec<SagaStep>,
    pub compensation_chain: Vec<CompensationAction>,
}

pub enum SagaState {
    Pending,
    Executing,
    Completed,
    Failed,
    Compensating,
    Compensated,
}

impl SagaCoordinator {
    pub fn execute_saga(&self, saga: Saga) -> Result<SagaResult> {
        self.store.save_saga(&saga)?;

        for (index, step) in saga.steps.iter().enumerate() {
            match self.execute_step(step, &saga).await {
                Ok(result) => self.store.record_step_success(saga.id, index, result)?,
                Err(e) => {
                    self.store.mark_saga_failed(saga.id)?;
                    return self.compensate_saga(&saga).await;
                }
            }
        }

        self.store.mark_saga_completed(saga.id)?;
        Ok(SagaResult { /* ... */ })
    }
}
```

**REFACTOR Phase** (Day 5):
- Extract step execution logic
- Add error handling middleware
- Create logging hooks
- Improve state transitions

**CLEANUP Phase** (Day 5):
- Format code
- Add comprehensive docstrings
- Verify all tests pass
- Commit: "Week 12, Cycle 1: Saga Coordinator Foundation (25 tests)"

---

### Week 13: Saga Store (PostgreSQL)
**Objective**: Persistent saga state with recovery

#### Cycle 2: Saga Store & Persistence (RED → GREEN → REFACTOR → CLEANUP)

**RED Phase** (Days 1-2):
```rust
#[test]
fn test_save_saga_creates_record() {
    let store = PostgresSagaStore::new(&pool);
    let saga = create_test_saga();
    store.save_saga(&saga)?;

    let retrieved = store.get_saga(saga.id)?;
    assert_eq!(retrieved.id, saga.id);
}

#[test]
fn test_record_step_execution() {
    store.record_step_execution(saga_id, 0, "users", &mutation)?;
    let saga = store.get_saga(saga_id)?;
    assert_eq!(saga.steps[0].state, StepState::Executing);
}

#[test]
fn test_find_stuck_sagas() {
    // Create saga stuck in executing state for >5 min
    store.create_stale_saga(saga_id, Duration::from_secs(300));

    let stuck = store.find_stuck_sagas(Duration::from_secs(300))?;
    assert!(stuck.contains(&saga_id));
}

#[test]
fn test_cleanup_old_sagas() {
    // Create sagas older than 24 hours
    store.create_old_saga(saga_id, Duration::from_secs(86400));

    store.cleanup_completed_sagas(Duration::from_secs(86400))?;
    assert!(!store.get_saga(saga_id).is_ok()); // Deleted
}
```

**Test Count**: 35+ tests
- CRUD operations
- Concurrent access
- State transitions
- Recovery detection
- Cleanup operations

**GREEN Phase** (Days 3-4):
```rust
pub struct PostgresSagaStore {
    pool: Arc<PgPool>,
}

// SQL Schema:
// CREATE TABLE federation_sagas (
//     id UUID PRIMARY KEY,
//     state TEXT NOT NULL,
//     started_at TIMESTAMPTZ NOT NULL,
//     completed_at TIMESTAMPTZ,
//     metadata JSONB
// );
//
// CREATE TABLE federation_saga_steps (
//     id UUID PRIMARY KEY,
//     saga_id UUID REFERENCES federation_sagas(id),
//     step_number INT NOT NULL,
//     subgraph TEXT NOT NULL,
//     mutation JSONB NOT NULL,
//     state TEXT NOT NULL,
//     result JSONB
// );

impl SagaStore for PostgresSagaStore {
    async fn save_saga(&self, saga: &Saga) -> Result<()> {
        sqlx::query(
            "INSERT INTO federation_sagas (id, state, started_at, metadata)
             VALUES ($1, $2, $3, $4)"
        )
        .bind(saga.id)
        .bind(saga.state.to_string())
        .bind(Utc::now())
        .bind(serde_json::to_value(&saga.metadata)?)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    async fn get_saga(&self, id: Uuid) -> Result<Saga> {
        // Load saga and all steps
    }

    async fn find_stuck_sagas(&self, threshold: Duration) -> Result<Vec<Uuid>> {
        // Find sagas in Executing state for > threshold
    }
}
```

**REFACTOR Phase** (Day 5):
- Connection pooling configuration
- Query optimization
- Index strategy
- Transaction handling

**CLEANUP Phase** (Day 5):
- Format code
- Add docstrings
- Commit: "Week 13, Cycle 2: Saga Store & PostgreSQL Persistence (35 tests)"

---

### Week 14: Compensation Logic
**Objective**: Automatic compensation generation & execution

#### Cycle 3: Compensation Logic (RED → GREEN → REFACTOR → CLEANUP)

**RED Phase** (Days 1-2):
```rust
#[test]
fn test_create_compensation_for_mutation() {
    let mutation = MutationType::Create;
    let compensation = build_compensation(mutation, "User", &variables)?;

    match compensation {
        CompensationAction::Delete { id } => {
            assert_eq!(id, variables["id"]);
        }
        _ => panic!("Expected Delete compensation"),
    }
}

#[test]
fn test_update_compensation_stores_previous_values() {
    let mutation = MutationType::Update;
    let previous = json!({ "name": "Alice", "email": "alice@example.com" });

    let compensation = build_compensation_with_previous(
        mutation,
        "User",
        &variables,
        &previous
    )?;

    match compensation {
        CompensationAction::Update { id, restore_values } => {
            assert_eq!(restore_values["name"], "Alice");
        }
        _ => panic!("Expected Update compensation"),
    }
}

#[test]
fn test_compensation_execution() {
    let saga = create_test_saga_with_failure();
    let compensation = saga.compensation_chain[0].clone();

    let result = execute_compensation(&compensation)?;
    assert!(result.success);
}

#[test]
fn test_compensation_order_is_reversed() {
    let saga = Saga {
        steps: vec![
            SagaStep { order: 0, subgraph: "users" },
            SagaStep { order: 1, subgraph: "orders" },
            SagaStep { order: 2, subgraph: "products" },
        ],
        // ...
    };

    let compensation = build_compensation_chain(&saga);
    assert_eq!(compensation[0].step_order, 2); // products first
    assert_eq!(compensation[1].step_order, 1); // orders second
    assert_eq!(compensation[2].step_order, 0); // users last
}
```

**Test Count**: 30+ tests
- Compensation type generation
- Compensation storage
- Compensation execution
- Rollback scenarios
- Idempotency

**GREEN Phase** (Days 3-4):
```rust
#[derive(Debug, Clone)]
pub enum CompensationAction {
    Create { id: Value, original_data: Value },
    Update { id: Value, restore_values: Value },
    Delete { id: Value, original_data: Value },
    Custom { query: String, params: Value },
}

pub fn build_compensation(
    mutation_type: MutationType,
    typename: &str,
    variables: &Value,
    previous_values: Option<&Value>,
) -> Result<CompensationAction> {
    match mutation_type {
        MutationType::Create => {
            // Extract ID from variables
            let id = variables["id"].clone();
            Ok(CompensationAction::Delete { id, original_data: variables.clone() })
        }
        MutationType::Update => {
            let id = variables["id"].clone();
            let restore_values = previous_values
                .cloned()
                .unwrap_or_else(|| json!({}));
            Ok(CompensationAction::Update { id, restore_values })
        }
        MutationType::Delete => {
            // Store full original data for restoration
            Ok(CompensationAction::Create {
                id: variables["id"].clone(),
                original_data: variables.clone(),
            })
        }
    }
}

impl Saga {
    pub fn build_compensation_chain(&mut self) {
        // Reverse order of steps
        let mut compensation = Vec::new();
        for step in self.steps.iter().rev() {
            let action = build_compensation(
                step.mutation_type,
                &step.typename,
                &step.variables,
                step.result.as_ref(),
            )?;
            compensation.push(action);
        }
        self.compensation_chain = compensation;
    }

    pub async fn execute_compensation_chain(&self) -> Result<()> {
        for (index, action) in self.compensation_chain.iter().enumerate() {
            match self.execute_compensation_action(action).await {
                Ok(_) => {
                    info!("Compensation {} succeeded", index);
                }
                Err(e) => {
                    error!("Compensation {} failed: {}", index, e);
                    // Log for manual intervention
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}
```

**REFACTOR Phase** (Day 5):
- Extract compensation builders
- Add custom compensation support
- Optimize compensation chain
- Add compensation metrics

**CLEANUP Phase** (Day 5):
- Format code
- Commit: "Week 14, Cycle 3: Compensation Logic (30 tests)"

---

### Week 15: Recovery Manager
**Objective**: Automatic recovery from crashes

#### Cycle 4: Recovery Manager (RED → GREEN → REFACTOR → CLEANUP)

**RED Phase** (Days 1-2):
```rust
#[test]
fn test_recover_pending_saga() {
    let saga = create_test_saga_stuck_in_pending();
    let manager = RecoveryManager::new(store);

    let recovered = manager.recover_stuck_sagas()?;
    assert!(recovered.contains(&saga.id));
}

#[test]
fn test_recover_executing_saga_retries_failed_step() {
    let saga = create_test_saga_stuck_on_step_1();

    let result = manager.recover_saga(saga.id)?;
    assert_eq!(result.state, SagaState::Completed);
}

#[test]
fn test_recovery_respects_max_retries() {
    let saga = create_repeatedly_failing_saga();

    // After 3 retries, should switch to compensation
    let result = manager.recover_saga_with_retry_limit(saga.id, 3)?;
    assert_eq!(result.state, SagaState::Compensating);
}

#[test]
fn test_background_recovery_runs_periodically() {
    let manager = RecoveryManager::new(store);
    manager.start_recovery_loop(Duration::from_secs(300))?;

    // Wait 6 seconds (mocked 5 min cycle)
    tokio::time::sleep(Duration::from_secs(6)).await;

    // Verify stuck sagas were recovered
    let recovered_count = store.count_recovered_sagas()?;
    assert!(recovered_count > 0);
}

#[test]
fn test_stale_saga_cleanup() {
    let old_saga = create_completed_saga_24h_ago();

    manager.cleanup_old_sagas(Duration::from_secs(86400))?;

    assert!(!store.get_saga(old_saga.id).is_ok()); // Deleted
}
```

**Test Count**: 40+ tests
- Crash recovery
- Retry logic
- Compensation triggering
- Background loop
- Cleanup operations
- Idempotency guarantees

**GREEN Phase** (Days 3-4):
```rust
pub struct RecoveryManager {
    store: Arc<dyn SagaStore>,
    executor: Arc<SagaExecutor>,
    max_retries: u32,
    recovery_interval: Duration,
}

impl RecoveryManager {
    pub async fn recover_stuck_sagas(&self, threshold: Duration) -> Result<Vec<Uuid>> {
        let stuck_ids = self.store.find_stuck_sagas(threshold).await?;
        let mut recovered = Vec::new();

        for saga_id in stuck_ids {
            match self.recover_saga(saga_id).await {
                Ok(_) => recovered.push(saga_id),
                Err(e) => {
                    error!("Failed to recover saga {}: {}", saga_id, e);
                    // Continue with others
                }
            }
        }
        Ok(recovered)
    }

    async fn recover_saga(&self, saga_id: Uuid) -> Result<SagaResult> {
        let mut saga = self.store.get_saga(saga_id).await?;
        let last_completed_step = saga.steps.iter().position(|s| s.state == StepState::Failed);

        match saga.state {
            SagaState::Executing | SagaState::Pending => {
                // Retry from last failed step
                if let Some(step_index) = last_completed_step {
                    for (index, step) in saga.steps[step_index..].iter().enumerate() {
                        self.executor.execute_step(step, &saga).await?;
                        self.store.record_step_success(saga_id, step_index + index, Value::Null).await?;
                    }
                }
                self.store.mark_saga_completed(saga_id).await?;
                Ok(SagaResult { /* ... */ })
            }
            SagaState::Failed => {
                // Start compensation
                saga.execute_compensation_chain().await?;
                self.store.mark_saga_compensated(saga_id).await?;
                Ok(SagaResult { /* ... */ })
            }
            _ => Ok(SagaResult { /* ... */ }),
        }
    }

    pub async fn start_recovery_loop(&self, interval: Duration) {
        let store = Arc::clone(&self.store);
        let executor = Arc::clone(&self.executor);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;
                if let Err(e) = self.recover_stuck_sagas(Duration::from_secs(300)).await {
                    error!("Recovery loop error: {}", e);
                }
            }
        });
    }

    pub async fn cleanup_old_sagas(&self, threshold: Duration) -> Result<usize> {
        self.store.cleanup_completed_sagas(threshold).await
    }
}
```

**REFACTOR Phase** (Day 5):
- Extract recovery strategies
- Add exponential backoff
- Improve logging
- Add metrics

**CLEANUP Phase** (Day 5):
- Format code
- Commit: "Week 15, Cycle 4: Recovery Manager (40 tests)"

---

### Week 16: API Integration
**Objective**: @transaction directive and GraphQL API

#### Cycle 5: GraphQL API Integration (RED → GREEN → REFACTOR → CLEANUP)

**RED Phase** (Days 1-2):
```rust
#[test]
fn test_transaction_directive_parsing() {
    let schema = r#"
        mutation CreateOrderWithInventory {
            createOrder(...) @transaction {
                id total
            }
        }
    "#;

    let parsed = parse_transaction_directive(schema)?;
    assert!(parsed.is_transactional);
}

#[test]
fn test_transaction_execution_returns_saga_id() {
    let response = execute_transaction_query(mutation)?;

    assert!(response.data["sagaId"].is_string());
    let saga_id = response.data["sagaId"].as_str();
    assert!(!saga_id.is_empty());
}

#[test]
fn test_transaction_query_returns_status() {
    let saga_id = execute_transaction(mutation)?;

    let status = query_transaction_status(saga_id)?;
    assert_eq!(status.state, "completed");
}

#[test]
fn test_non_transactional_mutation_succeeds() {
    // Without @transaction directive
    let response = execute_non_transaction_query(mutation)?;
    assert!(response.data["user"].is_object());
}
```

**Test Count**: 25+ tests
- Directive parsing
- Transaction execution
- Status queries
- Failure handling
- Backward compatibility

**GREEN Phase** (Days 3-4):
```rust
pub struct TransactionDirective;

impl TransactionDirective {
    pub fn parse(document: &Document) -> Result<Vec<TransactionInfo>> {
        // Extract @transaction directives from mutations
    }
}

pub async fn execute_transaction(
    coordinator: Arc<SagaCoordinator>,
    mutation: GraphQLQuery,
) -> Result<TransactionResponse> {
    let saga = coordinator.parse_mutation(&mutation)?;
    let saga_id = saga.id;

    // Execute asynchronously
    tokio::spawn(async move {
        let _ = coordinator.execute_saga(saga).await;
    });

    Ok(TransactionResponse {
        saga_id,
        status: "pending",
    })
}

pub async fn query_transaction_status(
    store: Arc<dyn SagaStore>,
    saga_id: Uuid,
) -> Result<TransactionStatus> {
    let saga = store.get_saga(saga_id).await?;
    Ok(TransactionStatus {
        saga_id,
        state: saga.state.to_string(),
        steps: saga.steps.iter().map(|s| StepStatus {
            subgraph: s.subgraph.clone(),
            state: s.state.to_string(),
        }).collect(),
        completed_at: saga.completed_at,
    })
}
```

**REFACTOR Phase** (Day 5):
- Add type safety
- Improve error messages
- Add logging hooks
- Enhance documentation

**CLEANUP Phase** (Day 5):
- Format code
- Commit: "Week 16, Cycle 5: GraphQL API Integration (25 tests)"

---

### Week 17-18: Observability & Testing
**Objective**: Metrics, tracing, dashboards, comprehensive tests

#### Cycle 6: Observability (RED → GREEN → REFACTOR → CLEANUP)

**Metrics to Add**:
- `federation_sagas_total{state}` - Counter by saga state
- `federation_saga_duration_seconds` - Histogram
- `federation_compensations_total{result}` - Counter
- `federation_saga_recoveries_total` - Counter
- `federation_saga_step_duration_seconds` - Histogram per subgraph

**Traces**:
- `federation.saga.execution` - Entire saga
- `federation.saga.step` - Each step
- `federation.saga.compensation` - Compensation chain
- `federation.saga.recovery` - Recovery operations

**Test Count**: 15+ tests
- Metric collection
- Trace generation
- Dashboard validation
- Performance verification

#### Cycle 7: Comprehensive Testing (RED → GREEN → REFACTOR → CLEANUP)

**Scenario Tests** (25+ tests):
- 2-step saga success path
- 3-step saga with step 2 failure
- 5-step saga with partial compensation needed
- Concurrent sagas
- Saga with timeout
- Saga with network retry
- Saga with inconsistent state recovery
- Large payload handling
- Cascade failure scenarios
- Manual recovery procedures

**Performance Tests** (10+ tests):
- <100ms for 3-step saga
- <500ms for 5-step saga with compensation
- Connection pool efficiency
- Memory leak detection
- Cleanup efficiency

---

## Critical Files to Create/Modify

### New Files to Create

1. **`crates/fraiseql-core/src/federation/saga_coordinator.rs`**
   - SagaCoordinator struct
   - Saga parsing and execution

2. **`crates/fraiseql-core/src/federation/saga_store.rs`**
   - SagaStore trait
   - PostgresSagaStore implementation

3. **`crates/fraiseql-core/src/federation/compensation.rs`**
   - Compensation action types
   - Compensation generation logic
   - Compensation execution

4. **`crates/fraiseql-core/src/federation/recovery.rs`**
   - RecoveryManager
   - Background recovery loop
   - Retry logic

5. **`crates/fraiseql-core/src/federation/transaction_directive.rs`**
   - @transaction directive parsing
   - API integration

6. **Tests**:
   - `tests/federation_saga_coordinator.rs`
   - `tests/federation_saga_store.rs`
   - `tests/federation_compensation.rs`
   - `tests/federation_recovery.rs`
   - `tests/federation_transaction_api.rs`
   - `tests/federation_saga_scenarios.rs`
   - `tests/federation_saga_performance.rs`

### Files to Modify

1. **`crates/fraiseql-core/src/federation/mod.rs`**
   - Export new modules

2. **`crates/fraiseql-core/src/schema/compiled.rs`**
   - Add transaction directive support

3. **`crates/fraiseql-server/src/graphql/executor.rs`**
   - Integrate transaction execution

---

## Testing Strategy

### Unit Tests (Per Module)
- Saga coordinator: 25 tests
- Saga store: 35 tests
- Compensation: 30 tests
- Recovery: 40 tests
- API integration: 25 tests

### Integration Tests
- End-to-end saga scenarios: 25 tests
- Performance validation: 10 tests
- Crash recovery simulation: 15 tests

### Total: 165+ tests

---

## Dependencies

**New Dependencies to Add**:
```toml
[dependencies]
uuid = { version = "1.0", features = ["serde", "v4"] }
tokio = { version = "1.35", features = ["full"] }
sqlx = { version = "0.7", features = ["postgres", "uuid", "json"] }
tokio-util = "0.7"
```

**No breaking changes** to existing APIs.

---

## Success Metrics

| Metric | Target | Verification |
|--------|--------|--------------|
| Test Pass Rate | 100% | cargo test passes |
| Saga Success Rate | >99% | Load test simulation |
| Recovery Time | <30s | Crash recovery test |
| Compensation Latency | <100ms per step | Performance test |
| Code Coverage | >90% | cargo tarpaulin |
| Clippy Warnings | 0 | cargo clippy clean |

---

## Rollout Plan

1. **Week 12-15**: Core implementation (Weeks 12-15)
   - Deploy saga coordinator to staging
   - Run load tests
   - Validate recovery

2. **Week 16-18**: API & Observability
   - Integrate @transaction directive
   - Add metrics and tracing
   - Run end-to-end scenarios

3. **Week 19+**: Production hardening
   - Performance tuning
   - Documentation
   - Runbooks for operations team

---

## Next Phase Dependency

Phase 4 (Apollo Router Integration) depends on Phase 3 being complete:
- Need working saga pattern for end-to-end testing
- Need transaction API for router queries
- Need observability for monitoring integration

---

## Status

- [~] Phase 3 In Progress
  - [ ] Week 12: Saga Coordinator (Target: 25 tests)
  - [ ] Week 13: Saga Store (Target: 35 tests)
  - [ ] Week 14: Compensation (Target: 30 tests)
  - [ ] Week 15: Recovery (Target: 40 tests)
  - [ ] Week 16: API Integration (Target: 25 tests)
  - [ ] Week 17-18: Observability & Testing (Target: 20+ tests)
  - [ ] Week 19: Finalization & Documentation

**Current Progress**: Starting Week 12

---

## Notes

- Sagas are **fire-and-forget**: Mutation returns immediately with saga_id
- Recovery loop runs **every 5 minutes** in background
- Compensation chain is **immutable** once saga starts
- All state is **persistent** in PostgreSQL
- Metrics exported to **Prometheus**
- Tracing integrated with **Jaeger**

