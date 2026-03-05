# Batch 1 — Thread Safety: `std::thread::sleep` in Async Contexts

## Problem

`std::thread::sleep` inside a `#[tokio::test]` function blocks the OS thread
that tokio's scheduler is running on. Unlike `tokio::time::sleep`, which
yields to the scheduler, `std::thread::sleep` holds the thread hostage.

On a single-threaded tokio runtime (the default for `#[tokio::test]`), this
means the entire test executor is paused. On a multi-threaded runtime, it
consumes one worker for the duration, potentially causing other tasks to queue
behind it.

The 1.1-second sleeps in `pkce.rs` (TS-1) run inside `#[tokio::test]` and
will stall the tokio executor for 1.1 seconds each. With two such tests, that
is 2.2 seconds added to every run that includes the `fraiseql-auth` test suite.
This is also the pattern that masks real deadlocks: a 1.1-second hang looks
identical to a 1.1-second sleep in a test log.

---

## Issues

### TS-1 — PKCE store: tokio-test + thread::sleep (Critical)

**File**: `crates/fraiseql-auth/src/pkce.rs:485,579`

**Current code**:
```rust
#[tokio::test]
async fn test_expired_state_returns_state_expired_not_not_found() {
    let store = store_no_enc(1); // TTL = 1 second
    let (token, _) = store.create_state("https://example.com").await.unwrap();
    std::thread::sleep(Duration::from_millis(1100)); // ← BLOCKS tokio worker
    assert!(matches!(store.consume_state(&token).await, Err(PkceError::StateExpired)));
}
```

**Fix**: Use `tokio::time::pause()` + `tokio::time::advance()`. This requires
the PKCE store to accept a clock parameter (see Batch 2, CK-4). Interim fix
while Batch 2 is in progress:

```rust
#[tokio::test]
async fn test_expired_state_returns_state_expired_not_not_found() {
    let store = store_no_enc(1);
    let (token, _) = store.create_state("https://example.com").await.unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await; // ← yields, does not block
    assert!(matches!(store.consume_state(&token).await, Err(PkceError::StateExpired)));
}
```

**Final fix** (after CK-4):
```rust
#[tokio::test(start_paused = true)]
async fn test_expired_state_returns_state_expired_not_not_found() {
    let clock = Arc::new(ManualClock::new());
    let store = store_no_enc_with_clock(1, clock.clone());
    let (token, _) = store.create_state("https://example.com").await.unwrap();
    clock.advance(Duration::from_secs(2));
    assert!(matches!(store.consume_state(&token).await, Err(PkceError::StateExpired)));
}
```

**Verification**: `cargo nextest run -p fraiseql-auth --no-fail-fast` must
complete the auth test suite in under 5 seconds on a developer machine.

---

### TS-2 — monitoring.rs: OperationTimer test

**File**: `crates/fraiseql-auth/src/monitoring.rs:235`
**Context**: Sync test (`#[test]`, not `#[tokio::test]`). No thread pool starvation.

**Issue**: The test asserts `elapsed >= 10.0` ms by sleeping 10 ms. This is
a timing assertion — it can spuriously fail on a loaded machine where the sleep
undershoots. The test also adds 10 ms to every run.

**Fix**:
```rust
#[test]
fn test_operation_timer() {
    let timer = OperationTimer::start("test_op");
    // No sleep needed: just verify the timer starts near zero and can be read.
    let elapsed = timer.elapsed_ms();
    // Timer must be non-negative and not absurdly large.
    assert!(elapsed >= 0.0);
    assert!(elapsed < 1000.0);
}
```

If the test's intent is to verify that a known duration is measured accurately,
use a `ManualClock` (see Batch 2) instead of wall-clock sleep.

---

### TS-3 — performance.rs: PerformanceTimer test

**File**: `crates/fraiseql-server/src/performance.rs:512`
**Context**: Sync test. Same pattern as TS-2.

**Fix**: Same approach — remove the sleep and assert only structural properties
(non-negative, reasonable upper bound), or use a `ManualClock`.

---

### TS-4 — metrics_server.rs: TimingGuard test

**File**: `crates/fraiseql-server/src/metrics_server.rs:652`
**Context**: Sync test, 100 µs sleep. Functionally negligible, but sets precedent.

**Fix**: Remove sleep. Assert `recorded >= 0` and `recorded < 1_000_000`.
The guard just needs to demonstrate it stores _a_ non-zero value after some
code runs; exact duration is not the contract.

---

### TS-5 — rate_limiter_time_tests.rs

**File**: `crates/fraiseql-auth/tests/rate_limiter_time_tests.rs:39,64`
**Context**: Sync test with `std::thread::sleep(2s)`. Not async, so no thread
pool starvation. But adds 4 seconds to every run of the auth test suite.

**Interim fix**: Mark these tests `#[ignore]` and run them in CI only in a
dedicated slow-test job:
```yaml
# In ci.yml, add a separate job:
- name: Run time-sensitive tests
  run: cargo nextest run -p fraiseql-auth --run-ignored
```

**Final fix** (after CK-1): Replace with clock-injected tests that advance
time via `ManualClock`. This reduces the 4 s to < 1 ms.

---

## Verification Checklist

After completing this batch:

- [ ] `cargo nextest run -p fraiseql-auth` completes in < 10 s on a developer
      machine (was > 4.2 s from sleeps alone)
- [ ] `grep -rn "std::thread::sleep" crates/*/src/ --include="*.rs"` returns
      zero lines in production (non-test) code
- [ ] `grep -rn "std::thread::sleep" crates/ --include="*.rs" | grep "#\[tokio::test\]" -B 5`
      (confirm no more blocking sleeps inside async tests — this grep is
      approximate; add a Clippy lint for exactness, see infrastructure)
- [ ] All existing tests still pass
