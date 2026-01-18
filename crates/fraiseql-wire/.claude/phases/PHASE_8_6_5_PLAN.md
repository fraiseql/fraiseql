# Phase 8.6.5: Stream Pause/Resume — Implementation Plan

**Status**: Ready for implementation
**Date**: 2026-01-13
**Dependencies**: Phases 8.6.1 through 8.6.4 ✅ Complete

---

## Objective

Implement **idempotent pause/resume semantics** for streams, enabling explicit user control over query execution.

When paused:

- Suspend background task (Postgres stops reading)
- Keep connection alive
- Stop yielding items from the stream
- Preserve buffered rows (no loss)
- Update metrics to reflect paused state

When resumed:

- Restart background task
- Continue from where it left off
- Respect occupancy patterns across pause boundary
- Maintain adaptive chunking state

Idempotence: `pause().pause()` is safe, `resume().resume()` is safe, `resume()` before `pause()` is a no-op.

---

## Architecture Overview

### Control Plane Addition

Current (Phases 8.6.1-8.6.4):

```
Automatic constraints:
  - Memory bounds (hard ceiling, soft warnings)
  - Adaptive chunking (self-tuning based on occupancy)
  - Occupancy metrics (backpressure visibility)
```

New (Phase 8.6.5):

```
Manual control plane:
  - pause() → suspend background task
  - resume() → restart background task
  - Complements adaptive chunking + memory bounds
```

### Implementation Strategy: Option B (Real Pause)

**What we're NOT doing**: "Stop yielding but keep producing"

- ❌ Consumer stops polling
- ❌ Background task keeps reading/buffering
- ❌ Memory can grow unbounded (violates hard ceiling)
- ❌ Defeats the purpose of pause

**What we ARE doing**: "Suspend background task entirely"

- ✅ Consumer stops polling
- ✅ Background task stops reading
- ✅ Memory bounded by current channel occupancy
- ✅ Connection stays open (no reconnect on resume)
- ✅ True pause semantics

---

## Design Details

### JsonStream Additions

#### New State Machine

```rust
enum StreamState {
    Running,      // Background task is reading
    Paused,       // Background task is blocked/suspended
    Completed,    // Query completed (one-way terminal)
    Failed(Error) // Query failed (one-way terminal)
}
```

#### New Fields on JsonStream

```rust
pub struct JsonStream<T> {
    // Existing fields
    rx: mpsc::Receiver<Result<T, StreamError>>,
    // ... other fields ...

    // New fields
    state: Arc<Mutex<StreamState>>,           // Current state
    pause_signal: Arc<Notify>,                 // Signal to pause
    resume_signal: Arc<Notify>,                // Signal to resume
    paused_occupancy: Arc<AtomicUsize>,       // Buffered rows when paused
}
```

#### New Public Methods

```rust
impl<T> JsonStream<T> {
    /// Pause the stream. Idempotent.
    ///
    /// When called:
    /// - Sets state to Paused
    /// - Background task will stop reading on next iteration
    /// - Memory usage stabilizes at current buffered rows
    /// - Calling again is a no-op
    pub async fn pause(&mut self) -> Result<(), JsonStreamError> {
        // Implementation
    }

    /// Resume the stream. Idempotent.
    ///
    /// When called:
    /// - Sets state to Running
    /// - Background task wakes up and continues reading
    /// - Stream yields items normally
    /// - Calling before pause() is a no-op
    pub async fn resume(&mut self) -> Result<(), JsonStreamError> {
        // Implementation
    }

    /// Get current state (Running, Paused, Completed, Failed)
    pub fn state(&self) -> StreamState {
        // Implementation
    }

    /// Get buffered rows while paused (for diagnostics)
    pub fn paused_occupancy(&self) -> usize {
        // Implementation
    }
}
```

### Background Task Changes

#### Integration Point: Connection Layer (`src/connection/conn.rs`)

Current flow:

```rust
loop {
    // 1. Read rows from Postgres
    let rows = read_row_chunk();

    // 2. Observe occupancy (for adaptive chunking)
    if let Some(new_size) = adaptive.observe(...) {
        // Adjust
    }

    // 3. Send to MPSC channel
    sender.send(chunk).await?;

    // 4. Check for query end
    if is_complete { break; }
}
```

New flow with pause/resume:

```rust
loop {
    // Check pause signal (non-blocking)
    if should_pause.load(...) {
        // Enter pause mode
        state.store(Paused);
        // Block until resume signal
        resume_signal.notified().await;
        state.store(Running);
        // Resume reading from where we left off
    }

    // 1. Read rows from Postgres
    let rows = read_row_chunk();

    // 2. Observe occupancy (for adaptive chunking)
    if let Some(new_size) = adaptive.observe(...) {
        // Adjust
    }

    // 3. Send to MPSC channel
    sender.send(chunk).await?;

    // 4. Check for query end
    if is_complete { break; }
}
```

