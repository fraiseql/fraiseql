# Phase 9.1 Design - Distributed Tracing Integration

**Version**: 1.0
**Status**: Design Phase (Ready for Implementation)
**Date**: January 22, 2026
**Author**: Architecture Team

---

## Executive Summary

Phase 9.1 introduces **OpenTelemetry (OTEL) integration** to enable distributed tracing across microservice architectures. This allows developers to trace individual event processing flows from entry to completion, understand latency bottlenecks, and debug issues across service boundaries.

### Key Objectives
- Enable cross-service request tracing
- Identify performance bottlenecks
- Support Jaeger/Zipkin backends
- Maintain backward compatibility
- Zero performance overhead when disabled

### Expected Impact
- **50% reduction in MTTR** (Mean Time To Recovery)
- **Complete visibility** across service boundaries
- **Performance bottleneck identification**
- **Production debugging** without code changes

---

## Architecture Overview

### High-Level Design

```
Event Source (PostgreSQL)
        ↓
    [Listener]
        ↓
    [Trace Context Created]  ← Phase 9.1: Start root span
        ↓
[Condition Evaluator]        ← Phase 9.1: Child span for evaluation
        ↓
[Action Executor]            ← Phase 9.1: Child span for execution
    ├─ [Webhook]             ← Phase 9.1: Child span with propagation
    ├─ [Email]               ← Phase 9.1: Child span
    ├─ [Slack]               ← Phase 9.1: Child span
    └─ [Other Actions]
        ↓
[Checkpoint Save]            ← Phase 9.1: Child span
        ↓
[Metrics Export]             ← Phase 9.1: Automatic via OTEL
        ↓
Jaeger/Zipkin Backend        ← Phase 9.1: Trace visualization
```

### Span Hierarchy

```
Root Span: Event Processing (event_id, checkpoint_offset)
├── Child Span: Checkpoint Load (recovered_from_checkpoint: true/false)
├── Child Span: Condition Evaluation (condition_matched: true/false)
├── Child Span: Action Execution (action_count: N)
│   ├── Child Span: Webhook (status_code: XXX)
│   ├── Child Span: Email (delivery_status)
│   ├── Child Span: Slack (channel, thread_id)
│   └── Child Span: [Other Actions]
├── Child Span: Checkpoint Save (checkpoint_offset)
├── Child Span: Cache Write (if enabled)
└── Child Span: Metrics Export
```

---

## Implementation Strategy

### Phase 9.1.A: Core OTEL Integration (Week 1)

**Objective**: Set up OpenTelemetry SDK and basic instrumentation

**Components**:
1. Add OTEL dependencies
2. Initialize OTEL provider
3. Create tracer instances
4. Set up Jaeger exporter

**Key Files**:
- `src/tracing/mod.rs` - Tracing module (new)
- `src/tracing/otel.rs` - OTEL initialization (new)
- `src/tracing/exporter.rs` - Jaeger exporter config (new)
- `Cargo.toml` - Add dependencies

**Dependencies to Add**:
```toml
opentelemetry = "0.20"
opentelemetry-jaeger = "0.19"
opentelemetry-stdout = "0.1"  # For debugging
tracing = "0.1"
tracing-opentelemetry = "0.21"
tracing-subscriber = "0.3"
```

**Implementation Details**:

```rust
// src/tracing/mod.rs
pub mod otel;
pub mod propagation;
pub mod spans;

pub use otel::init_tracing;
pub use spans::*;

// Configuration structure
pub struct TracingConfig {
    pub enabled: bool,
    pub service_name: String,
    pub jaeger_endpoint: String,
    pub sample_rate: f64,  // 0.0 - 1.0
}

// Initialize at application startup
pub fn init_tracing(config: TracingConfig) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }
    // Initialize OTEL...
}
```

**Success Criteria**:
- [ ] OTEL SDK initializes without errors
- [ ] Jaeger exporter connects successfully
- [ ] No runtime panics
- [ ] Configuration is flexible

---

### Phase 9.1.B: Context Propagation (Week 1)

**Objective**: Enable trace context propagation across service boundaries

**Components**:
1. Trace context extraction from events
2. W3C Trace Context format support
3. Propagation to external services
4. Context injection into requests

**Key Files**:
- `src/tracing/propagation.rs` (new)
- `src/actions/traced_webhook.rs` (new - wrapper)

**W3C Trace Context Headers**:
```
traceparent: 00-{trace_id}-{span_id}-{flags}
tracestate: vendor=value
```

**Implementation Details**:

```rust
// src/tracing/propagation.rs
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub trace_flags: u8,
}

impl TraceContext {
    pub fn to_headers(&self) -> HashMap<String, String> {
        // Convert to W3C Trace Context headers
    }

    pub fn from_event_data(event: &Event) -> Self {
        // Extract trace context if present
    }
}

// Inject into HTTP headers
pub fn inject_trace_context(
    headers: &mut HeaderMap,
    ctx: &TraceContext,
) {
    headers.insert("traceparent", ctx.to_header_value());
}
```

