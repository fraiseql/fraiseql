# Federation Observability Plan: Critical Review

## Executive Summary

The plan is comprehensive in scope but lacks implementation rigor. It reads more as a "wish list" than a detailed technical specification. Key issues: **metric cardinality explosion**, **missing implementation details**, **undefined metrics**, **incomplete test strategy**, and **missing runbooks**.

---

## Critical Issues (Must Fix)

### 1. Metric Cardinality Explosion ⚠️

**Problem**: The plan specifies far too many labels on metrics, creating exponential cardinality growth.

**Example**:
```
federation_entity_resolution_duration_ms
  - By: typename, resolution_strategy, hop_level, subgraph

If: 50 types × 3 strategies × 3 hop_levels × 3 subgraphs = 1,350 combinations
```

This will overwhelm Prometheus storage and slow down queries.

**Impact**:
- Storage bloat (1000x growth with real-world cardinality)
- Query timeout in dashboards
- Memory exhaustion in Prometheus instance

**Fix Needed**:
1. Reduce labels per metric (max 2-3 per metric)
2. Separate high-cardinality concerns:
   - `federation_entity_resolutions_total` - By: resolution_strategy, status
   - `federation_entity_resolution_duration_ms` - By: resolution_strategy
   - Typename/subgraph tracked in structured logs, not metrics

3. For attributes that vary widely (typename), use:
   - Structured logs instead of metrics
   - Or use lower-cardinality categories (e.g., "user-type", "order-type", "product-type")

### 2. Undefined/Inconsistent Metrics 🔴

**Problem**: Some metrics are mathematically undefined or contradictory.

**Examples**:

1. **`federation_query_overhead_ms`**
   ```
   Definition: total_federation_latency - local_subgraph_latencies

   Issue: "local_subgraph_latencies" is NOT a metric defined anywhere
   ```
   - How do we extract raw subgraph latency without federation overhead?
   - This metric is compute-impossible without changing architecture

2. **`federation_entity_deduplication_ratio`** (gauge)
   ```
   How to compute: deduplicated_count / original_count

   Issue: This should be a counter not a gauge
   - Gauge implies current state (what's currently deduplicated?)
   - Should track: entities_before_dedup / entities_after_dedup as counter per batch
   ```

3. **`federation_query_hop_latency_ms`**
   ```
   Labels: hop_level, subgraph_name

   Issue: "hop_level" and "subgraph_name" don't correlate
   - Hop level is about query depth (1st, 2nd, 3rd call)
   - Subgraph name is which subgraph we're calling
   - In a 3-hop query, which hop/subgraph combination are we measuring?
   - Same subgraph might be queried at different hop levels
   ```

**Fix Needed**:
1. Define metrics precisely with examples
2. Specify exact calculation method
3. Validate that metric computation is possible with available data
4. Remove impossible metrics

### 3. Missing Health Check Implementation Details 📋

**Problem**: The plan mentions a "background health check runner" but leaves critical details undefined.

**Undefined**:
- ✗ How often does it run? (Every 5s? 30s? 1m?)
- ✗ Where does it run? (Main thread? Separate tokio task?)
- ✗ How is it triggered? (Interval? Event-based?)
- ✗ What data structure tracks error counts? (Circular buffer? Time-window aggregator?)
- ✗ How do we compute `error_count_last_minute`? (Needs time-series window)
- ✗ Thread-safety? (Concurrent updates to health status?)

**Impact**: Cannot implement Phase 3 without these details.

**Fix Needed**:
1. Specify health check interval (suggest: 30 seconds for readiness, 5 seconds for liveness)
2. Specify implementation approach (tokio::spawn background task)
3. Specify error count window (rolling 60-second window with bucket-based aggregation)
4. Provide pseudocode for health check runner
5. Specify thread-safe data structure (likely Mutex<VecDeque<ErrorTimestamp>>)

### 4. Tracing Backend Not Specified 🔌

