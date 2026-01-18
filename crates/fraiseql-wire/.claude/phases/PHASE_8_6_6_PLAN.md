# Phase 8.6.6: Pause/Resume Refinements & Polish — Implementation Plan

**Status**: Ready for implementation
**Date**: 2026-01-13
**Dependencies**: Phase 8.6.5 ✅ Complete

---

## Objective

Polish and enhance the pause/resume feature with:

1. **Custom bounds enforcement** - Pass min/max to AdaptiveChunking
2. **Pause timeout** - Auto-resume after configured duration
3. **Per-pause metrics** - Track pause duration (histogram)
4. **Pause reason tracking** - Optional diagnostic field
5. **Dashboard metrics** - Gauge for current chunk size

All refinements are **optional enhancements** that improve observability and control without breaking existing API.

---

## Refinement 1: Custom Bounds Enforcement

### Current State

- QueryBuilder stores `adaptive_min_chunk_size` and `adaptive_max_chunk_size`
- These are passed to `streaming_query()` but ignored by AdaptiveChunking
- AdaptiveChunking hardcodes min=16, max=1024

### Changes Needed

**File**: `src/stream/adaptive_chunking.rs`

Add new method:

```rust
pub fn with_bounds(mut self, min_size: usize, max_size: usize) -> Self {
    self.min_size = min_size;
    self.max_size = max_size;
    self
}
```

**File**: `src/connection/conn.rs` (lines ~625-634)

Update initialization:

```rust
let mut adaptive = if enable_adaptive_chunking {
    let mut adp = AdaptiveChunking::new();

    // Apply custom bounds if provided
    if let Some(min) = adaptive_min_chunk_size {
        if let Some(max) = adaptive_max_chunk_size {
            adp = adp.with_bounds(min, max);
        }
    }

    Some(adp)
} else {
    None
};
```

### Tests

- `test_adaptive_bounds_enforced_min` - min bound respected
- `test_adaptive_bounds_enforced_max` - max bound respected
- `test_adaptive_bounds_custom_range` - custom range works end-to-end

---

## Refinement 2: Pause Timeout

### Current State

- pause() blocks indefinitely until resume()
- No auto-resume capability

### Changes Needed

**File**: `src/stream/json_stream.rs`

Add to JsonStream struct:

```rust
pause_timeout: Option<Duration>,  // Optional timeout for pause
```

Add new method:

```rust
/// Set timeout for pause (auto-resume after duration)
pub fn set_pause_timeout(&mut self, duration: Duration) {
    self.pause_timeout = Some(duration);
}

/// Clear pause timeout
pub fn clear_pause_timeout(&mut self) {
    self.pause_timeout = None;
}
```

**File**: `src/connection/conn.rs` (lines ~654-667)

Update pause check with timeout:

```rust
{
    let current_state = state_lock.lock().await;
    if *current_state == crate::stream::StreamState::Paused {
        tracing::debug!("stream paused, waiting for resume");
        drop(current_state);

        // Wait for resume signal with optional timeout
        if let Some(timeout) = pause_timeout {
            match tokio::time::timeout(timeout, resume_signal.notified()).await {
                Ok(_) => {
                    // Resume signal received
                    tracing::debug!("stream resumed");
                },
                Err(_) => {
                    // Timeout expired, auto-resume
                    tracing::debug!("pause timeout expired, auto-resuming");
                    crate::metrics::counters::stream_pause_timeout_expired(&entity_for_metrics);
                }
            }
        } else {
            // No timeout, wait indefinitely
            resume_signal.notified().await;
            tracing::debug!("stream resumed");
        }

        let mut state = state_lock.lock().await;
        *state = crate::stream::StreamState::Running;
    }
}
```

### Metrics

Add new counter:

```rust
pub fn stream_pause_timeout_expired(entity: &str) {
    counter!("fraiseql_stream_pause_timeout_expired_total", ...)
        .increment(1);
}
```

### Tests

- `test_pause_timeout_auto_resumes` - timeout expires and auto-resumes
- `test_pause_no_timeout_blocks` - without timeout, pause blocks indefinitely
- `test_pause_timeout_explicit_resume` - explicit resume before timeout

