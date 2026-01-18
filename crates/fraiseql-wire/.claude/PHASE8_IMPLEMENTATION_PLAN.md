# Phase 8: Lightweight State Machine Implementation Plan

## Objective

Reduce startup overhead by ~0.5-1ms by using a lightweight `Arc<AtomicU8>` for stream state tracking instead of `Arc<Mutex<StreamState>>` until pause/resume is actually needed.

## Current Architecture (Phase 6)

```rust
pub struct JsonStream {
    pause_resume: Option<PauseResumeState>,  // Lazy-allocated
}

pub struct PauseResumeState {
    state: Arc<Mutex<StreamState>>,  // Full Mutex (expensive)
    pause_signal: Arc<Notify>,
    resume_signal: Arc<Notify>,
    paused_occupancy: Arc<AtomicUsize>,
    pause_timeout: Option<Duration>,
}
```

**Current behavior**:

- `pause_resume` is `None` until `pause()` called
- When `pause()` is called, all Arc allocations happen at once
- Even though only ~3% of queries call `pause()`

## Phase 8 Optimization

### New Architecture

```rust
// Lightweight state tracking (allocated ALWAYS, but cheap)
pub struct JsonStream {
    state_atomic: Arc<AtomicU8>,      // 0=Running, 1=Paused, 2=Complete
    pause_resume: Option<PauseResumeState>,  // Still lazy
}

// Only allocated on first pause() call
pub struct PauseResumeState {
    state: Arc<Mutex<StreamState>>,
    pause_signal: Arc<Notify>,
    resume_signal: Arc<Notify>,
    paused_occupancy: Arc<AtomicUsize>,
    pause_timeout: Option<Duration>,
}
```

### State Values

```rust
const STATE_RUNNING: u8 = 0;    // Stream is active
const STATE_PAUSED: u8 = 1;     // Stream is paused
const STATE_COMPLETE: u8 = 2;   // Stream is complete
const STATE_ERROR: u8 = 3;      // Stream encountered error
```

### Implementation Strategy

#### 1. Define State Constants

File: `src/stream/json_stream.rs`

Add at top of module:

```rust
const STATE_RUNNING: u8 = 0;
const STATE_PAUSED: u8 = 1;
const STATE_COMPLETE: u8 = 2;
const STATE_ERROR: u8 = 3;
```

#### 2. Add Atomic State to JsonStream

File: `src/stream/json_stream.rs`

Add to `pub struct JsonStream`:

```rust
/// Lightweight state tracking (AtomicU8)
/// Used for queries that never pause (97% of cases)
/// Values: 0=Running, 1=Paused, 2=Complete, 3=Error
state_atomic: Arc<AtomicU8>,
```

#### 3. Update JsonStream::new()

File: `src/stream/json_stream.rs`

In the constructor, add:

```rust
let state_atomic = Arc::new(AtomicU8::new(STATE_RUNNING));

JsonStream {
    state_atomic,
    // ... rest of fields ...
    pause_resume: None,
}
```

#### 4. Create Helper Methods

File: `src/stream/json_stream.rs`

```rust
impl JsonStream {
    /// Get current state (fast path - uses AtomicU8)
    pub(crate) fn state_atomic_get(&self) -> u8 {
        self.state_atomic.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Set state to paused (called when pause() invoked)
    pub(crate) fn state_atomic_set_paused(&self) {
        self.state_atomic.store(
            STATE_PAUSED,
            std::sync::atomic::Ordering::Release,
        );
    }

    /// Set state to complete
    pub(crate) fn state_atomic_set_complete(&self) {
        self.state_atomic.store(
            STATE_COMPLETE,
            std::sync::atomic::Ordering::Release,
        );
    }

    /// Check if stream is paused (fast check)
    pub(crate) fn is_paused_atomic(&self) -> bool {
        self.state_atomic_get() == STATE_PAUSED
    }

    /// Check if stream is complete
    pub(crate) fn is_complete_atomic(&self) -> bool {
        self.state_atomic_get() == STATE_COMPLETE
    }

    /// Clone atomic state for background task
    pub(crate) fn clone_state_atomic(&self) -> Arc<AtomicU8> {
        Arc::clone(&self.state_atomic)
    }
}
```

#### 5. Update Background Task

File: `src/connection/conn.rs`

Replace state checks with atomic checks where possible:

```rust
// OLD (Phase 6):
if let (Some(ref state_lock), Some(ref pause_signal), ...) =
    (&state_lock, &pause_signal, ...)
{ ... }

// NEW (Phase 8):
// Fast path: check atomic state first
let current_state = state_atomic.load(Ordering::Acquire);
if current_state == STATE_PAUSED {
    // Need to upgrade to full pause/resume handling
    if let (Some(ref state_lock), ...) = (&state_lock, ...) {
        // Use Mutex for pause/resume
    }
}
```

#### 6. Update Pause/Resume Methods

