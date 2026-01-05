# Commit 4.5: Architecture Decision - Axum vs FastAPI

**Decision**: Implement GraphQL Operation Monitoring in **Rust (Axum)**, not Python (FastAPI)

**Date**: January 4, 2026
**Status**: ✅ Approved

---

## Executive Summary

### The Question
Where should we implement mutation slow detection and GraphQL operation monitoring?

### The Options
1. **Option A**: Python layer (FastAPI) - Mirrors Python Phase 19 work
2. **Option B**: Rust layer (Axum) - New direction for observability
3. **Option C**: Both (pluggable) - Support both frameworks

### The Decision
✅ **Option B: Rust (Axum) as primary, with optional FastAPI**

### Why Axum?

| Criteria | Axum | FastAPI | Winner |
|----------|------|---------|--------|
| **Performance** | Native async, no GIL | GIL overhead | 🏆 Axum |
| **Type Safety** | Compile-time | Runtime | 🏆 Axum |
| **Trace Context** | Direct integration | Middleware layer | 🏆 Axum |
| **HTTP/2** | Native | Via Uvicorn | 🏆 Axum |
| **Serialization Speed** | Serde (~100ns) | Pydantic (~1µs) | 🏆 Axum |
| **Request Interception** | Easy (extractors) | Easy (middleware) | 🤝 Tie |
| **Python Integration** | Via PyO3 bridge | Native | 🏆 FastAPI |
| **Developer Familiarity** | Newer framework | Established | 🏆 FastAPI |

### The Strategic Advantage

**Axum allows us to:**
1. **Measure full operation latency** - At HTTP handler before any Python GIL
2. **Integrate with trace context** directly - No bridging overhead
3. **Detect slow mutations reliably** - Before Python serialization overhead
4. **Future-proof for Axum-only** - Gradually move from Python to Rust
5. **No "pluggable server" complexity** - Settle on Axum as the primary

---

## Architecture Overview

### Layer Separation

```
┌────────────────────────────────────────────────────┐
│ HTTP/Axum Layer (Rust)              [Phase 16-18]  │
│ • Request/Response handling                         │
│ • WebSocket support                                 │
│ • Connection pooling                               │
├────────────────────────────────────────────────────┤
│ COMMIT 4.5: GraphQL Operation Monitoring [Phase 19]│
│ • Intercept all GraphQL operations                 │
│ • Extract operation type (query/mutation/sub)     │
│ • Start timing and span                            │
├────────────────────────────────────────────────────┤
│ GraphQL Pipeline (Rust + PyO3)      [Phase 1-15]  │
│ • Schema resolution                                │
│ • Query execution                                  │
│ • Field resolution                                 │
│ • Authorization checks                             │
├────────────────────────────────────────────────────┤
│ COMMIT 4.5: Record Metrics                        │
│ • Calculate duration                               │
│ • Count fields/errors                             │
│ • Detect slow mutations                            │
│ • Link to trace context                            │
├────────────────────────────────────────────────────┤
│ Response Construction                              │
│ • Serialize to JSON                                │
│ • Add trace headers                               │
├────────────────────────────────────────────────────┤
│ HTTP Response                                      │
└────────────────────────────────────────────────────┘
```

### Benefits of This Design

1. **Accurate Latency Measurement**
   - Captures total operation time
   - Including Python GIL contention
   - Before response serialization overhead

2. **Clean Separation of Concerns**
   - HTTP layer (Axum): Request routing, connection management
   - Monitoring layer (Commit 4.5): Metrics collection
   - GraphQL layer (Phase 1-15): Operation execution
   - Database layer (Commit 4): Query monitoring

3. **Trace Context Integration**
   - W3C Trace Context headers (Commit 2) available at Axum level
   - Bind operation metrics to distributed traces
   - No overhead for context passing

4. **Future-Proof Architecture**
   - Easy to move more Python logic to Rust
   - No FastAPI-specific patterns
   - Can eventually drop Python FastAPI completely

---

## Comparison: Axum vs FastAPI Implementations

### Axum (Chosen)