---

## Refinement 3: Per-Pause Duration Metrics

### Current State

- Count pause/resume events (counters)
- No timing information

### Changes Needed

**File**: `src/stream/json_stream.rs`

Add to JsonStream:

```rust
pause_start_time: Arc<Mutex<Option<std::time::Instant>>>,
```

**File**: `src/stream/json_stream.rs` - Update pause() method

Track when pause started:

```rust
pub async fn pause(&mut self) -> Result<()> {
    let mut state = self.state.lock().await;
    match *state {
        StreamState::Running => {
            self.pause_signal.notify_one();
            *state = StreamState::Paused;

            // Record pause start time
            let mut start_time = self.pause_start_time.lock().await;
            *start_time = Some(std::time::Instant::now());

            crate::metrics::counters::stream_paused(&self.entity);
            Ok(())
        }
        // ... rest unchanged
    }
}
```

**File**: `src/stream/json_stream.rs` - Update resume() method

Record pause duration:

```rust
pub async fn resume(&mut self) -> Result<()> {
    let mut state = self.state.lock().await;
    match *state {
        StreamState::Paused => {
            self.resume_signal.notify_one();
            *state = StreamState::Running;

            // Record pause duration
            let mut start_time = self.pause_start_time.lock().await;
            if let Some(start) = *start_time {
                let duration_ms = start.elapsed().as_millis() as u64;
                crate::metrics::histograms::stream_pause_duration(&self.entity, duration_ms);
                *start_time = None;
            }

            crate::metrics::counters::stream_resumed(&self.entity);
            Ok(())
        }
        // ... rest unchanged
    }
}
```

**File**: `src/metrics/histograms.rs`

Add new histogram:

```rust
pub fn stream_pause_duration(entity: &str, duration_ms: u64) {
    histogram!("fraiseql_stream_pause_duration_ms", "entity" => entity.to_string())
        .record(duration_ms as f64);
}
```

### Tests

- `test_pause_duration_recorded` - duration recorded in histogram
- `test_multiple_pauses_separate_durations` - each pause has own duration

---

## Refinement 4: Pause Reason Tracking

### Current State

- pause() takes no parameters
- No diagnostic info about why stream paused

### Changes Needed

**File**: `src/stream/json_stream.rs`

Add to JsonStream:

```rust
pause_reason: Arc<Mutex<Option<String>>>,
```

Add new method:

```rust
/// Pause with an optional reason for diagnostics
pub async fn pause_with_reason(&mut self, reason: Option<String>) -> Result<()> {
    let mut state = self.state.lock().await;
    match *state {
        StreamState::Running => {
            self.pause_signal.notify_one();
            *state = StreamState::Paused;

            // Store pause reason
            let mut stored_reason = self.pause_reason.lock().await;
            *stored_reason = reason.clone();

            // Log reason if provided
            if let Some(r) = &reason {
                tracing::debug!("stream paused: {}", r);
            }

            crate::metrics::counters::stream_paused(&self.entity);
            Ok(())
        }
        // ... rest unchanged
    }
}

/// Get current pause reason
pub fn pause_reason(&self) -> Option<String> {
    // Note: This would require a blocking get, so just return Option
    // User would need to check synchronously
    None  // Placeholder - may need Mutex lock
}
```

**Alternative simpler approach**: Just add tracing to existing pause():

```rust
pub async fn pause_with_reason(&mut self, reason: &str) -> Result<()> {
    tracing::info!("pausing stream: {}", reason);
    self.pause().await
}
```

(Recommend the simpler approach to avoid adding complexity)

### Tests

- `test_pause_with_reason_logged` - reason appears in logs

---

## Refinement 5: Dashboard Metrics (Gauges)

### Current State

- Counters for pause/resume events
- Histogram for pause duration
- No real-time gauge for current chunk size

### Changes Needed

**File**: `src/metrics/gauges.rs` (NEW)

Create new module:

```rust
use metrics::gauge;

pub fn current_chunk_size(entity: &str, size: usize) {
    gauge!("fraiseql_chunk_size_bytes", "entity" => entity.to_string())
        .set(size as f64);
}

pub fn stream_buffered_items(entity: &str, count: usize) {
    gauge!("fraiseql_stream_buffered_items", "entity" => entity.to_string())
        .set(count as f64);
}
```

