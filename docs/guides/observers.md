# FraiseQL Observer System

**Version**: 2.0
**Status**: Production Ready
**Last Updated**: 2026-01-23

## Overview

The FraiseQL Observer System enables event-driven architectures by triggering actions (webhooks, notifications, emails) when database changes occur. Observers listen for INSERT, UPDATE, or DELETE events on specific entity types and execute configured actions automatically.

### Key Features

- **Event-Driven**: Trigger actions on database changes (INSERT/UPDATE/DELETE)
- **Conditional Execution**: Use FraiseQL DSL to filter events
- **Multiple Actions**: Webhooks, Slack notifications, email alerts
- **Automatic Retries**: Exponential/linear/fixed backoff strategies
- **Type-Safe**: Defined alongside schema in all language SDKs
- **Compiled**: Zero runtime overhead, optimized at build time

### Architecture

```
Database Change (INSERT/UPDATE/DELETE)
         ↓
   Change Log Table (tb_entity_change_log)
         ↓
   Observer Runtime (polls for new events)
         ↓
   Condition Evaluation (FraiseQL DSL)
         ↓
   Action Execution (webhooks, notifications)
         ↓
   Retry on Failure (configurable backoff)
```

---

## Quick Start

### Python

```python
from fraiseql import ObserverBuilder, Webhook, SlackAction, EmailAction

# Observer 1: High-value orders → Webhook + Slack
ObserverBuilder.create('onHighValueOrder')
    .entity('Order')
    .event('INSERT')
    .condition('total > 1000')
    .add_action(Webhook.create('https://api.example.com/high-value-orders'))
    .add_action(SlackAction.create('#sales', '🎉 High-value order {id}: ${total}'))
    .register()

# Observer 2: Order shipped → Customer email
ObserverBuilder.create('onOrderShipped')
    .entity('Order')
    .event('UPDATE')
    .condition("status.changed() and status == 'shipped'")
    .add_action(EmailAction.with_from(
        to='{customer_email}',
        subject='Your order {id} has shipped!',
        body='Track it here: https://example.com/track/{id}',
        from_email='noreply@example.com'
    ))
    .register()

# Observer 3: Payment failures → Slack + Webhook with retries
ObserverBuilder.create('onPaymentFailure')
    .entity('Payment')
    .event('UPDATE')
    .condition("status == 'failed'")
    .add_action(SlackAction.create('#payments', '⚠️ Payment failed: {order_id}'))
    .add_action(Webhook.create('https://api.example.com/payment-failures'))
    .retry(max_attempts=5, backoff_strategy='exponential')
    .register()
```

### TypeScript

```typescript
import { ObserverBuilder, Webhook, SlackAction, EmailAction } from '@fraiseql/core';

// Observer 1: High-value orders
ObserverBuilder.create('onHighValueOrder')
    .entity('Order')
    .event('INSERT')
    .condition('total > 1000')
    .addAction(Webhook.create('https://api.example.com/high-value-orders'))
    .addAction(SlackAction.create('#sales', '🎉 High-value order {id}: ${total}'))
    .register();

// Observer 2: Order shipped
ObserverBuilder.create('onOrderShipped')
    .entity('Order')
    .event('UPDATE')
    .condition("status.changed() and status == 'shipped'")
    .addAction(EmailAction.withFrom({
        to: '{customer_email}',
        subject: 'Your order {id} has shipped!',
        body: 'Track it here: https://example.com/track/{id}',
        from: 'noreply@example.com'
    }))
    .register();
```

### Go

```go
import "github.com/fraiseql/fraiseql-go/observers"

// Observer 1: High-value orders
observers.NewBuilder("onHighValueOrder").
    Entity("Order").
    Event("INSERT").
    Condition("total > 1000").
    AddAction(observers.Webhook("https://api.example.com/high-value-orders")).
    AddAction(observers.SlackAction("#sales", "🎉 High-value order {id}: ${total}")).
    Register()

// Observer 2: Order shipped
observers.NewBuilder("onOrderShipped").
    Entity("Order").
    Event("UPDATE").
    Condition("status.changed() and status == 'shipped'").
    AddAction(observers.EmailActionWithFrom(
        "{customer_email}",
        "Your order {id} has shipped!",
        "Track it here: https://example.com/track/{id}",
        "noreply@example.com",
    )).
    Register()
```

### Java

