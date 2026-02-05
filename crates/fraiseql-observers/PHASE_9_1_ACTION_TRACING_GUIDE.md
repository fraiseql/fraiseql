# Phase 9.1.D - Action Tracing Guide

**Status**: Implementation Complete
**Last Updated**: January 22, 2026

---

## Overview

.1.D implements tracing instrumentation for action execution (webhook, email, Slack). This guide shows how to integrate action tracing with your observer system.

---

## Quick Start - Trace Webhook Actions

### Basic Webhook Tracing

```rust
use fraiseql_observers::tracing::WebhookTracer;
use fraiseql_observers::actions::WebhookAction;

#[tokio::main]
async fn main() -> Result<()> {
    // Create webhook action
    let webhook = WebhookAction::new();

    // Create tracer for the webhook endpoint
    let tracer = WebhookTracer::new("https://api.example.com/webhook".to_string());

    // Record execution start
    tracer.record_start();

    // Execute webhook action
    match webhook.execute(
        "https://api.example.com/webhook",
        &HashMap::new(),
        None,
        &event
    ).await {
        Ok(response) => {
            // Record success
            tracer.record_success(response.status_code, response.duration_ms);
        }
        Err(e) => {
            // Record failure
            tracer.record_failure(&e.to_string(), 0.0);
        }
    }

    Ok(())
}
```

### Webhook with Trace Context Propagation

```rust
use fraiseql_observers::tracing::{WebhookTracer, TraceContext};
use std::collections::HashMap;

let trace_context = TraceContext::new(
    "a".repeat(32),
    "b".repeat(16),
    0x01
);

let tracer = WebhookTracer::new("https://api.example.com/webhook".to_string());

// Get trace context headers
let headers = trace_context.to_headers();

// Record trace context injection
tracer.record_trace_context_injection(headers.len());

// Include headers in HTTP request
for (key, value) in headers {
    // Add to request headers
    println!("Header: {} = {}", key, value);
}
```

---

## Email Action Tracing

### Basic Email Tracing

```rust
use fraiseql_observers::tracing::EmailTracer;
use fraiseql_observers::actions::EmailAction;

let email = EmailAction::new();
let tracer = EmailTracer::new("user@example.com".to_string());

// Record email start
tracer.record_start("order_confirmation");

// Execute email action
match email.execute(
    "user@example.com",
    "Order Confirmation",
    Some("Your order {{ order_id }} is confirmed"),
    &event
).await {
    Ok(response) => {
        tracer.record_success(response.message_id.as_deref(), response.duration_ms);
    }
    Err(e) => {
        tracer.record_failure(&e.to_string(), 0.0);
    }
}
```

### Batch Email Tracing

```rust
use fraiseql_observers::tracing::EmailTracer;

let recipients = vec![
    "user1@example.com",
    "user2@example.com",
    "user3@example.com",
];

// Create tracers for batch
let tracers: Vec<EmailTracer> = recipients
    .iter()
    .map(|recipient| EmailTracer::new(recipient.to_string()))
    .collect();

// Record batch operation
if !tracers.is_empty() {
    tracers[0].record_batch_send(tracers.len());
}

// Record individual email starts
for tracer in &tracers {
    tracer.record_start("newsletter");
}

// Execute emails for each recipient
for (tracer, recipient) in tracers.iter().zip(recipients.iter()) {
    match send_email(recipient).await {
        Ok((message_id, duration_ms)) => {
            tracer.record_success(Some(&message_id), duration_ms);
        }
        Err(e) => {
            tracer.record_failure(&e.to_string(), 0.0);
        }
    }
}
```

---

## Slack Action Tracing

### Basic Slack Tracing

```rust
use fraiseql_observers::tracing::SlackTracer;
use fraiseql_observers::actions::SlackAction;

let slack = SlackAction::new();
let tracer = SlackTracer::new("#notifications".to_string());

// Record Slack start
tracer.record_start();

// Execute Slack action
match slack.execute(
    "https://hooks.slack.com/services/YOUR/WEBHOOK/URL",
    Some("#notifications"),
    Some("Event {{ event_type }} on {{ entity_type }}"),
    &event
).await {
    Ok(response) => {
        tracer.record_success(response.status_code, response.duration_ms);
    }
    Err(e) => {
        tracer.record_failure(&e.to_string(), 0.0);
    }
}
```

### Slack with Thread Management

```rust
use fraiseql_observers::tracing::SlackTracer;

let tracer = SlackTracer::new("#notifications".to_string());

tracer.record_start();

// Send message
let response = slack.execute(
    webhook_url,
    Some("#notifications"),
    Some("Order {{ order_id }} created"),
    &event
).await?;

tracer.record_success(response.status_code, response.duration_ms);

// Create thread for discussion
tracer.record_thread_created("ts-1234567890.123456");

// Track reactions
tracer.record_reaction("👍");
tracer.record_reaction("⚡");
```

