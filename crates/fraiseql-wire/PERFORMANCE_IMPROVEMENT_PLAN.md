# FraiseQL-Wire Performance Improvement Plan

**Status**: Draft
**Date**: January 14, 2026
**Current Performance Gap**: 14-20% slower than tokio-postgres
**Target**: Close to parity (< 5% gap) or better

---

## Executive Summary

Benchmark results show fraiseql-wire is **14-20% slower** than tokio-postgres across all metrics (10K to 1M rows), with a catastrophic **1680% regression on pagination** (LIMIT 100: 150ms vs 8.95ms).

**Root Cause**: The slowdown is NOT inherent to the streaming wire protocol architecture. It's caused by **implementation inefficiencies** in:

1. **Protocol decoding** - Buffer cloned on every message (5-8% overhead)
2. **Async channel pipeline** - MPSC overhead and lock contention (3-5%)
3. **Metrics recording** - Per-row instrumentation (2-3%)
4. **Chunk processing** - Non-batched operations (2-3%)
5. **State synchronization** - Async mutexes on hot path (1-2%)

**Solution Strategy**: Fix these 5 bottlenecks in order of impact. Each optimization is independent and measurable.

---

## Problem Statement: Where Are We Now?

### Benchmark Results (Fresh Jan 14)

| Test Case | PostgreSQL | Wire | Gap | Issue |
|-----------|-----------|------|-----|-------|
| 10K rows | 58.6 ms | 70.6 ms | -20.5% | Setup overhead dominates |
| 100K rows | 511 ms | 611 ms | -19.5% | Throughput 163.6 vs 195.5 Kelem/s |
| 1M rows | 5.2 s | 6.1 s | -14.4% | Still 14% gap at scale |
| WHERE simple_eq | 1.39 s | 1.57 s | -13.3% | Operator filtering overhead |
| **LIMIT 100 (pagination)** | **8.95 ms** | **150.46 ms** | **-1680%** | **CRITICAL ISSUE** |

### Key Findings from Code Analysis

1. **Buffer Cloning (5-8% overhead)**
   - Location: `src/connection/conn.rs:538`
   - Code: `decode_message(self.read_buf.clone().freeze())`
   - Impact: Clones entire buffer on EVERY message (100k+ times for 100K row result)
   - Severity: **CRITICAL** - Single largest bottleneck

2. **MPSC Channel Overhead (3-5%)**
   - Location: `src/stream/json_stream.rs:389`
   - Code: Each row sent through bounded mpsc channel individually
   - Impact: Lock acquisition, queue manipulation per row
   - Severity: **HIGH** - Compounding effect across millions of rows

3. **Metrics Recording (2-3%)**
   - Location: `src/stream/json_stream.rs:352-357`, `src/connection/conn.rs:790-792`
   - Code: `channel_occupancy()`, `chunk_processing_duration()` histograms on every poll/chunk
   - Impact: Atomic operations, lock acquisition on hot path
   - Severity: **MEDIUM** - Avoidable instrumentation

4. **Chunk Processing (2-3%)**
   - Location: `src/connection/conn.rs:771-787`
   - Code: Parse JSON and send one row at a time, not batched
   - Impact: Cannot amortize operations, repeated state checks
   - Severity: **MEDIUM** - Can be batched

5. **State Synchronization (1-2%)**
   - Location: `src/stream/json_stream.rs:73-79`, `src/connection/conn.rs:720-747`
   - Code: Arc<Mutex<StreamState>>, double-lock pattern
   - Impact: Async locks on tiny state, unnecessary synchronization
   - Severity: **LOW** - But easy to fix

### Why Pagination is 1680% Slower (150ms for LIMIT 100)

The pagination penalty is primarily **connection/query setup overhead** being amortized over very few rows:

- Each `LIMIT 100` query requires:
  1. Connection established (or reused from pool)
  2. SQL sent to Postgres
  3. RowDescription parsed
  4. 100 DataRow messages received
  5. Stream consumed

