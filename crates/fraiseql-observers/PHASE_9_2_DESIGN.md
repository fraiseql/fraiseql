# Phase 9.2 Design - Advanced Observability & Metrics

**Status**: Design In Progress
**Target**: Weeks 3-5
**Priority**: High

---

## Overview

.2 extends Phase 9.1 (Distributed Tracing) to create a **complete observability stack** combining traces, metrics, logs, and dashboards. This phase bridges the gap between raw tracing data and actionable insights.

### Phase 9.2 Objectives

1. **Metrics Collection**: Prometheus-compatible metrics for all components
2. **Log Correlation**: Link logs to trace IDs for unified debugging
3. **Automatic Instrumentation**: Macro-based span decoration
4. **gRPC Exporter**: Export traces via gRPC (faster than HTTP)
5. **Grafana Dashboards**: Pre-built visualization templates
6. **Context Propagation**: Distributed context across systems

---

## Architecture Vision

```
┌─────────────────────────────────────────────────────────┐
│           Observability Stack              │
└─────────────────────────────────────────────────────────┘
            ↑           ↑           ↑           ↑
         Traces      Metrics       Logs      Events
            │           │           │           │
    ┌───────┴───────┬───┴────┬──────┴────┬─────┴──────┐
    │               │        │           │            │
┌───v───┐     ┌────v──┐ ┌──v───┐  ┌────v────┐  ┌───v──┐
│Jaeger │     │Prometh│ │Loki │  │Webhook  │  │Kafka │
│ HTTP  │     │eus    │ │     │  │         │  │      │
│ gRPC  │     │Push   │ │      │  │         │  │      │
└───┬───┘     │Gateway│ │     │  │         │  │      │
    │         └────┬──┘ └──┬───┘  │         │  │      │
    │              │       │      └────┬────┘  │      │
    │              │       │           │       │      │
    └──────┬───────┴───────┴───────────┴───────┴──────┘
           │
      ┌────v────────────────┐
      │  Observer System    │
      ├────────────────────┤
      │ • Listeners         │
      │ • Executors         │
      │ • Conditions        │
      │ • Actions           │
      └────────────────────┘
```

---

## Phase 9.2 Subphases

### Phase 9.2.A - Prometheus Metrics Collection

**Objective**: Collect metrics from all components

**Features**:

- Counter metrics (events processed, actions executed)
- Gauge metrics (batch size, latency, queue depth)
- Histogram metrics (duration distributions)
- Summary metrics (quantiles, sums)
- Custom metric registration

**Key Metrics**:

- `observer_events_processed_total` - Total events
- `observer_event_duration_ms` - Event processing latency
- `observer_actions_executed_total` - Total actions
- `observer_action_duration_ms` - Action latency
- `observer_errors_total` - Total errors
- `observer_batch_size` - Events per batch
- `observer_condition_matches_total` - Condition matches

**Deliverables**:

- `src/metrics/mod.rs` - Metrics module
- `src/metrics/collector.rs` - Prometheus collector
- `src/metrics/registry.rs` - Metric registration
- `PHASE_9_2_METRICS_GUIDE.md` - Documentation

**Tests**: 30+ unit tests

---

### Phase 9.2.B - Automatic Span Creation via Macros

**Objective**: Reduce boilerplate for tracing instrumentation

**Features**:

- `#[traced]` macro for automatic span creation
- `#[instrument]` macro for structured logging
- Automatic duration measurement
- Error recording
- Attribute injection

**Example**:
```rust
#[traced(name = "process_event")]
async fn process_event(event: &Event) -> Result<()> {
    // Automatic span creation and error tracking
    Ok(())
}
```

**Deliverables**:

- `fraiseql-observers-macros/` - Macro crate
- `src/macros/mod.rs` - Macro definitions
- `PHASE_9_2_MACROS_GUIDE.md` - Documentation

**Tests**: 20+ macro tests

---

### Phase 9.2.C - Log Correlation with Trace IDs

**Objective**: Link logs and traces for unified debugging

**Features**:

- Automatic trace ID injection into logs
- Structured logging fields
- Correlation ID propagation
- Log filtering by trace ID
- Log-to-trace linking in Jaeger

