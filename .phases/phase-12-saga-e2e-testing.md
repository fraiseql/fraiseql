# Phase 12: Saga End-to-End (E2E) Testing

## Objective
Validate the complete distributed transaction saga system end-to-end by implementing comprehensive integration tests that exercise the coordinator → executor → compensator → recovery manager workflow under normal and failure conditions.

## Success Criteria
- [ ] 40+ new E2E scenario tests created and passing
- [ ] Test coverage for all saga state machines (forward phase, compensation, recovery)
- [ ] Multi-step saga execution (5, 7, 10+ step scenarios)
- [ ] Failure scenarios with automatic and manual compensation
- [ ] Concurrent saga handling (10+ concurrent sagas)
- [ ] Recovery manager interaction with stuck/in-flight sagas
- [ ] Crash recovery during forward and compensation phases
- [ ] All tests passing: `cargo test federation_saga_e2e_scenarios`
- [ ] Zero clippy warnings
- [ ] Code properly formatted

---

## TDD Cycles

### Cycle 1: Basic Multi-Step Saga Execution

**RED Phase**: Create tests for successful multi-step saga execution

```rust
// federation_saga_e2e_scenarios.rs - TESTS ONLY

#[tokio::test]
async fn test_saga_with_5_steps_all_succeed()
#[tokio::test]
async fn test_saga_with_7_steps_all_succeed()
#[tokio::test]
async fn test_saga_with_10_steps_all_succeed()
#[tokio::test]
async fn test_saga_execution_preserves_step_order()
#[tokio::test]
async fn test_each_step_receives_previous_step_output()
#[tokio::test]
async fn test_saga_result_contains_all_step_data()
#[tokio::test]
async fn test_concurrent_5_sagas_execute_independently()
#[tokio::test]
async fn test_concurrent_10_sagas_execute_independently()
```

**Tests verify**:
- Multi-step execution completes successfully
- Step results are chained/available to next step
- Concurrent sagas don't interfere with each other
- Each saga gets its own isolated state

**GREEN Phase**: Implement minimal test harness

In tests only (no production code changes yet):
- Create test saga builder helper functions
- Create test executor that simulates step execution
- Create test store that tracks saga state in memory
- Implement coordinator wrapper for testing

Example structure:
```rust
async fn build_test_saga(step_count: usize) -> (SagaCoordinator, Vec<Uuid>) {
    // Create coordinator
    // Create N steps
    // Create saga and return IDs
}

async fn execute_test_saga_to_completion(coordinator, saga_id) -> SagaResult {
    // Execute all steps
    // Return final result
}
```

**REFACTOR Phase**:
- Extract common test setup into fixtures module
- Create reusable scenario builders
- Organize test helpers by concern (executor, store, coordinator)

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

### Cycle 2: Single-Step Failure Scenarios

**RED Phase**: Create tests for step failures

```rust
#[tokio::test]
async fn test_first_step_failure_prevents_second_step()
#[tokio::test]
async fn test_middle_step_failure_stops_subsequent_steps()
#[tokio::test]
async fn test_last_step_failure_triggers_compensation()
#[tokio::test]
async fn test_failed_saga_transitions_to_failed_state()
#[tokio::test]
async fn test_failure_error_message_includes_step_context()
#[tokio::test]
async fn test_failure_records_completed_steps_count()
```

**Tests verify**:
- Failure at any step stops execution
- Saga state reflects failure correctly
- Error context is preserved
- Completed steps count is accurate

**GREEN Phase**:
- No production code needed (placeholder implementations handle this)
- Tests work with existing placeholder methods that always succeed
- Modify test harness to inject failure at specific steps

**REFACTOR Phase**:
- Extract failure injection into reusable helpers
- Create "step failure scenario" builder

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

### Cycle 3: Automatic Compensation Scenarios

**RED Phase**: Create tests for compensation phase execution

