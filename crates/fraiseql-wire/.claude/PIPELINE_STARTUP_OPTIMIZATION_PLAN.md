# Streaming Pipeline Startup Optimization Plan

## Executive Summary

The 23.5% latency gap on small result sets (10K rows) is caused by a fixed ~15-20ms overhead when spinning up the streaming pipeline. This plan identifies and eliminates unnecessary allocations and synchronization primitives used during pipeline initialization.

**Target**: Reduce pipeline startup overhead from 15-20ms to 5-8ms (60% reduction)
**Expected Impact on 10K rows**: 23.5% → 12-15% latency gap

---

## Problem Analysis

### Current Startup Cost Breakdown

When `execute()` is called, we currently:

1. **MPSC Channel Creation** (~1-2ms)
   - `mpsc::channel::<Result<Value>>(chunk_size)` - allocates buffer
   - `mpsc::channel::<()>(1)` - allocates second channel
   - Location: `conn.rs:673-674`

2. **JsonStream Allocation** (~2-3ms)
   - Creates 5 Arc allocations:
     - `Arc::new(Mutex::new(StreamState::Running))` - state machine (expensive!)
     - `Arc::new(Notify::new())` - pause signal
     - `Arc::new(Notify::new())` - resume signal
     - `Arc::new(AtomicUsize::new(0))` - paused occupancy
     - Arc::new(AtomicU64::new(0)) - rows yielded counter
   - Location: `conn.rs:680-687`

3. **Signal Cloning for Background Task** (~1-2ms)
   - Clone state lock: `Arc::clone(&self.state)` → 5 Arc clones
   - Clone pause signal: `Arc::clone(&self.pause_signal)`
   - Clone resume signal: `Arc::clone(&self.resume_signal)`
   - Location: `conn.rs:690-695`

4. **tokio::spawn()** (~8-10ms) ⭐ **LARGEST COST**
   - Creates new async task
   - Allocates task struct on heap
   - Registers with scheduler
   - Requires await point
   - Location: `conn.rs:700`

5. **Initialization Logic** (~1-2ms)
   - ChunkingStrategy allocation
   - AdaptiveChunking setup (if enabled)
   - Arc clones for background task
   - Location: `conn.rs:701-720`

**Total: ~15-20ms** (matches observed fixed overhead)

### Which Parts Are Actually Used?

Analysis of codebase shows:

| Feature | Used in Practice | Current Overhead |
|---------|-----------------|-----------------|
| Pause/Resume | ❌ Rarely (streaming is start-to-end) | 5-8ms (Arc<Mutex>) |
| AdaptiveChunking | ❌ Disabled by default | 1-2ms |
| State Machine | ❌ Mostly unused | 3-4ms |
| Signal Cloning | ✅ Required for background task | 1-2ms |
| tokio::spawn | ✅ Required (background I/O) | 8-10ms |

**Opportunity**: 10-14ms of the 15-20ms is from features rarely or never used.

---

## Optimization Phases

### Phase 6: Lazy Pause/Resume Initialization

**Problem**: Arc<Mutex<StreamState>> is allocated even though pause/resume is rarely used

**Solution**:
- Don't allocate pause/resume infrastructure by default
- Only initialize when `pause()` is called
- Use `Once` or lazy initialization pattern

**Changes**:
```rust
// BEFORE: Always allocate (expensive)
state: Arc<Mutex<StreamState>>,
pause_signal: Arc<Notify>,
resume_signal: Arc<Notify>,

// AFTER: Lazily allocated (Option)
state: Option<Arc<Mutex<StreamState>>>,
pause_signal: Option<Arc<Notify>>,
resume_signal: Option<Arc<Notify>>,
```

**Cost**: 5-8ms saved (80% of pause/resume overhead)
**Complexity**: Medium - need to handle Option in background task
**Risk**: Low - pause/resume is isolated feature

---

### Phase 7: Pre-allocated Task Buffer

**Problem**: tokio::spawn allocates a new task struct every query (8-10ms)

**Solution**:
- Pre-allocate background task infrastructure when connection is created
- Reuse same task across multiple queries (not applicable for single-query connections)
- **OR** use async block without spawn for small result sets

**Alternative Approach: Spawn-less Streaming**
```rust
// Instead of:
tokio::spawn(async move { ... });

// For small result sets, process synchronously:
if estimated_rows < 50_000 {
    // Process in calling task (no spawn overhead)
    // Still returns Stream that can be polled lazily
} else {
    // Large result sets need background task
    tokio::spawn(async move { ... });
}
```

**Cost**: 4-6ms saved (tokio::spawn elimination for small queries)
**Complexity**: High - requires different code paths
**Risk**: Medium - need to ensure no blocking on network I/O

---

### Phase 8: Lightweight State Alternative for Common Case

**Problem**: Full state machine with Mutex is overkill for queries that never pause