#### Dropping Stream While Paused

When `JsonStream` is dropped while paused:

1. Background task sees channel closed (receiver dropped)
2. Task exits cleanly (no resources leak)
3. No hanging connections

---

## Implementation Steps

### Step 1: Extend JsonStream Type (src/stream/mod.rs)

**Files**: `src/stream/mod.rs`

Changes:

- Import `Arc`, `Mutex`, `Notify` from tokio/std
- Add new fields to `JsonStream<T>` struct
- Implement `state()`, `paused_occupancy()` getters
- Add `StreamState` enum to pub API

Test:

```bash
cargo test --lib stream
# Verify struct compiles and fields are accessible
```

### Step 2: Implement Pause Logic (src/stream/mod.rs)

**Files**: `src/stream/mod.rs`

The `pause()` method:

```rust
pub async fn pause(&mut self) -> Result<(), JsonStreamError> {
    let mut state = self.state.lock().await;

    match *state {
        StreamState::Running => {
            // Signal background task to pause
            self.pause_signal.notify_one();
            // Wait for task to actually pause
            // (via state change or timeout)
            *state = StreamState::Paused;
            Ok(())
        }
        StreamState::Paused => Ok(()), // Idempotent
        StreamState::Completed | StreamState::Failed(_) => {
            Err(JsonStreamError::CannotPauseTerminalStream)
        }
    }
}
```

Test:

```bash
cargo test --lib stream::tests::test_pause_idempotent
cargo test --lib stream::tests::test_pause_running
cargo test --lib stream::tests::test_pause_completed_fails
```

### Step 3: Implement Resume Logic (src/stream/mod.rs)

**Files**: `src/stream/mod.rs`

The `resume()` method:

```rust
pub async fn resume(&mut self) -> Result<(), JsonStreamError> {
    let mut state = self.state.lock().await;

    match *state {
        StreamState::Paused => {
            // Signal background task to resume
            self.resume_signal.notify_one();
            *state = StreamState::Running;
            Ok(())
        }
        StreamState::Running => Ok(()), // Idempotent (no-op)
        StreamState::Completed | StreamState::Failed(_) => {
            Err(JsonStreamError::CannotResumeTerminalStream)
        }
    }
}
```

Test:

```bash
cargo test --lib stream::tests::test_resume_idempotent
cargo test --lib stream::tests::test_resume_before_pause
cargo test --lib stream::tests::test_resume_completed_fails
```

### Step 4: Connect Pause Signal to Background Task (src/connection/conn.rs)

**Files**: `src/connection/conn.rs`

Changes:

- Pass `pause_signal` and `resume_signal` Notify handles to background task
- Add pause check at top of main loop
- When paused: block on `resume_signal.notified()`

```rust
// In streaming_query background task:
loop {
    // Check if we should pause
    if pause_requested.load(Ordering::Relaxed) {
        // Update state
        state_lock.store(StreamState::Paused);
        // Block until resume
        resume_signal.notified().await;
        // Resume
        state_lock.store(StreamState::Running);
    }

    // ... rest of loop ...
}
```

Test:

```bash
cargo test --test integration --test pause
# (integration test against real DB)
```

### Step 5: Update Error Types (src/error.rs)

**Files**: `src/error.rs`

Add new error variants:

```rust
pub enum JsonStreamError {
    // Existing...

    // New
    CannotPauseTerminalStream,
    CannotResumeTerminalStream,
}
```

### Step 6: Update Metrics (src/metrics/counters.rs)

**Files**: `src/metrics/counters.rs`

Add metrics:

```rust
pub fn stream_paused(entity: &str) {
    // Counter: fraiseql_stream_paused_total
    // Label: entity
}

pub fn stream_resumed(entity: &str) {
    // Counter: fraiseql_stream_resumed_total
    // Label: entity
}

pub fn stream_paused_duration_ms(entity: &str, duration_ms: u64) {
    // Histogram: fraiseql_stream_paused_duration_ms
    // Label: entity
}
```

### Step 7: Integration Tests (tests/integration_pause_resume.rs)

**Files**: `tests/integration_pause_resume.rs` (NEW)

Test scenarios:

```rust
#[tokio::test]
async fn test_pause_stops_reading() {
    // Start stream, pause after 100 rows
    // Verify no new rows arrive
    // Check metrics
}

#[tokio::test]
async fn test_resume_continues() {
    // Pause, verify stopped
    // Resume, verify continues
    // Verify all rows arrived in order
}

#[tokio::test]
async fn test_pause_idempotent() {
    // Pause twice, verify second is no-op
}

#[tokio::test]
async fn test_resume_before_pause() {
    // Resume before pause, verify no-op
}

#[tokio::test]
async fn test_drop_while_paused() {
    // Drop stream while paused
    // Verify task exits cleanly
    // Verify no resource leaks
}

#[tokio::test]
async fn test_pause_with_memory_bounds() {
    // Pause stream
    // Verify memory usage stays bounded
    // Resume, verify continues
}

#[tokio::test]
async fn test_pause_with_adaptive_chunking() {
    // Pause stream during active chunking
    // Resume, verify adaptive state preserved
}

#[tokio::test]
async fn test_pause_state_transitions() {
    // Verify state enum correctly reflects transitions
    // Running → Paused → Running → Completed
}
```

