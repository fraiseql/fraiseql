# Phase 7: Observer System Integration with Entity Change Log

**Status**: Ready to implement
**Duration**: Estimated 8-10 hours
**Objective**: Connect the Observer System (Phase 6) to `tb_entity_change_log` to activate real-time event processing

---

## Overview

### The Current State

Phase 6 delivered a complete **Observer System** with:
- ✅ 7 action types (webhook, Slack, email, SMS, push, search, cache)
- ✅ Condition DSL for filtering
- ✅ Retry logic with exponential/linear/fixed backoff
- ✅ Dead Letter Queue for failed actions
- ✅ 74 comprehensive tests

**Missing**: The system is **not yet connected to the database**. Observers are defined and can execute, but there's no **real event source** - events aren't being pulled from mutations.

### The Event Source: `tb_entity_change_log`

The database schema already includes `tb_entity_change_log`:
- **Centralized audit log** for all entity mutations (INSERT, UPDATE, DELETE)
- **Debezium envelope** format with `before`/`after` values
- **Monotonic sequence numbers** for ordering
- **Multi-tenant isolation** via `fk_customer_org`
- **Status field** for tracking operation outcomes

### Phase 7's Mission

Connect the Observer System to `tb_entity_change_log` so:
1. Mutations are captured in the change log (already happening via triggers)
2. Observer system polls/listens for new entries
3. Entries are parsed into `EntityEvent`
4. Events matched against observers
5. Actions executed in real-time
6. Results recorded in DLQ if failures occur

**End Result**: A fully operational event-driven system where database changes trigger observer actions automatically.

---

## Architecture: Event Flow

```
PostgreSQL Mutation (INSERT/UPDATE/DELETE)
    ↓
Database Trigger
    ↓
INSERT INTO tb_entity_change_log (object_type, object_id, object_data, ...)
    ↓
ChangeLogListener (new Phase 7 component)
    │
    ├─ Poll or LISTEN/NOTIFY for new entries
    ├─ Parse Debezium envelope
    └─ Build EntityEvent
        ↓
    ObserverExecutor.process_event()
        ↓
    EventMatcher finds applicable observers
        ↓
    ConditionParser evaluates conditions
        ↓
    Action Executors invoke (webhook, Slack, etc.)
        ↓
    Success: recorded in execution summary
    Failure: moved to observer_dlq_items
```

---

## Subphases (8-10 hours total)

### **Subphase 7.1: ChangeLog Listener (2-3 hours)**

**Goal**: Create component to read from `tb_entity_change_log`

**Files to Create**:
- `crates/fraiseql-observers/src/listener/change_log.rs` - Main listener
- `crates/fraiseql-observers/src/listener/parser.rs` - Debezium envelope parser

**Key Components**:

1. **ChangeLogListener**
   - Trait for different polling strategies
   - PostgreSQL implementation using `tokio::task` for polling
   - Alternative: LISTEN/NOTIFY implementation (optional)
   - Configuration for poll interval, batch size

2. **Debezium Parser**
   - Parse `object_data` JSONB into structured format
   - Extract operation type (c/u/d/r)
   - Extract before/after values
   - Build `EntityEvent` with proper `EventKind`

3. **Sequence Tracking**
   - Remember last processed `id` from `tb_entity_change_log`
   - Prevent reprocessing same events
   - Handle restarts and resume from last known position

**Tests** (8-10 tests):
- `test_debezium_envelope_parsing`
- `test_insert_operation_detection`
- `test_update_operation_detection`
- `test_delete_operation_detection`
- `test_before_after_value_extraction`
- `test_invalid_json_handling`
- `test_sequence_tracking`
- `test_batch_processing`

---

### **Subphase 7.2: Event Conversion (1-2 hours)**

**Goal**: Convert change log entries to observer-compatible `EntityEvent`

**Files to Update**:
- `crates/fraiseql-observers/src/event.rs` - Add conversion methods

**Conversions Needed**:

```rust
// From Debezium envelope to EntityEvent
fn from_debezium_envelope(
    entry: ChangeLogEntry,
    envelope: DebeeziumEnvelope
) -> EntityEvent {
    // Extract operation: c→Created, u→Updated, d→Deleted
    // Extract entity_type from entry.object_type
    // Extract entity_id from entry.object_id
    // Use "after" values for the event data
    // Set timestamp from entry.created_at
}
```

**Mapping**:
- `operation='c'` → `EventKind::Created`
- `operation='u'` → `EventKind::Updated`
- `operation='d'` → `EventKind::Deleted`
- `operation='r'` → `EventKind::Custom`

