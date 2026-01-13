# Phase 8.6: Streaming & Resource Management

## Objective

Enhance streaming architecture with observability, adaptive mechanics, and explicit lifecycle control. Move fraiseql-wire toward production readiness by providing visibility into backpressure, enabling memory-bounded streaming, and supporting stream state queries.

**Target Duration**: 2-3 weeks
**Scope**: 6 sub-phases with progressive value delivery
**Success Metrics**: All metrics working, tests passing, benchmarks show no regression

---

## Current State (Phase 8.5 Completion)

✅ **Phase 8.5 Complete (100%)**:
- 17 production metrics implemented
- 105 passing tests
- Zero-overhead metric collection (~0.042% overhead)
- Complete observability framework

**Streaming Architecture** (from Explore analysis):
- One-way MPSC channel with bounded capacity (default: 256 rows)
- Automatic backpressure via tokio::select!
- Chunking strategy reduces per-item overhead
- Cancellation-safe drop semantics
- O(chunk_size) memory scaling

**Key Invariants**:
- Single active query per connection
- Exactly one JSON column named `data`
- No buffering of full result sets
- Cancellation stops query immediately

---

## Phase 8.6 Architecture

### 8.6.1: Channel Occupancy Metrics

**Objective**: Add direct visibility into channel backpressure
**Effort**: 2-3 days
**Files**:
- `src/metrics/histograms.rs` - Add occupancy tracking
- `src/metrics/labels.rs` - Add backpressure label (optional)
- `src/stream/json_stream.rs` - Wrap receiver to track occupancy
- `tests/metrics_integration.rs` - Verify histogram recording

**Design**:

```rust
// In src/stream/json_stream.rs
pub struct JsonStream {
    receiver: mpsc::Receiver<Result<Value>>,
    _cancel_tx: mpsc::Sender<()>,
    buffer_depth: Arc<AtomicUsize>,  // Track current items in channel
}

// In JsonStream::poll_next()
fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    // Before reading, record current buffer depth
    let depth = self.receiver.len();
    self.buffer_depth.store(depth, Ordering::Relaxed);

    // Record as histogram metric
    histogram!("fraiseql_channel_occupancy_rows", "entity" => "...").record(depth as f64);

    // Continue with normal recv
    self.receiver.poll_recv(cx)
}
```

**Metrics Added**:
- `fraiseql_channel_occupancy_rows{entity}` - Histogram of buffer depth per poll
- Shows backpressure patterns: low values → fast consumer, high values → slow consumer

**Tests**:
- Verify occupancy recorded for each poll
- Test occupancy spikes under load
- Verify occupancy zeros on stream completion

**Acceptance**:
- [ ] Histogram metric added to `histograms.rs`
- [ ] JsonStream tracks buffer depth
- [ ] Metric recorded on each poll_next()
- [ ] 3+ tests cover occupancy tracking
- [ ] Zero regression in benchmarks

---

### 8.6.2: Stream Statistics API

**Objective**: Allow consumers to query stream state inline
**Effort**: 2-3 days
**Files**:
- `src/stream/json_stream.rs` - Add stats() method
- `src/stream/mod.rs` - Export StreamStats type
- `tests/streaming_integration.rs` - Stats in real queries

**Design**:

```rust
// In src/stream/mod.rs
#[derive(Debug, Clone)]
pub struct StreamStats {
    /// Current items buffered in channel
    pub items_buffered: usize,
    /// Estimated memory used by buffered items (bytes)
    pub estimated_memory: usize,
    /// Total rows yielded so far
    pub total_rows_yielded: u64,
    /// Rows filtered out by Rust predicates
    pub total_rows_filtered: u64,
}

// In src/stream/json_stream.rs
impl JsonStream {
    /// Get current stream statistics
    pub fn stats(&self) -> StreamStats {
        StreamStats {
            items_buffered: self.receiver.len(),
            estimated_memory: self.estimate_buffer_memory(),
            total_rows_yielded: self.rows_yielded.load(Ordering::Relaxed),
            total_rows_filtered: self.rows_filtered.load(Ordering::Relaxed),
        }
    }

    fn estimate_buffer_memory(&self) -> usize {
        // Conservative estimate: assume avg JSON = 2KB per item
        self.receiver.len() * 2048
    }
}
```