```rust
#[tokio::test]
async fn test_failed_saga_with_automatic_strategy_compensates()
#[tokio::test]
async fn test_compensation_executes_in_reverse_order()
#[tokio::test]
async fn test_compensation_skips_non_completed_steps()
#[tokio::test]
async fn test_all_compensations_succeed_saga_state_compensated()
#[tokio::test]
async fn test_partial_compensation_failure_recorded()
#[tokio::test]
async fn test_compensation_complete_failure_recorded()
#[tokio::test]
async fn test_compensation_result_available_for_audit()
#[tokio::test]
async fn test_saga_transitions_from_failed_to_compensating_to_compensated()
#[tokio::test]
async fn test_compensation_duration_metrics_recorded()
```

**Tests verify**:
- Compensation executes in reverse (N..1)
- Only completed steps are compensated
- Compensation continues even if individual steps fail
- Final state reflects compensation result
- Metrics are collected

**GREEN Phase**:
- Implement compensation execution in test harness
- Track compensation state transitions
- Collect compensation metrics

**REFACTOR Phase**:
- Extract compensation state machine into helper
- Create compensation result builders
- Organize compensation test fixtures

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

### Cycle 4: Manual Compensation Strategy

**RED Phase**: Create tests for manual compensation

```rust
#[tokio::test]
async fn test_failed_saga_with_manual_strategy_transitions_to_manual_compensation_required()
#[tokio::test]
async fn test_manual_strategy_does_not_auto_compensate()
#[tokio::test]
async fn test_manual_compensation_can_be_triggered_after_failure()
#[tokio::test]
async fn test_manual_compensation_executes_same_as_automatic()
#[tokio::test]
async fn test_cancel_saga_triggers_compensation_regardless_of_strategy()
```

**Tests verify**:
- Manual strategy doesn't auto-trigger compensation
- Compensation can be manually triggered
- Manual compensation works correctly
- Cancel always compensates

**GREEN Phase**:
- Implement strategy selection in coordinator
- Add manual trigger for compensation

**REFACTOR Phase**:
- Extract strategy logic into reusable helpers

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

### Cycle 5: Concurrent Saga Scenarios

**RED Phase**: Create tests for concurrent saga handling

```rust
#[tokio::test]
async fn test_10_concurrent_sagas_execute_independently()
#[tokio::test]
async fn test_50_concurrent_sagas_execute_independently()
#[tokio::test]
async fn test_concurrent_sagas_with_different_strategies()
#[tokio::test]
async fn test_concurrent_sagas_some_fail_some_succeed()
#[tokio::test]
async fn test_in_flight_saga_list_accurate_during_concurrent_execution()
#[tokio::test]
async fn test_concurrent_compensation_does_not_interfere()
```

**Tests verify**:
- Multiple sagas execute concurrently without interference
- State is isolated per saga
- Mixed success/failure scenarios work
- In-flight list is accurate

**GREEN Phase**:
- Implement concurrent execution in test harness
- Use tokio::spawn for concurrent task execution
- Collect results from all concurrent sagas

**REFACTOR Phase**:
- Create concurrent scenario builder
- Extract concurrent execution helpers

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

### Cycle 6: Recovery Manager Integration

**RED Phase**: Create tests for recovery manager interaction

```rust
#[tokio::test]
async fn test_pending_saga_transitioned_by_recovery_manager()
#[tokio::test]
async fn test_stuck_executing_saga_detected_by_recovery_manager()
#[tokio::test]
async fn test_stale_saga_cleaned_up_by_recovery_manager()
#[tokio::test]
async fn test_recovery_manager_processes_sagas_in_batches()
#[tokio::test]
async fn test_recovery_manager_resilient_to_single_saga_failure()
#[tokio::test]
async fn test_recovery_manager_metrics_accurate()
#[tokio::test]
async fn test_recovered_saga_continues_execution()
#[tokio::test]
async fn test_recovery_manager_and_executor_coordinate_correctly()
```

