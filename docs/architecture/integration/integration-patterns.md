# FraiseQL Integration Patterns: Federation, Webhooks, and Messaging

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Integration architects, backend engineers, microservices specialists

---

## Executive Summary

FraiseQL integrates with external systems through three primary patterns:

1. **Federation** — Compose multiple GraphQL services (Apollo Federation v2)
2. **Webhooks** — Push events to external HTTP endpoints
3. **Messaging** — Publish events to message brokers (Kafka, RabbitMQ, etc.)

Each pattern provides different trade-offs between consistency, latency, and complexity.

---

## 1. Federation Patterns

### 1.1 Basic Federation (HTTP)

Standard Apollo Federation v2 with HTTP subgraph communication:

```
┌─────────────────┐
│  Apollo Router  │
│  (Gateway)      │
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
┌───▼──┐  ┌──▼───┐
│Users │  │Orders│
│Subgraph │  │Subgraph│
└────────┘  └───────┘

HTTP `_entities` calls for federation
```

**Implementation:**

```python
# Users subgraph
@fraiseql.type
@fraiseql.key(fields=["id"])
class User:
    id: ID!
    name: str
    email: str

# Orders subgraph (extended type)
@fraiseql.type(extend=True)
@fraiseql.key(fields=["id"])
class User:
    id: ID! = fraiseql.external()
    orders: [Order] = fraiseql.requires(fields=["id"])
```

**Latency characteristics:**

```
Single entity resolution: 50-200ms (HTTP roundtrip)
Federation 1 level deep: 50-200ms
Federation 2 levels deep: 100-400ms (cascading calls)
Federation 3+ levels: Unacceptable (avoid)
```

### 1.2 Database-Linked Federation (PostgreSQL FDW)

Optimization for same-database FraiseQL-to-FraiseQL federation:

```
┌──────────────────────────────────────┐
│ PostgreSQL (Primary Cluster)         │
├──────────────────────────────────────┤
│ Schema: users_schema                 │
│ ├─ tb_user, v_user                   │
│ └─ Foreign table: orders_schema.v_order (via FDW)
│                                       │
│ Schema: orders_schema                │
│ ├─ tb_order, v_order                 │
│ └─ Foreign table: users_schema.v_user (via FDW)
└──────────────────────────────────────┘

Both subgraphs accessible via database-level join
```

**Setup FDW:**

```sql
-- In users database:
CREATE EXTENSION IF NOT EXISTS postgres_fdw;

CREATE SERVER orders_fdw FOREIGN DATA WRAPPER postgres_fdw
  OPTIONS (host 'orders-db', dbname 'orders_db', port '5432');

CREATE USER MAPPING FOR current_user SERVER orders_fdw
  OPTIONS (user 'fdw_user', password 'secret');

-- Foreign table
CREATE FOREIGN TABLE orders_schema_v_order (
    pk_order INTEGER,
    id UUID,
    user_id UUID,
    data JSONB
) SERVER orders_fdw
  OPTIONS (schema_name 'orders_schema', table_name 'v_order');
```

**Entity resolution with FDW:**

```sql
-- Resolve User with orders (FDW join)
CREATE FUNCTION resolve_user_with_orders(keys UUID[]) RETURNS JSONB[] AS $$
  SELECT array_agg(
    u.data || jsonb_build_object(
      'orders', COALESCE(o.orders, '[]'::jsonb)
    ) ORDER BY idx
  )
  FROM unnest(keys) WITH ORDINALITY AS t(key, idx)
  JOIN users_schema.v_user u ON u.id = t.key
  LEFT JOIN (
    SELECT user_id, jsonb_agg(data ORDER BY created_at DESC) AS orders
    FROM orders_schema_v_order
    WHERE user_id = ANY(keys)
    GROUP BY user_id
  ) o ON o.user_id = u.id
$$ LANGUAGE sql STABLE;
```

**Latency characteristics:**

```
Single entity resolution: 5-15ms (database join, no network)
Federation 1 level deep: 5-15ms (10x faster than HTTP)
Federation 2 levels deep: 10-30ms (same database, all FDW)
```

### 1.3 Hybrid Federation (Mixed HTTP and FDW)

Combine HTTP and database-level federation:

```
┌────────────────────────────────────┐
│ Users (FraiseQL on PostgreSQL)     │
├────────────────────────────────────┤
│ ├─ Orders via FDW (same DB): 10ms  │
│ ├─ Products via HTTP (Apollo): 100ms
│ └─ Inventory via FDW (same DB): 10ms
```

**Strategy selection (auto-detect):**

