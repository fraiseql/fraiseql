# FraiseQL Observer System

A comprehensive, event-driven observer system for FraiseQL that enables post-mutation side effects through reliable, configurable action execution with built-in retry logic and dead letter queue support.

## Features

- **Event-Driven Architecture**: React to database mutations (INSERT, UPDATE, DELETE) in real-time
- **Flexible Actions**: 7 action types with more planned
  - Webhook: HTTP POST to external endpoints
  - Slack: Send messages to Slack channels
  - Email: Send emails via SMTP
  - SMS: Send text messages (stub, future implementation)
  - Push Notifications: Send mobile push notifications (stub)
  - Search: Index/update/delete documents in search engines (stub)
  - Cache: Invalidate or refresh cache entries (stub)
- **Condition Evaluation**: DSL for conditional action execution
  - Field comparisons: `status = 'shipped'`, `total > 100`
  - Change detection: `CHANGED(status)`, `CHANGED_TO(status, 'active')`
  - Logical operators: `AND`, `OR`, `NOT`
  - Field checks: `HAS_FIELD(email)`
- **Reliability**:
  - Automatic retry logic with configurable backoff (exponential, linear, fixed)
  - Dead Letter Queue for failed actions
  - Failure policies (Log, Alert, DLQ)
  - Event audit trail
- **Performance**:
  - Bounded channel for backpressure handling
  - Concurrent action execution
  - Non-blocking event processing
- **Observability**:
  - Structured logging with tracing
  - Execution summaries with timing
  - DLQ views for monitoring and debugging

## Architecture

```
Database Mutation
      ↓
PostgreSQL LISTEN/NOTIFY
      ↓
EventListener (watches on separate connection)
      ↓
Bounded MPSC Channel (backpressure management)
      ↓
ObserverExecutor (main engine)
      ├─ EventMatcher (find applicable observers)
      ├─ ConditionParser (evaluate conditions)
      ├─ Action Executors (webhook, Slack, etc.)
      ├─ Retry Logic (exponential/linear/fixed backoff)
      └─ DeadLetterQueue (failed actions for manual retry)
```

## Quick Start

### Define Observers

```rust
use fraiseql_observers::{
    config::{ActionConfig, ObserverDefinition, RetryConfig, FailurePolicy},
    event::EventKind,
};
use std::collections::HashMap;

// Define a webhook action
let webhook_action = ActionConfig::Webhook {
    url: Some("https://example.com/webhook".to_string()),
    url_env: None,
    headers: {
        let mut m = HashMap::new();
        m.insert("Authorization".to_string(), "Bearer token".to_string());
        m
    },
    body_template: Some(r#"{"event": "{{entity_type}}", "id": "{{entity_id}}"}"#.to_string()),
};

// Define an observer
let observer = ObserverDefinition {
    id: "order_created".to_string(),
    name: "Notify on Order Created".to_string(),
    entity_type: "Order".to_string(),
    event_kind: Some(EventKind::Created),
    condition: None, // No condition = always execute
    actions: vec![webhook_action],
    enabled: true,
};
```

### Process Events

```rust
use fraiseql_observers::{
    event::{EntityEvent, EventKind},
    executor::ObserverExecutor,
    matcher::EventMatcher,
    testing::mocks::MockDeadLetterQueue,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

// Create executor
let dlq = Arc::new(MockDeadLetterQueue::new());
let executor = ObserverExecutor::new(EventMatcher::new(), dlq);

// Create event
let event = EntityEvent::new(
    EventKind::Created,
    "Order".to_string(),
    Uuid::new_v4(),
    json!({
        "id": "12345",
        "total": 150.00,
        "customer": "Alice"
    }),
);

// Process event
let summary = executor.process_event(&event).await?;
println!("Executed {} actions, {} succeeded, {} failed",
    summary.total_actions(),
    summary.successful_actions,
    summary.failed_actions);
```

## Configuration

### Action Types

#### Webhook
```json
{
  "type": "webhook",
  "url": "https://example.com/webhook",
  "headers": {
    "Authorization": "Bearer token"
  },
  "body_template": "{\"status\": \"{{ status }}\"}"
}
```

#### Slack
```json
{
  "type": "slack",
  "webhook_url": "https://hooks.slack.com/services/...",
  "channel": "#notifications",
  "message_template": "Order {{entity_id}} created"
}
```

#### Email
```json
{
  "type": "email",
  "to": "admin@example.com",
  "subject": "New Order",
  "body_template": "Order {{entity_id}} has been placed"
}
```

#### SMS
```json
{
  "type": "sms",
  "phone": "+1234567890",
  "message_template": "Order {{entity_id}} shipped"
}
```

#### Push Notification
```json
{
  "type": "push",
  "device_token": "device123",
  "title_template": "Order Update",
  "body_template": "Order {{entity_id}} status: {{status}}"
}
```

#### Search Index
```json
{
  "type": "search",
  "index": "orders",
  "id_template": "order_{{entity_id}}"
}
```

#### Cache
```json
{
  "type": "cache",
  "key_pattern": "order:{{entity_id}}",
  "action": "invalidate"
}
```

### Retry Strategies

```rust
use fraiseql_observers::config::{RetryConfig, BackoffStrategy};

// Exponential backoff: 100ms, 200ms, 400ms, 800ms (capped at 5s)
let exponential = RetryConfig {
    max_attempts: 4,
    initial_delay_ms: 100,
    max_delay_ms: 5000,
    backoff_strategy: BackoffStrategy::Exponential,
};

// Linear backoff: 500ms, 1000ms, 1500ms
let linear = RetryConfig {
    max_attempts: 3,
    initial_delay_ms: 500,
    max_delay_ms: 5000,
    backoff_strategy: BackoffStrategy::Linear,
};

// Fixed backoff: always 1000ms
let fixed = RetryConfig {
    max_attempts: 3,
    initial_delay_ms: 1000,
    max_delay_ms: 1000,
    backoff_strategy: BackoffStrategy::Fixed,
};
```