**Special Handling**:
- Multi-tenant: Include `fk_customer_org` in context
- Author tracking: Include `fk_contact` (user who made change)
- Metadata preservation: Pass `extra_metadata` through

**Tests** (6-8 tests):
- `test_insert_to_entity_event`
- `test_update_to_entity_event`
- `test_delete_to_entity_event`
- `test_multitenant_context_preserved`
- `test_author_metadata_captured`
- `test_timestamp_accuracy`

---

### **Subphase 7.3: Listener Integration (2-3 hours)**

**Goal**: Integrate listener into the observer executor lifecycle

**Files to Update**:
- `crates/fraiseql-observers/src/lib.rs` - Add listener module
- `crates/fraiseql-observers/src/executor.rs` - Integration hooks
- `crates/fraiseql-observers/src/config.rs` - Listener config

**Components**:

1. **ListenerConfig**
   ```rust
   pub struct ListenerConfig {
       pub db_url: String,
       pub poll_interval_ms: u64,          // e.g., 100ms
       pub batch_size: usize,              // e.g., 100 events at a time
       pub resume_from_id: Option<i64>,    // Resume from checkpoint
   }
   ```

2. **Observable Lifecycle**
   ```rust
   // Start listener in background
   let listener_handle = listener.start().await?;

   // Listen in loop
   while let Some(event) = listener.next_event().await {
       executor.process_event(&event).await?;
   }

   // Graceful shutdown
   listener.stop().await?;
   ```

3. **Checkpoint Management**
   - Save last processed ID to persistent storage
   - Allow recovery after crashes
   - Configurable checkpoint interval

**Tests** (10-12 tests):
- `test_listener_starts`
- `test_listener_polls_change_log`
- `test_events_received_in_order`
- `test_batch_processing`
- `test_checkpoint_saved`
- `test_resume_from_checkpoint`
- `test_listener_graceful_shutdown`
- `test_duplicate_event_prevention`

---

### **Subphase 7.4: Error Handling & Resilience (1-2 hours)**

**Goal**: Handle failures gracefully

**Scenarios to Handle**:

1. **Database Connection Loss**
   - Retry with exponential backoff
   - Max 10 retries before alerting
   - Graceful degradation (queue events locally)

