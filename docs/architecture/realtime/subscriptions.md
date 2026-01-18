# Subscriptions: Event Projections from the Database

**Version:** 1.0
**Date:** January 11, 2026
**Status:** Accepted
**Audience:** Runtime Developers, Integration Engineers, Database Architects

---

## 1. Overview

### What Are FraiseQL Subscriptions?

FraiseQL subscriptions are **compiled projections of database events** delivered through multiple transport adapters. Unlike traditional GraphQL subscriptions that execute resolvers on-demand, FraiseQL subscriptions:

- **Originate from database transactions** — Events are sourced from committed database changes
- **Compiled, not interpreted** — Subscription schemas are static, known at build time
- **Transport-agnostic** — Same event stream delivers to graphql-ws clients, webhooks, Kafka, etc.
- **Deterministic** — No user code execution, no dynamic logic
- **Buffered and replayed** — Events persisted in `tb_entity_change_log` for durability

### Why Subscriptions Work Differently in FraiseQL

**Traditional GraphQL Subscriptions:**

```
Client subscribes to User.nameChanged
    ↓
Server executes resolver function
    ↓
Resolver polls database or listens to app events
    ↓
Resolver emits value to client
```

**FraiseQL Subscriptions:**

```
Database commits transaction (user.name updated)
    ↓
Database notifies change via LISTEN/NOTIFY
    ↓
Change captured in tb_entity_change_log
    ↓
Event matching filters from CompiledSchema
    ↓
Delivered via transport adapter (graphql-ws, webhook, Kafka)
```

### Use Cases

**1. Real-Time UI Updates**

- Client subscribes to OrderCreated events
- Receives updates within typical target envelope of <10ms (local network, reference deployment)
- No polling, deterministic delivery

**2. Event Streaming to External Systems**

- Backend service consumes events via Kafka adapter
- Replicates data to data warehouse
- Replays events from any point in time

**3. Multi-Tenant Change Notification**

- Organization receives updates for their entities only
- Row-level filtering enforced at compile time
- No cross-tenant data leakage

**4. Audit Trail Emission**

- All mutations automatically create subscription events
- Events sent to logging/analytics system
- Immutable record of all changes

---

## 2. Architecture

### 2.1 High-Level Event Flow

```
┌─────────────────────────────────────────────────────────────┐
│ Application (GraphQL Mutation / Direct SQL)                │
│ Executes: mutation CreateOrder($user_id, $amount)          │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ↓
        ┌──────────────────────────────────────┐
        │ PostgreSQL Database Transaction      │
        │ ├─ INSERT into tb_order              │
        │ ├─ Audit columns: created_at, etc.   │
        │ └─ COMMIT                            │
        └──────────────────────────┬───────────┘
                                   │
                ┌──────────────────┴──────────────────┐
                ↓                                     ↓
        ┌───────────────────┐            ┌──────────────────────┐
        │ LISTEN / NOTIFY   │            │ CDC (Change Data     │
        │ (Low-latency)     │            │  Capture)            │
        │                   │            │ (Durable)            │
        └─────────┬─────────┘            └──────────┬───────────┘
                  │                                   │
                  └──────────────┬────────────────────┘
                                 ↓
                    ┌────────────────────────────┐
                    │ tb_entity_change_log       │
                    │ (Event Buffer / Durability)│
                    │ ├─ event_id                │
                    │ ├─ entity_type (Order)     │
                    │ ├─ entity_id               │
                    │ ├─ operation (INSERT)      │
                    │ ├─ data (JSONB)            │
                    │ └─ created_at              │
                    └────────────┬────────────────┘
                                 │
                    ┌────────────┴─────────────┐
                    ↓                         ↓
    ┌──────────────────────────┐  ┌──────────────────────┐
    │ Subscription Matcher     │  │ Subscription Matcher │
    │ (Filter evaluation)      │  │ (Filter evaluation)  │
    │ ├─ WHERE user_id = ?     │  │ ├─ WHERE org_id = ?  │
    │ └─ Match against         │  │ └─ Match against     │
    │   subscriptions          │  │   subscriptions      │
    └─────────┬────────────────┘  └──────────┬───────────┘
              │                               │
    ┌─────────┴─────────┬────────────────────┘
    ↓                   ↓
┌─────────────────┐  ┌──────────────────┐
│ graphql-ws      │  │ Kafka Adapter    │
│ Adapter         │  │                  │
│ (WebSocket)     │  │ Event Stream:    │
│                 │  │ ├─ OrderCreated  │
│ Clients:        │  │ ├─ OrderUpdated  │
│ ├─ Browser UI   │  │ └─ OrderDeleted  │
│ ├─ Mobile App   │  │                  │
│ └─ Dashboard    │  │ Consumers:       │
│                 │  │ ├─ Analytics     │
│ Target: <10ms   │  │ ├─ Warehouse     │
│ (typical)       │  │ └─ Replication   │
└─────────────────┘  └──────────────────┘
     (real-time UI)      (event streaming)
```

### 2.2 Components

**Database Layer (PostgreSQL)**

- Transactions commit changes to `tb_*` tables
- LISTEN/NOTIFY notifies subscription system of changes
- CDC captures changes for durability and replay

**Event Buffer (`tb_entity_change_log`)**

