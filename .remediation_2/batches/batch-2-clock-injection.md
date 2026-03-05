# Batch 2 — Clock Injection

## Problem

Five components in `fraiseql-core` and `fraiseql-auth` call `SystemTime::now()`
directly, making their time-dependent logic untestable without real wall-clock
delays. This produces two concrete problems:

1. **Tests must sleep** to exercise TTL expiry, window rollover, or cache staleness.
   See Batch 1 for the consequences.
2. **Bugs in window/expiry logic cannot be caught deterministically.** A test
   that sleeps 1.1 seconds to cross a 1-second TTL will pass on most machines
   and fail on any machine where that sleep undershoots.

## Design: `Clock` Trait

```rust
// crates/fraiseql-core/src/utils/clock.rs  (new file)

use std::time::{Duration, SystemTime};

/// Abstraction over the system clock. Inject this into any component
/// that needs time-based logic, enabling deterministic testing.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> SystemTime;
}

/// Production implementation: delegates to `SystemTime::now()`.
#[derive(Debug, Clone, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[inline]
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Test implementation: manually advanceable, starts at UNIX_EPOCH + 1000s
/// to avoid edge cases near epoch.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
pub struct ManualClock {
    current: std::sync::Arc<std::sync::Mutex<SystemTime>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl ManualClock {
    pub fn new() -> Self {
        Self {
            current: std::sync::Arc::new(std::sync::Mutex::new(
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000),
            )),
        }
    }

    /// Advance the clock by `delta`. All callers sharing this `Arc` see
    /// the new time immediately.
    pub fn advance(&self, delta: Duration) {
        *self.current.lock().expect("ManualClock poisoned") += delta;
    }

    /// Set the clock to an absolute time.
    pub fn set(&self, t: SystemTime) {
        *self.current.lock().expect("ManualClock poisoned") = t;
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Clock for ManualClock {
    fn now(&self) -> SystemTime {
        *self.current.lock().expect("ManualClock poisoned")
    }
}
```

Export from `fraiseql-core` prelude and from `fraiseql-test-utils`.

---

## Rollout Plan

### CK-1 — rate_limiting.rs

**File**: `crates/fraiseql-core/src/validation/rate_limiting.rs:105`

**Before**:
```rust
fn current_window(window_secs: u64) -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / window_secs
}
```

**After** — inject clock into the rate limiter struct:
```rust
pub struct RateLimiter<C: Clock = SystemClock> {
    // existing fields ...
    clock: C,
}

impl<C: Clock> RateLimiter<C> {
    pub fn new_with_clock(config: RateLimiterConfig, clock: C) -> Self { ... }

    fn current_window(&self, window_secs: u64) -> u64 {
        self.clock.now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / window_secs
    }
}

// Convenience constructor for production:
impl RateLimiter<SystemClock> {
    pub fn new(config: RateLimiterConfig) -> Self {
        Self::new_with_clock(config, SystemClock)
    }
}
```

**New test (replaces TS-5)**:
```rust
#[test]
fn test_window_rollover_does_not_leak_across_windows() {
    let clock = ManualClock::new();
    let mut limiter = RateLimiter::new_with_clock(
        RateLimiterConfig { max_requests: 2, window_secs: 60 },
        clock.clone(),
    );

    assert!(limiter.check("user1").is_ok()); // request 1
    assert!(limiter.check("user1").is_ok()); // request 2
    assert!(limiter.check("user1").is_err()); // over limit

    clock.advance(Duration::from_secs(61)); // advance past window

    assert!(limiter.check("user1").is_ok()); // new window, limit reset
}
```

---

### CK-2 — cache/result.rs

**File**: `crates/fraiseql-core/src/cache/result.rs:593`

**Strategy**: Replace `current_time_secs()` free function with a
`Clock`-parameterised method on `QueryResultCache`.

The cache already stores `u64` timestamps for TTL; the clock must produce
the same unit. Add a helper to `Clock`:
```rust
// default method on Clock trait:
fn now_secs(&self) -> u64 {
    self.now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
```

Then `QueryResultCache<C: Clock>` stores `clock: Arc<C>` and calls
`self.clock.now_secs()` instead of the free function.

---

### CK-3 — security/rls_policy.rs

**File**: `crates/fraiseql-core/src/security/rls_policy.rs:308,321,339`

Three `SystemTime::now()` sites manage policy cache expiry. Apply same
pattern as CK-2: add `clock: Arc<dyn Clock>` to `RlsPolicyCache`.

---

### CK-4 — fraiseql-auth/src/pkce.rs

**File**: PKCE state store

After CK-1 through CK-3 are done, add `clock: Arc<dyn Clock>` to the PKCE
`StateStore`. Then update the tests to use `ManualClock` + `start_paused = true`
(TS-1 final fix).

---

### CK-5 — security/kms/base.rs

**File**: `crates/fraiseql-core/src/security/kms/base.rs:19`

The KMS `current_timestamp_secs()` is used for key rotation scheduling.
Add `Clock` injection to `KeyManagementService`. This enables testing key
rotation without setting up real wall-clock timers.

---

## Migration Path for Existing Call Sites

Any struct that currently calls `SystemTime::now()` inline should:

1. Add `clock: Arc<dyn Clock>` (or generic `C: Clock`) to its fields.
2. Replace all `SystemTime::now()` with `self.clock.now()`.
3. Provide a `::new()` that uses `SystemClock` (backwards-compatible).
4. Provide a `::new_with_clock(clock: C)` for test use.

The `Arc<dyn Clock>` vs. `C: Clock` generic tradeoff:
- Generic (`C: Clock`): zero-cost, preferred for hot paths (cache, executor).
- `Arc<dyn Clock>`: simpler for structs that are already heap-allocated and
  not in hot paths (PKCE store, KMS).

---

## Verification Checklist

- [ ] `grep -rn "SystemTime::now()" crates/*/src/ --include="*.rs"` returns
      zero results outside `clock.rs` itself
- [ ] `cargo nextest run -p fraiseql-core` contains no `sleep` in test output
- [ ] `cargo nextest run -p fraiseql-auth` completes in under 5 seconds
- [ ] All 5 rate limiter time tests now use `ManualClock` and take < 1 ms each
- [ ] `ManualClock` and `SystemClock` are exported from `fraiseql-test-utils`