2. **Malformed Change Log Entries**
   - Log error with entry ID
   - Skip and continue (don't block pipeline)
   - Move to error queue

3. **Observer Execution Failures**
   - Already handled by Phase 6 (DLQ)
   - But need to track in listener logs

4. **Out of Sync**
   - Detect if change log is growing faster than processing
   - Alert if backlog exceeds threshold (e.g., 1000 events)
   - Metrics: lag, throughput, error rate

**Tests** (8-10 tests):
- `test_connection_retry`
- `test_malformed_entry_handling`
- `test_backlog_alerting`
- `test_graceful_degradation`
- `test_metrics_collection`

---

### **Subphase 7.5: Integration Tests (2-3 hours)**

**Goal**: End-to-end tests with real database

**Test Suite**: `tests/e2e_integration_test.rs`

**Scenarios**:

1. **Simple Flow**
   - Insert into `tb_user`
   - Change log entry created
   - Observer system receives event
   - Webhook action fired
   - Result logged

2. **Multi-Entity**
   - Multiple entity types
   - Multiple observers
   - Each fires correctly

3. **Filtering**
   - Observer with condition
   - Only matching events trigger action
   - Non-matching events ignored

4. **Failure & Recovery**
   - Webhook endpoint down
   - Action retried
   - Succeeded after recovery

5. **Performance**
   - 1000 events/sec throughput
   - < 100ms latency (event creation to action execution)
   - Memory stable

**Tests** (15-20 tests):
- `test_e2e_insert_workflow`
- `test_e2e_multiple_observers`
- `test_e2e_condition_filtering`
- `test_e2e_webhook_execution`
- `test_e2e_slack_execution`
- `test_e2e_failure_and_recovery`
- `test_e2e_throughput_1000_events`
- `test_e2e_latency_under_100ms`
- `test_e2e_memory_stability`
- `test_e2e_multitenant_isolation`

---

## Implementation Strategy

### Step 1: Create `listener/change_log.rs`
```rust
pub struct ChangeLogListener {
    db_pool: PgPool,
    config: ListenerConfig,
    last_processed_id: i64,
}

impl ChangeLogListener {
    pub async fn next_batch(&mut self) -> Result<Vec<EntityEvent>> {
        // SELECT id, object_type, object_id, object_data, ...
        // FROM tb_entity_change_log
        // WHERE id > last_processed_id
        // ORDER BY id ASC
        // LIMIT batch_size
    }
}
```

### Step 2: Create `listener/parser.rs`
```rust
pub fn parse_debezium_envelope(
    data: &Value
) -> Result<DebeeziumEnvelope> {
    // Extract: before, after, op, source
    // Validate: op in (c,u,d,r)
}

pub fn to_entity_event(
    entry: ChangeLogEntry,
    envelope: DebeeziumEnvelope
) -> Result<EntityEvent> {
    // Map operation to EventKind
    // Extract data from "after"
    // Build EntityEvent
}
```

### Step 3: Update `executor.rs`
```rust
pub async fn run_listener_loop(&self, listener: &mut ChangeLogListener) {
    loop {
        match listener.next_batch().await {
            Ok(events) => {
                for event in events {
                    let _ = self.process_event(&event).await;
                }
            }
            Err(e) => {
                error!("Listener error: {}", e);
                // Backoff and retry
            }
        }
    }
}
```

### Step 4: Update `lib.rs`
```rust
pub mod listener;

pub use listener::{ChangeLogListener, ListenerConfig};
```

---

## Testing Checklist

- [ ] 8-10 parser tests (Debezium envelope handling)
- [ ] 6-8 conversion tests (ChangeLog → EntityEvent)
- [ ] 10-12 integration tests (listener lifecycle)
- [ ] 8-10 error handling tests
- [ ] 15-20 e2e tests (full workflow)
- [ ] **Total: 50-60 new tests**

**Running Tests**:
```bash
# Unit tests
cargo test -p fraiseql-observers --lib

# Integration tests
cargo test -p fraiseql-observers --test e2e_integration

# Full suite
cargo test -p fraiseql-observers
```

---

## Database Setup (One-Time)

The database should already have `tb_entity_change_log` from schema conventions, but verify:

```sql
SELECT * FROM information_schema.tables
WHERE table_name = 'tb_entity_change_log';
```

If not present, run:
```sql
-- See /home/lionel/code/fraiseql/docs/specs/schema-conventions.md section 6
-- Create tb_entity_change_log table with indexes
```

---

## Success Criteria

✅ ChangeLogListener reads from `tb_entity_change_log`
✅ Debezium envelopes parsed correctly
✅ EntityEvents created from change log entries
✅ Events routed to observers successfully
✅ 50-60 new tests passing
✅ Zero unsafe code
✅ Zero clippy warnings
✅ End-to-end workflow tested
✅ Documentation complete

---

## Deliverables

1. **Code**
   - `listener/change_log.rs` (200-250 LOC)
   - `listener/parser.rs` (150-200 LOC)
   - Updated `executor.rs` (50-100 LOC)
   - Updated `lib.rs` (10 LOC)
   - Total: ~400-500 new LOC

2. **Tests**
   - 50-60 new tests covering all scenarios
   - End-to-end integration tests

3. **Documentation**
   - Update README with listener architecture
   - Document configuration options
   - Add troubleshooting guide

---

## Next Phase (Phase 8)

After Phase 7 completes:
- **Phase 8A**: Full-text search integration
- **Phase 8B**: Caching & performance optimization
- **Phase 8C**: Job queues & scheduling for long-running tasks

---

## Time Estimate Breakdown

| Task | Duration |
|------|----------|
| Subphase 7.1 (ChangeLog Listener) | 2-3 hours |
| Subphase 7.2 (Event Conversion) | 1-2 hours |
| Subphase 7.3 (Integration) | 2-3 hours |
| Subphase 7.4 (Error Handling) | 1-2 hours |
| Subphase 7.5 (E2E Tests) | 2-3 hours |
| **Total** | **8-13 hours** |

**Buffer**: 1-2 hours for testing/debugging

---

## Risks & Mitigation

| Risk | Mitigation |
|------|-----------|
| Debezium format incompatibilities | Test with actual DB triggers early |
| Performance (poll interval too slow) | Profile with 1000+ events/sec load |
| Checkpoint recovery bugs | Test crash scenarios thoroughly |
| Multi-tenant data leaks | Strict tenant isolation in queries |

---

## Success Metrics (Phase 7 Complete)

- ✅ Full end-to-end: mutation → observer action
- ✅ < 100ms latency (event creation to execution)
- ✅ Throughput: 1000+ events/sec
- ✅ 50-60 tests passing
- ✅ Production-ready reliability
