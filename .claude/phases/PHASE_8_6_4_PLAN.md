# Phase 8.6.4: Adaptive Chunk Sizing — Implementation Plan

**Date**: 2026-01-13
**Status**: PLANNING
**Duration**: 4-5 hours
**Dependencies**: 8.6.1 (occupancy metrics), 8.6.2 (StreamStats), 8.6.3 (memory bounds)

---

## Executive Summary

Phase 8.6.4 implements **self-tuning chunk sizes** that automatically adjust batch sizes based on observed backpressure (channel occupancy). When the channel fills up (producer waiting, consumer slow), chunk_size **decreases** to reduce pressure and lower latency. When the channel empties (consumer starving, producer efficient), chunk_size **increases** to optimize batching and reduce context switches.

**Key Design Decision**: Measurement-based adjustment with wide hysteresis band (20%-80% occupancy threshold) to prevent thrashing and instability.

**Critical Insight**: `chunk_size` controls both the MPSC channel capacity AND the batch size for Postgres row parsing. High occupancy means producer is backed up → reduce batch size. Low occupancy means consumer wants more → increase batch size.

---

## Objectives

1. **Auto-tune chunk_size** based on channel occupancy patterns
2. **Respect memory bounds** from Phase 8.6.3 (never exceed max_memory)
3. **Prevent pathological behavior** with min/max bounds and hysteresis
4. **Provide visibility** via metrics on every adjustment
5. **Maintain composability** with existing phases (orthogonal feature)
6. **Zero performance regression** (< 1% overhead)

---

## Architecture

### Core Components

#### 1. AdaptiveChunking Type (`src/stream/adaptive_chunking.rs` - NEW)

**Critical Semantics** (from code analysis):

`chunk_size` controls **both**:
1. MPSC channel capacity (line 608 in conn.rs): `mpsc::channel(chunk_size)`
2. Batch size for Postgres row parsing (ChunkingStrategy)

**Producer-side flow**:
- Reads rows from Postgres
- Accumulates in RowChunk until `chunk.len() >= chunk_size`
- Parses all rows in batch
- Sends each parsed Value to channel (blocking if channel full)

**Consumer-side flow**:
- Drains from channel via poll_recv
- Occupancy = how many Values buffered in channel at any moment

**Control signal interpretation**:
- **High occupancy** (>80%): Producer waiting on channel capacity, consumer slow to drain
  → **Reduce chunk_size**: smaller batches reduce pressure, lower latency per item

- **Low occupancy** (<20%): Consumer faster than producer, frequent context switches
  → **Increase chunk_size**: larger batches amortize parsing cost, less frequent wakeups