- Persists all events with monotonic sequence numbers
- Enables replay from any point in time
- Acts as backpressure buffer if transport is slow
- Debezium-compatible envelope format

**Subscription Matcher**

- Evaluates compiled subscription filters
- Groups events by destination (graphql-ws client, webhook, Kafka topic)
- Transforms event to projection shape

**Transport Adapters**

- `graphql-ws`: WebSocket for browser clients
- `webhooks`: HTTP POST to external endpoints
- `kafka`: Push to Kafka topic
- `grpc`: Future extension for service-to-service

**Client Connections**

- Establish transport connection (WebSocket, webhook, etc.)
- Authenticate and authorize
- Subscribe to specific subscription types with filters
- Receive events until disconnect

### 2.3 Event Format

All subscription events follow this structure:

```json
{
  "event_id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "event_name": "OrderCreated",
  "entity_name": "Order",
  "entity_id": "ord_123",
  "operation": "CREATE",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "data": {
    "id": "ord_123",
    "user_id": "usr_456",
    "amount": 99.99,
    "created_at": "2026-01-11T15:35:00.123456Z"
  }
}
```

**Mapping:**

- `event_name`: Subscription type name (e.g., `OrderCreated`)
- `entity_name`: GraphQL type (e.g., `Order`)
- `operation`: CREATE, UPDATE, DELETE
- `data`: Projected fields (determined by subscription query)

---

## 3. Subscription Schema Authoring

### 3.1 Declaring Subscriptions

Subscriptions are declared using the same schema authoring languages as types and queries.

**Python Example:**

```python
@fraiseql.subscription
class OrderCreated:
    """Events for new orders created by the authenticated user."""

    # Compile-time filter: Only current user's orders
    where: WhereOrder = fraiseql.where(user_id=fraiseql.context.user_id)

    # Fields to project from the Order entity
    id: ID
    user_id: ID
    created_at: DateTime
    amount: Decimal

    @fraiseql.variable(name="since_date")
    class Filter:
        """Optional runtime filter for timestamp range."""
        created_at: DateTimeRange


@fraiseql.subscription
class UserDeleted:
    """Events for users deleted (admin only)."""

    # Authorization: Admin context required
    where: WhereUser = fraiseql.where(
        fraiseql.context.role.contains("admin")
    )

    # Soft delete: Only fire if deleted_at is set
    id: ID
    email: Email
    deleted_at: DateTime


@fraiseql.subscription
class OrderStatusChanged:
    """Events for status changes on organization's orders."""

    # Multi-tenant filtering
    where: WhereOrder = fraiseql.where(
        fk_org=fraiseql.context.org_id
    )

    # Nested projection (Order → OrderStatus entity)
    id: ID
    status: OrderStatus
    updated_at: DateTime
    updated_by_user: User
```

### 3.2 Compile-Time Validation

When the schema is compiled, the compiler:

1. **Identifies all subscription types**
   - Validates `@fraiseql.subscription` decorators
   - Ensures each subscription is based on a valid entity type

2. **Validates WHERE filters**
   - `user_id=context.user_id` → Must exist in Order type
   - `role.contains("admin")` → Must be valid operator for role field
   - Context variables must be available (authentication required)

3. **Validates field projections**
   - All fields requested must exist in entity type
   - Nested fields checked recursively (Order → User)
   - Soft-delete logic applied automatically (WHERE deleted_at IS NULL)

4. **Generates subscription dispatch table**

   ```json
   {
     "subscriptions": {
       "OrderCreated": {
         "entity_type": "Order",
         "filter_sql": "WHERE user_id = $1 AND deleted_at IS NULL",
         "filter_params": ["user_id"],
         "fields": ["id", "user_id", "created_at", "amount"],
         "auth_required": true,
         "operation": "CREATE"
       }
     }
   }
   ```

### 3.3 Multiple Key Subscriptions

Subscriptions can filter on multiple fields:

```python
@fraiseql.subscription
class OrderUpdated:
    """Subscription for specific order updates."""

    # Both compile-time constraints
    where: WhereOrder = fraiseql.where(
        fk_org=fraiseql.context.org_id,
        status=fraiseql.context.allowed_statuses  # Must be in auth context
    )

    id: ID
    status: str
    updated_at: DateTime
```

---

## 4. Transport Protocols

### 4.1 GraphQL WebSocket (graphql-ws)

The primary transport for real-time UI updates using the standard `graphql-ws` protocol.

#### Connection Lifecycle

```
Client                              Server
  │                                   │
  ├──────── Connection Request ──────→│
  │                                   │
  │◄─── Connection Acknowledgement ──│
  │                                   │
  ├──────── Subscribe Message ────────→│
  │ {                                 │
  │   "type": "subscribe",            │
  │   "id": "1",                      │
  │   "payload": {                    │
  │     "operationName": null,        │
  │     "query": "subscription {...}" │
  │     "variables": {                │
  │       "since_date": "2026-01-01"  │
  │     }                             │
  │   }                               │
  │ }                                 │
  │                                   │
  │◄── Subscribe Acknowledgement ────│
  │                                   │
  │◄─ Next (Event Payload) ──────────│
  │ {                                 │
  │   "type": "next",                 │
  │   "id": "1",                      │
  │   "payload": {                    │
  │     "data": {                     │
  │       "orderCreated": {           │
  │         "id": "ord_123",          │
  │         "amount": 99.99           │
  │       }                           │
  │     }                             │
  │   }                               │
  │ }                                 │
  │                                   │
  │◄─ Next (Event Payload) ──────────│
  │ ...                               │
  │                                   │
  ├──────── Unsubscribe Message ─────→│
  │ {                                 │
  │   "type": "complete",             │
  │   "id": "1"                       │
  │ }                                 │
  │                                   │
  │◄───── Complete Acknowledgement ──│
  │                                   │
  ├───────── Close Connection ───────→│
  │                                   │
```