```java
import com.fraiseql.observers.*;

// Observer 1: High-value orders
ObserverBuilder.create("onHighValueOrder")
    .entity("Order")
    .event("INSERT")
    .condition("total > 1000")
    .addAction(Webhook.create("https://api.example.com/high-value-orders"))
    .addAction(SlackAction.create("#sales", "🎉 High-value order {id}: ${total}"))
    .register();

// Observer 2: Order shipped
ObserverBuilder.create("onOrderShipped")
    .entity("Order")
    .event("UPDATE")
    .condition("status.changed() and status == 'shipped'")
    .addAction(EmailAction.withFrom(
        "{customer_email}",
        "Your order {id} has shipped!",
        "Track it here: https://example.com/track/{id}",
        "noreply@example.com"
    ))
    .register();
```

### PHP

```php
use FraiseQL\{ObserverBuilder, Webhook, SlackAction, EmailAction};

// Observer 1: High-value orders
ObserverBuilder::create('onHighValueOrder')
    ->entity('Order')
    ->event('INSERT')
    ->condition('total > 1000')
    ->addAction(Webhook::create('https://api.example.com/high-value-orders'))
    ->addAction(SlackAction::create('#sales', '🎉 High-value order {id}: ${total}'))
    ->register();

// Observer 2: Order shipped
ObserverBuilder::create('onOrderShipped')
    ->entity('Order')
    ->event('UPDATE')
    ->condition("status.changed() and status == 'shipped'")
    ->addAction(EmailAction::withFrom(
        '{customer_email}',
        'Your order {id} has shipped!',
        'Track it here: https://example.com/track/{id}',
        'noreply@example.com'
    ))
    ->register();
```

---

## Concepts

### Events

Observers trigger on three database event types:

| Event | Trigger | Use Case |
|-------|---------|----------|
| `INSERT` | New row created | Welcome emails, order notifications |
| `UPDATE` | Row modified | Status changes, data sync |
| `DELETE` | Row deleted | Cleanup, archive, audit trail |

### Conditions

Use FraiseQL DSL to filter events:

```python
# Simple comparison
.condition('total > 1000')

# Field changes (UPDATE events only)
.condition('status.changed()')

# Combined conditions
.condition("status.changed() and status == 'shipped'")

# Multiple fields
.condition("total > 1000 and customer_tier == 'premium'")
```

**Available Functions**:
- `field.changed()` - True if field was modified (UPDATE only)
- `field.old_value()` - Previous value before UPDATE
- `field.new_value()` - New value after UPDATE

### Actions

#### 1. Webhook Action

Send HTTP POST request with event data:

```python
# Static URL
Webhook.create('https://api.example.com/webhook')

# URL from environment variable
Webhook.with_env('WEBHOOK_URL')

# With custom headers
Webhook.create('https://api.example.com/webhook', headers={
    'Authorization': 'Bearer {API_TOKEN}'
})

# Custom body template
Webhook.create('https://api.example.com/webhook', body_template='''
{
    "type": "order_created",
    "order_id": "{{id}}",
    "total": {{total}},
    "data": {{_json}}
}
''')
```

**Payload Format**:
```json
{
    "event": "INSERT",
    "entity": "Order",
    "id": "order-123",
    "data": {
        "id": "order-123",
        "customer_email": "user@example.com",
        "status": "pending",
        "total": 1500.00
    },
    "timestamp": "2026-01-23T10:30:00Z"
}
```

#### 2. Slack Action

Send message to Slack channel:

```python
# Basic message
SlackAction.create('#sales', 'New order {id} by {customer_email}')

# With emojis and formatting
SlackAction.create('#sales', '🎉 High-value order {id}: ${total}')

# URL from environment
SlackAction.with_env('SLACK_WEBHOOK_URL', '#alerts', 'Payment failed: {id}')
```

**Template Variables**:
- `{field_name}` - Insert field value
- `${field_name}` - Currency formatting
- `{_json}` - Full JSON payload

#### 3. Email Action

Send email notifications:

```python
# Simple email
EmailAction.create(
    to='admin@example.com',
    subject='New order {id}',
    body='Order {id} created for ${total}'
)

# Dynamic recipient from event data
EmailAction.with_from(
    to='{customer_email}',
    subject='Your order {id} has shipped!',
    body='Track: https://example.com/track/{id}',
    from_email='noreply@example.com'
)

# HTML email
EmailAction.create(
    to='admin@example.com',
    subject='Order Summary',
    body='<h1>Order {id}</h1><p>Total: ${total}</p>',
    content_type='text/html'
)
```

