# Phase 8.6: Design Notes & Implementation Guidance

**Date**: 2026-01-13
**Context**: Feedback from architecture review
**Scope**: Clarifies critical design decisions for 8.6.3+ phases

---

## Critical Design Decisions

### 1. Error Semantics: MemoryLimitExceeded

**What We Have**:
```rust
MemoryLimitExceeded {
    limit: usize,
    current: usize,
}
```

**What We Must Ensure**:

#### Distinguishability
- ✅ Already clear: separate from `Cancelled`, `ConnectionClosed`, `Io`
- ✅ Non-transient: `is_retriable()` returns false
- ⚠️ **Semantic clarity needed**: Document that this is a **terminal error**

**Why it matters**:
- Consumer must not attempt to retry the same query
- Consumer should consider:
  - Increasing consumer throughput (faster `.next()` calls)
  - Reducing batch size (lower chunk_size via config)
  - Switching to unbounded mode (remove max_memory)

#### Documentation Requirement
Add to error.rs doc comment:
```rust
/// Memory limit exceeded
///
/// **Terminal error**: The consumer cannot keep pace with data arrival.
///
/// Causes:
/// - Consumer processing is too slow (slow `next()` calls)
/// - Buffered items are larger than estimated
/// - Memory limit is too restrictive for this workload
///
/// NOT retriable: Retrying same query with same consumer will hit same limit.
///
/// Solutions:
/// 1. Increase consumer throughput (faster `.next()` polling)
/// 2. Reduce items in flight (configure lower `chunk_size`)
/// 3. Remove memory limit (use unbounded mode)
/// 4. Use different transport (consider `tokio-postgres` for flexibility)
```

#### Category Mapping
```rust
"memory_limit_exceeded" → Use for alerting/metrics
```

---

### 2. Enforcement Strategy: Where & When

**Current Plan**: Check in `poll_next()`

**Decision Point**: Pre-enqueue vs Post-enqueue

#### Option A: Pre-Enqueue (Before channel send)
```rust
fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    // Check BEFORE receiving from channel
    let occupancy = self.receiver.len();
    let estimated = occupancy * 2048;

    if let Some(limit) = self.max_memory {
        if estimated > limit {
            return Poll::Ready(Some(Err(
                Error::MemoryLimitExceeded { limit, current: estimated }
            )));
        }
    }

    // Normal recv
    self.receiver.poll_recv(cx)
}
```

**Behavior**:
- Stops consuming when buffer reaches limit
- Clean cutoff
- Producer blocked (backpressure visible)

#### Option B: Post-Dequeue (After receiving item)
```rust
fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    // Normal recv first
    match self.receiver.poll_recv(cx) {
        Poll::Ready(Some(Ok(value))) => {
            // Dequeued 1 item, check remaining
            let occupancy = self.receiver.len();
            let estimated = occupancy * 2048;

            if let Some(limit) = self.max_memory {
                if estimated > limit {
                    // Return this item, then error next call
                    self.memory_exceeded_next = true;
                    return Poll::Ready(Some(Ok(value)));
                }
            }

            Poll::Ready(Some(Ok(value)))
        }
        // ...
    }
}
```

**Behavior**:
- Allows one "burst" item through
- Smoother user experience (consumers can drain buffer)
- More lenient on producer backpressure

#### Recommendation
**Use Option A (Pre-Enqueue)**:
1. Simpler semantics (no state machine)
2. Clear enforcement boundary
3. Producer gets immediate backpressure signal
4. Consumer sees error deterministically

---

### 3. UX of max_memory() API

**What We're Adding**:
```rust
pub fn max_memory(mut self, bytes: usize) -> Self {
    self.max_memory = Some(bytes);
    self
}
```

**Constraints**:

#### Default Behavior
✅ **Must**: Default to unbounded (no max_memory)
- Maintains backward compatibility
- Avoids surprising existing code
- Opt-in safety model

#### Composition with Adaptive Chunking
⚠️ **Critical**: Don't hardwire assumptions