#### Example: Browser Client

```javascript
// React component using graphql-ws
import { useSubscription } from '@apollo/client';

const OrdersSubscription = gql`
  subscription OrderCreated($since_date: DateTime) {
    orderCreated(since_date: $since_date) {
      id
      amount
      created_at
    }
  }
`;

export function LiveOrders() {
  const { data, loading, error } = useSubscription(
    OrdersSubscription,
    {
      variables: {
        since_date: new Date('2026-01-01').toISOString()
      }
    }
  );

  if (loading) return <p>Listening for orders...</p>;
  if (error) return <p>Subscription error: {error.message}</p>;

  return (
    <div>
      {data?.orderCreated && (
        <Order order={data.orderCreated} />
      )}
    </div>
  );
}
```

#### Error Handling

```json
{
  "type": "error",
  "id": "1",
  "payload": [
    {
      "message": "Subscription not found",
      "extensions": {
        "code": "SUBSCRIPTION_NOT_FOUND"
      }
    }
  ]
}
```

**Common errors:**

- `AUTHENTICATION_REQUIRED` — User not authenticated
- `FORBIDDEN` — User lacks authorization for subscription
- `SUBSCRIPTION_NOT_FOUND` — Subscription type not defined in schema
- `INVALID_VARIABLES` — Runtime variable types incorrect

### 4.2 HTTP Webhooks

For push-based delivery to external systems.

#### Webhook Event

```json
POST https://external-service.example.com/webhooks/fraiseql

{
  "event_id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "event_name": "OrderCreated",
  "entity_name": "Order",
  "entity_id": "ord_123",
  "operation": "CREATE",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "data": {
    "id": "ord_123",
    "user_id": "usr_456",
    "amount": 99.99,
    "created_at": "2026-01-11T15:35:00.123456Z"
  },
  "signature": "sha256=<hmac_signature>"
}
```

#### Webhook Configuration

```python
config = FraiseQLConfig(
    webhooks={
        "OrderCreated": {
            "url": "https://analytics.example.com/events",
            "auth": {"token": "secret_key"},
            "retry_max_attempts": 3,
            "retry_backoff_seconds": [1, 5, 30]
        }
    }
)
```

#### Delivery Semantics

- **At-least-once:** Event may be delivered multiple times
- **Ordered per entity:** Events for same entity arrive in order
- **Retried on failure:** 3 retries with exponential backoff
- **Signature verification:** HMAC-SHA256 for security

### 4.3 Kafka / Event Streaming

For high-throughput consumption by backend systems.

#### Kafka Topic

Topic name: `fraiseql.{entity_type}.{operation}`

Examples:

- `fraiseql.order.created`
- `fraiseql.user.updated`
- `fraiseql.order.deleted`

#### Kafka Message

```json
Key: "ord_123" (entity_id)

Value:
{
  "event_id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "event_name": "OrderCreated",
  "entity_name": "Order",
  "entity_id": "ord_123",
  "operation": "CREATE",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "data": {...}
}
```

#### Kafka Configuration

```python
config = FraiseQLConfig(
    kafka={
        "enabled": True,
        "bootstrap_servers": ["kafka:9092"],
        "subscriptions": {
            "OrderCreated": {
                "topic": "fraiseql.order.created",
                "partition_by": "entity_id"  # Orders with same ID → same partition
            }
        }
    }
)
```

#### Delivery Semantics

- **At-least-once:** Messages may duplicate (use idempotent processing)
- **Ordered per partition:** Events for same entity arrive in order
- **Offset management:** Consumer tracks processed events
- **Replay capable:** Seek to any offset to replay events

### 4.4 gRPC (Future)

For low-latency service-to-service streaming.

```protobuf
service FraiseQLSubscriptions {
  rpc StreamEvents (StreamRequest) returns (stream Event);
}

message StreamRequest {
  string subscription_name = 1;
  google.protobuf.Struct variables = 2;
  string auth_token = 3;
}

message Event {
  string event_id = 1;
  string entity_name = 2;
  string entity_id = 3;
  string operation = 4;
  google.protobuf.Timestamp timestamp = 5;
  google.protobuf.Struct data = 6;
}
```

---

## 5. Filtering & Variables

### 5.1 Compile-Time WHERE Clauses

Subscriptions filter events using WHERE clauses evaluated at compile time and rendered as SQL predicates.

```python
@fraiseql.subscription
class OrderCreated:
    # Filter: Only orders for authenticated user
    where: WhereOrder = fraiseql.where(
        user_id=fraiseql.context.user_id
    )

# Compiled to:
# WHERE user_id = $1 (with $1 bound to context.user_id at runtime)
```

**Available context variables:**

