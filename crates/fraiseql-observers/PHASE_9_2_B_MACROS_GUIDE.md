# Phase 9.2.B - Automatic Span Creation via Macros Guide

**Status**: Complete
**Last Updated**: January 22, 2026

---

## Overview

Phase 9.2.B provides procedural macros for automatic span creation and structured logging, eliminating boilerplate instrumentation code. Two macros are available:

- `#[traced]` - Automatic span creation with timing and error tracking
- `#[instrument]` - Structured logging with function arguments

---

## Quick Start

### Step 1: Add Macro Crate to Dependencies

```toml
# Cargo.toml
[dependencies]
fraiseql-observers = { version = "2.0", features = ["macros"] }
fraiseql-observers-macros = "2.0"
tracing = "0.1"
```

### Step 2: Use the Macros

```rust
use fraiseql_observers_macros::{traced, instrument};

#[traced(name = "process_event")]
async fn process_event(event: &Event) -> Result<()> {
    // Automatic span creation, timing, error tracking
    Ok(())
}

#[instrument]
fn validate_data(user_id: u32, name: String) {
    // Automatic logging with arguments
}
```

### Step 3: View Traces in Jaeger

Spans created by macros automatically integrate with Phase 9.1 tracing and export to Jaeger.

---

## The `#[traced]` Macro

Creates an automatic span around a function with automatic timing and error handling.

### Syntax

```rust
#[traced]                              // Use function name as span name
#[traced(name = "custom_span_name")]  // Use custom span name
async fn my_function() { }

#[traced]
fn sync_function() { }
```

### What It Does

1. Creates a tracing span with given or derived name
2. Records function entry at DEBUG level
3. Measures execution time
4. Records function exit:
   - Success: logs duration at DEBUG level
   - Error: logs duration and error at WARN level
5. Returns original result unchanged

### Examples

#### Basic Async Function

```rust
#[traced(name = "fetch_user")]
async fn fetch_user(user_id: u32) -> Result<User> {
    // Automatic span creation
    // Automatic timing measurement
    // Automatic error recording
    database.get_user(user_id).await
}
```

**Generated Trace Structure**:
```
Span: fetch_user
├─ Event: function entered
├─ Event: span completed successfully (duration_ms=45)
└─ Result: Ok(User { ... })
```

#### Function with Error Handling

```rust
#[traced(name = "validate_email")]
async fn validate_email(email: &str) -> Result<()> {
    if !email.contains('@') {
        return Err(anyhow!("Invalid email format"));
    }
    Ok(())
}
```

**Trace for Error Case**:
```
Span: validate_email
├─ Event: function entered
├─ Event: span failed (duration_ms=2, error="Invalid email format")
└─ Result: Err(...)
```

#### Synchronous Function

```rust
#[traced]
fn process_batch(items: Vec<Item>) -> usize {
    // Works with sync functions too
    items.len()
}
```

### Span Name Selection

**Automatic** (recommended):
```rust
#[traced]  // Span name = "my_function"
async fn my_function() { }
```

**Custom**:
```rust
#[traced(name = "custom_name")]  // Span name = "custom_name"
async fn my_function() { }
```

**String Literal Required**:
```rust
#[traced(name = "valid")]        // ✅ Works
#[traced(name = some_variable)]  // ❌ Compiler error
```

### Return Value Handling

The macro preserves the original return type and value:

```rust
#[traced]
fn get_count() -> usize {
    42  // Returns 42 unchanged
}

#[traced]
async fn get_result() -> Result<String> {
    Ok("data".to_string())  // Returns Ok(...) unchanged
}
```

### Lifetime and Generics

Fully compatible with complex function signatures:

```rust
#[traced]
async fn process<'a, T: Send>(input: &'a [T]) -> Result<Vec<T>>
where
    T: Clone,
{
    // Works with lifetimes and generic bounds
    Ok(input.to_vec())
}
```

### Visibility

Works with all visibility modifiers:

```rust
#[traced]
pub async fn public_function() { }

#[traced]
async fn private_function() { }

#[traced]
pub(crate) async fn crate_function() { }
```

### Async Runtime Support

Works with any async runtime (Tokio, async-std, etc.):