**Implementation Notes**:
- `receiver.len()` is cheap (reads bounded queue length)
- Memory estimate is conservative (2KB default, configurable)
- Counters tracked via Arc<AtomicU64> (no locks)
- No polling side effects

**Tests**:
- Stats available before consuming any rows
- Stats update as stream is consumed
- Filtered items tracked separately from yielded
- Memory estimate stays reasonable

**Acceptance**:
- [ ] StreamStats type defined and public
- [ ] JsonStream implements stats() method
- [ ] All fields updated correctly
- [ ] 4+ tests cover stats API
- [ ] Documentation with examples

---

### 8.6.3: Memory Estimation & Bounds

**Objective**: Enable memory-limited streaming with graceful enforcement
**Effort**: 3-4 days
**Files**:
- `src/stream/json_stream.rs` - Memory tracking
- `src/connection/conn.rs` - Max memory configuration
- `src/client/query_builder.rs` - API: max_memory()
- `src/metrics/histograms.rs` - Memory metric
- `tests/streaming_integration.rs` - Memory limit tests

**Design**:

```rust
// In src/stream/json_stream.rs
pub struct JsonStream {
    // ... existing fields
    max_memory: Option<usize>,  // None = unbounded
    current_memory: Arc<AtomicUsize>,
}

impl Stream for JsonStream {
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Check memory bounds
        if let Some(limit) = self.max_memory {
            let current = self.current_memory.load(Ordering::Relaxed);
            if current > limit {
                let over = current - limit;
                tracing::warn!("memory limit exceeded by {} bytes", over);
                crate::metrics::counters::memory_limit_exceeded(&entity, over as u64);
                return Poll::Ready(Some(Err(Error::MemoryLimitExceeded {
                    limit,
                    current,
                })));
            }
        }

        // Normal poll continues...
    }
}

// In src/client/query_builder.rs
pub struct QueryBuilder {
    // ... existing fields
    max_memory: Option<usize>,
}

impl QueryBuilder {
    /// Limit buffered memory to N bytes
    pub fn max_memory(mut self, bytes: usize) -> Self {
        self.max_memory = Some(bytes);
        self
    }
}
```

**Error Type** (in `src/error.rs`):
```rust
pub enum Error {
    // ... existing variants
    MemoryLimitExceeded {
        limit: usize,
        current: usize,
    },
}
```

**Behavior**:
- Default: no limit (backward compatible)
- With limit: emit error when exceeded
- Includes metric for alerting
- Connection state unclear after error (unsafe to reuse)

**Tests**:
- Set limit below single JSON object → error immediately
- Set limit = expected buffer size → no errors
- Set limit = buffer size / 2 → error when half-full
- Verify error includes actual/limit values

**Acceptance**:
- [ ] max_memory() method on QueryBuilder
- [ ] Memory tracking in JsonStream
- [ ] MemoryLimitExceeded error type
- [ ] Metric on limit breach
- [ ] 5+ tests cover all scenarios

---

### 8.6.4: Adaptive Chunk Sizing

**Objective**: Automatically adjust chunk_size based on backpressure conditions
**Effort**: 4-5 days
**Files**:
- `src/stream/adaptive_chunking.rs` (new)
- `src/connection/conn.rs` - Use adaptive strategy
- `src/stream/mod.rs` - Export AdaptiveChunking
- `src/metrics/histograms.rs` - Adaptive adjustment metrics
- `tests/streaming_integration.rs` - Verify adaptation behavior

**Design**:

```rust
// In src/stream/adaptive_chunking.rs
pub struct AdaptiveChunking {
    current_size: usize,
    min_size: usize,    // 16
    max_size: usize,    // 1024
    adjustment_window: usize,  // measurements before adjusting
    measurements: Vec<ChannelOccupancy>,
}

#[derive(Copy, Clone)]
struct ChannelOccupancy {
    occupancy: usize,  // 0-100%
    timestamp: Instant,
}

impl AdaptiveChunking {
    pub fn new() -> Self {
        Self {
            current_size: 256,  // Start with default
            min_size: 16,
            max_size: 1024,
            adjustment_window: 50,  // Measure 50 polls before adjusting
            measurements: Vec::new(),
        }
    }

    /// Record channel occupancy percentage
    pub fn observe_occupancy(&mut self, items_buffered: usize, capacity: usize) {
        let pct = (items_buffered * 100) / capacity;
        self.measurements.push(ChannelOccupancy {
            occupancy: pct,
            timestamp: Instant::now(),
        });

        if self.measurements.len() >= self.adjustment_window {
            self.adjust();
            self.measurements.clear();
        }
    }

    fn adjust(&mut self) {
        let avg_occupancy = self.measurements.iter().map(|m| m.occupancy).sum::<usize>()
            / self.measurements.len();

        // If buffer mostly full (> 80%), increase chunk size to batch more
        if avg_occupancy > 80 {
            self.current_size = (self.current_size * 1.5).min(self.max_size as f64) as usize;
            tracing::debug!("increased chunk_size to {}", self.current_size);
        }
        // If buffer mostly empty (< 20%), decrease chunk size for lower latency
        else if avg_occupancy < 20 {
            self.current_size = (self.current_size / 1.5).max(self.min_size as f64) as usize;
            tracing::debug!("decreased chunk_size to {}", self.current_size);
        }
    }

    pub fn get_chunk_size(&self) -> usize {
        self.current_size
    }
}
```

**Integration**:
- Use AdaptiveChunking in conn.rs streaming_query()
- Pass observed occupancy on each chunk flush
- Record adjustment in metrics
- Bounds prevent pathological behavior

**Metrics**:
- `fraiseql_chunk_size_adjusted{entity, direction}` - Counter for increases/decreases
- `fraiseql_adaptive_chunk_size{entity}` - Current size histogram

**Tuning Parameters**:
- Min: 16 rows (minimum sensible batch)
- Max: 1024 rows (prevent memory spikes)
- Window: 50 measurements (balance reactivity vs noise)
- Thresholds: 20%/80% (wide hysteresis band)

**Tests**:
- Stable occupancy → no adjustments
- High occupancy → increases chunk_size
- Low occupancy → decreases chunk_size
- Bounds respected (16 ≤ size ≤ 1024)
- Metrics recorded on adjustment

**Acceptance**:
- [ ] AdaptiveChunking type defined
- [ ] observe_occupancy() called per chunk
- [ ] Adjustment logic correct
- [ ] Metrics recorded
- [ ] 6+ tests cover adaptation scenarios
- [ ] Bounds enforced
- [ ] Backward compat: existing code still works

---

### 8.6.5: Stream Pause/Resume

**Objective**: Allow consumers to suspend stream without dropping connection
**Effort**: 5-7 days
**Files**:
- `src/stream/json_stream.rs` - Add pause/resume control
- `src/stream/mod.rs` - Export StreamState enum
- `src/connection/conn.rs` - Suspend background task
- `src/metrics/counters.rs` - Pause/resume metrics
- `tests/streaming_integration.rs` - Pause/resume behavior

**Design**:

```rust
// In src/stream/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Active,
    Paused,
    Completed,
    Failed,
}

// In src/stream/json_stream.rs
pub struct JsonStream {
    receiver: mpsc::Receiver<Result<Value>>,
    _cancel_tx: mpsc::Sender<()>,
    pause_tx: mpsc::UnboundedSender<PauseCommand>,
    state: Arc<AtomicU8>,  // Encoded StreamState
    // ... other fields
}

enum PauseCommand {
    Pause,
    Resume,
    Cancel,
}

impl JsonStream {
    /// Pause the stream (suspends background task, keeps connection alive)
    pub async fn pause(&mut self) -> Result<()> {
        self.pause_tx.send(PauseCommand::Pause)
            .map_err(|_| Error::StreamAlreadyClosed)?;
        self.state.store(StreamState::Paused as u8, Ordering::Release);
        crate::metrics::counters::stream_paused(&entity);
        Ok(())
    }

    /// Resume the stream
    pub async fn resume(&mut self) -> Result<()> {
        if self.state.load(Ordering::Acquire) != StreamState::Paused as u8 {
            return Err(Error::StreamNotPaused);
        }
        self.pause_tx.send(PauseCommand::Resume)
            .map_err(|_| Error::StreamAlreadyClosed)?;
        self.state.store(StreamState::Active as u8, Ordering::Release);
        crate::metrics::counters::stream_resumed(&entity);
        Ok(())
    }

    /// Get current stream state
    pub fn state(&self) -> StreamState {
        StreamState::from_u8(self.state.load(Ordering::Acquire))
    }
}

// Background task integration (conn.rs)
tokio::spawn(async move {
    loop {
        tokio::select! {
            cmd = pause_rx.recv() => {
                match cmd {
                    Some(PauseCommand::Pause) => {
                        tracing::debug!("pausing stream");
                        // Suspend reading, but don't close connection
                        loop {
                            tokio::select! {
                                cmd = pause_rx.recv() => {
                                    if matches!(cmd, Some(PauseCommand::Resume)) {
                                        break;
                                    }
                                }
                                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                            }
                        }
                    }
                    Some(PauseCommand::Resume) => {
                        // Resume was received while already paused, loop continues
                    }
                    None => break,  // Stream closed
                }
            }
            msg_result = self.receive_message() => {
                // Normal message processing...
            }
        }
    }
});
```