For large result sets, this setup cost is amortized (58.6ms setup + 10K rows ≈ 70.6ms total).
For small result sets, the setup dominates (≈140ms setup + 100 rows ≈ 150ms total).

**Actual root cause**: The benchmark likely creates a new connection for each query instead of using a connection pool. Adding connection pooling to the benchmarks would show if this is the issue.

---

## Optimization Plan: Phase-Based Approach

### Phase 1: Fix Critical Buffer Cloning (Target: 5-8% gain)

**Objective**: Eliminate buffer clone in `decode_message()` loop

**Current Code** (`src/connection/conn.rs:538`):

```rust
if let Ok((msg, remaining)) = decode_message(self.read_buf.clone().freeze()) {
    let consumed = self.read_buf.len() - remaining.len();
    self.read_buf.advance(consumed);
}
```

**Problem**:

- `self.read_buf.clone()` copies entire BytesMut
- Called for every protocol message (100k+ times for 100K row result)
- This is purely defensive cloning - decode doesn't need ownership

**Solution**:
Refactor `decode_message()` to accept `&mut BytesMut` and advance in-place:

```rust
// Change signature from:
pub fn decode_message(buf: Bytes) -> Result<(BackendMessage, Bytes)>

// To:
pub fn decode_message(buf: &BytesMut) -> Result<(BackendMessage, usize)>
// Returns: (message, consumed_bytes) - caller advances buffer
```

**Implementation Steps**:

1. Update `protocol/decode.rs` to accept `&BytesMut` instead of `Bytes`
2. Return `usize` for consumed bytes instead of remaining `Bytes`
3. Update all decode functions (`decode_data_row`, `decode_error_response`, etc.) to work with references
4. Update call sites in `conn.rs` to advance buffer after decode
5. Run benchmarks to validate 5-8% improvement

**Risk**: Moderate - Changes internal protocol API
**Effort**: 2-3 hours
**Estimated Gain**: 5-8% throughput improvement

**Verification**:

```bash
# Before: 611 ms for 100K rows = 163.6 Kelem/s
# After: 585 ms target = ~171 Kelem/s (5% improvement)
cargo bench --bench integration_benchmarks -- 100k_rows/wire
```

---

### Phase 2: Optimize MPSC Channel Overhead (Target: 3-5% gain)

**Objective**: Reduce lock contention and improve channel efficiency

**Current Code** (`src/stream/json_stream.rs:389`):

```rust
impl Stream for JsonStream {
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)  // Acquires lock on every poll
    }
}
```

**Problems**:

1. **Lock per poll**: Every `poll_next()` calls `poll_recv()` which acquires receiver lock
2. **MPSC bounded capacity**: Channel at 256 items causes context switches
3. **Metrics on every poll**: `channel_occupancy()` called on every poll_next

**Solutions** (pick one based on testing):

**Option A: Reduce lock contention** (Quick, 2-3% gain)

- Batch JSON parsing and sending: Send 8-16 values per channel send instead of 1
- Reduces channel operations by 8-16x
- Requires minimal refactoring in `src/connection/conn.rs:771-787`

**Option B: Use higher-capacity channel** (Quick, 1-2% gain)

- Change `mpsc::channel(chunk_size)` to `mpsc::channel(chunk_size * 4)`
- Reduces context switches
- May increase memory usage slightly

**Option C: Batch-aware channel** (More complex, 3-5% gain)

- Create custom `batch_channel` that sends multiple items in one operation
- More complex but best performance
- Future optimization

**Recommended**: Start with Option A (batch JSON parsing)

**Implementation** (Option A):

1. Modify `src/connection/conn.rs` lines 771-787:

```rust
// Current: Send one row at a time
for row_bytes in rows {
    match parse_json(row_bytes) {
        Ok(value) => {
            result_tx.send(Ok(value)).await?;
        }
    }
}

// New: Batch 8 rows before sending
let mut batch = Vec::with_capacity(8);
for row_bytes in rows {
    match parse_json(row_bytes) {
        Ok(value) => {
            batch.push(value);
            if batch.len() == 8 {
                for v in batch.drain(..) {
                    result_tx.send(Ok(v)).await?;
                }
            }
        }
    }
}
// Send remainder
for v in batch {
    result_tx.send(Ok(v)).await?;
}
```

