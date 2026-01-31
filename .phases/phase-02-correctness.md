# Phase 2: Correctness

**Status**: 🔵 READY TO START
**Objective**: Validate all systems work correctly with unified architecture
**Expected Duration**: 3-4 days

---

## Success Criteria

- [ ] Integration tests cover unified event pipeline
- [ ] SubscriptionManager correctly uses ChangeLogListener
- [ ] All examples work end-to-end
- [ ] Comprehensive E2E test suite passes
- [ ] Error handling validated across all features
- [ ] Federation saga tests pass (from Phase 16 GA)
- [ ] No known bugs in core functionality
- [ ] All tests green

---

## Objective

Phase 1 established the unified architecture and refactored the code. Phase 2 validates that:

1. The unified architecture actually works in practice
2. All features integrate correctly
3. Error handling is comprehensive
4. Examples accurately reflect current behavior
5. E2E flows work as documented

This is the correctness phase - we're making sure the system behaves as designed.

---

## TDD Cycles

### Cycle 1: Subscription Manager Integration Tests

**Objective**: Verify SubscriptionManager correctly integrates with ChangeLogListener

**RED Phase** ✅ COMPLETE
- Write 24 failing tests covering:
  - SubscriptionManager initialization with ChangeLogListener
  - WebSocket subscription lifecycle (connect, query, data flow, disconnect)
  - Multiple concurrent subscriptions
  - Error handling (database unavailable, invalid schema)
  - Subscription filtering and projection
  - Event bridge integration
- Tests properly fail for missing implementations

**GREEN Phase** ✅ COMPLETE
- Implemented EventBridge in fraiseql-server/src/subscriptions/
- Connected ChangeLogListener → EventBridge → SubscriptionManager
- All 24 tests now pass
- Event conversion from EntityEvent to SubscriptionEvent working
- mpsc channel for event routing implemented
- Background task spawning for event processing working

**REFACTOR Phase** ✅ COMPLETE
- Code organized into clean module structure
- EventBridge properly abstracts event routing
- Configuration pattern established for extensibility
- Event conversion logic clean and testable

**CLEANUP Phase** ✅ COMPLETE
- Code formatted with cargo fmt
- No clippy warnings
- Module properly exported in lib.rs
- Commit with clear description of changes

**Status**: ✅ COMPLETE - All 24 tests passing

### Cycle 2: End-to-End Feature Tests

**Objective**: Verify all major features work end-to-end

**RED Phase** ✅ COMPLETE
- Created 32 comprehensive E2E tests covering:
  - Query execution (5 tests): simple fields, variables, nested, aliases, multiple roots
  - Mutations (4 tests): CREATE, UPDATE, DELETE, batch operations
  - Relationships (3 tests): one-to-many, deep nesting, field projection
  - Aggregations (4 tests): COUNT, SUM, AVG, GROUP BY
  - Filtering & Sorting (5 tests): WHERE, ORDER BY, complex filters, relationships
  - Pagination (2 tests): LIMIT/OFFSET, cursor-based
  - Subscriptions (4 tests): CREATE/UPDATE events, concurrent, filtering
  - Error Handling (5 tests): validation, not found, type mismatch, authorization, invalid input
- Created GraphQLQuery and GraphQLResponse mock types
- All 32 tests pass (structure validation phase)

**GREEN Phase**
- Run tests against live database
- Ensure all pathways work
- Add missing database setup if needed
- Verify error cases are handled

**REFACTOR Phase**
- Consolidate test utilities
- Improve test readability
- Extract common setup
- Organize test suite

**CLEANUP Phase**
- Fix any remaining warnings
- Format consistently
- Commit with coverage metrics

**Status**: 🟡 IN PROGRESS - RED phase complete, GREEN phase ready

### Cycle 3: Federation Saga Validation

**Objective**: Verify saga orchestration works across multiple data sources

**RED Phase** ✓
- Write failing tests for:
  - Multi-step saga execution
  - Compensation on failure
  - Partial success handling
  - Deadletter queue (DLQ) for failed sagas
  - Observer notifications on saga completion
  - Tracing context propagation
  - Transaction isolation

**GREEN Phase**
- Validate existing saga implementation
- Test against multiple data sources
- Verify compensation logic
- Check notification system

**REFACTOR Phase**
- Simplify saga state machine
- Improve error recovery
- Optimize observer notification batching
- Clarify compensation order

**CLEANUP Phase**
- Remove debug logging
- Format code
- Commit with details

### Cycle 4: Error Handling Validation

**Objective**: Verify error handling is comprehensive and consistent

**RED Phase** ✓
- Write tests for error paths:
  - Database connection failures
  - Query parse errors
  - Schema validation errors
  - Authorization failures
  - Timeout errors
  - Invalid input (SQL injection attempts, XSS attempts)
  - Network errors (for observers, webhooks)
  - Resource exhaustion (too many subscriptions, large results)
- Verify error responses include:
  - Clear error message
  - Error code/type
  - Request ID for tracing
  - Suggestion for fix (where applicable)

**GREEN Phase**
- Ensure all error cases return proper responses
- Add missing error handlers
- Verify HTTP status codes are correct
- Test error propagation

**REFACTOR Phase**
- Consolidate error types
- Improve error messages
- Add context to errors
- Create error formatting helpers