```python
# At compile time, detect federation targets:
if target_subgraph.is_fraiseql and target_db_type == source_db_type:
    resolution_strategy = "database_linking"  # FDW
else:
    resolution_strategy = "http"  # Standard federation
```

**Example:**

```python
@fraiseql.type
class Product:
    id: ID!
    name: str

    # This comes from Orders subgraph (FraiseQL, same DB)
    @fraiseql.requires(fields=["id"])
    orders: [Order]  # Will use FDW (fast)

    # This comes from Inventory subgraph (Apollo Server)
    @fraiseql.requires(fields=["id"])
    inventory: Inventory  # Will use HTTP (standard)
```

---

## 2. Webhook Patterns

### 2.1 Webhook Delivery

Push events to external HTTP endpoints:

```
FraiseQL Event
    ↓
Webhook Dispatcher
    ├─ Serialize event to JSON
    ├─ Sign with HMAC
    ├─ POST to webhook URL
    └─ Track delivery status

External System
    ├─ Verify HMAC signature
    ├─ Deserialize event
    ├─ Process event
    └─ Return 200 OK

FraiseQL marks delivered
```

### 2.2 Webhook Configuration

Configure webhooks:

```python
@fraiseql.webhook(
    name="order_created_webhook",
    url="https://external.com/webhooks/order_created",
    events=["order_created"],
    secret="webhook_secret_key_123"
)
def on_order_created(event):
    """Webhook for order creation"""
    pass

# Register webhook
fraiseql.webhooks.register(
    event_type="order_created",
    webhook_url="https://external.com/webhooks/order_created",
    secret="webhook_secret_key_123"
)
```

### 2.3 Webhook Payload Format

Standard webhook format:

```json
{
  "id": "evt-abc123",
  "type": "order_created",
  "timestamp": "2026-01-15T10:30:45Z",
  "data": {
    "order_id": "order-789",
    "user_id": "user-456",
    "total": 150.00,
    "items": [...]
  },
  "metadata": {
    "webhook_id": "webhook-123",
    "attempt": 1,
    "timestamp": "2026-01-15T10:30:45Z"
  },
  "signature": "sha256=abcdef123..."
}
```

### 2.4 Webhook Retry Logic

Handle delivery failures:

```python
# Retry strategy
Attempt 1: Immediate
Attempt 2: +5 seconds (exponential backoff)
Attempt 3: +25 seconds
Attempt 4: +125 seconds
Attempt 5: +625 seconds
Max attempts: 5 (over ~20 minutes)

# Final failure
After 5 failed attempts:
  ├─ Mark webhook delivery as failed
  ├─ Alert operations team
  ├─ Can manually retry via dashboard
```

### 2.5 Webhook Signature Verification

Secure webhooks with HMAC:

```python
import hmac
import hashlib

# Webhook payload
payload = json.dumps(event).encode()

# Shared secret
secret = "webhook_secret_key_123"

# Calculate signature
signature = hmac.new(
    secret.encode(),
    payload,
    hashlib.sha256
).hexdigest()

# Include in header
headers = {
    "X-FraiseQL-Signature": f"sha256={signature}"
}

# Recipient verifies
received_signature = request.headers.get("X-FraiseQL-Signature")
expected_signature = hmac.new(
    secret.encode(),
    request.body,
    hashlib.sha256
).hexdigest()

if not hmac.compare_digest(received_signature, f"sha256={expected_signature}"):
    raise ValueError("Invalid signature")
```

### 2.6 Webhook Idempotency

Handle duplicate deliveries:

```python
# Webhook includes event ID
event = {
    "id": "evt-abc123",  # Unique event identifier
    "type": "order_created",
    "data": {...}
}

# Recipient deduplicates
recipient_side_dedup:
    if db.get_event_id("evt-abc123"):
        return  # Already processed

    # Process event
    process_event(event)

    # Mark as processed
    db.mark_event_processed("evt-abc123")
```

---

## 3. Message Broker Patterns

### 3.1 Kafka Integration

Publish events to Kafka topics:

```python
@fraiseql.kafka_publisher(
    topic="fraiseql.events",
    broker="kafka://broker1:9092,broker2:9092"
)
async def publish_to_kafka(event):
    """Publish FraiseQL events to Kafka"""
    pass

# Configuration
fraiseql.messaging.configure({
    "kafka": {
        "enabled": True,
        "brokers": ["kafka1:9092", "kafka2:9092"],
        "topic": "fraiseql.events",
        "compression": "snappy"
    }
})
```

**Kafka message format:**