```rust
/// Tracks channel occupancy and decides when to adjust chunk_size
pub struct AdaptiveChunking {
    /// Current chunk size (mutable)
    current_size: usize,

    /// Absolute bounds (never exceed these)
    min_size: usize,          // 16 rows (minimum sensible batch)
    max_size: usize,          // 1024 rows (prevent memory spikes)

    /// Tuning parameters
    adjustment_window: usize, // 50 measurements before adjusting
    measurements: VecDeque<Occupancy>,

    /// Prevents thrashing (hysteresis)
    last_adjustment_time: Option<Instant>,
    min_adjustment_interval: Duration,  // 1 second
}

/// Single observation of channel occupancy
#[derive(Copy, Clone, Debug)]
struct Occupancy {
    percentage: usize,  // 0-100
    items_buffered: usize,
    timestamp: Instant,
}

impl AdaptiveChunking {
    /// Create with default bounds
    pub fn new() -> Self {
        Self {
            current_size: 256,  // Start at default
            min_size: 16,
            max_size: 1024,
            adjustment_window: 50,
            measurements: VecDeque::with_capacity(50),
            last_adjustment_time: None,
            min_adjustment_interval: Duration::from_secs(1),
        }
    }

    /// Record an occupancy observation
    /// Returns Some(new_size) if adjustment should happen
    pub fn observe(&mut self, items_buffered: usize, capacity: usize) -> Option<usize> {
        let pct = (items_buffered * 100) / capacity.max(1);
        self.measurements.push_back(Occupancy {
            percentage: pct,
            items_buffered,
            timestamp: Instant::now(),
        });

        // Keep only last N measurements
        while self.measurements.len() > self.adjustment_window {
            self.measurements.pop_front();
        }

        // Only adjust if we have enough observations
        if self.measurements.len() >= self.adjustment_window {
            if self.should_adjust() {
                return self.calculate_adjustment();
            }
        }

        None
    }

    /// Calculate average occupancy over window
    fn average_occupancy(&self) -> usize {
        if self.measurements.is_empty() {
            return 0;
        }
        let sum: usize = self.measurements.iter().map(|m| m.percentage).sum();
        sum / self.measurements.len()
    }

    /// Check if conditions warrant adjustment
    fn should_adjust(&self) -> bool {
        // Don't adjust too frequently
        if let Some(last_adj) = self.last_adjustment_time {
            if last_adj.elapsed() < self.min_adjustment_interval {
                return false;
            }
        }

        // Only adjust if we have clear signal (outside hysteresis band)
        let avg = self.average_occupancy();
        avg > 80 || avg < 20  // Outside 20-80% band
    }

    /// Decide new chunk size
    fn calculate_adjustment(&mut self) -> Option<usize> {
        let avg = self.average_occupancy();
        let old_size = self.current_size;

        let new_size = if avg > 80 {
            // High occupancy: producer is waiting, consumer is slow
            // → DECREASE chunk_size to reduce pressure and latency
            ((self.current_size as f64 / 1.5).floor() as usize).max(self.min_size)
        } else if avg < 20 {
            // Low occupancy: consumer is fast, producer is lagging
            // → INCREASE chunk_size to optimize batching efficiency
            ((self.current_size as f64 * 1.5).ceil() as usize).min(self.max_size)
        } else {
            old_size
        };

        // Only return if actual change
        if new_size != old_size {
            self.current_size = new_size;
            self.last_adjustment_time = Some(Instant::now());
            self.measurements.clear();  // Reset window after adjustment
            return Some(new_size);
        }

        None
    }

    /// Get current chunk size
    pub fn current_size(&self) -> usize {
        self.current_size
    }
}
```

**Key Design Decisions**:
- ✅ Measurement-based (not threshold-crossing-based) for stability
- ✅ Hysteresis band (20%-80%) prevents frequent oscillation
- ✅ Minimum adjustment interval (1 second) prevents thrashing
- ✅ Clear window reset after adjustment (fresh observations)
- ✅ Conservative adjustment factor (1.5x) - smooth changes
- ✅ Strict bounds (16-1024) - prevent pathological extremes

#### 2. Integration into Connection Layer (`src/connection/conn.rs`)

**Location**: Background task inside `streaming_query()` (lines 616-743)

```rust
// At method start, initialize adaptive chunking
let mut adaptive = AdaptiveChunking::new();

// Main loop
loop {
    tokio::select! {
        _ = cancel_rx.recv() => {
            // Cancellation handling
            break;
        }
        msg_result = self.receive_message() => {
            // ... message handling ...

            chunk.push(json_bytes);

            // Check if chunk should be flushed
            let should_flush = if chunk.is_full(&strategy) {
                // Traditional: chunk is at capacity
                true
            } else {
                // Could add early flush on timeout here later
                false
            };

            if should_flush {
                // ... send chunk to channel ...

                // ADAPTIVE CHUNKING: Observe occupancy and potentially adjust
                let occupancy = result_tx.len();
                if let Some(new_size) = adaptive.observe(occupancy, chunk_size) {
                    // Adjustment needed!
                    chunk_size = new_size;
                    strategy = ChunkingStrategy::new(new_size);

                    // Record metric
                    crate::metrics::counters::adaptive_chunk_adjusted(
                        &entity,
                        old_size,
                        new_size,
                    );

                    tracing::debug!(
                        entity = &entity,
                        old_size = old_size,
                        new_size = new_size,
                        "chunk size adjusted"
                    );
                }

                chunk = strategy.new_chunk();
            }
        }
    }
}
```