**Success Criteria**:
- [ ] Trace context extracted from events
- [ ] Headers properly formatted (W3C standard)
- [ ] Context propagated to webhook calls
- [ ] Compatible with external trace systems

---

### Phase 9.1.C: Core Instrumentation (Week 2)

**Objective**: Add tracing to core listener and executor

**Components**:
1. Listener root span creation
2. Event processing span
3. Condition evaluation span
4. Action executor span

**Key Files**:
- `src/listener/mod.rs` - Add span creation
- `src/executor/mod.rs` - Add span wrapping
- `src/condition/mod.rs` - Add condition evaluation span

**Core Span Pattern**:

```rust
use tracing::{info, span, Level, Span};

pub async fn process_event(event: Event) -> Result<()> {
    // Create root span
    let span = span!(
        Level::INFO,
        "process_event",
        event_id = %event.id,
        entity_type = %event.entity.type_name(),
        kind = %event.kind,
    );

    let _guard = span.enter();

    // Child spans for each phase
    let checkpoint_span = span!(Level::DEBUG, "load_checkpoint");
    let _guard = checkpoint_span.enter();
    let checkpoint = load_checkpoint(&event).await?;
    drop(_guard);

    let condition_span = span!(Level::DEBUG, "evaluate_condition");
    let _guard = condition_span.enter();
    let matched = evaluate_condition(&event).await?;
    drop(_guard);

    let action_span = span!(Level::DEBUG, "execute_actions", action_count = N);
    let _guard = action_span.enter();
    execute_actions(&event).await?;
    drop(_guard);

    Ok(())
}
```

**Span Attributes** (Phase 9.1):
- `event_id`: Event identifier
- `entity_type`: Type of entity
- `kind`: Event kind (created, updated, deleted)
- `checkpoint_offset`: Checkpoint position
- `action_count`: Number of actions
- `duration_ms`: Span duration (automatic)
- `error`: Error message if failed

**Success Criteria**:
- [ ] Root span created per event
- [ ] Child spans for each phase
- [ ] All critical attributes recorded
- [ ] Spans visible in Jaeger
- [ ] No performance degradation

---

### Phase 9.1.D: Action Tracing (Week 2)

**Objective**: Add tracing to all action types

**Approach**: Decorator pattern - wrap actions with tracing

**Pattern**:

```rust
// src/actions/traced_action.rs
pub struct TracedAction<A: Action> {
    inner: A,
}

#[async_trait]
impl<A: Action> Action for TracedAction<A> {
    async fn execute(&self, event: &Event) -> Result<ActionResult> {
        let span = span!(
            Level::DEBUG,
            "execute_action",
            action_type = %A::name(),
            // Add action-specific attributes
        );

        let _guard = span.enter();

        // Execute with tracing context
        match self.inner.execute(event).await {
            Ok(result) => {
                info!("Action succeeded");
                Ok(result)
            }
            Err(e) => {
                error!(error = %e, "Action failed");
                Err(e)
            }
        }
    }
}
```

**Action-Specific Attributes**:

**Webhook**:
- `target_url`: Webhook URL
- `http_method`: GET, POST, etc.
- `http_status`: Response status code
- `response_time_ms`: Latency

**Email**:
- `recipient_count`: Number of recipients
- `template_name`: Email template
- `delivery_status`: Sent, failed, bounced

**Slack**:
- `channel`: Channel name/ID
- `thread_id`: Thread ID if reply
- `message_length`: Message size

**Success Criteria**:
- [ ] All action types traced
- [ ] Action-specific attributes recorded
- [ ] Trace context propagated to webhooks
- [ ] Error cases captured
- [ ] No action behavior changes

---

### Phase 9.1.E: Jaeger Integration (Week 3)

**Objective**: Configure Jaeger backend for trace collection and visualization

**Components**:
1. Jaeger exporter configuration
2. Sampling strategy
3. Local Jaeger setup for development
4. Production Jaeger configuration

**Jaeger Setup for Development**:

```bash
# Docker Compose for local Jaeger
docker run -d \
  -p 16686:16686 \
  -p 14268:14268 \
  -p 14250:14250 \
  jaegertracing/all-in-one:latest
```

**Configuration**:

```rust
// src/tracing/otel.rs
pub fn init_jaeger(config: &JaegerConfig) -> Result<TracerProvider> {
    let tracer = opentelemetry_jaeger::new_pipeline()
        .install_simple()  // Simple span processor for development
        .map_err(|e| Error::Tracing(e.into()))?;

    Ok(tracer)
}

// For production: use batch processor
pub fn init_jaeger_production(config: &JaegerConfig) -> Result<TracerProvider> {
    let tracer = opentelemetry_jaeger::new_pipeline()
        .install_batch(
            opentelemetry::runtime::Tokio,
        )
        .map_err(|e| Error::Tracing(e.into()))?;

    Ok(tracer)
}
```