---

## Generic Action Span Tracking

### Using ActionSpan for Any Action

```rust
use fraiseql_observers::tracing::ActionSpan;

let span = ActionSpan::new(
    "webhook".to_string(),
    "notify_user".to_string()
);

// Record action start
span.record_start_span();

// Execute action and measure timing
let start = std::time::Instant::now();
let success = execute_action().await.is_ok();
let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

// Record result
span.record_result_span(success, duration_ms);

// Record any errors
if !success {
    span.record_span_error("Connection timeout after 5s");
}
```

---

## Action Batch Execution

### Using ActionBatchExecutor

```rust
use fraiseql_observers::tracing::ActionBatchExecutor;

let mut executor = ActionBatchExecutor::new();

// Add actions to batch
executor.add_action("webhook", "notify_user");
executor.add_action("email", "send_confirmation");
executor.add_action("slack", "alert_team");

// Execute actions and collect results
let results = vec![
    (true, 50.0),      // webhook success, 50ms
    (true, 150.0),     // email success, 150ms
    (false, 3000.0),   // slack failure, 3000ms (timeout)
];

// Record batch execution with tracing
executor.execute_batch(&results);

// Track any errors
let errors = vec![
    ("slack", "webhook rate limited")
];
executor.record_batch_errors(&errors);
```

---

## Action Chain with Trace Propagation

### Using ActionChain for Sequential Actions

```rust
use fraiseql_observers::tracing::{ActionChain, TraceContext};

// Start with root trace context
let trace_context = TraceContext::new(
    "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
    "00f067aa0ba902b7".to_string(),
    0x01
);

// Create action chain
let mut chain = ActionChain::new(trace_context);

// Add actions sequentially, each gets child span
let webhook_ctx = chain.add_action("webhook");
let email_ctx = chain.add_action("email");
let slack_ctx = chain.add_action("slack");

// Each action maintains same trace_id but different span_id
assert_eq!(webhook_ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
assert_eq!(email_ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
assert_eq!(slack_ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");

// Execute actions with trace headers
let all_headers = chain.execute_action_chain();

// Each action can include trace headers in outgoing HTTP requests
for (action_name, headers) in actions.iter().zip(all_headers.iter()) {
    println!("Executing {} with trace context", action_name);
    for (key, value) in headers {
        println!("  {} = {}", key, value);
    }
}
```

---

## Integration Patterns

### Pattern 1: Traced Webhook Handler

```rust
use fraiseql_observers::tracing::{WebhookTracer, TraceContext};
use fraiseql_observers::actions::WebhookAction;

pub async fn execute_webhook_with_tracing(
    webhook_url: &str,
    event: &EntityEvent,
    trace_context: &TraceContext,
) -> Result<()> {
    let tracer = WebhookTracer::new(webhook_url.to_string());
    let webhook = WebhookAction::new();

    tracer.record_start();

    // Prepare headers with trace context
    let mut headers = HashMap::new();
    for (key, value) in trace_context.to_headers() {
        headers.insert(key, value);
    }
    tracer.record_trace_context_injection(headers.len());

    // Execute webhook
    match webhook.execute(webhook_url, &headers, None, event).await {
        Ok(response) => {
            tracer.record_success(response.status_code, response.duration_ms);
            Ok(())
        }
        Err(e) => {
            tracer.record_failure(&e.to_string(), 0.0);
            Err(e)
        }
    }
}
```

### Pattern 2: Traced Executor with Multiple Actions

```rust
use fraiseql_observers::tracing::ActionBatchExecutor;

pub async fn execute_actions_with_tracing(
    actions: &[Action],
    event: &EntityEvent,
) -> Result<()> {
    let mut executor = ActionBatchExecutor::new();

    // Track each action
    for action in actions {
        executor.add_action(&action.action_type, &action.name);
    }

    // Execute and collect results
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for action in actions {
        let start = std::time::Instant::now();
        match execute_action(action, event).await {
            Ok(_) => {
                let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                results.push((true, duration_ms));
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                results.push((false, duration_ms));
                errors.push((&action.name[..], e.to_string()));
            }
        }
    }

    // Record batch execution
    executor.execute_batch(&results);

    // Record errors
    let error_refs: Vec<_> = errors
        .iter()
        .map(|(name, msg)| (*name, msg.as_str()))
        .collect();
    executor.record_batch_errors(&error_refs);

    Ok(())
}
```

---

## Tracing Output

### In Jaeger UI

When you navigate to http://localhost:16686:

1. **Select Service**: `observer-service`
2. **Traces show**:
   - Root span: `process_event`
   - Child span: `execute_action`
     - WebhookTracer logs: `Webhook action starting`, `Webhook action succeeded`
     - EmailTracer logs: `Email action starting`, `Batch email send`
     - SlackTracer logs: `Slack action starting`, `Created Slack thread`