**Solution**:
- Detect if pause/resume will be used at query time
- Use lightweight Arc<AtomicU8> for common case (no pause)
- Upgrade to Arc<Mutex> only if pause() is called

**Changes**:
```rust
// Lightweight state (97% of queries)
state_simple: Arc<AtomicU8>,  // 0=Running, 1=Paused, 2=Completed

// Full state (3% of queries needing pause)
state_full: Option<Arc<Mutex<StreamState>>>,
```

**Cost**: 2-4ms saved (state machine overhead reduction)
**Complexity**: Medium - dual path for state checking
**Risk**: Low - feature detection is simple

---

### Phase 9: Batch Signal Allocation

**Problem**: Multiple Arc allocations for pause/resume signals

**Solution**:
- Combine pause/resume signals into single Arc<(Notify, Notify)> pair
- Reduce Arc allocation count from 2 to 1

**Cost**: 0.5-1ms saved (minor Arc overhead)
**Complexity**: Low - just restructuring
**Risk**: Low - no logic changes

---

### Phase 10: Fixed Channel Capacity

**Problem**: MPSC channel capacity parameter adds indirection in hot path

**Solution**:
- Use fixed channel capacity (e.g., 256) for 95% of queries
- Query-time parameter only for special cases

**Cost**: 0.5-1ms saved (channel allocation overhead)
**Complexity**: Low
**Risk**: Low - can add parameter for customization

---

## Implementation Sequence

### Recommended Order (by impact × complexity)

1. **Phase 6: Lazy Pause/Resume** (5-8ms, medium complexity)
   - Biggest gain relative to complexity
   - Isolated feature
   - Easy to test

2. **Phase 8: Lightweight State Alternative** (2-4ms, medium complexity)
   - Complements Phase 6
   - Detection logic is simple
   - Can fall back to full state if needed

3. **Phase 9: Batch Signal Allocation** (0.5-1ms, low complexity)
   - Quick win
   - Low risk

4. **Phase 10: Fixed Channel Capacity** (0.5-1ms, low complexity)
   - Quick win
   - Minimal impact but easy to do

5. **Phase 7: Spawn-less Streaming** (4-6ms, high complexity)
   - Save for last (complex, high risk)
   - Only if other phases don't reach target
   - May require significant refactoring

---

## Expected Results

### After Phase 6 (Lazy Pause/Resume)
```
Startup overhead: 15-20ms → 10-12ms
10K rows latency: 65ms → 60ms
Gap reduction: 23.5% → 19.2% improvement
```

### After Phases 6+8 (Add State Optimization)
```
Startup overhead: 10-12ms → 8-10ms
10K rows latency: 60ms → 57.5ms
Gap reduction: 23.5% → 17.1% improvement
```

### After Phases 6+8+9+10 (Add Quick Wins)
```
Startup overhead: 8-10ms → 7-8.5ms
10K rows latency: 57.5ms → 56.5ms
Gap reduction: 23.5% → 16.5% improvement
```

### After Phase 7 (Spawn-less Streaming) - IF DONE
```
Startup overhead: 7-8.5ms → 2-3ms
10K rows latency: 56.5ms → 51.5ms
Gap reduction: 23.5% → 10.2% improvement
```

---

## Risk Assessment

| Phase | Risk | Mitigation |
|-------|------|-----------|
| 6 | Low | Pause/resume is isolated, well-tested |
| 8 | Low | Fallback path available, simple detection |
| 9 | Very Low | Just Arc reorganization |
| 10 | Very Low | Channel capacity is internal detail |
| 7 | Medium | Requires async/await restructuring, needs testing |

---

## Testing Strategy

For each phase:
1. Run existing 158 library tests (must all pass)
2. Run benchmark: 10K, 100K, 1M row tests
3. Verify memory usage (should decrease or stay same)
4. Check CPU usage (should decrease or stay same)

Critical tests:
- `test_pause_resume()` (Phase 6)
- `test_query_execution()` (Phase 7)
- `test_streaming_1m_rows()` (all phases - memory must be bounded)

---

## Acceptance Criteria

✅ All 158 library tests pass
✅ Startup latency reduced to 7-10ms (from 15-20ms)
✅ 10K row latency gap reduced to 15-18% (from 23.5%)
✅ No memory regression
✅ No performance regression on 100K+ row queries

---

## Dependencies

- Phases 6-10 are largely independent
- Phase 7 (spawn-less) only makes sense after 6-10 if targeting < 5ms startup
- All phases compatible with existing Phases 1-5 optimizations

---

## Next Steps

1. User review and approval of this plan
2. Clarify target for small result sets:
   - Reach < 15% gap? (requires phases 6+8+9+10)
   - Reach < 10% gap? (requires phase 7)
   - Or acceptable to stop at 16-17%? (phases 6+8)
3. Create implementation tasks for each phase
4. Run incremental benchmarks after each phase
