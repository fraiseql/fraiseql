# Phase 3: Distributed Transactions (Saga Pattern)

**Duration**: 8 weeks (weeks 8-15)
**Lead Role**: Senior Rust Engineer
**Impact**: CRITICAL - Enables reliable mutations across federated subgraphs
**Goal**: Implement saga coordinator with state persistence, compensation, and recovery

---

## Objective

Enable **reliable distributed mutations** across federated subgraphs with automatic compensation and crash recovery. Transforms FraiseQL from "query-only reliable" to "mutations guaranteed atomic or fully compensated".

### Key Insight
Saga pattern is industrial-standard for distributed transactions without 2-phase commit's availability costs.

---

## Success Criteria

### Must Have
- [ ] Saga coordinator with forward + compensation steps
- [ ] Persistent saga state (PostgreSQL)
- [ ] Automatic compensation on failure
- [ ] Background recovery from crashes
- [ ] `@transaction` directive in schema
- [ ] 165+ new tests passing
- [ ] Saga success rate >99%

### Performance Targets
- [ ] Saga overhead: <100ms for 3 steps
- [ ] Recovery latency: <5 seconds after crash
- [ ] Compensation latency: <100ms per step

---

## Architecture

### Saga Coordinator

```rust
// crates/fraiseql-core/src/federation/saga_coordinator.rs

pub struct SagaCoordinator {
    saga_store: Arc<dyn SagaStore>,
    execution_engine: Arc<SagaExecutionEngine>,
    recovery_manager: Arc<SagaRecoveryManager>,
}

impl SagaCoordinator {
    pub async fn execute_saga(&self, steps: Vec<SagaStep>) -> Result<SagaResult>;
    pub async fn compensate_saga(&self, saga_id: Uuid) -> Result<()>;
    pub async fn recover_in_flight_sagas(&self) -> Result<Vec<Uuid>>;
}
```

### Saga State Machine

```
┌─────────┐
│ Pending │  (Initial state, queued)
└────┬────┘
     │ execute_saga()
     ↓
┌──────────┐
│Executing │  (Forward steps in progress)
└────┬─────┘
     │
     ├─ All steps succeed → Completed
     │
     └─ Any step fails → Compensating
                           ↓
                      ┌─────────────┐
                      │Compensating │ (Running compensation steps)
                      └──────┬──────┘
                             │
                             ├─ All compensations succeed → Compensated
                             │
                             └─ Compensation fails → Failed
```

### Saga Store Schema

```sql
CREATE TABLE federation_sagas (
    id UUID PRIMARY KEY,
    state TEXT NOT NULL,              -- pending, executing, completed, failed
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    error_message TEXT,
    metadata JSONB
);

CREATE TABLE federation_saga_steps (
    id UUID PRIMARY KEY,
    saga_id UUID REFERENCES federation_sagas(id),
    step_number INT NOT NULL,
    subgraph TEXT NOT NULL,
    mutation JSONB NOT NULL,
    compensation JSONB NOT NULL,
    state TEXT NOT NULL,              -- pending, executing, completed, failed
    result JSONB,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error_message TEXT
);
```

---

## TDD Cycles

### Cycle 1: Saga Coordinator Foundation (Weeks 8-9)
- Saga coordinator structure
- Step execution engine
- State transitions

### Cycle 2: Saga Store & Persistence (Weeks 9-10)
- PostgreSQL saga store
- State persistence
- Query API

### Cycle 3: Compensation Logic (Weeks 10-11)
- Automatic compensation generation (Create→Delete, Update→Update, Delete→Create)
- Manual compensation specification
- Backward step execution

### Cycle 4: Recovery Manager (Weeks 11-12)
- Background recovery loop
- In-flight saga detection
- Stale saga cleanup

### Cycle 5: API Integration (Weeks 12-13)
- `@transaction` directive in schema
- Mutation execution with saga wrapper
- Error handling & compensation

### Cycle 6-8: Observability & Testing (Weeks 13-15)
- Saga metrics (duration, success rate)
- Distributed tracing
- Comprehensive tests (50+ scenarios)
- Production deployment guide

---

## Key Deliverables

1. **Saga Coordinator**: Orchestrate distributed mutations
2. **State Persistence**: Store saga state across restarts
3. **Compensation Engine**: Automatic compensation generation
4. **Recovery Manager**: Background recovery from failures
5. **Observability**: Metrics, tracing, dashboards
6. **Documentation**: Deployment guide, examples

---

## Critical Files to Modify/Create

- `crates/fraiseql-core/src/federation/saga_coordinator.rs` (NEW)
- `crates/fraiseql-core/src/federation/saga_store.rs` (NEW)
- `crates/fraiseql-core/src/federation/compensation.rs` (NEW)
- `crates/fraiseql-core/src/federation/saga_recovery.rs` (NEW)
- `crates/fraiseql-core/src/federation/mutation_executor.rs` - Integrate sagas

---

## Next Phase Dependencies

Phase 4 (Apollo Router) and Phase 5 (Type Merging) can proceed in parallel after this phase.

---

**Phase Status**: Planning
**Estimated Tests**: +165
**Estimated Code**: 3,500 lines
**Complexity**: VERY HIGH - Distributed systems involved