### Step 8: Documentation & Examples (README updates)

**Files**: `README.md` or new `examples/pause_resume.rs`

Example code:

```rust
use fraiseql_wire::FraiseClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FraiseClient::connect("postgres://localhost/db").await?;

    let mut stream = client
        .query::<serde_json::Value>("projects")
        .execute()
        .await?;

    // Collect some rows
    for _ in 0..100 {
        if let Some(result) = stream.next().await {
            println!("Item: {}", result?);
        }
    }

    // Pause stream (background task stops reading)
    stream.pause().await?;
    println!("Stream paused");

    // Buffered rows available but no new reading
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Resume stream
    stream.resume().await?;
    println!("Stream resumed");

    // Continue consuming
    while let Some(result) = stream.next().await {
        println!("Item: {}", result?);
    }

    Ok(())
}
```

---

## Files to Modify/Create

| File | Type | Impact | Priority |
|------|------|--------|----------|
| `src/stream/mod.rs` | MODIFY | Add state machine, pause(), resume() | 1 (Core) |
| `src/connection/conn.rs` | MODIFY | Integrate pause signal into background task | 1 (Core) |
| `src/error.rs` | MODIFY | Add new error variants | 2 (Support) |
| `src/metrics/counters.rs` | MODIFY | Add pause/resume metrics | 2 (Support) |
| `tests/integration_pause_resume.rs` | CREATE | Integration tests (7 scenarios) | 3 (Validation) |
| `examples/pause_resume.rs` | CREATE | Example code | 4 (Documentation) |
| `README.md` | MODIFY | Document API | 4 (Documentation) |

---

## Acceptance Criteria

- [ ] `pause()` method implemented and idempotent
- [ ] `resume()` method implemented and idempotent
- [ ] `state()` getter reflects current state correctly
- [ ] Background task checks pause signal on each iteration
- [ ] Paused stream stops reading from Postgres
- [ ] Resumed stream continues from where it left off
- [ ] Metrics recorded (paused_total, resumed_total, paused_duration)
- [ ] All 7 integration test scenarios pass
- [ ] Example code compiles and runs
- [ ] No regressions (all 114 existing tests still pass)
- [ ] Zero clippy warnings
- [ ] Code documented with examples

---

## Non-Goals (DO NOT DO)

- ❌ Support pause during ORDER BY execution
- ❌ Buffering entire result set (defeats memory bounds)
- ❌ Client-side resumption from saved point (one connection = one stream)
- ❌ Nested pause/resume semantics
- ❌ Pause metrics per-second sampled (only count on pause/resume events)

---

## Success Metrics

✅ **Functional**:

- pause() and resume() work correctly
- Idempotence verified by tests
- Background task correctly integrated

✅ **Operational**:

- Metrics expose pause events
- Example code demonstrates typical usage
- Error messages are clear

✅ **Quality**:

- 100% test pass rate (114 + 7 new = 121 total)
- Zero regressions
- Code reviewed and documented

---

## Next Steps

1. Implement Step 1-3 (JsonStream modifications)
2. Run unit tests (should pass immediately)
3. Implement Step 4 (connection layer integration)
4. Run integration tests (should pass with real DB)
5. Add metrics and error types (Steps 5-6)
6. Add example code and documentation
7. Final verification: `cargo test` + `cargo clippy`
8. Commit with detailed message
9. Update completion report

---

## Timeline

**Estimated breakdown**:

- Steps 1-3: 30 minutes (stream type + methods)
- Step 4: 30 minutes (connection integration + testing)
- Steps 5-6: 15 minutes (error types + metrics)
- Step 7: 30 minutes (integration tests)
- Step 8: 15 minutes (docs + examples)

**Total**: ~2-2.5 hours (similar to Phase 8.6.4)

---

## Related Issues & Decisions

**Why not store pause signal on JsonStream?**

- Connection task is in background, needs separate ownership
- Use `Arc<Notify>` to share across task boundary

**Why not make pause/resume sync?**

- Pause must signal background task and wait for it to actually pause (async)
- Resume needs to wait for task to resume reading (async)
- Making them sync would block caller indefinitely

**Why idle/block instead of cancel?**

- Cancellation would lose buffered rows
- Idle preserves connection state (no reconnect needed)
- User can drop stream anytime to cancel

**Why occupancy limits during pause?**

- Memory bounds still apply (hard ceiling doesn't change)
- If paused with high buffer, new resume could exceed soft limit
- Metrics track this (paused_occupancy)