**Key Integration Points**:
1. Create `AdaptiveChunking` before loop
2. Observe occupancy after each chunk send
3. If adjustment returned, update both `chunk_size` and `strategy`
4. Record metric with labels (entity, old_size, new_size)
5. Tracing for debugging

#### 3. Metrics (`src/metrics/counters.rs`)

```rust
/// Record a chunk size adjustment
pub fn adaptive_chunk_adjusted(
    entity: &str,
    old_size: usize,
    new_size: usize,
) {
    let direction = if new_size > old_size { "increase" } else { "decrease" };

    counter!(
        "fraiseql_adaptive_chunk_adjusted_total",
        labels::ENTITY => entity.to_string(),
        "direction" => direction.to_string(),
        "old_size" => old_size.to_string(),
        "new_size" => new_size.to_string(),
    )
    .increment(1);
}
```

**Metric Name**: `fraiseql_adaptive_chunk_adjusted_total{entity, direction, old_size, new_size}`

**Use Cases**:
- Alert if adjustments happen too frequently (sign of instability)
- Monitor distribution of old_size/new_size to understand patterns
- Track per-entity adaptation behavior

#### 4. QueryBuilder API (`src/client/query_builder.rs`)

```rust
pub struct QueryBuilder {
    // ... existing fields ...
    enable_adaptive_chunking: bool,  // NEW: default true
    adaptive_min_chunk_size: Option<usize>,  // NEW: override default min
    adaptive_max_chunk_size: Option<usize>,  // NEW: override default max
}

impl QueryBuilder {
    /// Enable or disable adaptive chunk sizing (default: enabled)
    pub fn adaptive_chunking(mut self, enabled: bool) -> Self {
        self.enable_adaptive_chunking = enabled;
        self
    }

    /// Override minimum chunk size for adaptation (default: 16)
    pub fn adaptive_min_size(mut self, size: usize) -> Self {
        self.adaptive_min_chunk_size = Some(size);
        self
    }

    /// Override maximum chunk size for adaptation (default: 1024)
    pub fn adaptive_max_size(mut self, size: usize) -> Self {
        self.adaptive_max_chunk_size = Some(size);
        self
    }
}
```

**API Design**:
- ✅ Adaptive enabled by default (opt-out, not opt-in)
- ✅ Optional bounds override for power users
- ✅ Simple, discoverable methods
- ✅ Backward compatible (can disable if issues arise)

#### 5. Connection Method Signature

```rust
pub async fn streaming_query(
    mut self,
    query: &str,
    chunk_size: usize,
    max_memory: Option<usize>,
    enable_adaptive_chunking: bool,              // NEW
    adaptive_min_chunk_size: Option<usize>,     // NEW
    adaptive_max_chunk_size: Option<usize>,     // NEW
) -> Result<crate::stream::JsonStream> {
    // ... initialization ...
}
```

---

## Implementation Steps

### Step 1: Create AdaptiveChunking Module (30 min)
- [ ] Create `src/stream/adaptive_chunking.rs`
- [ ] Implement `AdaptiveChunking` type with all methods
- [ ] Add docstrings and examples
- [ ] Export from `src/stream/mod.rs`

### Step 2: Add Metrics (15 min)
- [ ] Add `adaptive_chunk_adjusted()` to `src/metrics/counters.rs`
- [ ] Verify metric naming conventions
- [ ] Check label cardinality (size values could be high - mitigate)

### Step 3: Extend QueryBuilder (20 min)
- [ ] Add fields to `QueryBuilder` struct
- [ ] Implement three new methods
- [ ] Pass values through to connection layer
- [ ] Update docstrings

### Step 4: Integrate into Connection (40 min)
- [ ] Create `AdaptiveChunking` instance
- [ ] Observe occupancy after each chunk send
- [ ] Handle adjustment returns
- [ ] Update chunk_size and strategy
- [ ] Record metrics
- [ ] Add tracing logs