3. **View Details**:
   - Click any span to see all recorded attributes
   - View timing for each action
   - See trace context headers propagation

### Example Trace Output

```
process_event [200ms]
├── condition_evaluation [10ms]
│   └── (matched: true)
└── execute_action [190ms]
    ├── webhook_tracer [45ms]
    │   └── url: https://api.example.com/webhook
    │   └── status_code: 200
    │   └── Injected 2 trace context headers
    ├── email_tracer [80ms]
    │   └── recipient: user@example.com
    │   └── subject: Order Confirmation
    │   └── Batch send to 3 recipients
    └── slack_tracer [65ms]
        └── channel: #notifications
        └── status_code: 200
        └── Created thread: ts-1234567890
```

---

## Configuration for Action Tracing

### Environment Variables

```bash
# Enable action-level tracing details
export TRACING_ACTION_DETAILS=true

# Set webhook timeout warning threshold
export TRACING_WEBHOOK_TIMEOUT_WARN_MS=5000

# Set email batch size logging threshold
export TRACING_EMAIL_BATCH_WARN_SIZE=100

# Set Slack rate limit tracking
export TRACING_SLACK_RATE_LIMIT_TRACKING=true
```

### YAML Configuration

```yaml
tracing:
  enabled: true
  service_name: observer-service
  jaeger:
    endpoint: http://localhost:14268/api/traces
    sample_rate: 1.0

  # Action-specific tracing
  actions:
    webhook:
      trace_headers_injection: true
      track_status_codes: true
      track_retries: true

    email:
      track_batch_operations: true
      track_message_ids: true
      track_smtp_details: false

    slack:
      track_threads: true
      track_reactions: true
      track_rate_limits: true
```

---

## Performance Considerations

### Overhead

| Operation | Overhead | Notes |
|-----------|----------|-------|
| WebhookTracer instantiation | < 0.1ms | Negligible |
| EmailTracer batch tracking | < 0.2ms | Per batch |
| SlackTracer thread tracking | < 0.05ms | Per thread |
| ActionSpan creation | < 0.1ms | Lightweight |
| Trace header injection | < 1ms | Per action |

### Optimization Tips

1. **Use batch operations** for multiple actions
2. **Sample aggressively** for high-volume webhooks: `JAEGER_SAMPLE_RATE=0.01`
3. **Disable action details** in production: `TRACING_ACTION_DETAILS=false`
4. **Use ActionChain** for coordinated execution

---

## Testing Action Tracing

### Unit Test Example

```rust
#[test]
fn test_webhook_tracer_with_trace_context() {
    let trace_context = TraceContext::new(
        "a".repeat(32),
        "b".repeat(16),
        0x01,
    );

    let tracer = WebhookTracer::new("http://example.com/webhook".to_string());
    tracer.record_start();

    let headers = trace_context.to_headers();
    tracer.record_trace_context_injection(headers.len());

    tracer.record_success(200, 42.5);
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_action_batch_execution_with_tracing() {
    let mut executor = ActionBatchExecutor::new();
    executor.add_action("webhook", "notify");
    executor.add_action("email", "confirm");

    let results = vec![(true, 50.0), (true, 100.0)];
    executor.execute_batch(&results);

    // Verify tracing events were recorded
    assert_eq!(executor.actions.len(), 2);
}
```

---

## Troubleshooting

### Actions Not Appearing in Traces

1. **Check**: Is tracing enabled? `TRACING_ENABLED=true`
2. **Check**: Is Jaeger running? `docker ps | grep jaeger`
3. **Check**: Are tracers being called? Add debug logging

### Trace Context Not Propagating

1. **Verify**: TraceContext is created before actions
2. **Verify**: Headers are included in HTTP request
3. **Verify**: Receiving service accepts traceparent header

### High Overhead

1. **Reduce sampling**: `JAEGER_SAMPLE_RATE=0.1`
2. **Disable action details**: `TRACING_ACTION_DETAILS=false`
3. **Batch actions**: Use ActionChain instead of individual tracers

---

## Next Steps

- **Week 3**: Integrate with Jaeger backend for full visualization
- **Week 4**: Add action tracing to retry logic
- **Week 5**: Create dashboard for action performance analysis
- **Week 6**: Document complete action tracing patterns

---

## Related Documents

- [Phase 9.1 Design](PHASE_9_1_DESIGN.md)
- [Phase 9.1 Implementation Guide](PHASE_9_1_IMPLEMENTATION_GUIDE.md)
- [Core Instrumentation Guide](PHASE_9_1_IMPLEMENTATION_GUIDE.md#integration-points)

---

**Document**: Phase 9.1.D - Action Tracing Guide
**Status**: Complete
**Last Updated**: January 22, 2026