**Error Types** (in `src/error.rs`):
```rust
pub enum Error {
    // ... existing
    StreamNotPaused,
    StreamAlreadyClosed,
}
```

**Semantics**:
- Pause: Background task waits in loop, connection stays open
- Resume: Background task continues reading from Postgres
- Drop: Cancellation signal sent (same as before)
- State: Queried via `stream.state()`

**Metrics**:
- `fraiseql_stream_paused_total{entity}` - Pause events
- `fraiseql_stream_resumed_total{entity}` - Resume events
- `fraiseql_stream_pause_duration_ms{entity}` - Pause time

**Tests**:
- Pause stream, resume, continue consuming
- Pause, drop → cleanup happens correctly
- Resume when not paused → error
- Stats available while paused
- Pause/resume doesn't lose buffered rows

**Acceptance**:
- [ ] pause() and resume() methods
- [ ] StreamState enum with all variants
- [ ] state() method returns current state
- [ ] Background task respects pause signal
- [ ] Connection stays open during pause
- [ ] 7+ tests cover pause/resume scenarios
- [ ] Metrics recorded

---

### 8.6.6: Cancellation Backpressure

**Objective**: Make cancellation async-aware with optional timeout
**Effort**: 2-3 days
**Files**:
- `src/stream/json_stream.rs` - Add cancel_async() method
- `src/connection/conn.rs` - Graceful shutdown
- `src/metrics/counters.rs` - Cancellation timing
- `tests/streaming_integration.rs` - Cancellation tests

**Design**:

```rust
// In src/stream/json_stream.rs
impl JsonStream {
    /// Cancel stream and wait for background task to finish
    pub async fn cancel(self) -> Result<()> {
        let start = Instant::now();

        // Signal cancellation
        self.cancel_tx.send(()).ok();

        // Wait for background task to finish (drops at end of scope)
        drop(self);

        let duration = start.elapsed();
        crate::metrics::histograms::cancellation_duration(&entity, duration.as_millis() as u64);

        Ok(())
    }

    /// Cancel with timeout
    pub async fn cancel_timeout(self, timeout: Duration) -> Result<()> {
        let start = Instant::now();

        self.cancel_tx.send(()).ok();

        // Task will be dropped when self goes out of scope
        // But we can't really enforce timeout without Arc<JoinHandle>
        // Alternative: return bool for "cancelled cleanly"

        drop(self);
        let duration = start.elapsed();

        if duration > timeout {
            return Err(Error::CancellationTimeout(duration));
        }

        Ok(())
    }
}
```

**Alternative Design** (if timeout enforcement needed):
```rust
// Store JoinHandle in JsonStream
pub struct JsonStream {
    // ... existing fields
    task: Arc<Option<JoinHandle<()>>>,  // Or use Option<JoinHandle>
}

impl JsonStream {
    pub async fn cancel_timeout(mut self, timeout: Duration) -> Result<()> {
        // Send cancel signal
        self.cancel_tx.send(()).ok();

        // Wait for task with timeout
        match tokio::time::timeout(timeout, /* task.await */) {
            Ok(_) => Ok(()),
            Err(_) => Err(Error::CancellationTimeout(timeout)),
        }
    }
}
```

**Current Limitation**:
- Drop-based cleanup doesn't expose completion
- Task completion depends on receiving cancel signal
- Alternative requires storing JoinHandle (adds complexity)

**Recommendation**: Start with simple `cancel_timeout()` that returns timeout error if cleanup takes too long.

**Metrics**:
- `fraiseql_cancellation_duration_ms{entity}` - Time to cancel