**File**: `src/metrics/mod.rs`

Add module:

```rust
pub mod gauges;
```

**File**: `src/connection/conn.rs`

Record chunk size after adjustments (lines ~700-705):

```rust
// After setting new chunk_size
crate::metrics::gauges::current_chunk_size(&entity_for_metrics, current_chunk_size);
```

**File**: `src/stream/json_stream.rs` - in poll_next()

Record buffered items (lines ~127-130):

```rust
// Record channel occupancy as gauge
let occupancy = self.receiver.len() as usize;
crate::metrics::gauges::stream_buffered_items(&self.entity, occupancy);
crate::metrics::histograms::channel_occupancy(&self.entity, occupancy as u64);
```

### Tests

- `test_chunk_size_gauge_recorded` - gauge reflects current size
- `test_buffered_items_gauge_recorded` - gauge reflects buffered count

---

## Implementation Sequence

### Step 1: Custom Bounds Enforcement (20 min)

1. Add `with_bounds()` to AdaptiveChunking
2. Wire it up in connection layer
3. Test min/max enforcement

**Verification**: `cargo test --lib adaptive_chunking`

### Step 2: Pause Timeout (25 min)

1. Add `pause_timeout` field to JsonStream
2. Add methods to get/set timeout
3. Update pause check loop with timeout logic
4. Add metric for timeout expiry

**Verification**: `cargo test --lib stream`

### Step 3: Per-Pause Duration Metrics (20 min)

1. Track pause start time
2. Calculate duration on resume
3. Record histogram
4. Test that durations are recorded

**Verification**: `cargo test --lib metrics`

### Step 4: Pause Reason Tracking (10 min)

1. Add simple `pause_with_reason()` method
2. Just log the reason (defer complex state storage)
3. Test that reasons are logged

**Verification**: `cargo test --lib stream`

### Step 5: Dashboard Metrics (15 min)

1. Create `gauges.rs` module
2. Add two gauge metrics
3. Record in appropriate places
4. Test gauges are updated

**Verification**: `cargo test --lib metrics`

### Step 6: Full Integration Test (10 min)

1. Run all unit tests
2. Verify no regressions
3. Run clippy
4. Final verification

**Total**: ~100 minutes (~1.5-2 hours)

---

## Files to Modify/Create

| File | Type | Changes |
|------|------|---------|
| `src/stream/adaptive_chunking.rs` | MODIFY | Add `with_bounds()` method |
| `src/connection/conn.rs` | MODIFY | Wire custom bounds + pause timeout + gauge recording |
| `src/stream/json_stream.rs` | MODIFY | Add pause timeout, duration tracking, reason method |
| `src/metrics/gauges.rs` | CREATE | New gauges module |
| `src/metrics/mod.rs` | MODIFY | Export gauges module |
| `src/metrics/counters.rs` | MODIFY | Add pause_timeout_expired counter |
| `src/metrics/histograms.rs` | MODIFY | Add stream_pause_duration histogram |

---

## Acceptance Criteria

- [ ] Custom bounds enforced (min/max applied to AdaptiveChunking)
- [ ] Pause timeout auto-resumes after duration
- [ ] Per-pause durations recorded in histogram
- [ ] Pause reason method available (logged)
- [ ] Chunk size gauge updated
- [ ] Buffered items gauge updated
- [ ] All 116+ tests passing
- [ ] Zero new clippy warnings
- [ ] No regressions

---

## Success Metrics

✅ **Functional**:

- All 5 refinements working
- Metrics properly recorded
- Composition verified

✅ **Operational**:

- Users can set pause timeout
- Users can specify pause reason
- Dashboard metrics visible

✅ **Quality**:

- 100% test pass rate
- Zero regressions
- Code reviewed

---

## Next Steps

1. Start with Step 1 (custom bounds - simplest)
2. Implement sequentially, testing after each
3. Run full suite at end
4. Commit with comprehensive message
5. Update completion report

This approach keeps changes small and focused, making testing and debugging easier.