### Step 5: Comprehensive Testing (60 min)
- [ ] Unit tests for `AdaptiveChunking` logic
- [ ] Integration tests for end-to-end behavior
- [ ] Test occupancy thresholds (10%, 30%, 70%, 90%)
- [ ] Test bounds enforcement (can't go below 16 or above 1024)
- [ ] Test hysteresis (no adjustment in 20-80% band)
- [ ] Test min adjustment interval (don't thrash)
- [ ] Test metrics are recorded correctly
- [ ] Test metrics with custom bounds
- [ ] Test composition with max_memory
- [ ] Stress test with rapid occupancy changes

### Step 6: Verify & Document (30 min)
- [ ] Run full test suite
- [ ] Run benchmarks (verify < 1% overhead)
- [ ] Check for clippy warnings
- [ ] Update README if needed
- [ ] Write completion report

---

## Testing Strategy

### Unit Tests (`tests/adaptive_chunking_unit.rs`)

```rust
#[test]
fn test_adaptive_no_adjustment_in_hysteresis_band() {
    let mut adaptive = AdaptiveChunking::new();

    // Simulate occupancy at 50% (inside 20-80% band)
    for _ in 0..50 {
        assert_eq!(adaptive.observe(128, 256), None);
    }

    // No adjustment should happen
    assert_eq!(adaptive.current_size(), 256);
}

#[test]
fn test_adaptive_decreases_on_high_occupancy() {
    let mut adaptive = AdaptiveChunking::new();
    let original_size = 256;

    // Simulate 90% occupancy (producer backed up, consumer slow)
    for _ in 0..49 {
        adaptive.observe(230, 256);
    }

    // Should trigger DECREASE to reduce pressure
    let result = adaptive.observe(230, 256);
    assert!(result.is_some());

    let new_size = result.unwrap();
    assert!(new_size < original_size);  // INVERTED: decrease on high
    assert!(new_size >= 16);
}

#[test]
fn test_adaptive_increases_on_low_occupancy() {
    let mut adaptive = AdaptiveChunking::new();

    // Simulate 10% occupancy (consumer fast, producer efficient)
    for _ in 0..49 {
        adaptive.observe(26, 256);
    }

    // Should trigger INCREASE to optimize batching
    let result = adaptive.observe(26, 256);
    assert!(result.is_some());

    let new_size = result.unwrap();
    assert!(new_size > 256);  // INVERTED: increase on low
    assert!(new_size <= 1024);
}

#[test]
fn test_adaptive_respects_bounds() {
    let mut adaptive = AdaptiveChunking::new();

    // Try to increase beyond max
    for _ in 0..500 {
        adaptive.observe(250, 256);
    }

    assert!(adaptive.current_size() <= 1024);
}

#[test]
fn test_adaptive_min_adjustment_interval() {
    let mut adaptive = AdaptiveChunking::new();

    // First adjustment
    for _ in 0..50 {
        adaptive.observe(230, 256);
    }
    let first = adaptive.observe(230, 256);
    assert!(first.is_some());

    let first_size = adaptive.current_size();

    // Immediately try again
    for _ in 0..50 {
        adaptive.observe(230, 256);
    }
    let second = adaptive.observe(230, 256);

    // Should not adjust immediately
    assert_eq!(second, None);
    assert_eq!(adaptive.current_size(), first_size);
}
```

### Integration Tests (`tests/adaptive_chunking_integration.rs`)

```rust
#[tokio::test]
async fn test_adaptive_chunking_with_real_query() {
    // Setup: Connect to test DB, create test data
    let client = FraiseClient::connect(TEST_DB_URL).await.unwrap();

    // Execute query with adaptive chunking enabled (default)
    let stream = client
        .query::<TestEntity>("entities")
        .adaptive_chunking(true)
        .chunk_size(256)
        .execute()
        .await
        .unwrap();

    let mut count = 0;
    let mut errors = vec![];

    while let Some(result) = stream.next().await {
        match result {
            Ok(_) => count += 1,
            Err(e) => errors.push(e),
        }
    }

    // Should consume successfully
    assert!(count > 0);
    assert!(errors.is_empty());
}

#[tokio::test]
async fn test_adaptive_chunking_disabled() {
    let client = FraiseClient::connect(TEST_DB_URL).await.unwrap();

    let stream = client
        .query::<TestEntity>("entities")
        .adaptive_chunking(false)  // Disabled
        .chunk_size(256)
        .execute()
        .await
        .unwrap();

    // Should work normally, just no adaptive behavior
    let mut count = 0;
    while let Some(result) = stream.next().await {
        if let Ok(_) = result {
            count += 1;
        }
    }

    assert!(count > 0);
}

#[tokio::test]
async fn test_adaptive_with_custom_bounds() {
    let client = FraiseClient::connect(TEST_DB_URL).await.unwrap();

    let stream = client
        .query::<TestEntity>("entities")
        .adaptive_chunking(true)
        .adaptive_min_size(32)   // Override default 16
        .adaptive_max_size(512)  // Override default 1024
        .chunk_size(256)
        .execute()
        .await
        .unwrap();

    let mut count = 0;
    while let Some(result) = stream.next().await {
        if let Ok(_) = result {
            count += 1;
        }
    }

    assert!(count > 0);
}

#[tokio::test]
async fn test_adaptive_respects_memory_bounds() {
    let client = FraiseClient::connect(TEST_DB_URL).await.unwrap();

    let stream = client
        .query::<TestEntity>("entities")
        .adaptive_chunking(true)
        .max_memory(100_000_000)  // 100 MB hard limit
        .chunk_size(256)
        .execute()
        .await
        .unwrap();

    // Should never hit memory limit even if adaptive increases chunk size
    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(_) => count += 1,
            Err(crate::error::Error::MemoryLimitExceeded { .. }) => {
                // Should not happen if adaptive respects bounds
                panic!("Hit memory limit despite adaptive tuning");
            }
            Err(e) => return Err(e),
        }
    }

    assert!(count > 0);
}

#[tokio::test]
async fn test_metrics_recorded_on_adjustment() {
    // Reset metrics
    let metrics_before = get_metric("fraiseql_adaptive_chunk_adjusted_total");

    let client = FraiseClient::connect(TEST_DB_URL).await.unwrap();
    let stream = client
        .query::<TestEntity>("entities")
        .adaptive_chunking(true)
        .chunk_size(256)
        .execute()
        .await
        .unwrap();

    // Consume all
    let mut count = 0;
    while let Some(result) = stream.next().await {
        if let Ok(_) = result {
            count += 1;
        }
    }

    // Check metrics
    let metrics_after = get_metric("fraiseql_adaptive_chunk_adjusted_total");

    // If adjustments happened, metric should increase
    // (May be 0 if occupancy stayed in hysteresis band)
    assert!(metrics_after >= metrics_before);
}
```

---

## Files to Create/Modify

### Create

| File | Purpose | Size |
|------|---------|------|
| `src/stream/adaptive_chunking.rs` | Core adaptive logic | ~200 lines |
| `tests/adaptive_chunking_unit.rs` | Unit tests | ~150 lines |
| `tests/adaptive_chunking_integration.rs` | Integration tests | ~200 lines |

### Modify

| File | Changes | Size |
|------|---------|------|
| `src/stream/mod.rs` | Export `AdaptiveChunking` | +2 lines |
| `src/client/query_builder.rs` | Add 3 new fields & methods | +40 lines |
| `src/client/fraise_client.rs` | Pass adaptive params | +5 lines |
| `src/connection/conn.rs` | Integrate adaptive logic | +50 lines |
| `src/metrics/counters.rs` | Add metric function | +10 lines |

**Total**: ~650 lines (400 new + 250 modified)

---

## Acceptance Criteria

- [ ] `AdaptiveChunking` type fully implemented
- [ ] All three QueryBuilder methods work correctly
- [ ] Metric recorded on every adjustment
- [ ] Bounds enforced (16 ≤ size ≤ 1024)
- [ ] Hysteresis works (no adjustment in 20-80% band)
- [ ] Min adjustment interval respected (1 second)
- [ ] Unit tests pass (6+ tests)
- [ ] Integration tests pass (4+ tests)
- [ ] Composition with max_memory verified
- [ ] Benchmarks show < 1% overhead
- [ ] All existing tests still pass (zero regressions)
- [ ] No clippy warnings
- [ ] Code reviewed and documented

---

## Performance Expectations

| Operation | Cost | Justification |
|-----------|------|---------------|
| Observe occupancy | O(1) | VecDeque push_back + optional calc |
| Calculate adjustment | O(n) where n=50 | Sum over window, O(1) in practice |
| Channel occupancy read | O(1) | Atomic load via receiver.len() |
| Metric record | O(1) | Counter increment |
| Total per-chunk overhead | < 1μs | Negligible compared to network I/O |

**Cumulative Overhead**: < 0.5% on full query (measurement-based, not every poll)

---

## Risk Mitigation

### Risk: Unstable Oscillation
**Mitigation**: Wide hysteresis band (20%-80%), min adjustment interval (1s), window reset

### Risk: Chunk Size Too Large (Memory Issues)
**Mitigation**: Hard max bound (1024), respect memory limit from 8.6.3

### Risk: Chunk Size Too Small (High Overhead)
**Mitigation**: Hard min bound (16), start conservative (256)

### Risk: Adjustment Decision Delayed
**Mitigation**: Measurement window = 50 observations (reasonable latency)

### Risk: Metric Label Cardinality
**Mitigation**: old_size/new_size are discrete values (16,24,36,54,...) - bounded set

---

## Composition with Phase 8.6.3 (Memory Bounds)

**Critical Integration**:

```
max_memory = 500 MB (from phase 8.6.3)
  ↓
max_items = 500_000_000 / 2048 ≈ 244K items
  ↓
adaptive.max_size = min(1024, max_items) = 1024
```

If memory limit is very tight:
```
max_memory = 50 MB
  ↓
max_items = 50_000_000 / 2048 ≈ 24K items
  ↓
adaptive.max_size = min(1024, max_items) = 1024 (still valid)
```

**Fallback strategy**: If max_memory would create max_size < 16:
- Don't enable adaptive chunking (disable automatically)
- Fall back to fixed chunk_size
- Record warning metric

---

## Expected Outcomes

### For Developers
- Zero configuration needed (adaptive by default)
- Can disable if needed: `.adaptive_chunking(false)`
- Can fine-tune bounds if needed: `.adaptive_min_size()`, `.adaptive_max_size()`

### For Operators
- Reduced need to manually tune chunk_size
- Metrics show adaptation behavior
- Memory limits still respected (hard ceiling)

### For Production
- Self-tuning reduces operational burden
- Backpressure handled gracefully
- Clear observability via metrics

---

## Success Metrics

1. **Correctness**: 100% test pass rate, zero regressions
2. **Performance**: < 1% overhead vs Phase 8.6.3
3. **Stability**: Metrics show smooth adaptation, no thrashing
4. **Observability**: At least 10 adaptive chunk adjustments across test suite

---

## Notes for Implementation

### JSON Stream Interaction
- Adaptive chunking is background-task-only
- No changes to JsonStream itself needed
- No changes to consumer API (poll_next, stats, etc.)

### Backward Compatibility
- All changes are additive
- Adaptive enabled by default (safe default)
- Can disable completely
- Existing code works unchanged

### Future Extensions (Beyond 8.6.4)
- Measurement-based chunk size (adjust based on JSON size)
- Time-based chunk flush (reduce latency if occupancy stays low)
- Different occupancy thresholds per entity type
- Integration with 8.6.5 (Pause/Resume) feedback

---

**Plan Status**: Ready for review and approval.

**Next Step**: User reviews plan, provides feedback if needed, approval to proceed with implementation.