**Tests verify**:
- Recovery manager processes pending sagas
- Stuck sagas are detected
- Stale sagas are cleaned
- Recovery doesn't lose state
- Metrics are accurate

**GREEN Phase**:
- Integrate recovery manager into test harness
- Implement recovery loop simulation
- Track recovery metrics

**REFACTOR Phase**:
- Extract recovery scenario builders
- Organize recovery test fixtures

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

### Cycle 7: Crash/Interruption Recovery Scenarios

**RED Phase**: Create tests for crash recovery

```rust
#[tokio::test]
async fn test_saga_recovers_from_crash_during_forward_phase()
#[tokio::test]
async fn test_saga_recovers_from_crash_during_compensation_phase()
#[tokio::test]
async fn test_saga_recovers_from_multiple_crashes()
#[tokio::test]
async fn test_step_1_completed_step_2_executing_crash()
#[tokio::test]
async fn test_step_3_completed_step_4_executing_crash_compensation_recovers()
#[tokio::test]
async fn test_crash_during_compensation_step_2_of_5()
#[tokio::test]
async fn test_resumed_saga_continues_from_correct_step()
#[tokio::test]
async fn test_no_step_reexecution_after_recovery()
```

**Tests verify**:
- Saga state is persisted and recovered correctly
- Execution resumes from correct position
- No duplicate step execution
- Compensation continues after crash

**GREEN Phase**:
- Implement crash simulation (remove saga from memory, reload from store)
- Verify state persistence across crashes
- Resume execution from recovery point

**REFACTOR Phase**:
- Create crash scenario helpers
- Extract resume/recovery logic

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

### Cycle 8: Complex Multi-Failure Scenarios

**RED Phase**: Create tests for complex failure combinations

```rust
#[tokio::test]
async fn test_multiple_step_failures_in_same_saga()
#[tokio::test]
async fn test_compensation_partial_failure_then_recovery_retry()
#[tokio::test]
async fn test_5_concurrent_sagas_2_fail_3_succeed()
#[tokio::test]
async fn test_cascading_failures_across_subgraphs()
#[tokio::test]
async fn test_timeout_during_forward_phase_triggers_compensation()
#[tokio::test]
async fn test_timeout_during_compensation_phase_records_partial_compensation()
#[tokio::test]
async fn test_network_error_triggers_retry_then_failure()
#[tokio::test]
async fn test_partial_result_data_handling()
```

**Tests verify**:
- Complex failure scenarios are handled correctly
- State transitions are accurate
- Metrics reflect actual behavior
- No data loss or corruption

**GREEN Phase**:
- Implement complex scenario execution
- Track state through multiple failures
- Verify correctness of final state

**REFACTOR Phase**:
- Extract complex scenario builders
- Create reusable failure injection helpers

**CLEANUP Phase**:
- Verify all tests pass
- Run clippy
- Format code

---

## Test File Structure

```
crates/fraiseql-core/tests/federation_saga_e2e_scenarios.rs
├── Module: fixtures
│   ├── test_saga_builder()
│   ├── test_coordinator_wrapper
│   ├── test_executor_harness
│   └── test_store_mock
├── Module: helpers
│   ├── build_test_saga()
│   ├── execute_saga_to_completion()
│   ├── inject_failure_at_step()
│   ├── inject_crash_at_step()
│   └── resume_saga_after_crash()
├── Module: scenarios
│   ├── test_basic_execution (Cycle 1)
│   ├── test_step_failures (Cycle 2)
│   ├── test_compensation (Cycle 3)
│   ├── test_manual_strategy (Cycle 4)
│   ├── test_concurrent (Cycle 5)
│   ├── test_recovery_manager (Cycle 6)
│   ├── test_crash_recovery (Cycle 7)
│   └── test_complex_failures (Cycle 8)
```

---

## Test Statistics Target