**Problem**: Plan mentions W3C Trace Context but doesn't specify where traces go.

**Undefined**:
- ✗ Which tracing backend? (Jaeger? Tempo? OpenTelemetry Collector?)
- ✗ How are traces exported? (HTTP? gRPC? OTLP?)
- ✗ Sampling strategy? (Sample all? 10%? Adaptive?)
- ✗ Trace retention? (1 hour? 24 hours?)
- ✗ What if tracing fails? (Fallback behavior?)

**Impact**: Phase 1 cannot proceed without backend choice.

**Fix Needed**:
1. Choose tracing backend (Jaeger for simplicity, Tempo for scalability)
2. Specify export protocol (OTLP HTTP for Tempo)
3. Define sampling strategy (all federation queries in dev, 10% in production)
4. Define retention (4 hours for federation traces)
5. Document failure handling (skip tracing, don't fail query)

### 5. Test Strategy Incomplete 🧪

**Problem**: Test lists are just checkboxes with no actual test cases defined.

**Example**:
```
**Tests**:
- [ ] Trace creation for entity resolution
- [ ] Trace propagation through HTTP calls
- [ ] Span attributes correctly set
- [ ] Trace context in subgraph requests
```

This is useless without:
- Actual test code
- Test data (sample entities, queries)
- Assertions (what makes test pass?)
- Edge cases (empty batches, errors, timeouts)

**Impact**: Phase 6 validation cannot be verified, quality is unknown.

**Fix Needed**:
1. Specify actual test scenarios:
   ```
   test_federation_entity_resolution_creates_span:
     - Input: 10 User entities, DB resolution
     - Expected: Span "federation.resolve_db_batch" created with:
       - Attribute: typename = "User"
       - Attribute: entity_count = 10
       - Metric: federation_entity_resolutions_total incremented
       - Duration recorded in federation_entity_resolution_duration_ms
   ```

2. Specify edge cases tested
3. Specify performance acceptance criteria (e.g., overhead < 5%)

### 6. Mutation Metrics Reference Non-Existent Features 🚫

**Problem**: Plan references mutation conflict detection and replication status that don't exist.

**Undefined**:
```
federation_mutation_conflicts_total
  - By: mutation_type, typename, conflict_type (version/constraint)
```

In `federation_docker_compose_integration.rs`, there's no conflict detection logic implemented.

**Also**: Dashboard 5.4 references "Replication Status" and "sync failures per subgraph" which aren't mutation executor features.

**Impact**: Cannot implement Phase 5 dashboards or Phase 6 tests for features that don't exist.

**Fix Needed**:
1. Remove mutation conflict metrics until conflict detection is implemented
2. Remove replication/sync metrics from dashboard unless mutation executor supports it
3. Focus on metrics that match implemented functionality
4. Flag as "future work" what isn't implemented yet

### 7. Alert Rules Not Validated Against SLOs ❌

**Problem**: Alert thresholds don't align with SLO targets.

**Example**:
```
SLO:  3-hop query < 250ms p99
Alert: federation_query_total_latency_ms[5m:p99] > 300ms

Issue: Alert fires at 300ms but SLO target is 250ms
       Means we're already failing SLO before alert fires (50ms violation)
```

**Another example**:
```
Alert: federation_query_hop_latency_ms{hop_level=~"2|3"}[5m:p99] > 150ms

Issue: But 3-hop queries should be < 250ms
       If we alert at 150ms per hop, that's too aggressive (3×150=450ms total)
       But if we only alert 2-hop, 3-hop queries could be slow and not alert
```

**Impact**: Alerts either miss problems or fire too often (alert fatigue).

**Fix Needed**:
1. Align alert thresholds to SLO targets:
   - 1-hop: SLO 30ms p99 → Alert at 45ms (1.5× SLO)
   - 2-hop: SLO 100ms p99 → Alert at 150ms (1.5× SLO)
   - 3-hop: SLO 250ms p99 → Alert at 375ms (1.5× SLO)

2. Clarify "per-hop" latency alerts - are these measuring individual calls or cumulative?

3. Add alert for "SLO window" metric (if we have recent SLO violation)

### 8. Span Attribute Over-specification 📊

**Problem**: The span hierarchy specifies too many attributes per span, which impacts performance.

**Example**:
```
Subgraph Request Span specifies:
- federation.subgraph_name: string
- federation.operation_type: "query" | "mutation"
- federation.entity_count: int
- federation.http_status: int
- federation.http_duration_ms: float
- federation.response_entity_count: int
- federation.response_error_count: int
```

But the code pattern shown only adds 2 attributes. This inconsistency will cause either:
1. Incomplete implementation (missing attributes)
2. Span bloat (too much data per span)

**Impact**: Either spans don't have needed info, or tracing becomes heavyweight.

**Fix Needed**:
1. Audit which attributes are actually needed for troubleshooting
2. Keep essential attributes only (name, duration, status, entity_count)
3. Put detailed data (http_status, response_entity_count) in logs, not spans
4. Limit to 5-7 attributes per span max

---

## Medium Issues (Should Fix)

### 9. Missing Connection Pool Metrics

The plan doesn't monitor subgraph HTTP connection pool health, which is critical:
- Are connections being reused?
- Is the pool saturated?
- Are connections timing out?

**Fix**: Add metrics:
```
federation_http_pool_connections_active: Gauge
federation_http_pool_connections_idle: Gauge
federation_http_pool_wait_time_ms: Histogram
```

### 10. Query Complexity Not Tracked

The plan doesn't measure query complexity (field count, nesting depth), which affects:
- Entity resolution strategy choice
- Performance expectations
- Cache hit rates

**Fix**: Add metric:
```
federation_query_complexity: Histogram
  - By: complexity_bucket (simple/medium/complex)
```

### 11. Inconsistent Terminology

The plan uses multiple terms for similar concepts:
- "hop_level" vs "max_hops" vs "max_hop_level"
- "subgraph_count" vs "subgraph_name"
- "typename" vs "type_name"

**Fix**: Define glossary and use consistently

### 12. Missing: Runbook Documentation

Phase 5 says "Alert runbooks" should be created, but they're not in the plan. The reference file `FEDERATION_OBSERVABILITY_RUNBOOK.md` doesn't exist.

**Fix**: Include actual runbook format:
```
## Alert: High Federation Query Latency

**Severity**: Warning
**Threshold**: P99 > 300ms for 5 minutes

### Troubleshooting Steps
1. Check subgraph health: /health/federation
2. Query slowest subgraph directly
3. Check connection pool stats
4. Review query complexity
5. Check if rate-limiting active

### Escalation Path
- If persists > 15min: page on-call
- Check if data migration in progress
```

### 13. Dashboard Feasibility Concerns

Some proposed dashboards are complex to implement:

**Dashboard 5.5 (Heatmap)**:
- Requires aggregating P99 latency over time buckets
- Needs color scale mapping (green/yellow/red)
- May be difficult to query efficiently in Grafana

**Dashboard 5.2 (Entity Resolution)**:
- "Slowest types to resolve" - requires sorting/ranking
- "Strategy Distribution" pie chart - need high-cardinality tracking (per-type)
- "Cache entries growth trend" - need time-series of cache size

**Fix**: Prioritize dashboards and mark complex ones as "phase 2"

### 14. Overhead Budget Not Defined

The plan doesn't specify:
- Maximum acceptable observability overhead
- How to measure overhead
- Fallback if overhead is too high

**Suggestion**: Define overhead budget:
- Tracing overhead: < 1% latency increase
- Metrics recording: < 0.5% latency increase
- Health checks: < 2% CPU utilization

---

## Minor Issues (Nice to Have)

### 15. Configuration Not Discussed

Should observability be configurable?
- Enable/disable by feature flag?
- Sampling rates configurable?
- Log level per module?

### 16. Backwards Compatibility

How do we add instrumentation without breaking existing code?
- All new metrics fields should be optional?
- Errors in observability should not fail queries?

### 17. Performance Benchmarks

Phase 6 mentions "performance benchmarks within threshold" but:
- No baseline defined
- No acceptance criteria specified
- Which operations are benchmarked?

---

## Issues by Severity

| Issue | Severity | Impact | Status |
|-------|----------|--------|--------|
| Metric cardinality explosion | 🔴 Critical | Prometheus unusable | Must fix |
| Undefined metrics | 🔴 Critical | Cannot implement | Must fix |
| Missing health check details | 🔴 Critical | Phase 3 blocked | Must fix |
| Tracing backend not specified | 🔴 Critical | Phase 1 blocked | Must fix |
| Test strategy incomplete | 🔴 Critical | Quality unknown | Must fix |
| Mutation metrics non-existent | 🔴 Critical | Phase 5 blocked | Must fix |
| Alert thresholds not validated | 🔴 Critical | Alert fatigue risk | Must fix |
| Span attributes over-specified | 🟡 Medium | Performance impact | Should fix |
| Missing connection pool metrics | 🟡 Medium | Incomplete visibility | Should fix |
| Query complexity not tracked | 🟡 Medium | Strategy decisions blind | Should fix |
| Inconsistent terminology | 🟡 Medium | Confusion during impl | Should fix |
| Missing runbook docs | 🟡 Medium | Operators unprepared | Should fix |
| Dashboard feasibility concerns | 🟡 Medium | Implementation risk | Should fix |
| Overhead budget undefined | 🟡 Medium | No performance control | Should fix |
| Config not discussed | 🟢 Minor | Flexibility limited | Nice to have |
| Backwards compatibility | 🟢 Minor | Code brittleness risk | Nice to have |
| Performance benchmarks vague | 🟢 Minor | Quality unclear | Nice to have |

---

## Recommendations

### Short Term (Before Implementation)

1. **Fix metric definitions**:
   - Reduce cardinality with label strategy
   - Remove impossible metrics
   - Validate all metrics are computable
   - Add missing pool and complexity metrics

2. **Specify tracing backend**:
   - Choose Jaeger or Tempo
   - Define export mechanism
   - Plan resource requirements

3. **Detail health checks**:
   - Define interval and timing
   - Specify data structures
   - Provide implementation sketch

4. **Define test strategy**:
   - Write actual test code
   - Define edge cases
   - Specify acceptance criteria

5. **Validate alerts**:
   - Align thresholds to SLOs
   - Define per-hop expectations
   - Create alert runbooks

### Implementation Order Changes

Current: Tracing → Metrics → Health → Logging → Dashboard → Testing

**Recommended**:
1. Health checks first (need subgraph status before other phases)
2. Core metrics (counters and basic histograms)
3. Tracing (once backend chosen)
4. Structured logging (once tracing is working)
5. Dashboards (once metrics are stable)
6. Testing & validation (continuous, not last)

### Documentation Additions Needed

Before implementing, create:
- Metric cardinality budget document
- Tracing architecture decision record (ADR)
- Health check implementation specification
- Alert response runbook template
- Dashboard implementation guide
- Performance overhead testing plan

---

## Conclusion

The plan is too ambitious and under-specified for implementation. It reads like a feature wishlist rather than a technical specification.

**Recommendation**:
- ✅ Keep Parts 1-2 (assessment and requirements) - these are solid
- ⚠️ Rewrite Part 3 (implementation) with specific details and realistic scope
- ⚠️ Simplify Part 5 (dashboards) - phase 1 should have 2-3 dashboards, not 5
- ✅ Keep Part 4 (alerts) but fix threshold validation
- ⚠️ Expand Part 6 (testing) with actual test code, not checkboxes

**Effort Estimate**: Current 3 weeks may be realistic IF critical issues are fixed first. Plan 1 week of refinement before coding.

---

**Review Complete**: 2026-01-28