### Retry Configuration

Automatic retry with backoff strategies:

```python
# Exponential backoff (default)
.retry(
    max_attempts=3,
    backoff_strategy='exponential',
    initial_delay_ms=1000,
    max_delay_ms=60000
)

# Linear backoff
.retry(
    max_attempts=5,
    backoff_strategy='linear',
    initial_delay_ms=2000,
    max_delay_ms=10000
)

# Fixed delay
.retry(
    max_attempts=10,
    backoff_strategy='fixed',
    initial_delay_ms=5000,
    max_delay_ms=5000
)
```

**Backoff Formulas**:
- **Exponential**: `delay = min(initial * 2^attempt, max)`
- **Linear**: `delay = min(initial * attempt, max)`
- **Fixed**: `delay = initial`

---

## Compiled Schema Format

Observers are compiled into JSON schema:

```json
{
    "version": "1.0",
    "types": [
        {
            "name": "Order",
            "fields": { ... }
        }
    ],
    "observers": [
        {
            "name": "onHighValueOrder",
            "entity": "Order",
            "event": "INSERT",
            "condition": "total > 1000",
            "actions": [
                {
                    "type": "webhook",
                    "url": "https://api.example.com/high-value-orders"
                },
                {
                    "type": "slack",
                    "channel": "#sales",
                    "message": "🎉 High-value order {id}: ${total}"
                }
            ],
            "retry": {
                "max_attempts": 3,
                "backoff_strategy": "exponential",
                "initial_delay_ms": 1000,
                "max_delay_ms": 60000
            }
        }
    ]
}
```

---

## Migration Guide

### Adding Observers to Existing Schemas

**Step 1**: Update your SDK to version 2.0+

```bash
# Python
pip install --upgrade fraiseql

# TypeScript
npm install @fraiseql/core@^2.0.0

# Go
go get github.com/fraiseql/fraiseql-go@v2.0.0

# Java
# Update version in pom.xml or build.gradle

# PHP
composer require fraiseql/fraiseql:^2.0
```

**Step 2**: Add observer definitions to your schema

```python
# In your existing schema file (e.g., schema.py)
from fraiseql import ObserverBuilder, Webhook

# Add after your type definitions
ObserverBuilder.create('onOrderCreated')
    .entity('Order')
    .event('INSERT')
    .add_action(Webhook.create('https://api.example.com/orders'))
    .register()
```

**Step 3**: Recompile schema

```bash
fraiseql-cli compile schema.json
```

**Step 4**: Deploy updated schema

```bash
# Development
fraiseql-server --schema schema.compiled.json

# Production
# Update deployed schema file and restart server
```

### Backward Compatibility

- Schemas without observers continue to work
- `observers` field is optional in compiled JSON
- No breaking changes to existing APIs
- Observer runtime only starts if observers are defined

---

## Best Practices

### 1. Naming Conventions

Use descriptive, action-oriented names:

```python
# ✅ Good
'onHighValueOrder'
'onOrderShipped'
'onPaymentFailure'
'onUserRegistration'

# ❌ Bad
'observer1'
'order_webhook'
'check_status'
```

### 2. Condition Design

Keep conditions simple and readable:

```python
# ✅ Good - Clear intent
.condition('total > 1000')
.condition("status.changed() and status == 'shipped'")

# ❌ Bad - Too complex, hard to debug
.condition('(total > 1000 and tier == "premium") or (total > 5000 and tier == "standard") and status != "cancelled"')
```

For complex logic, use multiple observers:

```python
# Observer 1: Premium high-value orders
ObserverBuilder.create('onPremiumHighValueOrder')
    .entity('Order')
    .event('INSERT')
    .condition('total > 1000 and tier == "premium"')
    .add_action(...)
    .register()

# Observer 2: Standard very-high-value orders
ObserverBuilder.create('onStandardVeryHighValueOrder')
    .entity('Order')
    .event('INSERT')
    .condition('total > 5000 and tier == "standard"')
    .add_action(...)
    .register()
```

### 3. Action Design