| Cycle | Category | Test Count | Purpose |
|-------|----------|-----------|---------|
| 1 | Basic Execution | 8 | Multi-step sagas, concurrency |
| 2 | Failure Detection | 6 | Step failures at various positions |
| 3 | Auto Compensation | 9 | Compensation execution and state |
| 4 | Manual Strategy | 5 | Manual compensation trigger |
| 5 | Concurrent | 6 | Multiple sagas running together |
| 6 | Recovery Manager | 8 | Recovery and cleanup |
| 7 | Crash Recovery | 8 | Persistence and resume |
| 8 | Complex Failures | 8 | Multi-failure scenarios |
| **Total** | | **58 tests** | |

---

## Dependencies

### Must Complete Before This Phase
- [x] Cycle 3: Saga Store (state persistence)
- [x] Cycle 4: Recovery Manager (background recovery)
- [x] Cycle 7: Saga Coordinator (orchestration)
- [x] Cycle 8: Saga Executor (forward execution)
- [x] Cycle 9: Saga Compensator (rollback execution)
- [x] Cycle 11: Observability (tracing instrumentation)

### Blocks
- Phase 13: Performance Benchmarking (needs E2E test suite as baseline)
- Phase 14: Production Deployment (needs comprehensive E2E validation)

---

## Key Design Decisions

### 1. Test Harness Architecture
- **Coordinator**: Real implementation (no mocking)
- **Executor**: Test wrapper that simulates step execution with failure injection
- **Compensator**: Test wrapper that simulates compensation with failure injection
- **Store**: In-memory mock that tracks state transitions
- **Recovery Manager**: Test harness that simulates background loop iterations

**Rationale**: Tests exercise real orchestration logic while controlling underlying operations

### 2. Failure Injection Strategy
- Use builder pattern: `TestScenario::new().with_step_failure_at(2).with_crash_at_step(5)`
- Failures injected at execution time, not at test setup
- Each failure type has dedicated injection point

**Rationale**: Clean separation of scenario setup and failure conditions

### 3. State Verification
- After each major step, verify saga state against expected state
- Use helper: `assert_saga_state(saga_id, SagaState::Executing, 2, 5)`
- Verify step-level state as well as saga-level state

**Rationale**: Catches state machine bugs early

### 4. Concurrency Testing
- Use `tokio::spawn` for concurrent saga execution
- Use `tokio::task::JoinSet` to collect results
- All assertions run after all tasks complete

**Rationale**: True concurrent execution, not pseudo-concurrency

### 5. Recovery Testing
- Simulate recovery by removing saga from memory, then calling recovery manager
- Verify saga is correctly resumed from store
- Verify no step re-execution

**Rationale**: Tests actual recovery path without mocking

---

## Implementation Notes

### For Test Harness Executor
```rust
pub struct TestStepExecutor {
    failure_at_step: Option<u32>,
    failure_error: String,
    duration_ms: u64,
}

impl TestStepExecutor {
    pub fn with_failure_at(mut self, step: u32) -> Self {
        self.failure_at_step = Some(step);
        self
    }

    pub async fn execute_step(&self, step_number: u32, ...) -> StepExecutionResult {
        if Some(step_number) == self.failure_at_step {
            StepExecutionResult {
                success: false,
                error: Some(self.failure_error.clone()),
                ...
            }
        } else {
            // Success path
        }
    }
}
```

### For Crash Simulation
```rust
pub async fn simulate_crash_and_resume(
    coordinator: &SagaCoordinator,
    saga_id: Uuid,
    crash_at_step: u32,
) -> Result<SagaResult> {
    // 1. Start saga execution
    // 2. Stop at crash_at_step
    // 3. Clear saga from active memory (simulate process crash)
    // 4. Call recovery manager iteration
    // 5. Verify saga resumes and completes
}
```