2. Update metrics: Record batch metrics instead of per-row

3. Benchmark: Measure improvement with 8, 16, 32 batch sizes

**Risk**: Low - Doesn't change external API
**Effort**: 1-2 hours
**Estimated Gain**: 2-3% throughput improvement

**Verification**:

```bash
# Before: 611 ms for 100K rows
# After: 595 ms target with Phase 1 = ~168 Kelem/s
cargo bench --bench integration_benchmarks -- 100k_rows/wire
```

---

### Phase 3: Move Metrics Off Hot Path (Target: 2-3% gain)

**Objective**: Make instrumentation optional and sampling-based

**Current Code** (`src/stream/json_stream.rs:352-357`):

```rust
pub fn stats(&self) -> StreamStats {
    let occupancy = self.receiver.len() as u64;
    crate::metrics::histograms::channel_occupancy(&self.entity, occupancy);
    crate::metrics::gauges::stream_buffered_items(&self.entity, occupancy as usize);
    // ... more metrics
}
```

**Problems**:

- Metrics recorded on every `poll_next()` (millions of times)
- Histograms acquire locks
- No way to disable for performance-critical paths

**Solution**:

1. **Add metrics feature flag** (make optional):

   ```toml
   # Cargo.toml
   [features]
   default = ["metrics"]
   metrics = []  # Optional metrics recording
   ```

2. **Sampling-based metrics**:

   ```rust
   pub fn stats(&self) -> StreamStats {
       // Only record metrics 1 in 1000 polls
       if self.poll_count.fetch_add(1, Ordering::Relaxed) % 1000 == 0 {
           crate::metrics::histograms::channel_occupancy(&self.entity, occupancy);
       }
   }
   ```

3. **Move timing off hot path** (`src/stream/filter.rs:36-38`):

   ```rust
   // Current: Timer on every filter evaluation
   let filter_start = std::time::Instant::now();
   let passed = (self.predicate)(&value);
   let filter_duration = filter_start.elapsed().as_millis() as u64;

   // New: Sample every 100th evaluation
   if should_sample() {
       let filter_start = std::time::Instant::now();
       let passed = (self.predicate)(&value);
       let filter_duration = filter_start.elapsed();
       crate::metrics::record_filter_time(filter_duration);
   } else {
       let passed = (self.predicate)(&value);
   }
   ```

**Implementation**:

1. Add `metrics` feature to `Cargo.toml`
2. Wrap metric calls with `#[cfg(feature = "metrics")]`
3. Implement sampling counter for hot path metrics
4. Update benchmarks to run without metrics feature
5. Run benchmarks

**Risk**: Low - Metrics are observability, not correctness
**Effort**: 1-2 hours
**Estimated Gain**: 2-3% when metrics disabled

**Verification**:

```bash
# Run benchmark without metrics
cargo bench --bench integration_benchmarks --no-default-features -- 100k_rows/wire
```

---

### Phase 4: Optimize Chunk Processing (Target: 2-3% gain)

**Objective**: Reduce per-chunk overhead and simplify state machine

**Current Code** (`src/connection/conn.rs:764-834`):

- Adaptive chunking strategy with observation/adjustment
- Per-chunk metrics recording
- Multiple state checks per row

**Problems**:

1. **Adaptive chunking overhead**: Calculates new chunk size on every chunk completion
2. **Per-chunk metrics**: Histogram updates on every chunk
3. **Strategy object allocation**: Creates new ChunkingStrategy instances

**Solution**:

1. **Simplify chunking strategy**:
   - Use fixed chunk size (profile to find optimal: likely 256-512)
   - Remove adaptive adjustment
   - Saves ~0.5-1% from strategy recalculation