File: `src/stream/json_stream.rs`

In `pause()` method:

```rust
pub async fn pause(&mut self) -> Result<()> {
    // Fast path: mark as paused using atomic
    self.state_atomic_set_paused();

    // Ensure full pause/resume infrastructure exists
    let pause_resume = self.ensure_pause_resume();

    // Now use full Mutex-based state machine
    let mut state = pause_resume.state.lock().await;
    // ... rest of pause logic ...
}
```

In `resume()` method - similar pattern

#### 7. Update Error Handling

File: `src/stream/json_stream.rs`, `src/connection/conn.rs`

When error occurs, set state using atomic:

```rust
match result {
    Ok(item) => {
        // Process item
    }
    Err(e) => {
        // Use atomic for error state
        self.state_atomic.store(STATE_ERROR, Ordering::Release);
        return Err(e);
    }
}
```

## Testing Strategy

### Unit Tests

1. Verify state transitions work with AtomicU8
2. Test pause/resume still works (upgrades to Mutex)
3. Test error state marking
4. Verify thread-safe state transitions

### Existing Tests

- All 158 existing tests should still pass
- No API changes (internal optimization only)
- Pause/resume behavior must be identical

### New Tests (if needed)

```rust
#[test]
fn test_atomic_state_transitions() {
    // Verify AtomicU8 state changes work
}

#[test]
fn test_pause_upgrade_to_mutex() {
    // Verify atomic state transitions to full Mutex on pause()
}
```

## Implementation Details

### Atomic Ordering

Use appropriate memory ordering:

- **`Acquire`** on reads: Ensure we see all previous writes
- **`Release`** on writes: Ensure all our writes are visible before next operation
- This is safe for state machine transitions

```rust
state_atomic.load(std::sync::atomic::Ordering::Acquire)
state_atomic.store(value, std::sync::atomic::Ordering::Release)
```

### Memory Cost

```
BEFORE (Phase 6):
├─ JsonStream with Option<PauseResumeState>  → ~500 bytes when None
├─ When pause() called:                        → +200 bytes (Mutex + Notify)

AFTER (Phase 8):
├─ JsonStream with Arc<AtomicU8>              → +8 bytes (Arc pointer)
├─ When pause() called:                        → +192 bytes (Mutex allocation moved to later)
├─ Net cost for non-pause queries:            → +8 bytes always
└─ Savings in non-pause critical path:        → 1-2ms (fewer allocations during startup)
```

### Performance Impact

```
Expected improvements:
├─ 1K rows:   ~37ms → ~36.5ms  (-1.5% = ~0.5ms)
├─ 10K rows:  ~52ms → ~51.5ms  (-1% = ~0.5ms)
├─ 50K rows:  ~121.5ms (no change, allocation is small % of total)
└─ 100K rows: ~209ms (no change)

Total Phase 8 impact: ~0.5ms on small result sets
```

## Risk Assessment

**Low Risk**:

- AtomicU8 is simple and well-understood
- Pause/resume behavior unchanged (still uses Mutex when needed)
- All existing tests should pass
- No API changes
- Backward compatible

**Potential Issues**:

- Memory ordering bugs (mitigated by using Acquire/Release)
- State transition race conditions (mitigated by careful ordering)
- Interactions with pause/resume (handled by ensure_pause_resume())

**Mitigation**:

- Run all 158 tests
- Verify with benchmarks
- Code review for atomic ordering correctness

## Files to Modify

1. **`src/stream/json_stream.rs`**
   - Add state constants
   - Add `state_atomic: Arc<AtomicU8>` to JsonStream
   - Add helper methods (state_atomic_get, is_paused_atomic, etc.)
   - Update pause/resume methods
   - Initialize atomic state in constructor

2. **`src/connection/conn.rs`**
   - Update background task state checking
   - Use fast atomic path first, fall back to Mutex if needed
   - Minimize Mutex lock scope

## Implementation Complexity

**Estimated effort**: 2-3 hours
**Lines of code**: ~50-100 lines added/modified
**Risk level**: Low
**Test coverage**: High (existing tests cover most scenarios)

## Success Criteria

✅ All 158 tests pass
✅ Benchmarks show 0.5-1ms improvement on small result sets
✅ No regression on large result sets
✅ Pause/resume still works identically
✅ Memory usage same or less
✅ Code is clean and well-commented

## Rollback Plan

If Phase 8 causes issues:

1. Remove `state_atomic` field and all atomic state methods
2. Revert to Phase 6 (all prior commits intact)
3. Single commit to roll back (~10 minutes)

---

## Next Steps

1. Implement changes to `src/stream/json_stream.rs`
2. Implement changes to `src/connection/conn.rs`
3. Run tests: `cargo test --lib`
4. Verify no regressions
5. Run benchmarks: `cargo bench --bench phase6_validation`
6. Compare before/after results
7. Commit with clear message

Ready to proceed with Phase 8 implementation?
