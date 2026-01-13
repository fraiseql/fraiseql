# Phase 8.5: Query Metrics & Observability - Implementation Plan

**Date**: 2026-01-13
**Status**: 📋 Ready for Implementation
**Effort**: Low-Medium (3-4 days)
**Complexity**: Medium (new crate integration, distributed instrumentation)

---

## Objective

Add comprehensive metrics collection to fraiseql-wire for production observability, enabling users to:
- Track query execution performance (latency, throughput)
- Monitor error rates and error categories
- Understand deserialization success/failure patterns
- Measure streaming efficiency (rows/chunk, backpressure)
- Integrate with Prometheus/observability stacks

---

## Key Deliverables

1. **`src/metrics/` module** - Metrics collection infrastructure
   - Counter metrics (queries, errors, rows)
   - Histogram metrics (latencies, distributions)
   - Label constants for consistency
   - Public API for user-side metrics

2. **Instrumentation in core paths**
   - Query execution (submit → complete)
   - Error handling (with category labels)
   - Row processing (per chunk, not per row)
   - Deserialization (per type tracking)

3. **Public metrics API**
   - `FraiseClient::metrics()` to retrieve current metrics
   - Prometheus-compatible export format
   - Integration examples

4. **Documentation & Examples**
   - `METRICS.md` - Metrics glossary and interpretation
   - `examples/metrics.rs` - Working example with Prometheus
   - Integration guide for common observability stacks

5. **Tests**
   - Metrics accuracy tests
   - Metrics under error conditions
   - Histogram distribution verification

---

## Architecture & Design

### Design Principles

✅ **Non-intrusive**: Metrics don't change public API or behavior
✅ **Low overhead**: Batch measurements (per chunk, not per row)
✅ **Flexible**: Users can export to Prometheus, OpenTelemetry, etc.
✅ **Accurate**: Account for all phases of query execution
✅ **Observable**: Include relevant labels for filtering/grouping

### Metrics Scope

#### 1. Query-Level Metrics

**Counters:**
- `fraiseql_queries_total` - Total queries submitted (label: entity, has_where_sql, has_where_rust, has_order_by)
- `fraiseql_query_success_total` - Successful query completions (label: entity)
- `fraiseql_query_error_total` - Failed queries (label: entity, error_category)
- `fraiseql_query_cancelled_total` - Cancelled queries (label: entity)

**Histograms:**
- `fraiseql_query_startup_duration_ms` - Time from submit to first DataRow (label: entity)
- `fraiseql_query_total_duration_ms` - Wall-clock query execution time (label: entity)
- `fraiseql_query_rows_processed` - Distribution of row counts per query (label: entity)
- `fraiseql_query_bytes_received` - Distribution of total bytes per query (label: entity)

#### 2. Row Processing Metrics

**Counters:**
- `fraiseql_rows_processed_total` - Rows received from Postgres (label: entity, status: ok/parse_error)
- `fraiseql_rows_filtered_total` - Rows filtered by where_rust() (label: entity)
- `fraiseql_rows_deserialized_total` - Rows successfully deserialized (label: entity, type_name)
- `fraiseql_rows_deserialization_failed_total` - Deserialization failures (label: entity, type_name, reason)

**Histograms:**
- `fraiseql_chunk_processing_duration_ms` - Time to process one chunk (label: entity)
- `fraiseql_chunk_size_rows` - Distribution of rows per chunk (label: entity)
- `fraiseql_json_parse_duration_ms` - JSON parsing time per chunk (label: entity)
- `fraiseql_filter_duration_ms` - Rust filter execution time per chunk (label: entity)
- `fraiseql_deserialization_duration_ms` - Deserialization time per chunk (label: entity, type_name)

#### 3. Error Metrics

**Counters:**
- `fraiseql_errors_total` - All errors (label: category, retriable, phase: auth/startup/query/streaming)
- `fraiseql_protocol_errors_total` - Protocol violations (label: message_type, field)
- `fraiseql_json_parse_errors_total` - JSON parsing failures (label: reason)
- `fraiseql_deserialization_errors_total` - Deserialization failures (label: type_name, reason)

#### 4. Connection-Level Metrics