```python
fraiseql.context.user_id         # Authenticated user ID
fraiseql.context.org_id          # Organization/tenant ID
fraiseql.context.role            # User role (string or list)
fraiseql.context.permissions     # User permissions
fraiseql.context.custom_claim    # Custom auth claim
```

**Example: Multi-tenant filtering**

```python
@fraiseql.subscription
class OrderUpdated:
    where: WhereOrder = fraiseql.where(
        fk_org=fraiseql.context.org_id,
        # Only notify on changes to orders in allowed statuses
        status=fraiseql.context.allowed_statuses
    )

    id: ID
    status: OrderStatus
    updated_at: DateTime
```

### 5.2 Runtime Variables

Subscriptions accept typed runtime variables for additional filtering.

```python
@fraiseql.subscription
class OrderCreated:
    where: WhereOrder = fraiseql.where(
        user_id=fraiseql.context.user_id
    )

    @fraiseql.variable(name="since_date", type=DateTime)
    @fraiseql.variable(name="min_amount", type=Decimal)
    class Filter:
        """Optional runtime filtering on timestamp and amount."""
        created_at: DateTimeRange
        amount: DecimalRange

    id: ID
    amount: Decimal
    created_at: DateTime

# Client-side usage
subscription OrderCreated(
  $since_date: DateTime
  $min_amount: Decimal
) {
  orderCreated(since_date: $since_date, min_amount: $min_amount) {
    id
    amount
    created_at
  }
}
```

**At runtime:**

1. Client provides variables: `{ "since_date": "2026-01-01", "min_amount": 50.00 }`
2. Compiler validates variable types match WHERE operator expectations
3. SQL predicate: `WHERE user_id = $1 AND created_at > $2 AND amount >= $3`
4. Only matching events delivered to client

### 5.3 Authorization Enforcement

Subscriptions enforce authorization rules at compile time with runtime-safe parameter binding:

**How it works:**

- Authorization rules are **defined and validated** at schema compile time (schema defines who can access what)
- Authorization values are **bound safely** at runtime (context.user_id, context.role, etc. resolved when subscription is established)
- No dynamic authorization logic—filters are deterministic SQL predicates evaluated by database

**Example:**

```python
@fraiseql.subscription
class SensitiveDataAccessed:
    # Only admins receive this subscription
    where: WhereAuditLog = fraiseql.where(
        fraiseql.context.role == "admin"
    )

    # If context.role != "admin", subscription unavailable
    # Compile-time error or runtime 403 FORBIDDEN
```

**Row-level authorization example:**

```python
@fraiseql.subscription
class UserProfileUpdated:
    # User only sees updates to their own profile
    where: WhereUser = fraiseql.where(
        id=fraiseql.context.user_id
    )

    id: ID
    email: Email
    name: str
    updated_at: DateTime

# If User ID = 123 subscribes, only receives updates where id = 123
# No cross-user data leakage possible (enforced at compile time)
```

---

## 6. Event Format & Transformation

### 6.1 Relationship to CDC Format

Subscription events are derived from CDC events in `tb_entity_change_log`.

**CDC Event (raw, in database):**

```json
{
  "event_id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "event_type": "entity:created",
  "entity_type": "Order",
  "entity_id": "ord_123",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "operation": {
    "before": null,
    "after": {
      "id": "ord_123",
      "user_id": "usr_456",
      "status": "pending",
      "amount": 99.99,
      "created_at": "2026-01-11T15:35:00.123456Z",
      "updated_at": "2026-01-11T15:35:00.123456Z"
    }
  }
}
```

**Subscription Event (projected, sent to client):**

```json
{
  "event_id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "event_name": "OrderCreated",
  "entity_name": "Order",
  "entity_id": "ord_123",
  "operation": "CREATE",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "data": {
    "id": "ord_123",
    "amount": 99.99,
    "created_at": "2026-01-11T15:35:00.123456Z"
  }
}
```

**Transformation logic:**

1. **Extract fields requested:** Only `id`, `amount`, `created_at` included (as per subscription definition)
2. **Apply WHERE filter:** Event matches `user_id = $1` (context user)
3. **Format for transport:** Remove internal CDC fields, structure for GraphQL/webhook response
4. **Add event metadata:** `event_id`, `event_name`, `operation`, `sequence_number`

### 6.2 Field Projection

Subscription selection sets determine which fields are included in the event.

```python
@fraiseql.subscription
class UserUpdated:
    # All user fields available for projection
    id: ID
    email: Email
    name: str
    phone: str
    role: str
    created_at: DateTime
    updated_at: DateTime

# Client requests only specific fields
subscription UserUpdated {
  userUpdated {
    id
    email
    name
    # phone, role, created_at, updated_at NOT requested, not included
  }
}

# Event delivered with only requested fields:
{
  "data": {
    "id": "usr_456",
    "email": "alice@example.com",
    "name": "Alice Smith"
  }
}
```

### 6.3 Nested Projections

Subscriptions can project nested entities.

```python
@fraiseql.subscription
class OrderCreated:
    id: ID
    amount: Decimal
    user: User  # Nested: include User entity
    created_at: DateTime

# Client requests nested fields
subscription OrderCreated {
  orderCreated {
    id
    amount
    user {
      id
      email
      name
    }
    created_at
  }
}

# Event includes nested data:
{
  "data": {
    "id": "ord_123",
    "amount": 99.99,
    "user": {
      "id": "usr_456",
      "email": "alice@example.com",
      "name": "Alice Smith"
    },
    "created_at": "2026-01-11T15:35:00.123456Z"
  }
}
```