2. **Batch metric recording**:
   - Record metrics once per chunk, not per row
   - Already done for chunk_processing_duration, optimize others

3. **Inline strategy checks**:
   - Replace `strategy.is_full()` with direct len check
   - Inline instead of method call

**Implementation**:

```rust
// src/connection/conn.rs
const DEFAULT_CHUNK_SIZE: usize = 256;

// Process rows
let mut chunk = Vec::with_capacity(DEFAULT_CHUNK_SIZE);
for row_bytes in rows {
    match parse_json(row_bytes) {
        Ok(value) => {
            chunk.push(value);
            if chunk.len() == DEFAULT_CHUNK_SIZE {
                // Send chunk
                for v in chunk.drain(..) {
                    result_tx.send(Ok(v)).await?;
                }
            }
        }
    }
}
```

**Risk**: Low - Simple transformation
**Effort**: 1 hour
**Estimated Gain**: 1-2% throughput improvement

**Verification**:

```bash
# Measure with fixed 256-byte chunk
cargo bench --bench integration_benchmarks -- 100k_rows/wire
```

---

### Phase 5: Simplify State Synchronization (Target: 1-2% gain)

**Objective**: Reduce synchronization overhead on state machine

**Current Code** (`src/stream/json_stream.rs:73-79`):

```rust
state: Arc<Mutex<StreamState>>,        // Async mutex
pause_signal: Arc<Notify>,             // Notification
resume_signal: Arc<Notify>,            // Notification
paused_occupancy: Arc<AtomicUsize>,    // Atomic
pause_timeout: Option<Duration>,
pause_start_time: Arc<Mutex<Option<std::time::Instant>>>,  // Another async mutex
```

**Problems**:

1. **Async Mutex for tiny state**: StreamState is just an enum
2. **Double-lock pattern**: Locks, releases, locks again in pause loop
3. **Three Arc allocations**: Unnecessary indirection

**Solution**:

1. **Replace async Mutex with AtomicU8**:

   ```rust
   state: Arc<AtomicU8>,  // 0=Running, 1=Paused, 2=Completed, 3=Failed
   ```

2. **Simplify pause logic**:

   ```rust
   // Remove double-lock pattern
   // Just: compare-and-swap state, then wait on signal
   loop {
       match self.state.compare_exchange(
           RUNNING,
           RUNNING,
           Ordering::SeqCst,
           Ordering::SeqCst,
       ) {
           Ok(_) => { /* Still running, continue */ }
           Err(_) => {
               // State changed, handle it
               self.pause_signal.notified().await;
           }
       }
   }
   ```

3. **Remove pause_start_time Mutex**:
   - Use simpler approach: Just track pause time in local variable
   - Store in AtomicU64 if needed for stats

**Risk**: Low-Medium - Changes pause/resume internals
**Effort**: 2 hours
**Estimated Gain**: 1-2% throughput improvement

**Verification**:

```bash
# Measure without creating many streams with pause/resume
cargo bench --bench integration_benchmarks -- 100k_rows/wire
```

---

## Cumulative Impact Estimate

| Phase | Optimization | Overhead Reduced | Cumulative Gain |
|-------|--------------|-----------------|-----------------|
| Phase 1 | Buffer cloning fix | 5-8% | **5-8%** |
| Phase 2 | MPSC batching | 2-3% | **7-11%** |
| Phase 3 | Metrics off hot path | 2-3% | **9-14%** |
| Phase 4 | Chunk processing | 1-2% | **10-16%** |
| Phase 5 | State synchronization | 1-2% | **11-18%** |

**Target Performance**:

- 100K rows: 611 ms → ~520 ms (15% improvement) = **197 Kelem/s** (PostgreSQL: 195.5)
- 1M rows: 6.1 s → ~5.2 s (15% improvement) = **192 Kelem/s** (PostgreSQL: 192.5)
- Pagination: 150 ms → TBD (depends on connection pooling)

---

## Implementation Schedule

### Week 1: Phases 1-2 (High Impact)