**Counters:**
- `fraiseql_connections_created_total` - Connections established (label: transport: tcp/unix)
- `fraiseql_connections_failed_total` - Connection failures (label: phase: transport/auth/startup, error_category)
- `fraiseql_authentications_total` - Auth attempts (label: mechanism: cleartext/scram)
- `fraiseql_authentications_successful_total` - Successful auth (label: mechanism)
- `fraiseql_authentications_failed_total` - Failed auth (label: mechanism, reason)

**Gauges:**
- `fraiseql_active_connections` - Current active connections
- `fraiseql_active_queries` - Current active queries
- `fraiseql_connection_state` - Current connection state (label: state)

#### 5. Backpressure & Channel Metrics

**Counters:**
- `fraiseql_channel_send_blocked_total` - Times send() blocked (label: reason: full/backpressure)
- `fraiseql_channel_send_duration_ms_total` - Cumulative send latency (label: entity)

**Histograms:**
- `fraiseql_channel_send_latency_ms` - Per-send latency distribution (measures backpressure)

---

## Implementation Steps

### Phase 8.5.1: Create Metrics Module

**Files to Create:**
- `src/metrics/mod.rs` - Public API, initialization
- `src/metrics/counters.rs` - Counter definitions
- `src/metrics/histograms.rs` - Histogram definitions
- `src/metrics/labels.rs` - Label constants

**Steps:**

1. **Create `src/metrics/mod.rs`**
   ```rust
   pub mod counters;
   pub mod histograms;
   pub mod labels;

   /// Initialize metrics collection
   pub fn init() {
       // Register all metrics
   }

   /// Retrieve current metrics snapshot
   pub fn snapshot() -> MetricsSnapshot {
       // Collect all metrics into a struct
   }

   /// Export metrics in Prometheus format
   pub fn prometheus_export() -> String {
       // Format all metrics for Prometheus scrape endpoint
   }
   ```

2. **Create `src/metrics/counters.rs`**
   ```rust
   use metrics::counter;

   pub fn record_query_submitted(entity: &str, has_where_sql: bool, has_where_rust: bool, has_order_by: bool) {
       counter!("fraiseql_queries_total",
           "entity" => entity.to_string(),
           "has_where_sql" => has_where_sql.to_string(),
           "has_where_rust" => has_where_rust.to_string(),
           "has_order_by" => has_order_by.to_string()
       ).increment(1);
   }

   pub fn record_query_success(entity: &str) { ... }
   pub fn record_query_error(entity: &str, error_category: &str) { ... }
   pub fn record_rows_processed(entity: &str, count: u64, status: &str) { ... }
   pub fn record_rows_filtered(entity: &str, count: u64) { ... }
   // ... etc
   ```

3. **Create `src/metrics/histograms.rs`**
   ```rust
   use metrics::histogram;

   pub fn record_query_startup_duration(entity: &str, duration_ms: u64) {
       histogram!("fraiseql_query_startup_duration_ms",
           "entity" => entity.to_string()
       ).record(duration_ms as f64);
   }

   pub fn record_chunk_processing_duration(entity: &str, duration_ms: u64) { ... }
   pub fn record_deserialization_duration(entity: &str, type_name: &str, duration_ms: u64) { ... }
   // ... etc
   ```

4. **Create `src/metrics/labels.rs`**
   ```rust
   pub const ENTITY_LABEL: &str = "entity";
   pub const ERROR_CATEGORY_LABEL: &str = "error_category";
   pub const TYPE_NAME_LABEL: &str = "type_name";
   pub const TRANSPORT_LABEL: &str = "transport";
   pub const MECHANISM_LABEL: &str = "mechanism";
   pub const STATUS_LABEL: &str = "status";
   // ... etc
   ```

5. **Update `Cargo.toml`**
   ```toml
   [dependencies]
   metrics = "0.22"
   metrics-prometheus = "1.0"  # Optional, for Prometheus support
   ```

6. **Update `src/lib.rs`**
   ```rust
   pub mod metrics;

   pub use metrics::{init_metrics, metrics_snapshot, prometheus_export};
   ```

### Phase 8.5.2: Instrument QueryBuilder