```rust
// In Axum middleware
pub struct OperationMetricsMiddleware {
    config: OperationMonitorConfig,
    metrics_storage: Arc<MetricsStorage>,
}

impl OperationMetricsMiddleware {
    pub async fn extract(&self, request: &GraphQLRequest, headers: &HeaderMap) {
        // 1. Extract operation type and name - O(n) where n = query length
        let (op_type, op_name) = GraphQLOperationDetector::detect(&request.query);

        // 2. Get trace context from W3C headers - O(1)
        let trace_context = extract_trace_context(headers);

        // 3. Start timing - O(1)
        let start = Instant::now();

        // Store in thread-local or Arc<Mutex> for recording after execution
    }

    pub async fn record(&self, metrics: OperationMetrics) {
        // Record timing, detect slow mutation, store metrics
        if metrics.duration_ms > threshold {
            self.emit_slow_operation_alert(&metrics);
        }
    }
}
```

**Advantages:**
- ✅ Native async, no event loop gymnastics
- ✅ Direct access to Axum extractors and state
- ✅ No context passing overhead
- ✅ Trace context available in headers directly
- ✅ Can measure from HTTP request arrival

**Disadvantages:**
- ❌ Requires knowledge of Axum (newer framework)
- ❌ No built-in request/response logging
- ❌ Some middleware patterns different from FastAPI

### FastAPI (Alternative, Not Chosen)

```python
# In FastAPI middleware
class OperationMetricsMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        # 1. Extract body and parse JSON
        body = await request.body()
        graphql_request = json.loads(body)

        # 2. Extract trace context from headers
        trace_context = extract_trace_context(request.headers)

        # 3. Start timing
        start = time.time()

        # 4. Call next middleware/handler
        response = await call_next(request)

        # 5. Record metrics
        duration = time.time() - start
        await self.monitor.record({
            "duration_ms": duration * 1000,
            "operation_type": graphql_request.get("operationType"),
            "trace_id": trace_context.trace_id,
        })

        return response
```

**Advantages:**
- ✅ Familiar Python patterns
- ✅ Easy debugging with print statements
- ✅ No Rust compilation required
- ✅ Works with existing FastAPI ecosystem

**Disadvantages:**
- ❌ BaseHTTPMiddleware overhead (~50-100µs per request)
- ❌ Need to re-parse request body (read consumed)
- ❌ GIL contention for concurrent requests
- ❌ Cannot measure Python serialization as part of operation

---

## Decision Justification

### The Core Problem We're Solving

**Problem**: How do we detect when mutations are slow?

```
Mutation execution timeline:

Request arrives (0ms)
    ↓
Axum handler extracts JSON (+0.1ms)
    ↓
Python GraphQL execution (+45ms) ← Could be slow here
    ↓
Resolve fields (+5ms)
    ↓
Database query (+10ms)
    ↓
Serialize response (+2ms)
    ↓
Axum sends response (total: 62.1ms)

Question: Was it the GraphQL resolver? The database? The serialization?
Answer: We need to measure at GraphQL operation level to know.
```

### Why This Point?

At the **GraphQL operation level** (Commit 4.5):

1. **Captures all overhead** - Includes schema traversal, resolver execution, DB time
2. **Separate from DB monitoring** - Not all slow operations are slow due to DB
3. **Accurate mutation detection** - See mutations specifically
4. **Clean integration point** - After request parsing, before response serialization

### Why Axum Specifically?

Axum is the ideal place because:

1. **We're already there** - Phase 16-18 built the Axum HTTP server
2. **We can measure accurately** - No intermediate layers
3. **Trace context available** - W3C headers in Axum middleware
4. **No GIL contention** - Rust async is pure async, not GIL-constrained
5. **Future-proof** - We're moving observability to the right layer

---

## What About the "Pluggable" Approach?

### Original Idea
Support multiple HTTP servers (FastAPI, Starlette, custom) with pluggable monitoring.

### Reality
- **Phase 16-18**: Implemented Axum HTTP server in Rust
- **Current State**: Axum is the high-performance production server
- **FastAPI**: Now optional for development/backward compatibility

### New Decision
Instead of complex "pluggable server" abstraction:
1. **Primary**: Axum (Rust) - for production observability
2. **Optional**: FastAPI (Python) - for development if needed
3. **No abstraction layer needed** - Keep them separate and simple

### Rationale
- Simpler architecture (no adapter pattern overhead)
- Faster execution (no indirection)
- Clearer code paths (easier to maintain)
- Better performance monitoring (no abstraction overhead)

---

## Implementation Location

### File Structure

```
fraiseql_rs/src/http/
├── axum_server.rs              [Existing]
├── observability_middleware.rs [Existing, to extend]
├── operation_metrics.rs        [NEW - Core metrics]
├── operation_monitor.rs        [NEW - Slow detection]
├── graphql_operation_detector.rs [NEW - Parse operations]
└── tests/
    └── operation_metrics_tests.rs [NEW - 40+ tests]
```