**Tests**:
- Cancel immediately after creation
- Cancel after consuming some rows
- Cancel timeout fires if task hangs

**Acceptance**:
- [ ] cancel() method (existing drop semantics)
- [ ] cancel_timeout() with Duration param
- [ ] Metrics recorded
- [ ] 3+ tests cover cancellation
- [ ] Clear documentation on timeout limitations

---

## Implementation Sequence

**Week 1**:
1. 8.6.1: Channel Occupancy Metrics (2-3 days)
2. 8.6.2: Stream Statistics API (2-3 days)
3. Integration testing & bug fixes (1 day)

**Week 2**:
4. 8.6.3: Memory Bounds (3-4 days)
5. 8.6.4: Adaptive Chunking (4-5 days)
6. Partial integration & testing (1 day)

**Week 3**:
7. 8.6.5: Pause/Resume (5-7 days) — OR defer to 8.7
8. 8.6.6: Cancellation Backpressure (2-3 days)
9. Full integration, benchmarks, documentation (2-3 days)

**Alternative Sequencing** (if time constrained):
- **Minimum (1 week)**: 8.6.1 + 8.6.2 + 8.6.3 (core observability & bounds)
- **Standard (2 weeks)**: Add 8.6.4 (adaptive mechanics)
- **Full (3 weeks)**: Add 8.6.5 + 8.6.6 (lifecycle control)

---

## Files to Modify/Create

### Core Implementation

```
src/
├── stream/
│   ├── json_stream.rs       (modify - add stats, memory tracking, pause/resume)
│   ├── adaptive_chunking.rs (create - adaptive chunk sizing)
│   └── mod.rs               (modify - export new types)
├── connection/
│   ├── conn.rs              (modify - use adaptive chunking, memory bounds)
│   └── mod.rs               (modify - export if needed)
├── client/
│   ├── query_builder.rs     (modify - add max_memory() API)
│   └── mod.rs               (modify - if needed)
├── metrics/
│   ├── histograms.rs        (modify - add channel occupancy, adaptive, cancellation)
│   ├── counters.rs          (modify - pause/resume/memory events)
│   └── labels.rs            (modify - if new labels needed)
├── error.rs                 (modify - add MemoryLimitExceeded, etc.)
└── lib.rs                   (modify - expose new types)

tests/
├── streaming_integration.rs (modify - add all Phase 8.6 tests)
├── metrics_integration.rs    (modify - add new metric tests)
└── stress_tests.rs          (modify - verify under load)

benches/
└── micro_benchmarks.rs      (modify - add adaptive chunking benchmarks)
```

### Documentation

```
.claude/phases/
└── PHASE_8_6_PLAN.md        (this file)

docs/
├── STREAMING_GUIDE.md       (new - Stream API usage)
├── ADAPTIVE_CHUNKING.md     (new - How adaptation works)
└── MEMORY_BOUNDS.md         (new - Setting limits)
```

---

## Testing Strategy

### Unit Tests
- Each sub-phase has dedicated unit tests
- Metrics recording verified
- Edge cases: empty streams, large objects, rapid pause/resume

### Integration Tests
- Real Postgres queries
- Metrics collected end-to-end
- Memory bounds enforced
- Adaptive behavior under realistic load

### Stress Tests
- 1M+ row queries with memory limits
- Rapid pause/resume cycles
- Occupancy spikes
- Verify metrics under high load

### Benchmarks
- Channel occupancy recording overhead
- Stats() method latency
- Pause/resume latency
- Adaptive chunk sizing overhead

---

## Acceptance Criteria

### Phase Completion

- [ ] All 6 sub-phases implemented
- [ ] 50+ new tests added (total 155+ passing)
- [ ] Zero test failures
- [ ] All metrics working and recorded
- [ ] Benchmarks show < 2% overhead from Phase 8.5
- [ ] Documentation complete (guides + inline comments)
- [ ] Code review passed

### Per-Sub-Phase

Each of 8.6.1 through 8.6.6 must satisfy its acceptance criteria (listed above).

### Performance Requirements

- Stats() call: < 1μs (inline operations only)
- Occupancy metric: < 0.1% overhead
- Adaptive chunking: < 1% overhead
- Pause/resume latency: < 10ms
- Channel send with memory check: < 1μs overhead
- Full query overhead: < 0.2% (cumulative)