---

## 7. Multi-Database Support

**PostgreSQL is the reference implementation for subscriptions.** Other databases follow the same architectural contract but may vary in maturity, feature completeness, and performance characteristics.

### 7.1 PostgreSQL (Phase 1 — Reference Implementation)

**Event capture mechanism:** LISTEN / NOTIFY + Logical Decoding

```sql
-- Enable logical decoding
CREATE PUBLICATION fraiseql_events FOR ALL TABLES;

-- Listen for changes
LISTEN fraiseql_changes;

-- Example notification from trigger
CREATE FUNCTION notify_change() RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('fraiseql_changes',
        json_build_object(
            'entity_type', TG_TABLE_NAME,
            'entity_id', NEW.id,
            'operation', TG_OP
        )::text
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER order_change_trigger AFTER INSERT OR UPDATE OR DELETE ON tb_order
FOR EACH ROW EXECUTE FUNCTION notify_change();
```

**Advantages (Reference Implementation):**

- Sub-millisecond latency (in-process notification)
- No additional infrastructure (built-in to PostgreSQL)
- Logical decoding for durability and replay
- Production-tested and battle-hardened
- Full feature parity with subscription architecture

**Limitations:**

- Notifications lost if server restarts (use CDC for durability)
- CDC requires enterprise features or wal2json plugin
- Single database only (no cross-database subscriptions)

### 7.2 MySQL (Phase 2)

**Event capture mechanism:** Binary log + Debezium or maxwell

```sql
-- Enable binary logging
SET GLOBAL binlog_format = 'ROW';

-- Debezium connector reads binary log
CREATE TABLE fraiseql_outbox (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    entity_type VARCHAR(255),
    entity_id VARCHAR(255),
    operation VARCHAR(10),
    data JSON,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Trigger captures changes
CREATE TRIGGER order_outbox AFTER INSERT ON tb_order
FOR EACH ROW
INSERT INTO fraiseql_outbox (entity_type, entity_id, operation, data)
VALUES ('Order', NEW.id, 'INSERT', JSON_OBJECT(...));
```

**Advantages:**

- Scalable to large datasets
- Debezium ecosystem mature and well-supported
- Works with managed MySQL services (AWS RDS, Cloud SQL)

**Limitations:**

- Additional infrastructure (Debezium/maxwell)
- Higher latency (seconds vs milliseconds)
- Outbox pattern required (separate table)

### 7.3 SQL Server (Phase 2)

**Event capture mechanism:** Change Data Capture (built-in)

```sql
-- Enable CDC on database
EXEC sys.sp_cdc_enable_db;

-- Enable CDC on specific table
EXEC sys.sp_cdc_enable_table
    @source_schema = 'dbo',
    @source_name = 'Order',
    @role_name = NULL;

-- Query CDC tables
SELECT * FROM cdc.dbo_Order_CT
WHERE __$start_lsn >= @previous_lsn
ORDER BY __$seqval;
```

**Advantages:**

- Built-in to SQL Server (no plugins)
- Mature, enterprise-grade CDC
- Integrated with query execution

**Limitations:**

- Enterprise/Standard editions only (not Express)
- Overhead on transactional workload
- Slightly higher latency than LISTEN/NOTIFY

### 7.4 SQLite (Phase 2)

**Event capture mechanism:** Triggers on temporary in-memory event log

```sql
-- Triggers capture changes to temp table
CREATE TEMP TABLE fraiseql_events (
    id INTEGER PRIMARY KEY,
    entity_type TEXT,
    entity_id TEXT,
    operation TEXT,
    data JSON,
    created_at TIMESTAMP
);

CREATE TRIGGER order_insert AFTER INSERT ON tb_order
FOR EACH ROW
INSERT INTO fraiseql_events (entity_type, entity_id, operation, data)
VALUES ('Order', NEW.id, 'INSERT', json(...));

-- Subscribers poll events from temp table
-- (No push capability; pull-based)
```

**Advantages:**

- No additional infrastructure
- Suitable for development/testing

**Limitations:**

- Pull-based only (clients must poll)
- In-memory (events lost on disconnect)
- Single process only (no network clients)
- Not suitable for production subscriptions

### 7.5 Database Abstraction

FraiseQL abstracts database-specific CDC via a plugin interface:

```rust
pub trait CDCBackend {
    async fn listen(&self, entity_types: Vec<String>) -> Result<EventStream>;
    async fn query_events(&self, after_seq: u64) -> Result<Vec<CDCEvent>>;
    async fn get_current_position(&self) -> Result<u64>;
}

// Implementations per database
impl CDCBackend for PostgresCDC { ... }
impl CDCBackend for MySQLCDC { ... }
impl CDCBackend for SQLServerCDC { ... }
impl CDCBackend for SQLiteCDC { ... }
```

---

## 8. System Architecture

### 8.1 Compilation

Subscriptions are **compiled at schema build time** into the CompiledSchema:

**Compile-time processing:**

- Parse subscription declarations (all authoring languages)
- Validate subscription fields against event entity types
- Compile WHERE filters to SQL predicates
- Bind authorization rules for event matching
- Generate subscription dispatch tables
- Type-check against type system (same rules as queries/mutations)