### No Changes Required To
- FastAPI layer (can remain as-is for optional use)
- Python monitoring modules (separate from Axum)
- GraphQL pipeline (unchanged)

---

## Performance Implications

### Per-Operation Overhead
- **Operation type detection**: ~5-10µs (regex on query string)
- **Timing measurement**: ~1µs (Instant::now())
- **Metrics storage**: ~50-100µs (Arc<Mutex> write)
- **Total overhead**: <150µs ≈ 0.15ms

**Target**: <1ms overhead per operation (easily achieved)

### Comparison

| Implementation | Overhead | Reason |
|---|---|---|
| **Axum (Chosen)** | ~0.15ms | Pure Rust async, zero-copy |
| **FastAPI** | ~1-2ms | BaseHTTPMiddleware overhead + GIL |
| **No Monitoring** | 0ms | Baseline |

Axum adds only 0.15% overhead for a 100ms operation.

---

## Integration Points

### With Commit 2 (W3C Trace Context)
```rust
// At Axum handler level, we have direct access to headers
let trace_context = extract_trace_context(&headers);
metrics.trace_id = trace_context.trace_id;
metrics.span_id = generate_span_id();

// Link this operation to distributed trace
emit_span(&trace_context, &metrics);
```

### With Commit 1 (Configuration)
```rust
// Load monitoring config from FraiseQLConfig
let config = get_fraiseql_config();
let monitor = GraphQLOperationMonitor::new(
    OperationMonitorConfig {
        slow_mutation_threshold_ms: config.slow_mutation_threshold_ms,
        slow_query_threshold_ms: config.slow_query_threshold_ms,
        // ...
    }
);
```

### With Commit 4 (DB Monitoring)
Completely separate:
- **Commit 4.5**: Measures GraphQL operation time (total)
- **Commit 4**: Measures SQL query time (subset)

Together they show: Total operation = GraphQL overhead + DB time

### With Commit 5 (Audit Logs)
```rust
// Operation metrics available for audit
if let Some(op_metrics) = &context.operation_metrics {
    audit_log.record(AuditEntry {
        operation_type: op_metrics.operation_type,
        duration: op_metrics.duration_ms,
        status: op_metrics.status,
        // ...
    });
}
```

---

## Risk Assessment

### Low Risk
✅ Axum already implemented and tested (Phase 16-18)
✅ No changes to existing GraphQL pipeline
✅ Metrics are read-only (don't affect execution)
✅ Configurable thresholds (can disable if needed)

### Medium Risk
⚠️ New Rust code in core performance path
   → Mitigated by extensive testing and benchmarking

### Migration Path
1. **Phase 19 Commit 4.5**: Implement in Axum
2. **Phase 19 Commits 1-3**: Keep Python version for compatibility
3. **Future**: Gradually migrate more observability to Rust
4. **Eventually**: Optional Python FastAPI layer only

---

## Success Criteria

- [x] Architecture decided and documented
- [ ] Implementation plan created (detailed in COMMIT-4.5-GRAPHQL-OPERATION-MONITORING.md)
- [ ] 40+ tests written and passing
- [ ] Mutation slow detection working
- [ ] <1ms overhead per operation
- [ ] Integrated with Trace Context (Commit 2)
- [ ] Ready for Audit Logs (Commit 5)

---

## Next Steps

1. **Approve this architecture decision** ✅
2. **Begin implementation** (Commit 4.5 implementation plan)
3. **Write core metrics** (operation_metrics.rs)
4. **Implement monitoring** (operation_monitor.rs)
5. **Add 40+ tests** (comprehensive coverage)
6. **Integration with Commits 1-2** (config, tracing)
7. **Ready for Commit 5** (audit logs)

---

## Related Documents

- `COMMIT-4.5-GRAPHQL-OPERATION-MONITORING.md` - Detailed implementation plan
- `PHASE-19-IMPLEMENTATION-STATUS.md` - Overall progress tracking
- `fraiseql_rs/src/http/mod.rs` - Axum module overview
- `PHASE-19-20-SUMMARY.md` - Phase 19-20 roadmap

---

## Approval

**Decision**: ✅ Implement Commit 4.5 in Rust (Axum) as primary approach

**Rationale**: Better performance, cleaner architecture, aligns with Phase 16-18 Axum implementation, future-proof for Rust-first observability.

**Alternative Accepted**: Optional FastAPI implementation for backward compatibility, but not primary focus.