**File**: `src/client/query_builder.rs`

**Changes:**
1. In `QueryBuilder::execute()`:
   ```rust
   pub async fn execute(mut self) -> Result<StreamType> {
       // Record query submission
       let start = Instant::now();
       metrics::counters::record_query_submitted(
           &self.entity,
           self.where_sql.is_some(),
           self.where_rust.is_some(),
           self.order_by.is_some(),
       );

       // ... existing execute logic ...
   }
   ```

### Phase 8.5.3: Instrument Connection

**File**: `src/connection/conn.rs`

**Changes:**
1. In `Connection::streaming_query()`:
   ```rust
   pub async fn streaming_query(
       mut self,
       query: String,
       chunk_size: usize,
   ) -> Result<JsonStream> {
       let startup_start = Instant::now();

       // ... existing startup logic ...

       // After RowDescription validation:
       let startup_duration = startup_start.elapsed();
       metrics::histograms::record_query_startup_duration(
           entity_from_query(&query),
           startup_duration.as_millis() as u64,
       );

       // Spawn background task with metrics recording
       tokio::spawn(async move {
           Self::background_query_task_with_metrics(
               reader,
               sender,
               cancel_rx,
               chunk_size,
               query.clone(),
           ).await
       });

       // ... rest of function ...
   }
   ```

2. In `Connection::authenticate()`:
   ```rust
   async fn authenticate(&mut self, config: &ConnectionConfig) -> Result<()> {
       let auth_start = Instant::now();
       let mechanism = match msg {
           BackendMessage::Authentication(auth) => match auth {
               AuthenticationMessage::CleartextPassword => "cleartext",
               AuthenticationMessage::Sasl { mechanisms } => "scram",
               // ...
           }
       };

       metrics::counters::record_auth_attempted(mechanism);

       // ... existing auth logic ...

       // On success:
       metrics::counters::record_auth_successful(mechanism);
       metrics::histograms::record_auth_duration(mechanism, auth_start.elapsed().as_millis() as u64);

       // On error:
       metrics::counters::record_auth_failed(mechanism, error.category());
   }
   ```

### Phase 8.5.4: Instrument Background Task

**File**: `src/connection/conn.rs` (background_query_task)

**Changes:**
1. Wrap chunk processing:
   ```rust
   async fn background_query_task_with_metrics(
       // ... existing params ...
       entity: String,
   ) {
       let query_start = Instant::now();
       let mut rows_processed = 0u64;
       let mut bytes_received = 0u64;
       let mut rows_in_chunk = 0u64;
       let mut chunk_start = Instant::now();

       loop {
           tokio::select! {
               _ = cancel_rx.recv() => {
                   metrics::counters::record_query_cancelled(&entity);
                   break;
               }
               msg_result = self.receive_message() => {
                   match msg_result {
                       Ok(BackendMessage::DataRow(fields)) => {
                           let chunk_start = Instant::now();

                           // Extract and parse JSON
                           let extracted_start = Instant::now();
                           let bytes = extract_json_bytes(&fields[0])?;
                           bytes_received += bytes.len() as u64;

                           // Parse JSON
                           let parsed_start = Instant::now();
                           let value = parse_json(&bytes)?;
                           let parse_duration = parsed_start.elapsed();

                           // Add to chunk
                           chunk.add(value);
                           rows_in_chunk += 1;
                           rows_processed += 1;

                           // When chunk ready:
                           if chunk.is_full(chunk_size) {
                               metrics::histograms::record_chunk_processing_duration(&entity, chunk_start.elapsed().as_millis() as u64);
                               metrics::histograms::record_chunk_size(&entity, rows_in_chunk);
                               metrics::counters::record_rows_processed(&entity, rows_in_chunk, "ok");

                               sender.send(Ok(value)).await?;
                               chunk.reset();
                               rows_in_chunk = 0;
                               chunk_start = Instant::now();
                           }
                       }
                       Ok(BackendMessage::CommandComplete(_)) => {
                           // Flush remaining rows
                           if rows_in_chunk > 0 {
                               metrics::histograms::record_chunk_processing_duration(&entity, chunk_start.elapsed().as_millis() as u64);
                               metrics::histograms::record_chunk_size(&entity, rows_in_chunk);
                               metrics::counters::record_rows_processed(&entity, rows_in_chunk, "ok");
                           }
                       }
                       Ok(BackendMessage::ReadyForQuery(_)) => {
                           // Query complete
                           let total_duration = query_start.elapsed();
                           metrics::counters::record_query_success(&entity);
                           metrics::histograms::record_query_total_duration(&entity, total_duration.as_millis() as u64);
                           metrics::histograms::record_query_bytes_received(&entity, bytes_received);
                           metrics::histograms::record_query_rows_processed(&entity, rows_processed);
                           break;
                       }
                       Err(e) => {
                           metrics::counters::record_query_error(&entity, e.category());
                           sender.send(Err(e)).await?;
                           break;
                       }
                   }
               }
           }
       }
   }
   ```