**Output:** Subscription metadata in CompiledSchema includes:

- Event entity type(s) and operation types (INSERT/UPDATE/DELETE)
- WHERE filter predicates
- Field projection list
- Authorization requirements
- Transport adapter configurations

### 8.2 Event Capture & Dispatch

At runtime, subscriptions follow a unified event pipeline:

**PostgreSQL (primary mechanism):**

- LISTEN/NOTIFY: Fast, in-process notification of database changes
- Triggers: Capture changes to `tb_entity_change_log`

**Other databases (same event structure):**

- MySQL: CDC via Debezium or Maxwell
- SQL Server: Native Change Data Capture (built-in)
- SQLite: Trigger-based capture to temporary table

**Event buffer:** All events written to `tb_entity_change_log` with:

- Monotonic sequence numbers (for replay and ordering)
- Debezium-compatible envelope format
- Full before/after data
- Transaction context

### 8.3 Transport Adapters

Same event stream dispatched to multiple transports simultaneously:

**graphql-ws (WebSocket)** — Real-time UI clients

- Standard graphql-ws protocol
- Sub-millisecond latency (reference deployment)
- Connection pooling and state management

**Webhooks** — External system integration

- Outgoing HTTP POST with signed payloads
- Retry logic with exponential backoff
- Delivery status tracking

**Kafka** — Event streaming to data warehouses

- Producer integration with topic mapping
- Offset tracking for consumer resume
- Batching and buffer management

**gRPC** — Inter-service events (future)

- Server streaming for scalable event delivery
- Language-agnostic protocol
- Connection multiplexing

### 8.4 Authorization & Filtering

Authorization enforced at **event capture time** (not delivery time):

**Compile-time rules:**

- WHERE clauses applied to event stream
- Row-level security policies enforced
- Field-level masking/redaction applied
- Multi-tenant isolation guaranteed

**Runtime binding:**

- Auth context variables resolved per subscriber
- User-specific filtering applied
- Compliance audits generated

---

## 9. Performance Characteristics

### 9.1 Event Latency

**Database to subscription delivery:**

| Path | Latency (Observed) | Notes |
|------|----------|-------|
| PostgreSQL LISTEN/NOTIFY | <1ms | In-process notification (reference deployment) |
| graphql-ws client (local) | ~5-10ms | Network round-trip included (target envelope) |
| graphql-ws client (remote) | 50-100ms | Network latency dominant (typical WAN) |
| Webhook delivery | 50-200ms | HTTP request + retry logic (depends on endpoint) |
| Kafka producer | <5ms | Async write to broker (target) |

**Example: User creates order in UI, sees confirmation**

```
1. Mutation committed (1ms)
2. Trigger fires, sends notification (0.5ms)
3. Listener receives (0.5ms)
4. Filter evaluates (0.5ms)
5. Transform to GraphQL (0.5ms)
6. Send to WebSocket (1ms)
7. Client receives (5-10ms network)
────────────────
Total: ~10ms (sub-second perceived latency)
```

### 9.2 Throughput

**Concurrent subscriptions:**

- Single process: 1,000-10,000 concurrent WebSocket connections (depends on memory)
- Horizontal scaling: Multiple FraiseQL instances behind load balancer
- Event buffering: `tb_entity_change_log` handles burst traffic

**Event throughput (observed in reference deployments):**

- PostgreSQL LISTEN/NOTIFY: 10,000+ events/second (target)
- Webhook delivery: Limited by HTTP endpoint capacity (external factor)
- Kafka: 100,000+ events/second (broker-dependent; target with typical configurations)

### 9.3 Memory Usage

**Per subscription:**

- graphql-ws connection: ~10-50 KB (WebSocket buffer)
- Event filter state: Negligible (compiled SQL predicates)
- Buffered events: 1-10 MB (configurable retention)

**Example:** 1,000 concurrent subscriptions (reference deployment)

- Connection buffers: ~50 MB
- Event buffer: ~100 MB
- **Total observed: ~150 MB** (typical, single process)

### 9.4 Resource Utilization

**CPU:**

- Event filtering: <1% per 1,000 events/second (database-side)
- GraphQL transformation: <1% per 1,000 events/second (deterministic)
- Negligible overhead for idle subscriptions

**Network:**

- graphql-ws keeps connection open (minimal bandwidth if idle)
- Webhook bursts: Limited by retry backoff
- Kafka: Configurable batch size

---

## 10. Limitations & Trade-Offs

### 10.1 Supported Semantics

**✅ Subscriptions CAN:**

- Project database entities (same fields as queries)
- Filter by compile-time WHERE clauses
- Filter by runtime variables
- Support nested projections
- Enforce row-level authorization (compile-time)
- Replay events from `tb_entity_change_log`
- Deliver to multiple transport adapters simultaneously
- Order events per entity (not globally)

### 10.2 Explicitly NOT Supported

**❌ Subscriptions CANNOT:**

- Execute arbitrary user code
- Modify subscription filter at runtime (must be declared at compile time)
- Subscribe across multiple entities in single query

  ```graphql
  # NOT ALLOWED: Subscribes to Order changes, but also User changes
  subscription {
    orderCreated { id }
    userUpdated { id }
  }
  ```

- Guarantee global event ordering (only per-entity ordering)
- Transform events via resolvers
- Access fields not declared in subscription schema