**Bad**:
```rust
// If adaptive chunking adjusts chunk_size automatically,
// and we enforce memory limits, they could fight each other:
// - Adaptive wants to increase chunk_size (reduce overhead)
// - Memory limit forces it down (respect bounds)
// → Thrashing behavior
```

**Good**:
```rust
// Option 1: Separate concerns
// - Memory limit: hard ceiling on buffered rows
// - Adaptive chunking: optimize within that ceiling
// → Chunking adjusts batch size, memory limit prevents overflow

// Option 2: Composition API
stream
    .max_memory(500_000_000)         // 500MB absolute limit
    .adaptive_chunking(true)         // Auto-tune within limit
    .execute()
    .await?
```

#### Memory Estimation Accuracy
⚠️ **Must document assumptions**:
```rust
/// Estimates buffered memory as items_buffered * 2KB
///
/// This is conservative but accurate for typical JSON documents:
/// - Small objects (< 2KB): underestimated memory (safer)
/// - Large objects (> 2KB): overestimated (hits limit earlier, but safe)
/// - Average (1-5KB): nearly perfect
///
/// For custom estimation, users can create wrapper stream that
/// tracks actual memory usage and provides custom error semantics.
```

---

## Phase Sequencing

### Why 8.6.3 → 8.6.4 is the right order