**Group related actions**:
```python
ObserverBuilder.create('onOrderShipped')
    .entity('Order')
    .event('UPDATE')
    .condition("status.changed() and status == 'shipped'")
    .add_action(Webhook.create('https://api.shipping.com/notify'))
    .add_action(EmailAction.with_from(...))  # Customer notification
    .add_action(SlackAction.create('#ops', 'Order shipped: {id}'))
    .register()
```

**Separate unrelated actions**:
```python
# ❌ Bad - Mixing concerns
ObserverBuilder.create('onOrderEvent')
    .entity('Order')
    .event('UPDATE')
    .condition('status.changed()')  # Too broad
    .add_action(...)  # Different actions for different statuses
    .register()

# ✅ Good - Specific observers
ObserverBuilder.create('onOrderShipped')
    .condition("status == 'shipped'")
    ...

ObserverBuilder.create('onOrderCancelled')
    .condition("status == 'cancelled'")
    ...
```

### 4. Retry Configuration

**Choose appropriate backoff**:

```python
# External APIs - Exponential backoff (avoid overwhelming)
.retry(max_attempts=5, backoff_strategy='exponential')

# Critical internal webhooks - Linear backoff (predictable)
.retry(max_attempts=3, backoff_strategy='linear')

# Non-critical notifications - Fixed delay (simple)
.retry(max_attempts=3, backoff_strategy='fixed')
```

**Set reasonable limits**:
```python
# ✅ Good - Balanced
.retry(max_attempts=3, initial_delay_ms=1000, max_delay_ms=60000)

# ⚠️ Risky - Too aggressive (can cause cascading failures)
.retry(max_attempts=100, initial_delay_ms=100, max_delay_ms=1000000)

# ⚠️ Risky - Too passive (events may be lost)
.retry(max_attempts=1, initial_delay_ms=10000, max_delay_ms=10000)
```

### 5. Security

**Use environment variables for sensitive URLs**:
```python
# ✅ Good
Webhook.with_env('WEBHOOK_URL')

# ❌ Bad - Hardcoded secrets
Webhook.create('https://api.example.com/webhook?token=secret123')
```

**Validate webhook endpoints**:
```python
# Add authentication headers
Webhook.create('https://api.example.com/webhook', headers={
    'Authorization': 'Bearer {WEBHOOK_TOKEN}',
    'X-FraiseQL-Signature': '{signature}'
})
```

### 6. Performance

**Limit observer count per entity**:
- < 5 observers per entity: ✅ Excellent
- 5-10 observers per entity: ⚠️ Monitor performance
- > 10 observers per entity: ❌ Consider refactoring

**Use specific conditions**:
```python
# ✅ Good - Specific, fewer executions
.condition('status == "shipped"')

# ❌ Bad - Broad, many executions
.condition('status != null')
```

**Optimize webhook payloads**:
```python
# ✅ Good - Only necessary fields
Webhook.create(..., body_template='{"id": "{{id}}", "status": "{{status}}"}')

# ❌ Bad - Full payload for large entities
Webhook.create(..., body_template='{{_json}}')  # May be 100KB+
```

---

## Monitoring & Debugging

### Observer Execution Logs

All observer executions are logged in `tb_observer_log`:

```sql
SELECT
    observer_name,
    entity_type,
    entity_id,
    event_type,
    status,  -- 'success', 'failed', 'retrying'
    attempt_count,
    error_message,
    created_at
FROM core.tb_observer_log
WHERE observer_name = 'onHighValueOrder'
ORDER BY created_at DESC
LIMIT 100;
```

### Dead Letter Queue (DLQ)

Failed events after max retries go to DLQ:

```sql
SELECT
    observer_name,
    entity_id,
    error_message,
    retry_count,
    last_attempted_at,
    event_data
FROM core.tb_observer_dlq
WHERE observer_name = 'onPaymentFailure'
ORDER BY last_attempted_at DESC;
```

**Reprocess DLQ entries**:
```sql
-- Mark for retry (picked up by runtime)
UPDATE core.tb_observer_dlq
SET retry_count = 0,
    status = 'pending'
WHERE observer_name = 'onPaymentFailure'
  AND entity_id = 'payment-123';
```

### Metrics

Monitor observer health:

```sql
-- Success rate by observer
SELECT
    observer_name,
    COUNT(*) as total_executions,
    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as successes,
    ROUND(100.0 * SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) / COUNT(*), 2) as success_rate
FROM core.tb_observer_log
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY observer_name
ORDER BY success_rate ASC;

-- Retry counts
SELECT
    observer_name,
    AVG(attempt_count) as avg_retries,
    MAX(attempt_count) as max_retries
FROM core.tb_observer_log
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY observer_name;
```