**Why these limitations exist:**

- Subscriptions are **projections**, not programs
- Filters must be compile-time deterministic
- No user code execution (aligned with FraiseQL philosophy)

### 10.3 Delivery Guarantees

**Guaranteed:**

- At-least-once delivery (events not lost)
- Per-entity ordering (events for same entity in order)
- Event idempotence (can process same event twice safely)

**NOT guaranteed:**

- Exactly-once delivery (transport-dependent)
- Global event ordering (use event sequence_number for ordering)
- Delivery to all transports simultaneously (Kafka may lag WebSocket)

### 10.4 Database Limitations

**PostgreSQL:**

- LISTEN/NOTIFY lost on server restart (use CDC/WAL for durability)
- No cross-database subscriptions
- Logical decoding requires wal_level=logical

**MySQL:**

- Requires Debezium/maxwell (additional infrastructure)
- Higher latency than PostgreSQL (seconds vs milliseconds)
- Binary log must be enabled

**SQL Server:**

- CDC available only in Standard+ editions
- Overhead on transactional load
- Enterprise licensing

**SQLite:**

- Pull-based only (no push)
- In-memory (no durability)
- Single process (no network clients)

---

## 11. Security & Authorization

### 11.1 Authentication

Subscriptions require authentication same as mutations:

```python
# Only authenticated users can subscribe
@fraiseql.subscription
class OrderCreated:
    where: WhereOrder = fraiseql.where(
        user_id=fraiseql.context.user_id
    )
    # Fails if context.user_id is None (unauthenticated)
```

### 11.2 Row-Level Authorization

WHERE clauses enforce row-level access control through compile-time rule definition and runtime-safe parameter binding:

**Mechanism:**

- Authorization rules **defined** at compile time in schema (WHERE clause states who can access what)
- Authorization values **bound** at runtime (context.user_id, context.org_id resolved from AuthContext when subscription established)
- Filters are deterministic SQL predicates—no dynamic logic

**Examples:**

```python
# User only sees their own orders
where: WhereOrder = fraiseql.where(user_id=fraiseql.context.user_id)

# Org admin sees org's orders
where: WhereOrder = fraiseql.where(fk_org=fraiseql.context.org_id)

# Admin sees everything (no WHERE filter)
where: WhereOrder = fraiseql.where()  # No filter = all rows
```

### 11.3 Field-Level Authorization

Projected fields can have authorization rules:

```python
@fraiseql.subscription
class OrderCreated:
    id: ID  # Always visible
    amount: Decimal  # Always visible

    # sensitive_notes only visible to admin
    sensitive_notes: Optional[str] = fraiseql.field(
        auth_required=["admin"]
    )

# If context.role != "admin", sensitive_notes omitted from events
```

### 11.4 Signature Verification (Webhooks)

Webhooks include HMAC-SHA256 signature for verification:

```javascript
// Webhook handler
const signature = req.headers['x-fraiseql-signature'];
const payload = req.rawBody;

const expected = crypto
  .createHmac('sha256', WEBHOOK_SECRET)
  .update(payload)
  .digest('hex');

if (signature !== expected) {
  return res.status(401).send('Signature mismatch');
}
```

---

## 12. Examples

### Example 1: Real-Time Order Dashboard

**Schema definition:**

```python
@fraiseql.subscription
class OrderCreated:
    """Stream new orders for the organization."""

    where: WhereOrder = fraiseql.where(
        fk_org=fraiseql.context.org_id
    )

    id: ID
    user_id: ID
    amount: Decimal
    created_at: DateTime
    user: User  # Nested projection


@fraiseql.subscription
class OrderStatusChanged:
    """Stream status updates for organization's orders."""

    where: WhereOrder = fraiseql.where(
        fk_org=fraiseql.context.org_id
    )

    id: ID
    old_status: OrderStatus
    new_status: OrderStatus
    updated_at: DateTime
```

**Client (React):**

```typescript
import { useSubscription } from '@apollo/client';

export function OrderDashboard() {
  const { data: newOrders } = useSubscription(gql`
    subscription {
      orderCreated {
        id
        user_id
        amount
        created_at
        user { name email }
      }
    }
  `);

  const { data: statusChanges } = useSubscription(gql`
    subscription {
      orderStatusChanged {
        id
        old_status
        new_status
        updated_at
      }
    }
  `);

  return (
    <div>
      <LiveOrderList orders={newOrders?.orderCreated || []} />
      <StatusUpdateFeed updates={statusChanges?.orderStatusChanged || []} />
    </div>
  );
}
```

### Example 2: Event Streaming to Analytics

**Schema definition:**

```python
@fraiseql.subscription
class UserRegistered:
    """Stream new user registrations (no filter, analytics event)."""

    where: WhereUser = fraiseql.where()  # All users

    id: ID
    email: Email
    created_at: DateTime
    source: str  # How they registered


@fraiseql.subscription
class PurchaseMade:
    """Stream purchases for analytics and revenue tracking."""

    where: WhereOrder = fraiseql.where(
        status="completed"  # Only completed orders
    )

    id: ID
    user_id: ID
    amount: Decimal
    items: list[OrderItem]
    created_at: DateTime
```

**Kafka configuration:**

