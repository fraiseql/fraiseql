# Phase 8.9: Multi-Listener Failover - Implementation Plan

**Date**: January 22, 2026
**Objective**: Implement shared checkpoint coordination for high-availability multi-listener setup
**Target**: 200+ tests passing (173 + 27 new), production-grade high availability

## Problem Statement

**Without Multi-Listener Failover**:
- Single listener is a single point of failure
- No automatic failover when listener crashes
- Manual intervention required to resume event processing
- Data loss if listener fails mid-batch
- No shared state between listener instances

**With Multi-Listener Failover**:
- Multiple listeners process events in parallel
- Automatic failover to backup listeners
- Shared checkpoint coordination prevents duplication
- Distributed state management via database
- Leader election for coordination
- Zero-downtime listener restarts

## Architecture Overview

### Multi-Listener Topology

```
                    PostgreSQL ChangeLog
                            │
         ┌──────────────────┼──────────────────┐
         │                  │                  │
    Listener-1         Listener-2         Listener-3
    (Primary)          (Standby)          (Standby)
         │                  │                  │
         └──────────────────┼──────────────────┘
                            │
                  Shared Checkpoint Store
                  (Leader Election)
                            │
                   ObserverExecutor
                   (Event Processing)
```

### Listener Coordination States

```
INITIALIZING
    ↓
CONNECTING (Acquiring lease)
    ↓
RUNNING (Processing events)
    ↓ (Lease expires / Lost connection)
RECOVERING
    ↓ (Can recover)
RUNNING (Resume from checkpoint)

OR

    ↓ (Cannot recover)
STOPPED
```

### Checkpoint Management

```
Listener 1                          Listener 2
LISTEN                              LISTEN
  │                                   │
  ├─→ Process batch                   │
  │   Checkpoint: 1000                │
  │   Store in DB ────────────────────→ See new checkpoint
  │   Acquire lease                   │
  │                                   ├─→ Wait (lease held)
  │                                   │   Monitor for timeout
  │   Release lease (process done)    │
  │                                   ├─→ Acquire lease
  │                                   └─→ Resume from 1000
```

## Implementation Steps

### Step 1: Listener State Machine (100 lines)
**File**: `src/listener/state.rs`

Listener lifecycle management:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerState {
    Initializing,
    Connecting,
    Running,
    Recovering,
    Stopped,
}

pub struct ListenerStateMachine {
    current_state: Arc<Mutex<ListenerState>>,
    state_change_time: Arc<Mutex<Instant>>,
    listener_id: String,
    max_recovery_attempts: u32,
}

impl ListenerStateMachine {
    pub fn new(listener_id: String) -> Self { ... }
    pub async fn transition(&self, next_state: ListenerState) -> Result<()> { ... }
    pub async fn get_state(&self) -> ListenerState { ... }
    pub async fn get_state_duration(&self) -> Duration { ... }
}
```

Tests (4):
- test_listener_state_transitions
- test_listener_state_duration_tracking
- test_listener_invalid_transitions
- test_listener_state_recovery

### Step 2: Distributed Checkpoint Leasing (120 lines)
**File**: `src/listener/lease.rs`

Lease-based distributed coordination:
```rust
pub struct CheckpointLease {
    listener_id: String,
    checkpoint_id: i64,
    lease_holder: Arc<Mutex<Option<String>>>,
    lease_acquired_at: Arc<Mutex<Option<Instant>>>,
    lease_duration_ms: u64,
    lease_store: Arc<dyn CheckpointStore>,
}

impl CheckpointLease {
    pub async fn acquire(&self) -> Result<bool> { ... }
    pub async fn release(&self) -> Result<()> { ... }
    pub async fn renew(&self) -> Result<bool> { ... }
    pub async fn is_valid(&self) -> Result<bool> { ... }
    pub async fn get_holder(&self) -> Result<Option<String>> { ... }
}
```

Tests (5):
- test_lease_acquisition
- test_lease_renewal
- test_lease_expiration
- test_lease_contested_acquisition
- test_lease_multiple_listeners

### Step 3: Multi-Listener Coordinator (150 lines)
**File**: `src/listener/coordinator.rs`

Coordinates multiple listeners:
```rust
pub struct MultiListenerCoordinator {
    listeners: Arc<DashMap<String, ListenerHandle>>,
    checkpoint_store: Arc<dyn CheckpointStore>,
    lease_manager: Arc<LeaseManager>,
    leader_id: Arc<Mutex<Option<String>>>,
}

pub struct ListenerHandle {
    listener_id: String,
    state_machine: ListenerStateMachine,
    checkpoint: Arc<Mutex<i64>>,
    last_heartbeat: Arc<Mutex<Instant>>,
}