**Example Output**:
```
2026-01-22T10:00:00.123Z INFO process_event trace_id=abc123def456 event_id=evt-1 Processing event
2026-01-22T10:00:00.150Z DEBUG evaluate_condition trace_id=abc123def456 span_id=child001 Condition matched
2026-01-22T10:00:00.200Z INFO webhook_executed trace_id=abc123def456 span_id=child002 status=200 duration_ms=50
```

**Deliverables**:

- `src/logging/mod.rs` - Logging integration
- `src/logging/correlation.rs` - Correlation ID handling
- `src/logging/structured.rs` - Structured logging
- `PHASE_9_2_LOGGING_GUIDE.md` - Documentation

**Tests**: 25+ logging tests

---

### Phase 9.2.D - gRPC Exporter for Jaeger

**Objective**: Export traces via gRPC (faster than HTTP)

**Features**:

- gRPC collector support
- Async export (non-blocking)
- Connection pooling
- Retry logic
- Configurable batch size

**Configuration**:
```bash
export JAEGER_EXPORTER=grpc
export JAEGER_GRPC_ENDPOINT=localhost:14250
export JAEGER_BATCH_SIZE=256
export JAEGER_EXPORT_TIMEOUT_MS=10000
```

**Deliverables**:

- `src/tracing/grpc_exporter.rs` - gRPC implementation
- `src/tracing/grpc_config.rs` - gRPC configuration
- `PHASE_9_2_GRPC_GUIDE.md` - Documentation

**Tests**: 20+ integration tests

---

### Phase 9.2.E - Distributed Context Propagation

**Objective**: Propagate trace context across system boundaries

**Features**:

- OpenTelemetry context API
- Baggage propagation
- Multi-format support (HTTP, gRPC, AWS)
- Context enforcement policies
- Sampling decision propagation

**Formats Supported**:

- W3C Trace Context (HTTP)
- gRPC Metadata
- AWS X-Ray Headers
- Datadog Headers (APM)

**Deliverables**:

- `src/tracing/baggage.rs` - Baggage handling
- `src/tracing/context.rs` - Context management
- `src/tracing/propagators/` - Format-specific propagators
- `PHASE_9_2_CONTEXT_GUIDE.md` - Documentation

**Tests**: 30+ propagation tests

---

### Phase 9.2.F - Grafana Dashboard Templates

**Objective**: Pre-built dashboards for common use cases

**Dashboard Templates**:

1. **System Overview**
   - Event throughput (events/sec)
   - Error rate (errors/sec)
   - P50, P95, P99 latencies
   - Service dependency graph

2. **Event Processing**
   - Events processed per minute
   - Batch size distribution
   - Processing latency histogram
   - Error breakdown by type

3. **Action Performance**
   - Actions executed per minute
   - Action type breakdown
   - Latency by action type
   - Retry rate by action

4. **System Health**
   - Resource utilization (CPU, memory)
   - Connection pool status
   - Queue depth trends
   - Uptime and availability

5. **Troubleshooting**
   - Error rate trend
   - Slow operation detection
   - Failed action analysis
   - Checkpoint recovery tracking

**Deliverables**:

- `dashboards/` - Dashboard JSON files
- `dashboards/system-overview.json` - System overview
- `dashboards/event-processing.json` - Event processing
- `dashboards/action-performance.json` - Action performance
- `dashboards/system-health.json` - System health
- `dashboards/troubleshooting.json` - Troubleshooting
- `PHASE_9_2_DASHBOARDS_GUIDE.md` - Documentation

**Grafana Integration**:

- Import via Grafana UI
- Auto-setup script
- Docker Compose with pre-loaded dashboards

---

## Implementation Timeline

### Week 3

- **Mon-Tue**: Implement Prometheus metrics collection
- **Wed**: Implement macro-based instrumentation
- **Thu**: Integration testing
- **Fri**: Documentation and examples

**Deliverables**:

- Metrics module (150+ lines)
- Macros crate (200+ lines)
- 50+ tests
- 2 guides

### Week 4