```json
{
  "event_id": "evt-abc123",
  "event_type": "order_created",
  "timestamp": "2026-01-15T10:30:45Z",
  "source": "fraiseql",
  "version": "2.0.0",
  "data": {
    "order_id": "order-789",
    "user_id": "user-456"
  }
}
```

### 3.2 RabbitMQ Integration

Publish events to RabbitMQ exchanges:

```python
@fraiseql.rabbitmq_publisher(
    exchange="fraiseql.events",
    routing_key="fraiseql.{event_type}"
)
async def publish_to_rabbitmq(event):
    """Publish FraiseQL events to RabbitMQ"""
    pass

# Configuration
fraiseql.messaging.configure({
    "rabbitmq": {
        "enabled": True,
        "url": "amqp://user:pass@localhost:5672/",
        "exchange": "fraiseql.events",
        "exchange_type": "topic",
        "durable": True
    }
})
```

### 3.3 Consumer Groups (Kafka)

Multiple consumers process events:

```
Kafka topic: fraiseql.events
├─ Consumer Group 1 (notifications)
│  ├─ Consumer 1A: Partition 0
│  ├─ Consumer 1B: Partition 1
│  └─ Consumer 1C: Partition 2
│
├─ Consumer Group 2 (analytics)
│  ├─ Consumer 2A: Partition 0
│  ├─ Consumer 2B: Partition 1
│  └─ Consumer 2C: Partition 2
│
└─ Consumer Group 3 (audit)
   ├─ Consumer 3A: Partition 0
   ├─ Consumer 3B: Partition 1
   └─ Consumer 3C: Partition 2

Each consumer group gets all events
Multiple consumers in same group share partitions
```

### 3.4 Event Stream Ordering

Guarantee ordering with message brokers:

```
Option 1: Topic-level (global order)
  ├─ All events go to single topic
  ├─ Consumers receive in order
  └─ Performance: Limited by single partition

Option 2: Event-type topics (per-entity order)
  ├─ fraiseql.events.orders
  ├─ fraiseql.events.users
  ├─ Each topic ordered within type
  ├─ Different types may interleave
  └─ Performance: Parallelized

FraiseQL choice: Option 2 (per-entity order)
```

---

## 4. Consistency Patterns

### 4.1 Eventual Consistency (Webhooks/Messaging)

When using webhooks or message brokers:

```
FraiseQL Event (T0)
  ├─ Immediately available to queries (database updated)
  ├─ Webhook sent (may take 100-500ms)
  ├─ Message broker published (may take 10-100ms)

External system processing:
  ├─ Receive webhook: T0 + 500ms
  ├─ Process event: T0 + 600ms
  ├─ Update external database: T0 + 700ms

External system query:
  ├─ At T0 + 100ms: Doesn't see change (not yet received)
  ├─ At T0 + 800ms: Sees change (processed)

Model: Eventual consistency (typically <1 second)
```

### 4.2 Request-Response Consistency (Federation)

When using federation (HTTP or FDW):

```
Query: Get User with Orders

FraiseQL (HTTP federation):
  ├─ Query users table
  ├─ For each user, call orders service
  ├─ All data from same logical point in time
  ├─ Consistent snapshot

Consistency: Strong (synchronous)
```

### 4.3 Idempotent Event Processing

Ensure external systems handle duplicate events:

```python
# External system receives events
# Webhook/message could be delivered twice

# Idempotent processing
async def process_order_created(event):
    # Check if already processed
    existing = db.query(
        "SELECT id FROM processed_events WHERE event_id = $1",
        [event.id]
    )

    if existing:
        return  # Already processed, safe to skip

    # Process event
    order = parse_order_data(event.data)
    db.insert("orders", order)

    # Mark as processed
    db.insert("processed_events", {"event_id": event.id, "processed_at": now()})
```

---

## 5. Integration Topology Patterns

### 5.1 Star Topology (Centralized)

One central FraiseQL service connects to many external systems:

```
        ┌─────────────┐
        │ FraiseQL    │
        │ (Central)   │
        └──────┬──────┘
    ┌───────┬──┴──┬────────┬──────┐
    │       │     │        │      │
┌───▼──┐ ┌─▼──┐ ┌▼───┐ ┌──▼──┐ ┌─▼───┐
│Kafka │ │ELK │ │S3  │ │Email│ │Auth │
└──────┘ └────┘ └────┘ └─────┘ └─────┘

Pros: Centralized control, single GraphQL API
Cons: Single point of failure, scalability limits
```

### 5.2 Federated Topology (Distributed)

Multiple FraiseQL services with federation:

```
┌──────────────────┐     ┌──────────────────┐
│ FraiseQL Users   │◄────►│ FraiseQL Orders  │
│ (Subgraph A)     │     │ (Subgraph B)     │
└──────────────────┘     └──────────────────┘
         ▲                       ▲
         │                       │
    ┌────▼────┐             ┌────▼────┐
    │ Auth    │             │ Kafka   │
    └─────────┘             └─────────┘

┌──────────────────┐
│ Apollo Router    │
│ (Gateway)        │
└──────────────────┘

Pros: Distributed, scalable, isolated
Cons: More complex deployment, eventual consistency
```

### 5.3 Hub-and-Spoke Topology (Hybrid)

Mix of federation and direct integrations:

```
        ┌─────────────┐
        │ FraiseQL    │
        │ (Hub)       │
        └──────┬──────┘
    ┌───────┬──┴──┬────────┐
    │       │     │        │
┌───▼──┐ ┌─▼───────▼───┐ ┌▼────────┐
│Kafka │ │ Federated   │ │ External│
│      │ │ Subgraphs   │ │ Systems │
└──────┘ │ (Users,     │ └─────────┘
         │  Orders)    │
         └─────────────┘

Pros: Flexible, balanced, controlled complexity
Cons: Moderate complexity, requires good design
```

---

## 6. Real-Time Synchronization Patterns

### 6.1 Dual-Write (Anti-pattern)

Don't do this:

```python
# ❌ WRONG: Write to both database and external system
def create_order(order):
    db.insert("orders", order)  # Write 1
    external_service.post("/orders", order)  # Write 2

# Problem: If Write 2 fails, inconsistency
# Order in FraiseQL but not in external system
# If Write 1 fails but Write 2 succeeds, reverse problem
# No atomicity across systems
```

### 6.2 Primary Database with CDC

Correct pattern:

```python
# ✅ CORRECT: Single write to database, events propagate
def create_order(order):
    db.insert("orders", order)  # Single write (atomic)

# Trigger fires → Event published
# → Webhooks called → External system updated
# → Kafka event → Analytics updated

# Inconsistency window: Order in FraiseQL, not yet in external
# But convergence guaranteed (eventual consistency)
```

---

## 7. Integration Monitoring

### 7.1 Metrics to Track

```
Federation:
  ├─ Subgraph latency: p50, p95, p99
  ├─ Entity resolution success rate
  ├─ Federation cache hit rate

Webhooks:
  ├─ Delivery success rate (target: >99%)
  ├─ Retry count distribution
  ├─ Latency from event to delivery
  └─ Failed webhook count (alert >0)

Messaging:
  ├─ Events published per second
  ├─ Consumer lag (target: <1 minute)
  ├─ Message loss (target: 0)
  └─ Throughput (MB/sec)
```

### 7.2 Alert Rules

```
Alert: Subgraph down
  ├─ Condition: Federation call fails 5+ times in 1 minute
  ├─ Action: Page on-call
  └─ Impact: Federation queries fail

Alert: Webhook delivery failing
  ├─ Condition: >5 consecutive failed deliveries
  ├─ Action: Alert operations team
  └─ Impact: External systems not notified

Alert: Consumer lag increasing
  ├─ Condition: Kafka lag >10 minutes
  ├─ Action: Scale consumers or investigate slowness
  └─ Impact: Analytics delayed
```

---

## 8. Best Practices

### 8.1 Federation

**DO:**

- ✅ Use federation for loosely-coupled services
- ✅ Use FDW for same-database services (10x faster)
- ✅ Design shallow federation (max 2 levels)
- ✅ Cache federation results
- ✅ Monitor subgraph latency

**DON'T:**

- ❌ Chain more than 2 levels of federation
- ❌ Use federation for internal services (too slow)
- ❌ Assume federation latency <100ms (plan for 100-200ms)

### 8.2 Webhooks

**DO:**

- ✅ Verify webhook signatures
- ✅ Implement idempotent processing
- ✅ Handle delivery failures gracefully
- ✅ Track webhook delivery status
- ✅ Provide webhook dashboard for debugging

**DON'T:**

- ❌ Trust webhook source without signature verification
- ❌ Process duplicate events twice
- ❌ Block webhook processing (use async)
- ❌ Ignore delivery failures

### 8.3 Messaging

**DO:**

- ✅ Use message brokers for high-throughput events
- ✅ Partition by entity for ordering
- ✅ Monitor consumer lag
- ✅ Implement dead-letter queues for failures
- ✅ Version message format

**DON'T:**

- ❌ Expect global event ordering (not possible)
- ❌ Use message broker for real-time (latency too high)
- ❌ Ignore message loss
- ❌ Deploy without monitoring consumer health

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL integrations balance consistency, latency, and complexity through chosen patterns.