impl MultiListenerCoordinator {
    pub fn new(checkpoint_store: Arc<dyn CheckpointStore>) -> Self { ... }
    pub async fn register_listener(&self, listener_id: String) -> Result<()> { ... }
    pub async fn deregister_listener(&self, listener_id: &str) -> Result<()> { ... }
    pub async fn get_listener_state(&self, listener_id: &str) -> Result<ListenerState> { ... }
    pub async fn elect_leader(&self) -> Result<String> { ... }
    pub async fn check_listener_health(&self) -> Result<Vec<ListenerHealth>> { ... }
}

pub struct ListenerHealth {
    listener_id: String,
    is_healthy: bool,
    last_checkpoint: i64,
    state: ListenerState,
}
```

Tests (6):
- test_listener_registration
- test_listener_deregistration
- test_listener_health_check
- test_leader_election
- test_leader_failover
- test_coordinator_state_consistency

### Step 4: Failover Logic (100 lines)
**File**: `src/listener/failover.rs`

Automatic failover handling:
```rust
pub struct FailoverManager {
    coordinator: Arc<MultiListenerCoordinator>,
    health_check_interval_ms: u64,
    failover_threshold_ms: u64,
}

pub struct FailoverEvent {
    failed_listener_id: String,
    failover_target_id: String,
    checkpoint: i64,
    timestamp: Instant,
}

impl FailoverManager {
    pub fn new(coordinator: Arc<MultiListenerCoordinator>) -> Self { ... }
    pub async fn detect_failures(&self) -> Result<Vec<String>> { ... }
    pub async fn trigger_failover(&self, failed_listener_id: &str) -> Result<FailoverEvent> { ... }
    pub async fn resume_from_checkpoint(&self, listener_id: &str, checkpoint: i64) -> Result<()> { ... }
    pub async fn start_health_monitor(&self) { ... }
}
```

Tests (5):
- test_failure_detection
- test_failover_trigger
- test_failover_checkpoint_consistency
- test_multiple_listener_failover
- test_failover_recovery

### Step 5: Listener Integration (100 lines)
**File**: `src/listener/mod.rs` (modifications)

Update listener to use coordinator:
```rust
pub struct ChangeLogListener {
    // Existing fields...
    coordinator: Option<Arc<MultiListenerCoordinator>>,
    listener_handle: Arc<ListenerHandle>,
    state_machine: Arc<ListenerStateMachine>,
}

impl ChangeLogListener {
    pub fn with_coordinator(
        mut self,
        coordinator: Arc<MultiListenerCoordinator>,
    ) -> Result<Self> { ... }

    pub async fn next_batch(&mut self) -> Result<Vec<ChangeLogEntry>> {
        // Acquire lease before fetching
        // Update checkpoint after successful processing
        // Handle lease expiration
        // Trigger failover if needed
    }
}
```

Tests (4):
- test_listener_with_coordinator
- test_listener_lease_acquisition_in_batch
- test_listener_checkpoint_update
- test_listener_failover_handling

### Step 6: Configuration (50 lines)
**File**: `src/config.rs` (modifications)

Add multi-listener configuration:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiListenerConfig {
    /// Enable multi-listener coordination
    pub enabled: bool,
    /// Listener ID for this instance
    pub listener_id: String,
    /// Lease duration in milliseconds
    pub lease_duration_ms: u64,
    /// Health check interval in milliseconds
    pub health_check_interval_ms: u64,
    /// Failover threshold in milliseconds
    pub failover_threshold_ms: u64,
    /// Maximum listeners in group
    pub max_listeners: usize,
}

impl Default for MultiListenerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listener_id: format!("listener-{}", Uuid::new_v4()),
            lease_duration_ms: 30000,
            health_check_interval_ms: 5000,
            failover_threshold_ms: 60000,
            max_listeners: 10,
        }
    }
}
```

Tests (2):
- test_multi_listener_config_defaults
- test_multi_listener_config_validation

### Step 7: Metrics Integration (50 lines)
**File**: `src/metrics/mod.rs` (modifications)

Add listener metrics:
```rust
#[cfg(feature = "metrics")]
pub struct ListenerMetrics {
    pub listeners_active: Gauge,
    pub listeners_failed: Counter,
    pub failovers_total: Counter,
    pub checkpoint_leases_held: Gauge,
    pub checkpoint_updates_total: Counter,
    pub leader_elections_total: Counter,
}
```

### Step 8: Comprehensive Tests (120 lines)
**File**: `src/listener/tests.rs`

Full test coverage:
- Multi-listener state machine tests (4)
- Lease management tests (5)
- Coordinator tests (6)
- Failover logic tests (5)
- Listener integration tests (4)
- End-to-end failover scenarios (3)

**Total: 27 new tests** → 173 + 27 = 200 tests passing

## Dependencies Required

Check Cargo.toml:
- `uuid` ✅ (already present)
- `tokio` ✅ (already present)
- `dashmap` ✅ (already present)
- `sqlx` ✅ (already present for database)

