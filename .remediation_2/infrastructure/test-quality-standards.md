# Test Quality Standards

## Purpose

This document establishes binding rules for all new tests added to FraiseQL.
It is written in response to two concrete issues found in the codebase:

1. `std::thread::sleep` inside `#[tokio::test]` blocks (TS-1: 1.1 s blocking
   sleep in async tests; stalls the tokio executor)
2. Five components calling `SystemTime::now()` inline with no clock injection,
   making deterministic time-based testing impossible (CK-1 through CK-5)

These rules apply to all PRs from this point forward.

---

## Rule 1: No `std::thread::sleep` in Async Tests

**Severity**: Error (PR will not be merged)

```
FORBIDDEN in any function annotated #[tokio::test]:
    std::thread::sleep(...)
    std::thread::sleep_ms(...)
```

### Why

`std::thread::sleep` blocks the OS thread. In a `#[tokio::test]` context with
a single-threaded executor (the default), this stalls the entire async runtime.
No other tasks run during the sleep. This:
- Makes tests slower than necessary
- Can mask deadlocks (a 1-second sleep looks identical to a 1-second hang)
- Makes it impossible to use `tokio::time::pause()` for deterministic control

### What to use instead

For a real delay in an async test (rare — see Rule 2 before accepting this):
```rust
tokio::time::sleep(Duration::from_millis(100)).await;
```

For time-dependent logic (TTL expiry, window rollover):
```rust
// Use ManualClock from fraiseql-test-utils (after Batch 2 is complete)
#[tokio::test(start_paused = true)]
async fn test_ttl_expiry() {
    let clock = Arc::new(ManualClock::new());
    let component = MyComponent::new_with_clock(clock.clone());
    clock.advance(Duration::from_secs(61));
    assert!(component.is_expired());
}
```

### Clippy lint

Add to `Cargo.toml` (workspace lints):

```toml
# This is not a built-in Clippy lint; enforce via CI script:
# grep -rn "std::thread::sleep" inside test functions annotated
# with #[tokio::test] — see security-review-gate.md CI checks.
```

Until a Clippy lint is available, enforce via code review and the CI lint
script in `security-review-gate.md`.

---

## Rule 2: Real-Time Delays in Tests Require Justification

**Severity**: Warning (requires reviewer comment)

Any test that includes a `tokio::time::sleep(...)` (the async version —
acceptable) must include a comment explaining why the delay is necessary
and why clock injection cannot replace it:

```rust
// JUSTIFICATION: This tests the actual I/O retry behavior of the
// PostgreSQL connection pool under load. Clock injection cannot simulate
// the real network timeout path. Delay is 50ms which is within
// CI time budget for this test group.
tokio::time::sleep(Duration::from_millis(50)).await;
```

Tests that sleep for longer than 500 ms require explicit reviewer approval.
Tests that sleep for longer than 5 s must be marked `#[ignore]` and moved
to a dedicated slow-test CI job.

---

## Rule 3: Time-Dependent Logic Must Accept a Clock

**Severity**: Error for new code; tracked as CK-1 through CK-5 for existing code

Any new struct or function whose behavior depends on the current time must
accept a `Clock` parameter:

```rust
// FORBIDDEN for new code:
pub struct TokenCache {
    entries: HashMap<String, CachedToken>,
}
impl TokenCache {
    pub fn is_expired(&self, key: &str) -> bool {
        // Direct wall-clock call — cannot be unit-tested
        let now = SystemTime::now();
        ...
    }
}

// REQUIRED:
pub struct TokenCache<C: Clock = SystemClock> {
    entries: HashMap<String, CachedToken>,
    clock: C,
}
impl<C: Clock> TokenCache<C> {
    pub fn is_expired(&self, key: &str) -> bool {
        let now = self.clock.now();
        ...
    }
}
```

The `Clock` trait and `ManualClock` implementation are in
`fraiseql-core::utils::clock` (added in Batch 2) and re-exported from
`fraiseql-test-utils`.

**Exception**: Free functions that call `SystemTime::now()` for logging,
audit timestamps, or other non-testable side effects are allowed. The rule
applies to logic that affects observable test output (cache hits, TTL expiry,
rate limit windows, etc.).

---

## Rule 4: Every Security-Sensitive Code Path Needs a Negative Test

**Severity**: Warning (reviewer must explicitly accept if missing)

For any code that enforces security invariants (auth, scope checks, rate
limiting, signature verification), the test suite must include at least one
test that demonstrates the **rejection path**:

```rust
// Positive test (allowed path) — necessary but not sufficient:
#[test]
fn valid_signature_is_accepted() { ... }

// Negative test (rejection path) — required:
#[test]
fn invalid_signature_is_rejected() { ... }

#[test]
fn forged_signature_is_rejected() { ... }
```

The negative test must verify:
1. The operation returns `Err` (not `Ok`)
2. The error variant is the expected one (not a random error)

---

## Rule 5: Integration Tests Must Not Share Global State

**Severity**: Error

Integration tests that modify global state (database rows, in-memory
registries, static configuration) must clean up after themselves, either
via RAII guards or by using isolated fixtures.

```rust
// FORBIDDEN:
static GLOBAL_COUNTER: AtomicU32 = AtomicU32::new(0);

#[tokio::test]
async fn test_a() {
    GLOBAL_COUNTER.store(5, Ordering::SeqCst);
    // ... test logic ...
    // NO cleanup — test_b may see stale state
}

// REQUIRED: each test creates its own isolated instance
#[tokio::test]
async fn test_a() {
    let counter = Arc::new(AtomicU32::new(0));
    counter.store(5, Ordering::SeqCst);
    // Dropped at end of scope
}
```

Database integration tests must use either:
- Separate schemas per test (e.g., `CREATE SCHEMA test_{uuid}; DROP SCHEMA ... CASCADE` on cleanup)
- Transactional test fixtures that roll back after each test
- `testcontainers` with a fresh container per test file

---

## Rule 6: Snapshot Tests for SQL Generation

Any new database operation that generates SQL must have a snapshot test.
Use `insta` (already a dependency). Name the snapshot with the pattern:
`sql_snapshots__{database}_{feature}__{test_name}`.

```rust
#[test]
fn new_window_function_generates_correct_postgres_sql() {
    let sql = generate_window_sql(/* ... */);
    insta::assert_snapshot!("sql_snapshots__postgres_window__new_function", sql);
}
```

Run `cargo insta review` after adding snapshot tests to commit the initial
expected values.

---

## Enforcement Summary

| Rule | Enforcement | PR impact |
|------|-------------|-----------|
| No `std::thread::sleep` in tokio tests | CI lint script | Block merge |
| Real sleeps ≥ 500 ms need justification | Code review | Reviewer comment required |
| Real sleeps ≥ 5 s need `#[ignore]` | Code review | Reviewer comment required |
| Time-dependent logic accepts Clock | Code review + clippy (future) | Block merge for new code |
| Security paths need negative tests | Code review | Reviewer explicit approval |
| No shared global state in tests | Code review | Block merge |
| New SQL paths need snapshot tests | Code review | Block merge |