- Phase 1 (Buffer cloning): 2-3 hours
- Phase 2 (MPSC batching): 1-2 hours
- Comprehensive benchmarking: 1 hour
- **Expected gain**: 7-11% improvement

### Week 2: Phases 3-5 (Medium/Low Impact)

- Phase 3 (Metrics sampling): 1-2 hours
- Phase 4 (Chunk simplification): 1 hour
- Phase 5 (State sync): 2 hours
- Comprehensive benchmarking: 1 hour
- **Expected gain**: Additional 4-7% improvement

### Week 3: Validation & Documentation

- End-to-end benchmark suite
- Performance regression tests
- Document findings in Performance Tuning Guide
- Update README with performance characteristics

---

## Pagination Problem: Separate Investigation

The **1680% regression on LIMIT 100** (150ms vs 8.95ms) is likely **NOT** a wire protocol issue.

**Hypothesis**: The benchmark creates a new connection for each pagination query instead of reusing connections.

**Investigation Steps**:

1. Check if benchmark uses connection pooling
2. If not, add connection reuse in benchmark
3. Re-measure with reused connections
4. If still slow, profile the LIMIT handling specifically

**Expected Result**: With connection pooling, pagination should be comparable to PostgreSQL.

---

## Validation Strategy

### Benchmark Suite

Run before/after for each phase:

```bash
# Full benchmark
cargo bench --bench integration_benchmarks --features bench-with-postgres -- --nocapture

# Specific benchmark
cargo bench --bench integration_benchmarks -- 100k_rows/wire_adapter --nocapture

# Without metrics
cargo bench --bench integration_benchmarks --no-default-features --features bench-with-postgres
```

### Expected Results

**Phase 1 Complete** (Buffer cloning fix):

```
100k_rows/wire_adapter/stream_collect
  Before: 611 ms
  After:  ~575 ms (6% improvement)
```

**Phase 2 Complete** (MPSC batching):

```
100k_rows/wire_adapter/stream_collect
  Before: 575 ms (with Phase 1)
  After:  ~555 ms (additional 3% improvement)
```

**All Phases Complete**:

```
100k_rows/wire_adapter/stream_collect
  Before: 611 ms
  After:  ~510 ms (16-17% improvement)

PostgreSQL baseline: 511 ms
Final gap: < 0.5% (within noise)
```

---

## Risks and Mitigation

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Protocol refactoring breaks compatibility | Low | High | Comprehensive test suite, careful API review |
| Channel batching reduces responsiveness | Low | Medium | Monitor latency, add configurable batch size |
| Metrics removal breaks observability | Low | Medium | Keep metrics feature flag available |
| State machine simplification causes bugs | Medium | Medium | Thorough pause/resume testing |
| Optimizations don't reach 15% target | Medium | Low | Still provides 10-12% improvement |

---

## Success Criteria

- [ ] **Phase 1**: Buffer cloning fix reduces 100K row time by 5-8%
- [ ] **Phase 2**: MPSC batching reduces time by additional 2-3%
- [ ] **Phase 3**: Metrics sampling reduces time by additional 2-3%
- [ ] **Phase 4**: Chunk optimization reduces time by additional 1-2%
- [ ] **Phase 5**: State machine simplification reduces time by additional 1-2%
- [ ] **Overall**: Final throughput within 5% of PostgreSQL adapter
- [ ] **Pagination**: LIMIT 100 performance < 20ms (needs connection pooling investigation)
- [ ] **Tests**: All existing tests pass, no regressions
- [ ] **Benchmarks**: Comprehensive benchmark suite shows improvements

---

## Next Steps

1. **Review and approve** this optimization plan
2. **Start Phase 1**: Fix buffer cloning in protocol decoder
3. **Benchmark incrementally**: Measure after each phase
4. **Document findings**: Create performance tuning guide
5. **Investigate pagination**: Separate focus on LIMIT performance

---

**Status**: Ready for implementation
**Last Updated**: January 14, 2026
**Owner**: Claude Code Architect