### Breaking Changes

**None planned**. All features are additive:
- Existing APIs unchanged
- New APIs optional
- Metrics automatically collected (no opt-in needed)
- Default behavior preserved

---

## Risk Mitigation

### Risk: Adaptive Chunking Instability

**Mitigation**:
- Wide hysteresis band (20-80%)
- Bounds prevent pathological sizes
- Metrics track every adjustment
- Can disable via config if needed

### Risk: Memory Bound Enforced Too Late

**Mitigation**:
- Check on every send(), not just chunk boundaries
- Record metric immediately
- Error includes details for debugging
- Tests verify enforcement timing

### Risk: Pause/Resume Complexity

**Mitigation**:
- Keep background task simple (select! with pause loop)
- Don't change connection state machine
- Drop cancellation still works
- Can defer to Phase 8.7 if time constrained

### Risk: Metrics Overhead

**Mitigation**:
- All changes use existing metrics crate
- No new dependencies
- Benchmarks verify overhead
- Can disable via metrics exporter config

---

## Success Stories (Expected Outcomes)

**For Operators**:
- Real-time visibility into backpressure via `fraiseql_channel_occupancy_rows`
- Memory limits prevent OOM: `stream.max_memory(1GB).execute()`
- Pause/resume for resource-constrained environments
- Query cancellation completion guarantees

**For Developers**:
- Inline stats API: `stream.stats().items_buffered`
- Adaptive system requires zero configuration
- Clear error messages on memory limits
- Comprehensive metrics for profiling

**For Production**:
- Self-tuning chunk sizes reduce manual configuration
- Backpressure metrics enable alerting
- Memory bounds catch runaway queries
- Pause/resume enables graceful degradation

---

## Notes for Implementation

### JSON Stream Modifications

The JsonStream type currently:
- Owns mpsc::Receiver<Result<Value>>
- Holds _cancel_tx to signal cancellation on drop
- Implements Stream trait

For Phase 8.6, we add:
- Arc<AtomicUsize> for buffer depth tracking
- Arc<AtomicU64> for counters (rows yielded, filtered)
- Arc<AtomicU8> for state tracking
- mpsc::UnboundedSender<PauseCommand> for pause/resume
- Option<usize> for max_memory bound

All additions are Arc-backed (cloneable if needed) and atomic-only (no locks).

### Backward Compatibility

All changes maintain backward compatibility:
- Existing execute() returns JsonStream unchanged
- New methods are optional (stats(), pause(), etc.)
- Metrics collected automatically (invisible to users)
- Default chunk_size behavior preserved
- No memory bounds by default

### Metric Integration

Uses existing `metrics` crate patterns:
- histogram!() for distributions
- counter!() for events
- Labels follow existing conventions
- All metrics named `fraiseql_*`

---

## Deliverables

### Code

- [ ] All 6 sub-phases fully implemented
- [ ] 50+ new tests
- [ ] Zero warnings/clippy issues
- [ ] Tests pass locally + CI

### Documentation

- [ ] Phase 8.6 plan (this file)
- [ ] Updated README with new APIs
- [ ] Streaming guide (stats, memory limits)
- [ ] Adaptive chunking guide
- [ ] Inline code comments
- [ ] Examples in doc comments

### Metrics

- [ ] Phase 8.6 completion report
- [ ] Benchmark results
- [ ] Test coverage report
- [ ] Performance validation

---

## Future Phases (Deferred)

**Phase 8.7: Connection Pooling** (1-2 weeks)
- Connection pool management
- Health checking and auto-reconnect
- Pool statistics

**Phase 8.8: Advanced Error Handling** (1 week)
- Retry policies with exponential backoff
- Circuit breaker pattern

**Phase 8.9: Production Hardening** (2-3 weeks)
- Deployment guides
- Operational best practices
- Production troubleshooting

---

## References

- **Phase 8.5**: Metrics & observability framework
- **Core Design**: `.claude/CLAUDE.md` - Hard invariants
- **Metrics API**: `src/metrics/mod.rs` - Available histogram/counter functions
- **Stream API**: `src/stream/json_stream.rs` - Current implementation
- **Architecture**: Streaming pipeline described in Explore analysis

---

**Plan Status**: Ready for user review and approval.
**Next Step**: User reviews plan, approves approach, implementation begins.