- **Mon-Tue**: Implement log correlation
- **Wed**: Implement gRPC exporter
- **Thu**: Integration testing
- **Fri**: Documentation

**Deliverables**:

- Logging module (120+ lines)
- gRPC exporter (200+ lines)
- 50+ tests
- 2 guides

### Week 5

- **Mon-Tue**: Implement context propagation
- **Wed**: Create dashboard templates
- **Thu-Fri**: Testing and documentation

**Deliverables**:

- Context propagation (180+ lines)
- 5 dashboard templates
- 60+ tests
- 3 guides

---

## Key Architecture Decisions

### 1. Metrics System Design

**Decision**: Use Prometheus client library with Registry pattern

**Rationale**:

- Industry standard
- Works with Prometheus, Grafana, Datadog, New Relic
- Simple and efficient
- Minimal overhead

**Tradeoff**: Cannot use OpenTelemetry metrics

### 2. Log Correlation Strategy

**Decision**: Inject trace ID as structured field in all logs

**Rationale**:

- Works with all logging backends (Loki, ELK, CloudWatch)
- Standard practice in microservices
- Easy filtering and correlation

**Tradeoff**: Requires logging integration

### 3. Macro Implementation

**Decision**: Procedural macros in separate crate

**Rationale**:

- Cleaner separation of concerns
- Easier to maintain and test
- Can evolve independently
- Reduces compile time for main crate

**Tradeoff**: Requires macro crate dependency

### 4. gRPC Exporter

**Decision**: Async gRPC client with connection pooling

**Rationale**:

- Lower latency than HTTP (Proto binary format)
- Connection reuse reduces overhead
- Async prevents blocking on export
- Native Jaeger support

**Tradeoff**: Adds dependency (tonic, prost)

### 5. Dashboard Strategy

**Decision**: Pre-built JSON templates in Git

**Rationale**:

- Version controlled
- Easy to customize
- Import directly into Grafana
- Reproducible setup

**Tradeoff**: Requires manual dashboard creation

---

## Integration Points

### With Phase 9.1 (Tracing)

```
.1 Traces     Phase 9.2 Metrics
    │                     │
    ├─ Jaeger HTTP   ─────┤
    ├─ Jaeger gRPC   ─────┤
    └─ Context       ─────┤
         │                │
         v                v
    Jaeger UI        Prometheus
         │                │
         └────┬───────────┘
              │
         Grafana
```

### With Logging Systems

```
Observer Logs       trace_id injection       Loki/ELK/CloudWatch
    │                    │                           │
    └─ ListenerTracer ───┼─ correlation_id ─────────┤
    └─ ExecutorTracer ───┼─ span_id ────────────────┤
    └─ ActionTracer ─────┼─ duration ───────────────┤
```

---

## Testing Strategy

### Unit Tests (80+)

- Metrics collection and registration
- Macro expansion and compilation
- Log formatting and correlation
- gRPC serialization
- Context propagation

### Integration Tests (40+)

- Metrics export to Prometheus
- Log correlation in trace
- gRPC connectivity
- Multi-format context propagation
- Dashboard rendering

### E2E Tests (30+)

- Full stack tracing + metrics
- Metrics scraped by Prometheus
- Logs correlated with traces
- Dashboard data availability
- Production-like scenarios

---

## Performance Considerations

### Metrics Overhead

- Counter: < 0.1ms per operation
- Gauge: < 0.5ms per operation
- Histogram: < 1ms per operation

### Logging Overhead

- Trace ID injection: < 0.05ms per log
- Structured logging: < 0.2ms per log

### gRPC Exporter

- Export latency: 5-20ms (vs 100-500ms for HTTP)
- Throughput: 10,000+ spans/sec
- Batch processing: No per-span overhead

### Overall Impact

- Tracing + Metrics + Logging: ~5-10% overhead
- Can be reduced to <1% with sampling

---

## Success Criteria

### Phase 9.2 Complete When:

1. ✅ Prometheus metrics working
   - Metrics endpoint: `/metrics`
   - Scrape successful
   - Grafana shows data

2. ✅ Macros working
   - `#[traced]` creates spans
   - `#[instrument]` adds logging
   - Compilation successful