**CLEANUP Phase**
- Fix warnings
- Format code
- Commit with details

### Cycle 5: Example Validation

**Objective**: Verify all documentation examples work as documented

**RED Phase** ✓
- Identify all code examples in:
  - Foundation documentation (docs/foundation/)
  - Core guides (docs/)
  - API documentation (docs/)
  - README files
- Create test cases for each example
- Verify they currently fail (code paths not tested)

**GREEN Phase**
- Make examples work
- Update outdated examples
- Add missing setup steps
- Verify output matches documentation

**REFACTOR Phase**
- Improve example clarity
- Add better error handling in examples
- Show common patterns
- Document prerequisites

**CLEANUP Phase**
- Fix formatting
- Verify all run without errors
- Commit with list of validated examples

---

## Test Strategy

### Unit Tests
- Location: `crates/*/tests/` and `crates/*/src/tests.rs`
- Coverage: Individual functions and error paths
- Tools: `cargo test --lib`

### Integration Tests
- Location: `crates/fraiseql-server/tests/integration/`
- Coverage: Feature interactions (subscriptions + database + observers)
- Tools: `cargo test --test '*'`

### E2E Tests
- Location: `tests/e2e/`
- Coverage: Full user workflows
- Tools: Docker + test database + real HTTP calls

### Running Tests

```bash
# All tests (unit + integration)
cargo test --all-features

# Specific package
cargo test -p fraiseql-server

# Integration tests only
cargo test --test '*'

# E2E tests (requires Docker)
./tests/e2e/run.sh

# With output
cargo test -- --nocapture --test-threads=1
```

---

## Key Validations

### 1. Unified Event Pipeline

Verify that all events flow through ChangeLogListener:
- Subscriptions use ChangeLogListener
- Observers use ChangeLogListener
- Sagas use ChangeLogListener
- No duplicate event sources

### 2. Database Adapters

Verify adapters work for:
- PostgreSQL (primary)
- MySQL (secondary)
- SQLite (development)
- SQL Server (enterprise)

### 3. Error Recovery

Verify system recovers from:
- Database connection loss
- Network timeouts
- Invalid inputs
- Resource exhaustion

### 4. Performance Baselines

Establish baseline metrics:
- Query latency: < 50ms p95
- Subscription response: < 100ms
- Saga execution: < 300ms
- Memory per connection: < 1MB

---

## Known Issues to Validate

### From Phase A Postmortem
- ✓ PostgresListener was wrong (reverted)
- ✓ ChangeLogListener is correct (validate integration)
- ✓ Observer framework is mature (test thoroughly)

### From Phase 16 GA Audit
- ⚠️ Protobuf dependency needs upgrade (critical)
- ⚠️ 18 clippy warnings (non-blocking but clean)
- ⚠️ 2 format issues (cosmetic)

---

## Dependencies

- **Requires**: Phase 1 (Foundation) - ✅ COMPLETE
- **Blocks**: Phase 3 (Performance Optimization)

---

## Files to Update

### New Test Files
- `crates/fraiseql-server/tests/integration/subscriptions_integration.rs` ✨
- `crates/fraiseql-server/tests/integration/federation_sagas.rs` ✨
- `tests/e2e/full_workflow.rs` ✨
- `tests/e2e/error_handling.rs` ✨

### Updated Files
- `crates/fraiseql-core/src/lib.rs` (documentation)
- `crates/fraiseql-server/src/subscriptions.rs` (if integration issues found)
- Examples in `docs/` and `docs/foundation/` (as needed)

### Documentation
- `.phases/phase-02-correctness.md` (this file)
- Test results summary in commit

---

## Definition of Done

Phase 2 is complete when:

1. ✅ All integration tests pass
2. ✅ All E2E tests pass
3. ✅ All examples verified working
4. ✅ No new clippy warnings introduced
5. ✅ Code formatted cleanly
6. ✅ Coverage increased or maintained
7. ✅ Commit message documents test additions

---

## Next Phase

**Phase 3: Performance Optimization** focuses on:
- Establishing performance baselines
- Query optimization
- Connection pooling tuning
- Caching improvements

See `.phases/phase-03-performance.md` for details.

---

## Notes

- Don't add new features during this phase - only validate correctness
- Focus on testing existing behavior, not implementing new behavior
- Use the tests to document expected behavior
- If bugs are found, fix them with RED/GREEN/REFACTOR/CLEANUP
- Performance issues are noted but fixed in Phase 3

---

## Quick Start

```bash
# 1. Read this file
cat .phases/phase-02-correctness.md

# 2. Start Cycle 1: Subscriptions
# Write failing tests first (RED)
cargo test -- --nocapture

# 3. Implement to pass (GREEN)
# Make minimal code changes

# 4. Improve design (REFACTOR)
cargo clippy --all-targets

# 5. Clean up (CLEANUP)
cargo fmt --all

# 6. Commit
git add .
git commit -m "test(subscriptions): Add integration tests for ChangeLogListener

## Changes
- Added SubscriptionManager integration tests
- Tested WebSocket lifecycle
- Verified event forwarding from ChangeLogListener

## Verification
✅ 20 new tests pass
✅ No clippy warnings
✅ Code formatted
"

# 7. Move to next cycle
```

---

**Phase 2 is ready to start.**

Begin with Cycle 1: Write failing subscription integration tests.