### For Concurrent Saga Testing
```rust
pub async fn execute_concurrent_sagas(
    coordinator: &SagaCoordinator,
    saga_count: usize,
) -> Vec<SagaResult> {
    let mut join_set = tokio::task::JoinSet::new();

    for i in 0..saga_count {
        let coordinator = coordinator.clone();
        join_set.spawn(async move {
            let saga_id = coordinator.create_saga(...).await?;
            coordinator.execute_saga(saga_id).await
        });
    }

    // Collect all results
    let mut results = vec![];
    while let Some(result) = join_set.join_next().await {
        results.push(result??);
    }
    results
}
```

---

## Verification Checklist

### Before Committing

```bash
# Run all saga tests
cargo test federation_saga_e2e_scenarios -- --nocapture

# Verify no regressions in other saga tests
cargo test federation_saga -- --exclude federation_saga_e2e_scenarios

# Full test suite
cargo test --all-features

# Clippy check
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt --check

# Build release
cargo build --release -p fraiseql-core
```

### After Each Cycle

- [ ] All tests in cycle pass
- [ ] Clippy warnings: 0
- [ ] Formatting verified
- [ ] No regressions in existing tests
- [ ] Commit with clear message

### Final Verification (All Cycles Complete)

- [ ] 58 tests created and passing
- [ ] All state machines covered (forward, compensation, recovery)
- [ ] Crash recovery validated
- [ ] Concurrent execution validated
- [ ] Recovery manager integration validated
- [ ] Zero clippy warnings
- [ ] Code properly formatted
- [ ] Documentation updated

---

## Commit Strategy

Commit after each cycle completion:

```
feat(federation): Cycle 12-X - [Cycle Title]

## Changes
- [Summary of tests added]

## Verification
✅ X new tests pass
✅ No regressions
✅ cargo clippy clean
✅ cargo fmt applied

Co-Authored-By: Claude <noreply@anthropic.com>
```

Final commit after all cycles:

```
feat(federation): Cycle 12 Complete - Saga E2E Testing

## Summary
Comprehensive end-to-end testing of saga system:
- 58 new E2E scenario tests
- Multi-step execution validation
- Failure and compensation scenarios
- Concurrent saga handling
- Recovery manager integration
- Crash recovery validation
- Complex multi-failure scenarios

## Statistics
- Tests Created: 58
- Test Coverage: All saga state machines
- Code Quality: ✅ 0 clippy warnings
- Build Status: ✅ Release build passing

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

## Timeline Estimate

| Phase | Duration | Status |
|-------|----------|--------|
| Cycle 1: Basic Execution | 3 hours | Pending |
| Cycle 2: Failure Scenarios | 2 hours | Pending |
| Cycle 3: Auto Compensation | 3 hours | Pending |
| Cycle 4: Manual Strategy | 2 hours | Pending |
| Cycle 5: Concurrent | 3 hours | Pending |
| Cycle 6: Recovery Manager | 3 hours | Pending |
| Cycle 7: Crash Recovery | 3 hours | Pending |
| Cycle 8: Complex Failures | 3 hours | Pending |
| **Total** | **22 hours** | |

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Test harness complexity | High | Start with simple helpers, iterate |
| State assertion explosions | Medium | Use helper functions, not inline assertions |
| Flaky concurrent tests | High | Use deterministic seeding, fixed task counts |
| Recovery testing accuracy | High | Carefully simulate store operations |
| Performance test slowness | Low | Run separate from normal tests with timeout |

---

## Success Metrics

✅ **Phase Complete When:**
1. All 58 tests pass consistently
2. No clippy warnings
3. Code properly formatted
4. All saga state machines covered
5. Multi-step, concurrent, failure, compensation, and recovery scenarios all validated
6. Documentation complete
7. Commits follow project standards

---

## Related Documents

- [Cycle 11: Observability Instrumentation](./phase-11-observability.md)
- [Cycle 10: Recovery Manager](./phase-10-recovery.md)
- [CLAUDE.md - Development Methodology](../.claude/CLAUDE.md)

---

**Status**: Ready for implementation
**Created**: 2026-01-29
**Target**: Complete by 2026-02-02