```rust
// With Tokio
#[traced]
#[tokio::main]
async fn main() {
    // Automatically traced
}

// In async blocks
#[traced]
async fn process() {
    // Runs on same runtime
}
```

---

## The `#[instrument]` Macro

Adds structured logging with function arguments automatically captured.

### Syntax

```rust
#[instrument]
fn my_function(user_id: u32, name: String) { }

#[instrument]
async fn async_function(id: u64) { }
```

### What It Does

1. At function entry, logs all function arguments
2. Arguments are formatted as structured fields: `arg_name = ?arg_value`
3. Target is set to the function name for filtering
4. Logs at DEBUG level
5. Works with both async and sync functions

### Examples

#### Logging Function Arguments

```rust
#[instrument]
fn create_user(user_id: u32, name: String, email: String) {
    // Automatically logs:
    // DEBUG create_user user_id=123 name="John" email="john@example.com" function entered

    println!("Creating user: {}", name);
}
```

**Generated Log**:
```
DEBUG create_user user_id=123 name="John" email="john@example.com" function entered
```

#### With Complex Types

```rust
#[instrument]
async fn process_event(event: Event, options: ProcessOptions) {
    // Logs with Debug formatting:
    // DEBUG process_event event=Event { ... } options=ProcessOptions { ... } function entered
}
```

#### No Arguments

```rust
#[instrument]
fn start_service() {
    // Logs: DEBUG start_service function entered
}
```

### Log Formatting

Arguments are logged with Debug formatting (`?`):

```rust
#[instrument]
fn log_data(numbers: Vec<u32>, user: User) {
    // Logs:
    // numbers=[1, 2, 3, 4]
    // user=User { id: 42, name: "Alice" }
}
```

### Combining with `#[traced]`

Use both macros together for span creation AND argument logging:

```rust
#[traced(name = "process_request")]
#[instrument]
async fn process_request(request_id: u64, payload: String) -> Result<()> {
    // Creates span "process_request"
    // Logs request_id and payload at function entry
    // Records timing and errors
    Ok(())
}
```

**Combined Output**:
```
Span: process_request
├─ Event: request_id=123 payload="data" function entered
├─ (function executes)
└─ Event: span completed successfully (duration_ms=150)
```

---

## Integration with Phase 9.1 Tracing

Macros automatically integrate with existing Phase 9.1 infrastructure:

### Trace Context Propagation

When `#[traced]` creates a span, it automatically:

1. Creates a child span under the current trace context
2. Preserves trace ID across function boundaries
3. Updates span ID for this function level

```rust
#[traced]
async fn root_operation() {
    // Root span created, trace_id generated
    child_operation().await;
}

#[traced]
async fn child_operation() {
    // Child span created under root_operation
    // Same trace_id, new span_id
}
```

### With Listener/Executor Tracers

Combine macros with Phase 9.1 manual instrumentation:

```rust
let listener = ListenerTracer::new("listener-1");
listener.record_startup();

#[traced]
async fn process_batch(batch: Vec<Event>) -> Result<()> {
    // Automatic span for batch processing
    for event in batch {
        listener.record_event(&event);
        process_event(event).await?;
    }
    Ok(())
}

#[traced]
async fn process_event(event: Event) -> Result<()> {
    // Nested span under process_batch
    // All under same trace
    Ok(())
}
```

### Exporting to Jaeger

Spans created by macros automatically export to Jaeger (Phase 9.1):

```rust
// Initialize Jaeger (Phase 9.1)
let tracing_config = TracingConfig::from_env()?;
init_tracing(tracing_config)?;

// Use macro - automatically exported
#[traced]
async fn process() {
    // Exported to Jaeger automatically
}
```

---

## Performance Characteristics

### Overhead

| Operation | Overhead | Notes |
|-----------|----------|-------|
| Span creation | < 1ms | Per function call |
| Argument logging | < 0.5ms | Debug formatting |
| Async function | Negligible | No blocking |
| Sync function | < 0.1ms | Timing measurement |

### Recommendations

**For High-Frequency Functions**:

```rust
// ❌ Don't use macros in hot paths
#[traced]
fn get_item(id: u32) -> Item {  // Called 1M times/sec
    items[id]
}

// ✅ Use sampling or skip tracing
fn get_item(id: u32) -> Item {
    items[id]
}

// Or use parent span only
#[traced]
fn get_all_items() -> Vec<Item> {
    (0..1000).map(|id| get_item(id)).collect()
}
```