No new dependencies needed!

## File Structure

```
src/listener/
├── mod.rs           (Modified: Add coordinator integration)
├── state.rs         (100 lines: State machine)
├── lease.rs         (120 lines: Lease management)
├── coordinator.rs   (150 lines: Multi-listener coordination)
├── failover.rs      (100 lines: Failover logic)
└── tests.rs         (120 lines: Comprehensive tests)

Modified files:
├── src/config.rs    (Add MultiListenerConfig)
├── src/lib.rs       (Export new types)
└── src/metrics/mod.rs (Add listener metrics)

Total: ~640 lines of new code, ~150 lines of modifications
```

Module exports in `src/lib.rs`:
```rust
pub use listener::{
    ListenerState, ListenerStateMachine, CheckpointLease,
    MultiListenerCoordinator, ListenerHealth, FailoverManager,
    FailoverEvent, MultiListenerConfig,
};
```

## Success Criteria

✅ **Functional**:
- [ ] Multi-listener registration and discovery working
- [ ] Lease-based checkpoint coordination correct
- [ ] Automatic failover triggers on listener failure
- [ ] Checkpoint consistency maintained across failovers
- [ ] State machine transitions valid
- [ ] Leader election deterministic

✅ **Quality**:
- [ ] 200+ tests passing (27 new)
- [ ] 100% Clippy compliant
- [ ] Zero unsafe code
- [ ] All error paths tested
- [ ] Deadlock-free coordination

✅ **Reliability**:
- [ ] No checkpoint duplication on failover
- [ ] No event loss during recovery
- [ ] State survives listener crashes
- [ ] Lease conflicts handled gracefully
- [ ] Health monitoring detects failures < 60s

✅ **Performance**:
- [ ] Lease acquisition < 100ms
- [ ] Failover detection < 30s
- [ ] Checkpoint updates < 50ms
- [ ] No blocking on coordinator operations

## Example Usage

```rust
// Create coordinator
let checkpoint_store = Arc::new(PostgresCheckpointStore::new(pool)?);
let coordinator = MultiListenerCoordinator::new(checkpoint_store);

// Register listeners
coordinator.register_listener("listener-1".to_string()).await?;
coordinator.register_listener("listener-2".to_string()).await?;

// Get listener for this instance
let listener_state = coordinator.get_listener_state("listener-1").await?;

// Create failover manager
let failover = FailoverManager::new(coordinator.clone());
failover.start_health_monitor().await;

// Use listener with coordinator
let listener = ChangeLogListener::new(config)
    .with_coordinator(coordinator.clone())?;

// Automatic failover happens on failure detection
loop {
    match listener.next_batch().await {
        Ok(entries) => process_entries(entries).await?,
        Err(ObserverError::ListenerFailover { checkpoint, .. }) => {
            // Failover triggered, checkpoint saved
            eprintln!("Failing over from checkpoint {}", checkpoint);
        }
        Err(e) => return Err(e),
    }
}
```

## Monitoring Dashboard

```json
{
  "panels": [
    {
      "title": "Active Listeners",
      "targets": [{"expr": "listeners_active"}]
    },
    {
      "title": "Failovers Per Hour",
      "targets": [{"expr": "rate(failovers_total[1h])"}]
    },
    {
      "title": "Lease Hold Time",
      "targets": [{"expr": "histogram_quantile(0.95, checkpoint_lease_duration_ms)"}]
    },
    {
      "title": "Leader Election Success Rate",
      "targets": [{"expr": "rate(leader_elections_total[5m])"}]
    },
    {
      "title": "Checkpoint Lag (Seconds)",
      "targets": [{"expr": "checkpoint_lag_seconds"}]
    }
  ]
}
```

## Alerting Rules

```yaml
groups:
  - name: multi_listener_alerts
    rules:
      - alert: ListenerDown
        expr: listeners_active < 1
        for: 5m
        annotations:
          summary: "No listeners active, events not being processed"

      - alert: HighFailoverRate
        expr: rate(failovers_total[5m]) > 0.1
        for: 10m
        annotations:
          summary: "High failover rate detected"

      - alert: CheckpointLagging
        expr: checkpoint_lag_seconds > 3600
        for: 30m
        annotations:
          summary: "Event processing lagging by over 1 hour"

      - alert: LeaseContention
        expr: checkpoint_leases_held > 2
        annotations:
          summary: "Multiple listeners holding leases (conflict)"
```

## Phase 8 Progress After Completion

```
Phase 8.7: Prometheus Metrics ✅ Complete (152 tests)
Phase 8.8: Circuit Breaker Pattern ✅ Complete (173 tests)
Phase 8.9: Multi-Listener Failover ✅ Complete (200 tests)
Total Progress: 69.2% (9 of 13 subphases complete)
```

Ready for Phase 8.10: CLI Tools 🚀