### Runtime Health

Check observer runtime status:

```bash
# HTTP health endpoint
curl http://localhost:8080/health/observers

# Response:
{
    "status": "running",
    "observer_count": 12,
    "events_processed": 45230,
    "errors": 23,
    "last_checkpoint": "2026-01-23T10:30:00Z",
    "uptime_seconds": 86400
}
```

---

## Advanced Patterns

### Fan-Out Pattern

Trigger multiple observers for a single event:

```python
# Observer 1: Analytics
ObserverBuilder.create('analyticsOrderCreated')
    .entity('Order')
    .event('INSERT')
    .add_action(Webhook.create('https://analytics.example.com/track'))
    .register()

# Observer 2: Inventory
ObserverBuilder.create('inventoryOrderCreated')
    .entity('Order')
    .event('INSERT')
    .add_action(Webhook.create('https://inventory.example.com/reserve'))
    .register()

# Observer 3: Notifications
ObserverBuilder.create('notifyOrderCreated')
    .entity('Order')
    .event('INSERT')
    .add_action(EmailAction.create(...))
    .register()
```

### Conditional Routing

Route events to different endpoints based on data:

```python
# Route 1: Domestic orders
ObserverBuilder.create('onDomesticOrder')
    .entity('Order')
    .event('INSERT')
    .condition('country == "US"')
    .add_action(Webhook.create('https://domestic.shipping.com'))
    .register()

# Route 2: International orders
ObserverBuilder.create('onInternationalOrder')
    .entity('Order')
    .event('INSERT')
    .condition('country != "US"')
    .add_action(Webhook.create('https://international.shipping.com'))
    .register()
```

### Cascading Events

Chain observers across entities:

```python
# Step 1: Order created → Generate invoice
ObserverBuilder.create('onOrderCreated')
    .entity('Order')
    .event('INSERT')
    .add_action(Webhook.create('https://billing.example.com/generate-invoice'))
    .register()

# Step 2: Invoice created → Send email
ObserverBuilder.create('onInvoiceCreated')
    .entity('Invoice')
    .event('INSERT')
    .add_action(EmailAction.with_from(
        to='{customer_email}',
        subject='Invoice {id}',
        body='...'
    ))
    .register()
```

### Circuit Breaker Pattern

Automatically disable observers after repeated failures:

```sql
-- Create monitoring view
CREATE VIEW v_observer_health AS
SELECT
    observer_name,
    COUNT(*) FILTER (WHERE status = 'failed') as failure_count,
    MAX(created_at) as last_failure
FROM core.tb_observer_log
WHERE created_at > NOW() - INTERVAL '1 hour'
GROUP BY observer_name
HAVING COUNT(*) FILTER (WHERE status = 'failed') > 50;

-- Alert when threshold exceeded
-- (Integrate with monitoring system)
```

---

## Troubleshooting

### Observer Not Triggering

**Check 1**: Verify observer is loaded
```bash
# Check compiled schema
jq '.observers[] | select(.name == "onHighValueOrder")' schema.compiled.json
```

**Check 2**: Verify condition evaluation
```sql
-- Test condition with sample data
SELECT
    id,
    total,
    CASE WHEN total > 1000 THEN 'MATCH' ELSE 'NO MATCH' END as condition_result
FROM v_order
WHERE id = 'order-123';
```

**Check 3**: Check change log
```sql
-- Verify event was logged
SELECT * FROM core.tb_entity_change_log
WHERE object_type = 'Order'
  AND object_id = 'order-123'
ORDER BY created_at DESC
LIMIT 10;
```

### Webhook Failures

**Check 1**: Verify endpoint is reachable
```bash
curl -X POST https://api.example.com/webhook \
  -H "Content-Type: application/json" \
  -d '{"test": true}'
```

**Check 2**: Check observer logs
```sql
SELECT error_message, attempt_count
FROM core.tb_observer_log
WHERE observer_name = 'onHighValueOrder'
  AND status = 'failed'
ORDER BY created_at DESC
LIMIT 10;
```

**Check 3**: Review DLQ
```sql
SELECT entity_id, error_message, event_data
FROM core.tb_observer_dlq
WHERE observer_name = 'onHighValueOrder';
```

### High Retry Counts