**For Moderate-Frequency Functions** (< 10k/sec):

```rust
#[traced]
async fn process_request(req: Request) -> Result<Response> {
    // Good use of macros
}
```

---

## Common Patterns

### Pattern 1: Layer-Based Tracing

```rust
// API layer
#[traced(name = "api_request")]
#[instrument]
pub async fn handle_request(req: HttpRequest) -> Result<HttpResponse> {
    process_request(req.into()).await
}

// Business logic layer
#[traced(name = "business_logic")]
async fn process_request(req: Request) -> Result<Response> {
    validate_request(&req)?;
    execute_action(&req).await
}

// Data layer
#[traced(name = "database_query")]
async fn execute_action(req: &Request) -> Result<Response> {
    database.query(&req.id).await
}
```

**Trace Structure**:
```
Span: api_request
└─ Span: business_logic
    └─ Span: database_query
        └─ [actual database work]
```

### Pattern 2: Error Tracking

```rust
#[traced]
async fn risky_operation() -> Result<Output> {
    step1().await?;          // Errors logged automatically
    step2().await?;          // With context
    Ok(Output::default())
}

#[traced]
async fn step1() -> Result<()> {
    database.connect().await  // Error logged at span level
}
```

### Pattern 3: Batch Processing

```rust
#[traced]
async fn process_batch(items: Vec<Item>) -> Result<Vec<Output>> {
    let mut results = Vec::new();

    for item in items {
        // Each process_item call creates child span
        match process_item(item).await {
            Ok(output) => results.push(output),
            Err(e) => {
                // Errors logged per item
                eprintln!("Error: {}", e);
            }
        }
    }

    Ok(results)
}

#[traced]
async fn process_item(item: Item) -> Result<Output> {
    // Child span per item
    Ok(Output::default())
}
```

### Pattern 4: Conditional Tracing

```rust
#[traced]
async fn maybe_trace(should_trace: bool, data: Data) -> Result<()> {
    if should_trace {
        expensive_operation(&data).await?;
    } else {
        quick_operation(&data).await?;
    }
    Ok(())
}
```

---

## Troubleshooting

### Issue: Span Not Appearing in Jaeger

**Cause**: Tracing not initialized

```rust
// Missing this:
let config = TracingConfig::from_env()?;
init_tracing(config)?;
```

**Solution**: Initialize Phase 9.1 tracing first

### Issue: No Arguments in Log

**Cause**: Using `#[traced]` without `#[instrument]`

```rust
// ❌ Only gets timing
#[traced]
fn my_function(user_id: u32) { }

// ✅ Gets timing AND arguments
#[traced]
#[instrument]
fn my_function(user_id: u32) { }
```

### Issue: Compilation Error with Complex Signature

**Cause**: Macro may have issues with very complex generic bounds

```rust
// ❌ Might fail
#[traced]
async fn complex<T: Trait1 + Trait2 + Send>(x: T) where T: Trait3 { }

// ✅ Workaround: Use type alias
type ComplexType = dyn Trait1 + Trait2 + Send;

#[traced]
async fn complex(x: &ComplexType) { }
```

### Issue: String Literal Required for Span Name

**Cause**: Attempting to use variable for span name

```rust
// ❌ Compiler error
let span_name = "my_span";
#[traced(name = span_name)]
async fn my_function() { }

// ✅ Use string literal
#[traced(name = "my_span")]
async fn my_function() { }

// ✅ Use default
#[traced]
async fn my_span() { }
```

---

## Advanced Usage

### Custom Spans with Macros

Combine macros with manual span creation:

```rust
use tracing::info_span;

#[traced(name = "custom_operation")]
async fn operation_with_custom_spans() {
    let span = info_span!("custom_part", id = 123);
    let _guard = span.enter();

    // Child span under traced macro span
    do_work().await;
}

#[traced]
async fn do_work() {
    // Nested within custom_operation
}
```

### With Metrics

Combine tracing with Phase 9.2.A metrics:

```rust
use fraiseql_observers::metrics::ObserverMetrics;

#[traced]
async fn tracked_operation(metrics: &ObserverMetrics) -> Result<()> {
    let start = std::time::Instant::now();

    let result = do_work().await;

    let duration_ms = start.elapsed().as_millis() as f64;
    metrics.action_duration_ms.observe(duration_ms);

    result
}
```

### Macro Composition

Stack multiple macros for combined effects:

```rust
#[traced(name = "api_endpoint")]    // Span creation + timing
#[instrument]                        // Structured logging
async fn endpoint(id: u32) -> Result<String> {
    // Both macros applied:
    // 1. Arguments logged at entry
    // 2. Span created with timing
    // 3. Errors captured automatically
    Ok(format!("Result {}", id))
}
```

---

## Integration Checklist

Before using macros in production:

- [ ] Phase 9.1 tracing initialized (`init_tracing()` called)
- [ ] Jaeger running and accessible
- [ ] Macro crate added to dependencies
- [ ] Key functions decorated with `#[traced]`
- [ ] Important function arguments logged with `#[instrument]`
- [ ] Spans visible in Jaeger at http://localhost:16686
- [ ] No compilation warnings
- [ ] Performance acceptable for your use case

---

## Migration Guide

### From Manual Instrumentation

**Before** (Phase 9.1 manual):
```rust
async fn process_event(event: &Event) -> Result<()> {
    let tracer = ListenerTracer::new("processor");
    tracer.record_event(event);

    let start = std::time::Instant::now();
    let result = execute(&event).await;
    let duration = start.elapsed().as_millis();

    tracing::debug!(duration_ms = duration, "event processed");
    result
}
```

**After** (Phase 9.2.B macros):
```rust
#[traced(name = "process_event")]
async fn process_event(event: &Event) -> Result<()> {
    execute(&event).await
}
```

### Gradual Adoption

You don't need to convert all functions at once:

```rust
// Phase 9.1 still works
let tracer = ListenerTracer::new("listener");

// Phase 9.2.B macros also work
#[traced]
async fn new_function() { }

// Mix both in same application
#[traced]
async fn hybrid_function() {
    // Macro-created span
    legacy_function().await;  // Manual span from Phase 9.1
}
```

---

## Next Phase: Phase 9.2.C

After macros work well, Phase 9.2.C adds:

- Log correlation with trace IDs
- Structured logging fields
- Correlation ID propagation
- Log filtering by trace ID

---

## Testing Macros

### Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_traced_macro() {
        let config = TracingConfig::default();
        init_tracing(config).ok();

        let result = traced_function().await;
        assert!(result.is_ok());
    }

    #[traced]
    async fn traced_function() -> Result<()> {
        Ok(())
    }
}
```

### Integration Test

```rust
#[tokio::test]
async fn test_trace_propagation() {
    let config = TracingConfig::from_env()?;
    init_tracing(config)?;

    root_function().await?;

    // Flush spans to Jaeger
    flush_spans()?;

    // Verify in Jaeger
    let traces = jaeger_client.find_traces("root_function")?;
    assert!(!traces.is_empty());
}

#[traced]
async fn root_function() -> Result<()> {
    child_function().await
}

#[traced]
async fn child_function() -> Result<()> {
    Ok(())
}
```

---

## File Reference

**Macro Implementation**: `crates/fraiseql-observers-macros/src/lib.rs`
- `traced()` macro: Lines 27-107
- `instrument()` macro: Lines 120-177

**Usage in Main Crate**: `crates/fraiseql-observers/src/lib.rs`
- Re-export macros from proc-macro crate

---

## Summary

Phase 9.2.B provides two powerful macros:

1. **`#[traced]`** - Automatic span creation with timing and error tracking
2. **`#[instrument]`** - Structured argument logging

These macros eliminate boilerplate instrumentation code while integrating seamlessly with Phase 9.1 distributed tracing and Jaeger visualization.

**Key Benefits**:

- ✅ Reduces instrumentation boilerplate
- ✅ Automatic timing and error tracking
- ✅ Seamless Phase 9.1 integration
- ✅ Works with any async runtime
- ✅ Zero runtime overhead when spans aren't sampled

---

**Document**: Phase 9.2.B - Automatic Span Creation via Macros Guide
**Status**: Complete
**Last Updated**: January 22, 2026