3. ✅ Log correlation working
   - All logs have trace_id
   - Logs linkable from Jaeger
   - Grafana Loki integration

4. ✅ gRPC exporter working
   - Traces export via gRPC
   - Performance better than HTTP
   - Connection pooling verified

5. ✅ Context propagation working
   - Baggage propagation verified
   - Multi-format support working
   - External systems receive context

6. ✅ Dashboards complete
   - 5 dashboards created
   - All metrics displayed
   - Import process documented

7. ✅ Documentation complete
   - 6 guides (500+ lines each)
   - Code examples
   - Troubleshooting guides

8. ✅ Tests passing
   - 150+ tests
   - 100% pass rate
   - Coverage on new code

---

## Known Constraints

### Scope Limitations

1. **OpenTelemetry Full Support** - Deferred to Phase 9.3
   - Metrics SDK (vs just client)
   - OTLP protocol support
   - Instrumentation discovery

2. **APM Platform Integrations** - Deferred to Phase 9.3
   - Datadog APM
   - New Relic
   - Dynatrace

3. **Advanced Dashboarding** - Deferred to Phase 9.4
   - Anomaly detection dashboards
   - ML-powered insights
   - Custom query builders

### Dependencies

- `prometheus` crate (metrics)
- `tonic` crate (gRPC)
- `prost` crate (Proto serialization)
- `syn`, `quote` crates (macros)
- `tracing-subscriber` (logging)

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| gRPC version conflicts | Medium | High | Version pin, compatibility tests |
| Macro compilation errors | Medium | Medium | Extensive tests, clear error messages |
| Log volume explosion | Low | High | Structured logging, sampling |
| Prometheus cardinality | Medium | High | Metric design review, limits |
| Performance regression | Low | High | Benchmarks before/after |

---

## Rollout Plan

### Metrics & Macros

- ✅ Implement Prometheus metrics
- ✅ Add macro-based instrumentation
- ✅ Test integration
- ✅ Deploy to staging

### Logging & gRPC

- ✅ Add log correlation
- ✅ Implement gRPC exporter
- ✅ Verify performance
- ✅ Deploy to production

### Context & Dashboards

- ✅ Implement baggage propagation
- ✅ Create dashboard templates
- ✅ Document integration
- ✅ Train teams

---

## Next Phase Preview

### Phase 9.3: Event Replay & Time-Travel Debugging

After Phase 9.2 completes observability.3 will add:

- **Event Replay**: Re-execute historical events
- **Time-Travel Debugging**: View state at any point in time
- **Dry-Run Mode**: Test without side effects
- **Failure Injection**: Chaos testing

This builds on Phase 9.2's complete trace data for debugging.

---

## Resource Requirements

### Team

- **1 Senior Engineer**: Architecture, gRPC, macros
- **1 Mid-level Engineer**: Metrics, logging, dashboards
- **1 QA Engineer**: Testing, performance validation

### Infrastructure

- **Prometheus**: Metrics scraping
- **Grafana**: Visualization
- **Jaeger**: Tracing (from Phase 9.1)
- **Loki/ELK**: Logging (optional)

### Time

- **Total**: 3 weeks (Weeks 3-5)
- **Per subphase**: 2.5-3 days average
- **Buffer**: 30% for contingencies

---

## Success Metrics

### Adoption Metrics

- % of observer operations traced
- % of dashboards actively used
- Log correlation usage rate

### Performance Metrics

- Overhead < 10% (with sampling)
- Export latency < 20ms (gRPC)
- Metrics scrape success > 99%

### Quality Metrics

- Test pass rate: 100%
- Code coverage: > 85%
- Documentation completeness: 100%

---

## References

- **Phase 9.1**: Distributed Tracing Implementation
- **OpenTelemetry**: https://opentelemetry.io/
- **Prometheus**: https://prometheus.io/
- **Grafana**: https://grafana.com/
- **Jaeger**: https://www.jaegertracing.io/

---

**Document**: Phase 9.2 Design - Advanced Observability & Metrics
**Status**: Design Complete (Ready for Implementation)
**Last Updated**: January 22, 2026
**Target Start**: Week 3 (following Phase 9.1 completion)