**Symptom**: Observer succeeds but requires many retries

**Diagnosis**:
```sql
SELECT
    observer_name,
    AVG(attempt_count) as avg_attempts,
    MAX(attempt_count) as max_attempts
FROM core.tb_observer_log
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY observer_name
HAVING AVG(attempt_count) > 2;
```

**Solutions**:
1. Increase timeout for slow endpoints
2. Optimize webhook payload size
3. Check network latency
4. Review endpoint performance

### Memory Issues

**Symptom**: Observer runtime consuming excessive memory

**Causes**:
- Large event payloads (> 1MB)
- Too many concurrent observers
- Memory leak in action handler

**Solutions**:
```python
# 1. Reduce payload size
Webhook.create(..., body_template='{"id": "{{id}}"}')  # Not {{_json}}

# 2. Limit batch size
# In fraiseql-server config
observer_runtime:
  poll_interval_ms: 100
  batch_size: 10  # Default: 100

# 3. Add resource limits
# In Docker/K8s deployment
resources:
  limits:
    memory: 512Mi
  requests:
    memory: 256Mi
```

---

## Performance Tuning

### Poll Interval

Adjust based on latency requirements:

```toml
# fraiseql-server config
[observer_runtime]
poll_interval_ms = 100  # Low latency (10 events/sec max per poll)
poll_interval_ms = 1000  # Standard (default)
poll_interval_ms = 5000  # Batch processing
```

**Trade-offs**:
- Lower interval: Lower latency, higher CPU usage
- Higher interval: Higher throughput, higher latency

### Batch Size

Process multiple events per poll:

```toml
[observer_runtime]
batch_size = 10   # Low volume
batch_size = 100  # Standard (default)
batch_size = 1000 # High volume
```

**Recommendations**:
- < 100 events/min: batch_size = 10
- 100-1000 events/min: batch_size = 100
- > 1000 events/min: batch_size = 1000

### Checkpoint Frequency

Balance durability vs performance:

```toml
[observer_runtime]
checkpoint_interval_ms = 1000   # Every second (safest)
checkpoint_interval_ms = 5000   # Every 5 seconds (default)
checkpoint_interval_ms = 10000  # Every 10 seconds (fastest)
```

---

## API Reference

The Observer API is available in all supported languages: Python, TypeScript, Go, Java, and PHP. See the quick start section above for language-specific examples.

---

## FAQ

### Can I use observers with existing tables?

Yes! Observers work with any table that has the FraiseQL change log trigger installed. For existing tables:

```sql
-- Add change log trigger to existing table
CREATE TRIGGER tr_order_change_log
AFTER INSERT OR UPDATE OR DELETE ON public.t_order
FOR EACH ROW EXECUTE FUNCTION core.fn_log_entity_change('Order');
```

### Do observers slow down INSERT/UPDATE/DELETE?

Minimal impact:
- Change log insertion: ~1-2ms overhead
- Observer execution: Async, does not block transaction
- Bulk operations: Use `DISABLE TRIGGER` for large imports

### Can I disable observers temporarily?

Yes:

```bash
# Stop observer runtime
curl -X POST http://localhost:8080/admin/observers/stop

# Restart observer runtime
curl -X POST http://localhost:8080/admin/observers/start
```

### How do I test observers in development?

Use mock webhook servers:

```bash
# Start mock server
docker run -p 8080:8080 mockserver/mockserver

# Configure observer to use mock
Webhook.create('http://localhost:8080/webhook')

# Verify requests
curl http://localhost:8080/mockserver/expectation
```

### Can observers call other FraiseQL mutations?

Yes, via webhooks:

```python
ObserverBuilder.create('onOrderCreated')
    .entity('Order')
    .event('INSERT')
    .add_action(Webhook.create('http://localhost:8000/graphql',
        body_template='''
        {
            "query": "mutation { createInvoice(orderId: \"{{id}}\") { id } }"
        }
        ''',
        headers={'Content-Type': 'application/json'}
    ))
    .register()
```

---

## Further Reading

For more information on optimizing observer performance and security, see the Architecture documentation:
- [Performance Optimization](../architecture/performance/advanced-optimization.md)
- [Security Model](../architecture/security/security-model.md)

---

**Questions?** Open an issue on [GitHub](https://github.com/fraiseql/fraiseql/issues) or join our [Discord](https://discord.gg/fraiseql).