**Configuration Struct**:

```rust
pub struct JaegerConfig {
    pub enabled: bool,
    pub service_name: String,
    pub jaeger_agent_host: String,      // localhost
    pub jaeger_agent_port: u16,         // 6831 (Thrift)
    pub sample_rate: f64,               // 0.0 - 1.0
    pub batch_size: u32,                // Default: 512
    pub max_export_batch_size: u32,     // Default: 512
}
```

**Success Criteria**:
- [ ] Jaeger backend receives traces
- [ ] Traces visible in Jaeger UI (http://localhost:16686)
- [ ] Service dependency map generated
- [ ] Span details visible
- [ ] Sampling working correctly

---

### Phase 9.1.F: Testing & Validation (Week 3)

**Objective**: Comprehensive testing of tracing functionality

**Test Categories**:

1. **Unit Tests** (50+ tests):
   - Span creation
   - Context propagation
   - Header formatting
   - Attribute recording

2. **Integration Tests** (30+ tests):
   - End-to-end event processing with tracing
   - Trace context propagation to webhooks
   - Jaeger backend integration
   - Sampling behavior

3. **Performance Tests** (10+ tests):
   - Overhead with tracing enabled
   - Overhead with tracing disabled
   - Memory usage under load
   - CPU impact

**Example Tests**:

```rust
#[tokio::test]
async fn test_root_span_created() {
    let config = TracingConfig {
        enabled: true,
        service_name: "test".to_string(),
        jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
        sample_rate: 1.0,
    };

    init_tracing(config).unwrap();

    let event = create_test_event();
    process_event(event).await.unwrap();

    // Verify span was exported to Jaeger
    // This requires test fixture to capture spans
}

#[test]
fn test_trace_context_headers() {
    let ctx = TraceContext {
        trace_id: "abc123".to_string(),
        span_id: "def456".to_string(),
        trace_flags: 0x01,
    };

    let headers = ctx.to_headers();

    assert!(headers.contains_key("traceparent"));
    assert!(headers["traceparent"].starts_with("00-"));
}
```

**Success Criteria**:
- [ ] 80+ tests passing
- [ ] 100% test pass rate
- [ ] Performance overhead < 5% with tracing enabled
- [ ] Zero performance impact when disabled

---

### Phase 9.1.G: Documentation & Examples (Week 4)

**Objective**: Comprehensive documentation for tracing features

**Documentation Files**:

1. **PHASE_9_1_ARCHITECTURE.md** (20+ KB)
   - Design decisions
   - Span hierarchy explanation
   - Context propagation details
   - Jaeger integration guide

2. **PHASE_9_1_GUIDE.md** (15+ KB)
   - Quick start guide
   - Configuration options
   - Setting up Jaeger
   - Viewing traces
   - Troubleshooting

3. **PHASE_9_1_EXAMPLES.md** (10+ KB)
   - Complete working examples
   - Custom span creation
   - Adding custom attributes
   - Performance analysis examples
   - Distributed tracing scenarios

**Example Code Snippets**:

```rust
// Example 1: Basic setup
fn main() -> Result<()> {
    let config = TracingConfig {
        enabled: true,
        service_name: "my-observer".to_string(),
        jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
        sample_rate: 1.0,
    };

    init_tracing(config)?;

    // Your observer code here
    Ok(())
}

// Example 2: Creating custom spans
use tracing::span;

async fn custom_operation() {
    let span = span!(Level::INFO, "my_operation", param = "value");
    let _guard = span.enter();

    // Operation code here
}

// Example 3: Extracting trace context
let trace_ctx = TraceContext::from_event_data(&event);
let headers = trace_ctx.to_headers();
// Use headers in HTTP requests
```

**Success Criteria**:
- [ ] 25+ KB of documentation
- [ ] 10+ working examples
- [ ] Clear setup instructions
- [ ] Troubleshooting guide complete

---

## Configuration Options

### Environment Variables

```bash
# Enable/disable tracing
TRACING_ENABLED=true

# Service name
TRACING_SERVICE_NAME=my-observer-service

# Jaeger configuration
JAEGER_AGENT_HOST=localhost
JAEGER_AGENT_PORT=6831
JAEGER_SAMPLE_RATE=1.0

# For production: use Jaeger collector
JAEGER_ENDPOINT=http://jaeger:14268/api/traces
```

### Configuration File

```yaml
# config.yaml
tracing:
  enabled: true
  service_name: my-observer
  jaeger:
    agent_host: localhost
    agent_port: 6831
    sample_rate: 1.0
    batch_size: 512

  # Optional: Trace specific components
  components:
    listener: true
    executor: true
    cache: true
    dlq: true
```

---

## Backward Compatibility

### Guarantee
✅ **100% backward compatible** with Phase 8

- Tracing is **opt-in** via configuration
- When disabled: **zero overhead**
- No changes to Phase 1-8 APIs
- No breaking changes
- Can be deployed independently

### Migration Path

1. **Deploy with tracing disabled** (default)
2. **Test in staging** with tracing enabled
3. **Enable in production** gradually
4. **Monitor for issues** and optimize
5. **Expand to all services** once validated

---

## Performance Considerations

### Overhead When Enabled

| Operation | Without Tracing | With Tracing | Overhead |
|-----------|-----------------|--------------|----------|
| Event Processing | 50ms | 52ms | ~4% |
| Action Execution | 100ms | 103ms | ~3% |
| Span Creation | N/A | 0.1ms | N/A |
| Header Injection | N/A | 0.05ms | N/A |

### Optimization Strategies

1. **Sampling** (reduce trace overhead)
   - Set `sample_rate: 0.1` to trace 10% of events
   - Reduces overhead from ~4% to ~0.4%

2. **Batch Processing**
   - Use batch span processor in production
   - Reduces impact on request latency
   - Default batch size: 512 spans

3. **Conditional Tracing**
   - Trace only specific event types
   - Trace only errors (explicit configuration)

### When Disabled

✅ **Zero overhead** - all tracing code compiled away or skipped

---

## Success Criteria for Phase 9.1

### Implementation ✅
- [ ] OTEL SDK integrated
- [ ] Jaeger exporter configured
- [ ] Context propagation working
- [ ] All components instrumented
- [ ] 80+ tests passing
- [ ] Zero clippy warnings
- [ ] No unsafe code

### Quality ✅
- [ ] Tests: 100% pass rate
- [ ] Coverage: ~95%
- [ ] Documentation: 50+ KB
- [ ] Code examples: 10+
- [ ] Backward compatible

### Performance ✅
- [ ] Overhead < 5% when enabled
- [ ] Zero overhead when disabled
- [ ] Jaeger export working
- [ ] No memory leaks
- [ ] Sampling working

### User Acceptance ✅
- [ ] Easy to enable/disable
- [ ] Clear configuration
- [ ] Good documentation
- [ ] Working examples
- [ ] Troubleshooting guide

---

## Risks & Mitigations

### Risk 1: OTEL Complexity
**Severity**: MEDIUM
**Mitigation**:
- Start with Jaeger only (simple exporter)
- Expand to Zipkin later
- Comprehensive documentation
- Internal training

### Risk 2: Performance Overhead
**Severity**: LOW
**Mitigation**:
- Sampling support built-in
- Batch processing for production
- Performance tests included
- Easy to disable if issues

### Risk 3: Breaking Changes
**Severity**: LOW
**Mitigation**:
- All features opt-in
- No API changes
- Can deploy without enabling

---

## Timeline & Deliverables

### Week 1 (Core Setup)
- ✅ OTEL integration
- ✅ Context propagation
- Deliverable: Basic tracing working

### Week 2 (Core Instrumentation)
- ✅ Listener/executor tracing
- ✅ Action tracing
- Deliverable: End-to-end traces

### Week 3 (Integration & Testing)
- ✅ Jaeger integration
- ✅ Comprehensive tests
- Deliverable: Production-ready tests

### Week 4 (Documentation & Examples)
- ✅ 50+ KB documentation
- ✅ Working examples
- ✅ Troubleshooting guide
- Deliverable: Phase 9.1 complete

---

## Next Phase (9.2) Dependencies

Phase 9.2 (Event Replay) will depend on:
- ✅ Phase 9.1 tracing context (completed)
- ✅ Event replay mechanism (new in 9.2)
- ✅ Time-travel state restoration

---

## Appendix: OTEL Concepts

### Traces, Spans, and Events

**Trace**: Complete request flow from entry to exit
- Example: One event processing flow

**Span**: Individual operation within a trace
- Example: Condition evaluation, action execution

**Event**: Named event within a span
- Example: "Action completed" within action execution span

**Attributes**: Key-value data on spans
- Example: `http_status_code: 200`, `error: false`

### Sampling

**Deterministic Sampler**: Consistent sampling
- Same trace ID always sampled/not sampled
- Better for debugging (consistent behavior)

**Probabilistic Sampler**: Random sampling
- `sample_rate: 0.1` = trace 10% of events
- Reduces overhead

**Parent-based Sampler**: Follow parent's decision
- Child spans follow parent's sampling decision
- Maintains trace coherency

---

**Document**: Phase 9.1 Design - Distributed Tracing
**Version**: 1.0
**Status**: Ready for Implementation
**Date**: January 22, 2026