```python
config = FraiseQLConfig(
    kafka={
        "enabled": True,
        "bootstrap_servers": ["kafka:9092"],
        "subscriptions": {
            "UserRegistered": {
                "topic": "analytics.users.registered",
                "partition_by": "id"
            },
            "PurchaseMade": {
                "topic": "analytics.orders.completed",
                "partition_by": "user_id"
            }
        }
    }
)
```

**Consumer (Python):**

```python
from kafka import KafkaConsumer
import json

consumer = KafkaConsumer(
    'analytics.orders.completed',
    bootstrap_servers=['kafka:9092'],
    value_deserializer=lambda m: json.loads(m.decode('utf-8'))
)

for message in consumer:
    event = message.value
    # Insert into data warehouse
    warehouse.insert_order(
        order_id=event['data']['id'],
        user_id=event['data']['user_id'],
        amount=event['data']['amount'],
        event_time=event['timestamp']
    )
```

### Example 3: Multi-Tenant Filtering with Variables

**Schema definition:**

```python
@fraiseql.subscription
class ActivityInOrganization:
    """Stream activity (creates, updates, deletes) in organization."""

    where: WhereAuditLog = fraiseql.where(
        fk_org=fraiseql.context.org_id
    )

    @fraiseql.variable(name="min_severity")
    class Filter:
        """Optional severity filter."""
        severity: AuditSeverity

    id: ID
    entity_type: str
    entity_id: str
    operation: str
    severity: AuditSeverity
    user: User
    created_at: DateTime
```

**Client with filtering:**

```graphql
subscription ActivityInOrganization($min_severity: AuditSeverity) {
  activityInOrganization(min_severity: $min_severity) {
    id
    entity_type
    operation
    severity
    user { name }
    created_at
  }
}
```

**Usage:**

```javascript
// Subscribe to high-priority events only
useSubscription(ActivityInOrganization, {
  variables: {
    min_severity: "HIGH"
  }
});
```

---

## 13. Appendix

### A. Debugging Subscriptions

**Check if subscription is defined:**

```bash
# Query introspection
query {
  __type(name: "Subscription") {
    fields {
      name
    }
  }
}
```

**Monitor event flow:**

```sql
-- Check event buffer
SELECT COUNT(*) as pending_events
FROM tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '1 minute';

-- Monitor subscription lag
SELECT entity_type, MAX(created_at) as last_event
FROM tb_entity_change_log
GROUP BY entity_type;
```

**Enable subscription tracing (Rust runtime):**

```rust
if config.debug {
    trace!("Subscription: OrderCreated");
    trace!("  Filter: WHERE user_id = {} AND deleted_at IS NULL", user_id);
    trace!("  Event matched: {}", event_matches_filter);
    trace!("  Delivered to: {} clients", client_count);
}
```

### B. Monitoring Metrics

**Key metrics to track:**

```
fraiseql.subscription.connections     # Current active connections
fraiseql.subscription.events_emitted   # Events matching filters
fraiseql.subscription.events_delivered # Events sent to clients
fraiseql.subscription.lag_seconds      # Delay from database to client
fraiseql.subscription.error_count      # Delivery failures
```

### C. Connection Pool Sizing

**Recommendation:**

- Pool size = (expected_concurrent_subscriptions / 10) + overhead
- Default: 20 connections
- Monitor connection count and adjust

**Example:**

```python
config = FraiseQLConfig(
    database_url="postgresql://...",
    subscriptions={
        "connection_pool_size": 50,  # For 500+ concurrent subscriptions
        "connection_timeout": 300,
        "idle_timeout": 60
    }
)
```

### D. References

**Related specifications:**

- `docs/specs/cdc-format.md` — CDC event structure and format
- `docs/specs/schema-conventions.md section 6` — `tb_entity_change_log` table definition
- `docs/architecture/core/execution-model.md section 9.3` — CDC integration
- Apollo Federation v2: `docs/architecture/integration/federation.md` — Cross-subgraph subscriptions (future)

**External standards:**

- graphql-ws protocol: <https://github.com/enisdenjo/graphql-ws/blob/master/PROTOCOL.md>
- Debezium format: <https://debezium.io/>
- Kafka consumers: <https://kafka.apache.org/documentation/#consumerconfigs>

---

## Summary

**Subscriptions in FraiseQL are event projections from the database, not GraphQL resolver-based subscriptions.**

**Key properties:**

- ✅ Database-native (LISTEN/NOTIFY, CDC)
- ✅ Compiled, not interpreted
- ✅ Transport-agnostic (graphql-ws, webhooks, Kafka, etc.)
- ✅ Deterministic, no user code
- ✅ Durable (buffered in `tb_entity_change_log`)

**Architecture:**

1. Database transaction commits
2. Change captured via LISTEN/NOTIFY or CDC
3. Event buffered in `tb_entity_change_log`
4. Filters evaluated against compiled predicates
5. Delivered via transport adapter (graphql-ws, webhook, Kafka)

**For real-time UI:** graphql-ws targets <10ms latency (local network, reference deployment)
**For event streaming:** Kafka provides durability and replay
**For external systems:** Webhooks with retry logic

**Security:**

- Row-level filtering enforced at compile time
- No cross-tenant data leakage
- Authorization via AuthContext

**Limitations:**

- Subscriptions are read-only (no mutations)
- Filters compile-time determined
- Per-entity ordering only

*End of Subscriptions Specification*