### Failure Policies

- **Log**: Log the error and continue
- **Alert**: Log the error with alert level and continue
- **DLQ**: Move failed action to Dead Letter Queue for manual retry

```rust
use fraiseql_observers::config::FailurePolicy;

let policy = FailurePolicy::DLQ; // Move to queue on failure
```

## Condition DSL

### Basic Comparisons

- `status = 'active'` - String equality
- `total > 100` - Numeric comparison
- `amount >= 50` - Greater than or equal
- `count < 10` - Less than

### Change Detection

- `CHANGED(field_name)` - Field value changed
- `CHANGED_TO(field_name, 'value')` - Field changed to specific value
- `CHANGED_FROM(field_name, 'value')` - Field changed from specific value

### Field Checks

- `HAS_FIELD(email)` - Field exists in event data

### Logical Operators

- `condition1 AND condition2` - Both must be true
- `condition1 OR condition2` - At least one must be true
- `NOT condition` - Invert condition

### Examples
```
// Notify on high-value orders
total > 100 AND status = 'pending'

// Alert when status changes to shipped
CHANGED_TO(status, 'shipped')

// Process if user has email
HAS_FIELD(email) AND (status = 'active' OR priority = 'high')
```

## Database Schema

The observer system uses three main tables for reliability:

- **observer_events**: Event audit log
- **observer_dlq_items**: Failed actions queue
- **observer_dlq_history**: Retry attempt history

See [SCHEMA.md](./SCHEMA.md) for complete schema documentation and maintenance procedures.

## Testing

The observer system has comprehensive test coverage:

```bash
# Run all tests
cargo test -p fraiseql-observers

# Run specific test
cargo test -p fraiseql-observers test_webhook_action_validation

# Run with output
cargo test -p fraiseql-observers -- --nocapture
```

### Test Categories

- Configuration validation (12 tests)
- Failure policies (6 tests)
- Event kinds (6 tests)
- Retry configurations (8 tests)
- Event creation (6 tests)
- Dead Letter Queue (8 tests)
- **Total: 74 tests passing**

## Monitoring

### DLQ Views

Check pending retries:
```sql
SELECT * FROM observer_pending_retries;
```

Find exhausted items:
```sql
SELECT * FROM observer_retry_exhausted;
```

View recent failures:
```sql
SELECT * FROM observer_recent_failures;
```

### Metrics

From execution summary:
```rust
let summary = executor.process_event(&event).await?;
println!("Duration: {:.2}ms", summary.total_duration_ms);
println!("Success rate: {}/{}",
    summary.successful_actions,
    summary.total_actions());
```

## Performance Considerations

- **Event Processing**: Non-blocking, concurrent execution
- **Connection Pooling**: Reuse database connections
- **Bounded Channels**: Configurable backpressure (default: 1000 events)
- **Timeout Handling**: Configurable per action
- **Memory**: Efficient JSONB storage for events and actions

## Error Handling

All errors implement the `FraiseQLError` trait:

```rust
pub enum FraiseQLError {
    InvalidConfig { message: String },
    NoMatchingObservers { event_type: String },
    InvalidCondition { reason: String },
    ConditionEvaluationFailed { reason: String },
    InvalidActionConfig { reason: String },
    ActionExecutionFailed { reason: String },
    ActionPermanentlyFailed { reason: String },
    TemplateRenderingFailed { reason: String },
    DatabaseError { reason: String },
    ListenerConnectionFailed { reason: String },
    UnsupportedActionType { action_type: String },
    DlqError { reason: String },
    RetriesExhausted { reason: String },
}
```

Error classification:

- **Transient**: Will retry (timeout, connection refused)
- **Permanent**: Won't retry (invalid config, unsupported type)

## Advanced Usage

### Custom Event Listeners

Implement the `EventSource` trait to integrate custom event sources:

```rust
use fraiseql_observers::traits::EventSource;
use async_trait::async_trait;

pub struct CustomEventSource {
    // Your implementation
}

#[async_trait]
impl EventSource for CustomEventSource {
    async fn next_event(&mut self) -> Option<EntityEvent> {
        // Return next event
    }
}
```

### Custom DLQ Implementation

Implement the `DeadLetterQueue` trait for custom retry logic:

```rust
use fraiseql_observers::traits::DeadLetterQueue;
use async_trait::async_trait;

pub struct CustomDLQ {
    // Your implementation
}

#[async_trait]
impl DeadLetterQueue for CustomDLQ {
    async fn push(&self, event: EntityEvent, action: ActionConfig, error: String) -> Result<Uuid> {
        // Store failed action
    }
    // ... implement other methods
}
```

## Future Enhancements

- [ ] SMS integration (Twilio, AWS SNS)
- [ ] Push notifications (Firebase, APNs)
- [ ] Search indexing (Elasticsearch, Meilisearch)
- [ ] Cache backends (Redis, Memcached)
- [ ] Scheduled actions
- [ ] Action dependencies
- [ ] Batch actions
- [ ] Metrics integration (Prometheus)
- [ ] Distributed tracing (Jaeger)

## License

Part of FraiseQL project - See main LICENSE file for details

## Contributing

Contributions welcome! See CONTRIBUTING.md for guidelines.