### Phase 8.5.5: Instrument Stream Types

**File**: `src/stream/typed_stream.rs`

**Changes:**
1. In `TypedJsonStream::poll_next()`:
   ```rust
   fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<T>>> {
       let deser_start = Instant::now();

       match futures::ready!(self.inner.poll_next_unpin(cx)) {
           Some(Ok(value)) => {
               // Deserialize with metrics
               match serde_json::from_value::<T>(value) {
                   Ok(item) => {
                       metrics::counters::record_deserialization_success(&self.type_name);
                       metrics::histograms::record_deserialization_duration(&self.type_name, deser_start.elapsed().as_millis() as u64);
                       Poll::Ready(Some(Ok(item)))
                   }
                   Err(e) => {
                       metrics::counters::record_deserialization_failure(&self.type_name, &e.to_string());
                       metrics::histograms::record_deserialization_duration(&self.type_name, deser_start.elapsed().as_millis() as u64);
                       Poll::Ready(Some(Err(Error::Deserialization { ... })))
                   }
               }
           }
           Some(Err(e)) => {
               metrics::counters::record_stream_error(&e.category());
               Poll::Ready(Some(Err(e)))
           }
           None => Poll::Ready(None),
       }
   }
   ```

**File**: `src/stream/filter.rs`

**Changes:**
1. In `FilteredStream::poll_next()`:
   ```rust
   fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Value>>> {
       loop {
           let filter_start = Instant::now();

           match futures::ready!(self.inner.poll_next_unpin(cx)) {
               Some(Ok(value)) => {
                   // Apply filter with metrics
                   if (self.predicate)(&value) {
                       metrics::counters::record_row_passed();
                       Poll::Ready(Some(Ok(value)))
                   } else {
                       metrics::counters::record_row_filtered();
                       // Continue to next item
                       continue;
                   }
               }
               Some(Err(e)) => Poll::Ready(Some(Err(e))),
               None => Poll::Ready(None),
           }
       }
   }
   ```

### Phase 8.5.6: Create Tests

**File**: `tests/metrics_integration.rs`

**Tests:**
1. `test_query_metrics_recorded` - Verify metrics captured for query
2. `test_error_metrics_by_category` - Error categorization metrics
3. `test_deserialization_metrics_per_type` - Type-specific tracking
4. `test_chunk_metrics_distribution` - Chunk size distribution
5. `test_prometheus_export_format` - Export format validation
6. `test_metrics_under_error_conditions` - Error path metrics

### Phase 8.5.7: Documentation & Examples

**Files to Create:**

1. **`METRICS.md`** - Comprehensive metrics guide
   - Metric definitions
   - Interpretation guidelines
   - Alert thresholds
   - Integration examples

2. **`examples/metrics.rs`** - Working example
   ```rust
   #[tokio::main]
   async fn main() -> Result<()> {
       // Initialize metrics
       fraiseql_wire::metrics::init();

       // ... query execution ...

       // Export Prometheus metrics
       println!("{}", fraiseql_wire::metrics::prometheus_export());

       Ok(())
   }
   ```

3. **`INTEGRATION_EXAMPLES.md`** - Integration guides
   - Prometheus scrape configuration
   - Grafana dashboard JSON
   - OpenTelemetry bridge
   - DataDog integration

---

## Success Criteria