**8.6.3 (Memory Bounds)**:
- ✅ Safety feature (prevents OOM)
- ✅ Defensive (catches misconfiguration)
- ✅ Orthogonal (doesn't interact with other features)
- ✅ Quick to implement (1-2 hours)
- **Enables**: Users can set memory limits and know they're safe

**8.6.4 (Adaptive Chunking)**:
- ✅ Performance feature (improves throughput)
- ✅ Data-driven (uses occupancy metrics from 8.6.1)
- ✅ Composes with 8.6.3 (respects memory limits)
- ⚠️ More complex (state machine, tuning parameters)
- **Enables**: Self-tuning without manual configuration

### Why NOT to skip to 8.6.5 (Pause/Resume)

**8.6.5 would introduce**:
- Significant state machine complexity
- Pause/resume semantics need careful definition
- Harder to test (state explosion)

**Better**: Land 8.6.4 first, then 8.6.5 is more straightforward

---

## Implementation Checklist for 8.6.3

### Phase 8.6.3: Memory Bounds

- [ ] **Error semantics**
  - [x] MemoryLimitExceeded type added
  - [ ] Update doc comments with terminal error semantics
  - [ ] Ensure `category()` returns "memory_limit_exceeded"
  - [ ] Ensure `is_retriable()` returns false

- [ ] **API Design**
  - [ ] Add `max_memory: Option<usize>` to QueryBuilder
  - [ ] Add `max_memory()` builder method with docs
  - [ ] Add `max_memory: Option<usize>` to JsonStream
  - [ ] Pass through from builder to stream

- [ ] **Implementation**
  - [ ] Add memory check in `poll_next()` (Option A: pre-enqueue)
  - [ ] Record metric on limit exceeded
  - [ ] Use `StreamStats` estimation (items_buffered * 2048)

- [ ] **Metrics**
  - [ ] Add `memory_limit_exceeded_total{entity}` counter
  - [ ] Record in counters.rs

- [ ] **Testing**
  - [ ] Unit test: error creation and properties (already done)
  - [ ] Integration test: set limit, exceed, get error
  - [ ] Integration test: set limit, stay under, no error
  - [ ] Integration test: unbounded (no limit) works

- [ ] **Documentation**
  - [ ] Update error doc comment (semantics)
  - [ ] Add example to QueryBuilder docs
  - [ ] Add note about interaction with adaptive chunking (future)

---

## Implementation Checklist for 8.6.4

### Phase 8.6.4: Adaptive Chunking (Deferred to next session)

**Requires**: 8.6.1 (occupancy metrics) + 8.6.3 (memory bounds)

- [ ] **Architecture**
  - [ ] Design AdaptiveChunking state machine
  - [ ] Define adjustment window size (proposal: 50 measurements)
  - [ ] Define occupancy thresholds (proposal: 20%/80%)
  - [ ] Define chunk size bounds (proposal: [16, 1024])

- [ ] **Implementation**
  - [ ] Add AdaptiveChunking type to stream/adaptive_chunking.rs
  - [ ] Implement `observe_occupancy()` method
  - [ ] Implement `adjust()` logic
  - [ ] Integrate into conn.rs streaming loop

- [ ] **Metrics**
  - [ ] Add `adaptive_chunk_size_increase_total{entity}` counter
  - [ ] Add `adaptive_chunk_size_decrease_total{entity}` counter
  - [ ] Add `adaptive_chunk_size_current{entity}` gauge

- [ ] **Testing**
  - [ ] Unit tests for adjustment logic
  - [ ] Tests for bounds enforcement
  - [ ] Integration tests showing chunk size evolution
  - [ ] Stress tests with varying workloads

- [ ] **Documentation**
  - [ ] Design doc explaining adaptation algorithm
  - [ ] Examples of chunk size patterns
  - [ ] Performance impact measurements

---

## Success Criteria

### 8.6.3 (Memory Bounds)
- ✅ Enforces memory limits correctly
- ✅ Error semantics clear (terminal, non-retriable)
- ✅ Zero regression in performance
- ✅ Fully backward compatible (unbounded by default)
- ✅ Composes well with future features

### 8.6.4 (Adaptive Chunking)
- ✅ Chunk size adapts to backpressure
- ✅ Respects memory limits from 8.6.3
- ✅ Improves throughput without sacrificing latency
- ✅ Stable (doesn't thrash around)

---

## Cautionary Notes

### What NOT to do

❌ **Don't hardwire memory estimation**:
```rust
// Bad: makes 8.6.4 harder
if items_buffered > 256 { /* enforce */ }  // Assumes 256 items always ok

// Good: use memory, respect adaptive chunk sizing
let estimated_memory = items_buffered * 2048;
if estimated_memory > limit { /* enforce */ }
```

❌ **Don't make pause/resume a blocker**:
- 8.6.5 is complex but orthogonal
- Can land without it
- Recommend doing after 8.6.4

❌ **Don't skip testing edge cases**:
- Limit exactly equals one item size (should work)
- Limit is 1 byte (should error immediately)
- Unbounded (None) should work forever

### What TO do

✅ **Document assumptions clearly**:
- Memory estimate is conservative
- Applies to buffered items only
- Doesn't include other overhead

✅ **Keep error messages actionable**:
```
"memory limit exceeded: 600MB buffered > 500MB limit
Suggestion: Increase consumer throughput, reduce chunk_size, or remove limit"
```

✅ **Test with real workloads**:
- 1K small objects
- 1K large objects
- Mixed sizes

---

## Architecture Review Recommendation

**If approving 8.6.1 + 8.6.2 (already done)**:

```
Status: ✅ Approved
Comment: "Excellent observability-first work.
Clear path to memory safety and adaptive performance."
```

**For 8.6.3 (ready to land)**:

```
Status: ⏳ Ready for review
Requirements:
- ✅ Error semantics documented
- ✅ Pre-enqueue enforcement strategy
- ✅ Default unbounded
- ⚠️ Watch for 8.6.4 composition

Expected: 1-2 hours to implement & test
Next: 8.6.4 (Adaptive Chunking)
```

**For 8.6.4 (future)**:

```
Status: 📋 Planned
Dependencies:
- 8.6.1 (Occupancy metrics) ✅
- 8.6.3 (Memory bounds) ⏳

Expected: 3-4 hours to implement & test
Value: Highest (improves throughput)
Risk: Medium (state machine complexity)
```

---

## Final Notes

This phase sequence embodies good design:

1. **8.6.1 (Observability)**: See what's happening
2. **8.6.2 (Introspection)**: Query state inline
3. **8.6.3 (Safety)**: Prevent bad outcomes
4. **8.6.4 (Optimization)**: Improve good outcomes

Each builds on prior work without interfering.

**Production readiness progression**:
- After 8.6.1: Observable
- After 8.6.2: Debuggable
- After 8.6.3: Safe
- After 8.6.4: Efficient

Solid engineering path. 👍