- ✅ All metric types implemented (counters, histograms, gauges)
- ✅ Metrics recorded at all key execution phases
- ✅ Prometheus export format validated
- ✅ Zero performance regression (< 1% overhead)
- ✅ Comprehensive documentation
- ✅ Working example with Prometheus
- ✅ All tests passing
- ✅ No breaking API changes

---

## Testing Strategy

### Unit Tests
- Test each metric recording function in isolation
- Verify label handling
- Test error conditions

### Integration Tests
- Execute query, verify metrics recorded
- Test error scenarios (auth failures, protocol errors, etc.)
- Test deserialization errors per type
- Verify histogram distributions

### Performance Tests
- Benchmark metrics overhead (should be < 1% for typical queries)
- Verify no allocation in hot paths
- Test under high throughput (1K+ rows/sec)

### Manual Testing
- Prometheus scrape endpoint
- Dashboard visualization
- Alert rule definition

---

## Effort Breakdown

| Task | Effort | Notes |
|------|--------|-------|
| Metrics module design | 4h | Planning + API design |
| Counter implementation | 4h | Define all counters |
| Histogram implementation | 4h | Define all histograms |
| QueryBuilder instrumentation | 3h | Minimal, non-intrusive |
| Connection instrumentation | 6h | Auth + streaming_query |
| Background task instrumentation | 8h | Most complex, careful measurement |
| Stream type instrumentation | 4h | TypedJsonStream, FilteredStream |
| Tests | 6h | Integration + benchmarks |
| Documentation | 4h | METRICS.md + examples |
| Review & iteration | 4h | Testing + refinement |
| **Total** | **43h** | **~5-6 days** |

---

## Risk Mitigation

### Risk: Performance Regression
**Mitigation:**
- All metrics operations are O(1)
- Batch measurements (per chunk, not per row)
- Optional feature flag for high-verbosity metrics
- Benchmark before/after

### Risk: Label Cardinality Explosion
**Mitigation:**
- Carefully limit label dimensions
- Use enum constants for label values
- Document label constraints
- Monitor metric count growth

### Risk: Memory Overhead
**Mitigation:**
- Metrics stored in external crate (bounded)
- No per-query state retained
- Periodic export & reset
- Profile memory usage

### Risk: Measurement Overhead in Hot Paths
**Mitigation:**
- Use batch measurements (per-chunk boundaries)
- Avoid allocation in poll_next()
- Use atomic operations for counters
- Profile hot paths

---

## Next Phase (Phase 8.6)

After Phase 8.5 completes, consider:
- **Phase 8.6: Connection Configuration Enhancements** (if needed)
- **Phase 7.3-7.6: Stabilization** (real-world testing, CI/CD)
- **Phase 8.7: Connection Pooling** (as separate crate)

---

## Implementation Timeline

- **Day 1**: Metrics module + counters + histograms
- **Day 2**: Instrumentation in QueryBuilder, Connection auth
- **Day 3**: Background task instrumentation, stream types
- **Day 4**: Tests + documentation + example
- **Day 5**: Review, iteration, performance validation

---

## Files Modified/Created

### New Files
- `src/metrics/mod.rs`
- `src/metrics/counters.rs`
- `src/metrics/histograms.rs`
- `src/metrics/labels.rs`
- `examples/metrics.rs`
- `tests/metrics_integration.rs`
- `METRICS.md`
- `INTEGRATION_EXAMPLES.md`

### Modified Files
- `Cargo.toml` - Add metrics crate
- `src/lib.rs` - Export metrics module
- `src/client/query_builder.rs` - Record query submission
- `src/connection/conn.rs` - Record auth, startup, completion
- `src/stream/typed_stream.rs` - Record deserialization
- `src/stream/filter.rs` - Record filtering
- `README.md` - Mention metrics feature

---

## References & Resources

- [metrics crate docs](https://docs.rs/metrics/)
- [Prometheus metric types](https://prometheus.io/docs/concepts/metric_types/)
- [OpenTelemetry spec](https://opentelemetry.io/docs/reference/specification/)
- [Instrumentation best practices](https://opentelemetry.io/docs/instrumentation/rust/)

---

**Ready to proceed with Phase 8.5 implementation!**
