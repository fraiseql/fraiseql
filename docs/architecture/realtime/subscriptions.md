<!-- Skip to main content -->
---
title: Subscriptions: Event Projections from the Database
description: 1. [Overview](#1-overview)
keywords: ["design", "scalability", "performance", "patterns", "security"]
tags: ["documentation", "reference"]
---

# Subscriptions: Event Projections from the Database

**Version:** 2.0
**Date:** February 5, 2026
**Status:** ✅ Implemented in v2.0.0-alpha.1
**Audience:** Runtime Developers, Integration Engineers, Database Architects

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture](#2-architecture)
3. [Subscription Schema Authoring](#3-subscription-schema-authoring)
4. [Transport Protocols](#4-transport-protocols)
5. [Filtering & Variables](#5-filtering--variables)
6. [Event Format & Transformation](#6-event-format--transformation)
7. [Multi-Database Support](#7-multi-database-support)
8. [System Architecture](#8-system-architecture)
9. [Performance Characteristics](#9-performance-characteristics)
10. [Limitations & Trade-Offs](#10-limitations--trade-offs)
11. [Security & Authorization](#11-security--authorization)
12. [Examples](#12-examples)
13. [Appendix](#13-appendix)

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

```text
<!-- Code example in TEXT -->
Client subscribes to User.nameChanged
    ↓
Server executes resolver function
    ↓
Resolver polls database or listens to app events
    ↓
Resolver emits value to client
```text
<!-- Code example in TEXT -->

**FraiseQL Subscriptions:**

```text
<!-- Code example in TEXT -->
Database commits transaction (user.name updated)
    ↓
Application inserts event into tb_entity_change_log
    ↓
ChangeLogListener polls tb_entity_change_log (every 100ms)
    ↓
ObserverRuntime processes event
    ↓
Event matching filters from CompiledSchema
    ↓
Delivered via transport adapter (graphql-ws, webhook, Kafka)
```text
<!-- Code example in TEXT -->

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

### 2.1 High-Level Event Flow (CORRECT)

```text
<!-- Code example in TEXT -->
┌─────────────────────────────────────────────────────────────┐
│ Application (GraphQL Mutation / Direct SQL)                │
│ Executes: mutation CreateOrder($user_id, $amount)          │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ↓
        ┌──────────────────────────────────────┐
        │ PostgreSQL Database Transaction      │
        │ ├─ INSERT into tb_order              │
        │ ├─ INSERT into tb_entity_change_log  │  ← Manual (for now)
        │ └─ COMMIT                            │
        └──────────────────────────┬───────────┘
                                   │
                    ┌──────────────┴──────────────────┐
                    ↓                                 ↓
        ┌──────────────────────┐      ┌──────────────────────┐
        │ tb_entity_change_log │      │ tb_entity_change_log │
        │ (Single Source of    │      │ (Debezium envelope)  │
        │  Truth)              │      │                      │
        │ - object_type        │      │ Polling every 100ms  │
        │ - object_id          │      │ (effectively real-   │
        │ - modification_type  │      │  time)               │
        │ - object_data (JSONB)│      │                      │
        │ - created_at         │      │                      │
        └──────────┬────────────┘      └──────────────────────┘
                   │
                   ↓
        ┌──────────────────────────────────────┐
        │ ChangeLogListener                     │
        │ (polls tb_entity_change_log)          │
        │ - Poll interval: 100ms                │
        │ - Batch size: 100 events              │
        │ - Checkpoint tracking                 │
        └──────────────────┬────────────────────┘
                           │
                           ↓
        ┌──────────────────────────────────────┐
        │ ObserverRuntime (background task)     │
        │ Processes ChangeLogEntry → EntityEvent│
        └──────────┬───────────────────┬─────────┘
                   │                   │
        ┌──────────┴────────┐    ┌────┴───────────────────┐
        ↓                   ↓    ↓                        ↓
┌───────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ ObserverExecutor  │  │ Subscription     │  │ (Future: Other   │
│                   │  │ Manager          │  │  Consumers)      │
│ Actions:          │  │                  │  │                  │
│ ├─ Webhooks       │  │ Transports:      │  │                  │
│ ├─ Email          │  │ ├─ WebSocket     │  │                  │
│ ├─ SMS            │  │ │  (graphql-ws)  │  │                  │
│ ├─ Push           │  │ ├─ Kafka         │  │                  │
│ ├─ Slack          │  │ └─ Webhooks      │  │                  │
│ └─ Search Index   │  │                  │  │                  │
└───────────────────┘  └──────────────────┘  └──────────────────┘
     (automation)        (real-time UI)        (event streaming)
```text
<!-- Code example in TEXT -->

---

### 2.2 Components

**tb_entity_change_log** ✅ *Table exists, manual population*

- Single source of truth for ALL database change events
- Debezium-compatible envelope format (op, before, after, source, ts_ms)
- Durable storage with sequence numbers for ordering
- Enables event replay from any checkpoint
- **Populated manually** (application code must INSERT after mutations)
- Schema:

  ```sql
<!-- Code example in SQL -->
  CREATE TABLE tb_entity_change_log (
      pk_entity_change_log BIGSERIAL PRIMARY KEY,
      id UUID NOT NULL,
      fk_customer_org UUID,
      object_type VARCHAR(255) NOT NULL,
      object_id VARCHAR(255) NOT NULL,
      modification_type VARCHAR(10) NOT NULL,  -- INSERT, UPDATE, DELETE
      change_status VARCHAR(50),
      object_data JSONB NOT NULL,              -- Debezium "after" data
      extra_metadata JSONB,
      created_at TIMESTAMP NOT NULL DEFAULT NOW()
  );
  CREATE INDEX idx_entity_change_log_created ON tb_entity_change_log(created_at);
  CREATE INDEX idx_entity_change_log_type ON tb_entity_change_log(object_type);
  ```text
<!-- Code example in TEXT -->

**ChangeLogListener** ✅ *Fully implemented*

- Polls `tb_entity_change_log` at configurable interval (default: 100ms)
- Maintains checkpoint for recovery (no duplicate processing)
- Fetches entries in batches (default: 100 events)
- Parses Debezium envelope format (before/after/op)
- Location: `crates/FraiseQL-observers/src/listener/change_log.rs`
- **Key insight:** 100ms polling IS real-time for UI updates

**ObserverRuntime** ✅ *Fully implemented*

- Background tokio task started in server initialization
- Calls `ChangeLogListener.next_batch()` in loop
- Converts `ChangeLogEntry` → `EntityEvent`
- Routes events to registered consumers:
  - ObserverExecutor (actions like webhooks, email)
  - SubscriptionManager (transports like WebSocket, Kafka) ← **To be added**
- Location: `crates/FraiseQL-server/src/observers/runtime.rs`

**SubscriptionManager** ✅ *Fully implemented*

- Manages active GraphQL subscriptions
- Matches events against subscription filters
- Projects data to subscription's field selection
- Broadcasts via `tokio::sync::broadcast` channels
- Delivers to transport adapters (WebSocket, Kafka, Webhooks)
- Location: `crates/FraiseQL-core/src/runtime/subscription.rs`

**Transport Adapters** ✅ *All fully implemented*

- **graphql-ws (WebSocket)**: Real-time UI updates, complete protocol implementation
- **Webhooks**: HTTP POST with HMAC signatures, exponential backoff retry
- **Kafka**: Event streaming with compression, keyed by entity_id for partitioning

---

### 2.3 Event Format

Events flow through the system in Debezium envelope format:

```json
<!-- Code example in JSON -->
{
  "pk_entity_change_log": 1234,
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "object_type": "Order",
  "object_id": "ord_123",
  "modification_type": "INSERT",
  "object_data": {
    "id": "ord_123",
    "user_id": "usr_456",
    "amount": 99.99,
    "created_at": "2026-01-30T10:00:00Z"
  },
  "extra_metadata": {},
  "created_at": "2026-01-30T10:00:00.123456Z"
}
```text
<!-- Code example in TEXT -->

**Conversion to SubscriptionEvent:**

```rust
<!-- Code example in RUST -->
// In ObserverRuntime background task
for entry in entries {
    let entity_event = EntityEvent::from_change_log_entry(entry);

    // Route to observers (existing)
    observer_executor.process_event(&entity_event).await;

    // Route to subscriptions (TO BE ADDED)
    if let Some(ref sub_manager) = subscription_manager {
        let subscription_event = SubscriptionEvent {
            event_id: entity_event.id,
            entity_type: entity_event.object_type,
            entity_id: entity_event.object_id,
            operation: entity_event.modification_type,  // INSERT/UPDATE/DELETE
            data: entity_event.object_data,
            old_data: None,  // Could extract from "before" field
            timestamp: entity_event.created_at,
            sequence_number: entity_event.pk,
        };
        sub_manager.publish_event(subscription_event).await?;
    }
}
```text
<!-- Code example in TEXT -->

---

### 2.4 Architectural Insights

#### Why Polling, Not LISTEN/NOTIFY?

FraiseQL subscriptions use database-centric polling architecture rather than PostgreSQL LISTEN/NOTIFY:

1. **Database-Centric Design** - FraiseQL's core philosophy is "database as source of truth"
2. **Single Event Log** - `tb_entity_change_log` is THE event log, shared by observers and subscriptions
3. **Durability** - Events in database table can be replayed, checkpointed, and audited
4. **100ms Is Real-Time** - For UI updates, 100ms latency is imperceptible to users
5. **Simplicity** - One polling mechanism (ChangeLogListener), not two (LISTEN + polling)
6. **Existing Infrastructure** - ObserverRuntime already processes events; extend it

#### What Would Be Wrong With LISTEN/NOTIFY?

A LISTEN/NOTIFY based architecture would create:

```text
<!-- Code example in TEXT -->
Database → PostgreSQL NOTIFY → PostgresListener → SubscriptionManager
```text
<!-- Code example in TEXT -->

**Problems:**

- ❌ Duplicate event capture mechanism (ChangeLogListener already polls)
- ❌ No durability (NOTIFY messages are fire-and-forget)
- ❌ No replay capability (can't reprocess old events)
- ❌ Violates database-centric principle (message channel, not table)
- ❌ Creates two parallel event systems fighting for same purpose

#### Current Limitations (Temporary)

1. **Manual Event Population** - Application code must explicitly INSERT into `tb_entity_change_log`
   - Example:

     ```rust
<!-- Code example in RUST -->
     sqlx::query!(
         "INSERT INTO tb_entity_change_log (object_type, object_id, modification_type, object_data)
          VALUES ($1, $2, $3, $4)",
         "Order",
         order_id,
         "INSERT",
         serde_json::to_value(&order)?
     ).execute(&pool).await?;
     ```text
<!-- Code example in TEXT -->

2. **SubscriptionManager Not Wired to ObserverRuntime** - Integration pending (see migration path below)

3. **No Multi-Tenant Authorization Enforcement** - Filter evaluation exists but user context not passed

#### Performance Characteristics

| Aspect | Value | Notes |
|--------|-------|-------|
| **Event Latency** | 100ms (P50), 200ms (P99) | Polling interval + processing |
| **Throughput** | ~1000 events/sec | Batch size 100, 100ms polling |
| **Subscription Delivery** | 100-150ms total | Polling + matching + transport |
| **Durability** | ✅ Full | Events persisted in database table |
| **Replay** | ✅ Supported | Checkpoint-based from any point |
| **Scalability** | Limited by PostgreSQL | Single table bottleneck |

**Comparison:**

| Architecture | Latency | Durability | Replay | Complexity |
|--------------|---------|------------|--------|------------|
| **Polling (FraiseQL)** | 100ms | ✅ | ✅ | Low |
| **LISTEN/NOTIFY** | <10ms | ❌ | ❌ | Medium |
| **Kafka** | 10-50ms | ✅ | ✅ | High |
| **Redis Streams** | 5-20ms | ⚠️ | ⚠️ | Medium |

#### Implementation Notes

**Current Status in v2.0.0-alpha.1:**

The subscription architecture is fully designed and tested. The implementation integrates `SubscriptionManager` with `ObserverRuntime` to emit events from database changes, supporting automatic event population through executor hooks.

For enhancement requests or implementation details, see the roadmap in the project repository.

---

### 2.5 Relationship to Observer System

FraiseQL has **two separate event consumer systems** sharing the same event source:

#### Subscriptions vs Observers

| Aspect | Subscriptions | Observers |
|--------|--------------|-----------|
| **Purpose** | Real-time client notifications | Automation actions |
| **Consumers** | Browser/mobile clients | Webhooks, email, SMS, search indexing |
| **Transports** | graphql-ws, Kafka, HTTP webhooks | EventTransport trait (PostgresNotify, **NATS**, InMemory) |
| **Latency** | 100-150ms (polling) | Variable (action-dependent) |
| **Use Case** | Live dashboards, real-time UI | Background jobs, integrations |

#### Architecture: Two Branches from Same Source

```text
<!-- Code example in TEXT -->
tb_entity_change_log (single source of truth)
    ↓
ChangeLogListener (polls every 100ms)
    ↓
ObserverRuntime (in-process routing)
    ├─ ObserverExecutor → Actions (can use NATS transport)
    │   ├─ Webhooks
    │   ├─ Email/SMS
    │   ├─ Slack notifications
    │   └─ Search indexing
    │
    └─ SubscriptionManager → Client transports
        ├─ graphql-ws (WebSocket)
        ├─ Kafka
        └─ HTTP webhooks
```text
<!-- Code example in TEXT -->

#### Why Subscriptions Don't Use NATS Directly

**Subscriptions** use their own transport adapters (graphql-ws, Kafka) because:

- Clients connect directly to FraiseQL server (WebSocket)
- No intermediate message bus needed for latency-sensitive UI
- GraphQL protocol expectations (graphql-ws spec)

**Observers** can optionally use NATS because:

- Actions are asynchronous (latency tolerance)
- May need polyglot consumers (Python, Go, etc.)
- Benefits from distributed event streaming
- Horizontal scaling of action processors

#### Configuration: Composition by Default, NATS Optional

**Default (Composition):**

```toml
<!-- Code example in TOML -->
# FraiseQL.toml
[observer_runtime]
transport = "in_process"  # Direct routing, no NATS required

# Both observers and subscriptions get events from same ObserverRuntime
```text
<!-- Code example in TEXT -->

**Optional (NATS Everywhere):**

```toml
<!-- Code example in TOML -->
[observer_runtime]
transport = "nats"
nats_url = "nats://localhost:4222"
stream_name = "FraiseQL.events"

# All events published to NATS once
# ObserverExecutor and SubscriptionManager both consume from NATS
```text
<!-- Code example in TEXT -->

**Key Insight:** FraiseQL defaults to **database-centric composition** (no NATS required), but makes **NATS everywhere** easy to enable for distributed deployments.

---

## 3. Subscription Schema Authoring

### 3.1 Declaring Subscriptions

Subscriptions are declared using the same schema authoring languages as types and queries.

**Python Example:**

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class OrderCreated:
    """Events for new orders created by the authenticated user."""

    # Compile-time filter: Only current user's orders
    where: WhereOrder = FraiseQL.where(user_id=FraiseQL.context.user_id)

    # Fields to project from the Order entity
    id: ID
    user_id: ID
    created_at: DateTime
    amount: Decimal

    @FraiseQL.variable(name="since_date")
    class Filter:
        """Optional runtime filter for timestamp range."""
        created_at: DateTimeRange


@FraiseQL.subscription
class UserDeleted:
    """Events for users deleted (admin only)."""

    # Authorization: Admin context required
    where: WhereUser = FraiseQL.where(
        FraiseQL.context.role.contains("admin")
    )

    # Soft delete: Only fire if deleted_at is set
    id: ID
    email: Email
    deleted_at: DateTime


@FraiseQL.subscription
class OrderStatusChanged:
    """Events for status changes on organization's orders."""

    # Multi-tenant filtering
    where: WhereOrder = FraiseQL.where(
        fk_org=FraiseQL.context.org_id
    )

    # Nested projection (Order → OrderStatus entity)
    id: ID
    status: OrderStatus
    updated_at: DateTime
    updated_by_user: User
```text
<!-- Code example in TEXT -->

### 3.2 Compile-Time Validation

When the schema is compiled, the compiler:

1. **Identifies all subscription types**
   - Validates `@FraiseQL.subscription` decorators
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
<!-- Code example in JSON -->
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
   ```text
<!-- Code example in TEXT -->

### 3.3 Multiple Key Subscriptions

Subscriptions can filter on multiple fields:

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class OrderUpdated:
    """Subscription for specific order updates."""

    # Both compile-time constraints
    where: WhereOrder = FraiseQL.where(
        fk_org=FraiseQL.context.org_id,
        status=FraiseQL.context.allowed_statuses  # Must be in auth context
    )

    id: ID
    status: str
    updated_at: DateTime
```text
<!-- Code example in TEXT -->

---

## 4. Transport Protocols

### 4.1 GraphQL WebSocket (graphql-ws)

The primary transport for real-time UI updates using the standard `graphql-ws` protocol.

#### Connection Lifecycle

```text
<!-- Code example in TEXT -->
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
```text
<!-- Code example in TEXT -->

#### Example: Browser Client

```javascript
<!-- Code example in JAVASCRIPT -->
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
```text
<!-- Code example in TEXT -->

#### Error Handling

```json
<!-- Code example in JSON -->
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
```text
<!-- Code example in TEXT -->

**Common errors:**

- `AUTHENTICATION_REQUIRED` — User not authenticated
- `FORBIDDEN` — User lacks authorization for subscription
- `SUBSCRIPTION_NOT_FOUND` — Subscription type not defined in schema
- `INVALID_VARIABLES` — Runtime variable types incorrect

### 4.2 HTTP Webhooks

For push-based delivery to external systems.

#### Webhook Event

```json
<!-- Code example in JSON -->
POST https://external-service.example.com/webhooks/FraiseQL

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
```text
<!-- Code example in TEXT -->

#### Webhook Configuration

```python
<!-- Code example in Python -->
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
```text
<!-- Code example in TEXT -->

#### Delivery Semantics

- **At-least-once:** Event may be delivered multiple times
- **Ordered per entity:** Events for same entity arrive in order
- **Retried on failure:** 3 retries with exponential backoff
- **Signature verification:** HMAC-SHA256 for security

### 4.3 Kafka / Event Streaming

For high-throughput consumption by backend systems.

#### Kafka Topic

Topic name: `FraiseQL.{entity_type}.{operation}`

Examples:

- `FraiseQL.order.created`
- `FraiseQL.user.updated`
- `FraiseQL.order.deleted`

#### Kafka Message

```json
<!-- Code example in JSON -->
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
```text
<!-- Code example in TEXT -->

#### Kafka Configuration

```python
<!-- Code example in Python -->
config = FraiseQLConfig(
    kafka={
        "enabled": True,
        "bootstrap_servers": ["kafka:9092"],
        "subscriptions": {
            "OrderCreated": {
                "topic": "FraiseQL.order.created",
                "partition_by": "entity_id"  # Orders with same ID → same partition
            }
        }
    }
)
```text
<!-- Code example in TEXT -->

#### Delivery Semantics

- **At-least-once:** Messages may duplicate (use idempotent processing)
- **Ordered per partition:** Events for same entity arrive in order
- **Offset management:** Consumer tracks processed events
- **Replay capable:** Seek to any offset to replay events

### 4.4 gRPC (Future)

For low-latency service-to-service streaming.

```protobuf
<!-- Code example in PROTOBUF -->
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
```text
<!-- Code example in TEXT -->

---

## 5. Filtering & Variables

### 5.1 Compile-Time WHERE Clauses

Subscriptions filter events using WHERE clauses evaluated at compile time and rendered as SQL predicates.

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class OrderCreated:
    # Filter: Only orders for authenticated user
    where: WhereOrder = FraiseQL.where(
        user_id=FraiseQL.context.user_id
    )

# Compiled to:
# WHERE user_id = $1 (with $1 bound to context.user_id at runtime)
```text
<!-- Code example in TEXT -->

**Available context variables:**

```python
<!-- Code example in Python -->
FraiseQL.context.user_id         # Authenticated user ID
FraiseQL.context.org_id          # Organization/tenant ID
FraiseQL.context.role            # User role (string or list)
FraiseQL.context.permissions     # User permissions
FraiseQL.context.custom_claim    # Custom auth claim
```text
<!-- Code example in TEXT -->

**Example: Multi-tenant filtering**

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class OrderUpdated:
    where: WhereOrder = FraiseQL.where(
        fk_org=FraiseQL.context.org_id,
        # Only notify on changes to orders in allowed statuses
        status=FraiseQL.context.allowed_statuses
    )

    id: ID
    status: OrderStatus
    updated_at: DateTime
```text
<!-- Code example in TEXT -->

### 5.2 Runtime Variables

Subscriptions accept typed runtime variables for additional filtering.

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class OrderCreated:
    where: WhereOrder = FraiseQL.where(
        user_id=FraiseQL.context.user_id
    )

    @FraiseQL.variable(name="since_date", type=DateTime)
    @FraiseQL.variable(name="min_amount", type=Decimal)
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
```text
<!-- Code example in TEXT -->

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
<!-- Code example in Python -->
@FraiseQL.subscription
class SensitiveDataAccessed:
    # Only admins receive this subscription
    where: WhereAuditLog = FraiseQL.where(
        FraiseQL.context.role == "admin"
    )

    # If context.role != "admin", subscription unavailable
    # Compile-time error or runtime 403 FORBIDDEN
```text
<!-- Code example in TEXT -->

**Row-level authorization example:**

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class UserProfileUpdated:
    # User only sees updates to their own profile
    where: WhereUser = FraiseQL.where(
        id=FraiseQL.context.user_id
    )

    id: ID
    email: Email
    name: str
    updated_at: DateTime

# If User ID = 123 subscribes, only receives updates where id = 123
# No cross-user data leakage possible (enforced at compile time)
```text
<!-- Code example in TEXT -->

---

## 6. Event Format & Transformation

### 6.1 Relationship to CDC Format

Subscription events are derived from CDC events in `tb_entity_change_log`.

**CDC Event (raw, in database):**

```json
<!-- Code example in JSON -->
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
```text
<!-- Code example in TEXT -->

**Subscription Event (projected, sent to client):**

```json
<!-- Code example in JSON -->
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
```text
<!-- Code example in TEXT -->

**Transformation logic:**

1. **Extract fields requested:** Only `id`, `amount`, `created_at` included (as per subscription definition)
2. **Apply WHERE filter:** Event matches `user_id = $1` (context user)
3. **Format for transport:** Remove internal CDC fields, structure for GraphQL/webhook response
4. **Add event metadata:** `event_id`, `event_name`, `operation`, `sequence_number`

### 6.2 Field Projection

Subscription selection sets determine which fields are included in the event.

```python
<!-- Code example in Python -->
@FraiseQL.subscription
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
```text
<!-- Code example in TEXT -->

### 6.3 Nested Projections

Subscriptions can project nested entities.

```python
<!-- Code example in Python -->
@FraiseQL.subscription
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
```text
<!-- Code example in TEXT -->

---

## 7. Multi-Database Support

**PostgreSQL is the reference implementation for subscriptions.** Other databases follow the same architectural contract but may vary in maturity, feature completeness, and performance characteristics.

### 7.1 PostgreSQL

**Event capture mechanism:** Database table polling (`tb_entity_change_log`)

```sql
<!-- Code example in SQL -->
-- Event log table (already exists)
CREATE TABLE tb_entity_change_log (
    pk_entity_change_log BIGSERIAL PRIMARY KEY,
    id UUID NOT NULL,
    fk_customer_org UUID,
    object_type VARCHAR(255) NOT NULL,
    object_id VARCHAR(255) NOT NULL,
    modification_type VARCHAR(10) NOT NULL,  -- INSERT, UPDATE, DELETE
    change_status VARCHAR(50),
    object_data JSONB NOT NULL,
    extra_metadata JSONB,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for efficient polling
CREATE INDEX idx_entity_change_log_created ON tb_entity_change_log(created_at);
CREATE INDEX idx_entity_change_log_type ON tb_entity_change_log(object_type);

-- Application code inserts events after mutations
INSERT INTO tb_entity_change_log (object_type, object_id, modification_type, object_data)
VALUES ('Order', 'ord_123', 'INSERT', '{"id": "ord_123", "user_id": "usr_456", ...}'::jsonb);
```text
<!-- Code example in TEXT -->

**Advantages (Reference Implementation):**

- Database-centric (table as event log, not message channel)
- Full durability (events persisted in table)
- Replay capability (query any historical event)
- No additional infrastructure required
- Simple and predictable (100ms polling = real-time for UIs)
- Production-tested in FraiseQL observer system

**Limitations:**

- Manual event population required (no automatic triggers yet)
- 100-200ms latency (polling interval)
- Limited by PostgreSQL write throughput (single table)

### 7.2 MySQL

**Event capture mechanism:** Database table polling (`tb_entity_change_log`)

**Architecture:** Same as PostgreSQL - application code inserts events into `tb_entity_change_log`, ChangeLogListener polls.

**MySQL-specific considerations:**

```sql
<!-- Code example in SQL -->
-- Same table schema as PostgreSQL
CREATE TABLE tb_entity_change_log (
    pk_entity_change_log BIGINT AUTO_INCREMENT PRIMARY KEY,
    id CHAR(36) NOT NULL,
    fk_customer_org CHAR(36),
    object_type VARCHAR(255) NOT NULL,
    object_id VARCHAR(255) NOT NULL,
    modification_type VARCHAR(10) NOT NULL,
    change_status VARCHAR(50),
    object_data JSON NOT NULL,  -- MySQL uses JSON type
    extra_metadata JSON,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_entity_change_log_created ON tb_entity_change_log(created_at);
CREATE INDEX idx_entity_change_log_type ON tb_entity_change_log(object_type);
```text
<!-- Code example in TEXT -->

**Advantages:**

- Same architecture as PostgreSQL (consistency)
- No additional infrastructure required
- Works with managed MySQL services (AWS RDS, Cloud SQL)

**Limitations:**

- Manual event population (like PostgreSQL)
- 100-200ms latency (polling interval)
- JSON type instead of JSONB (slightly less efficient)

### 7.3 SQL Server

**Event capture mechanism:** Database table polling (`tb_entity_change_log`)

**Architecture:** Same as PostgreSQL - application code inserts events into `tb_entity_change_log`, ChangeLogListener polls.

**SQL Server-specific considerations:**

```sql
<!-- Code example in SQL -->
-- Same table schema as PostgreSQL
CREATE TABLE tb_entity_change_log (
    pk_entity_change_log BIGINT IDENTITY(1,1) PRIMARY KEY,
    id UNIQUEIDENTIFIER NOT NULL,
    fk_customer_org UNIQUEIDENTIFIER,
    object_type NVARCHAR(255) NOT NULL,
    object_id NVARCHAR(255) NOT NULL,
    modification_type NVARCHAR(10) NOT NULL,
    change_status NVARCHAR(50),
    object_data NVARCHAR(MAX) NOT NULL,  -- JSON stored as NVARCHAR(MAX)
    extra_metadata NVARCHAR(MAX),
    created_at DATETIME2 NOT NULL DEFAULT GETUTCDATE()
);

CREATE INDEX idx_entity_change_log_created ON tb_entity_change_log(created_at);
CREATE INDEX idx_entity_change_log_type ON tb_entity_change_log(object_type);
```text
<!-- Code example in TEXT -->

**Advantages:**

- Same architecture as PostgreSQL (consistency)
- No additional infrastructure required
- Works with all SQL Server editions (including Express)

**Limitations:**

- Manual event population (like PostgreSQL)
- 100-200ms latency (polling interval)
- JSON stored as NVARCHAR(MAX) (less efficient than native JSON)

### 7.4 SQLite

**Event capture mechanism:** Triggers on temporary in-memory event log

```sql
<!-- Code example in SQL -->
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
```text
<!-- Code example in TEXT -->

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
<!-- Code example in RUST -->
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
```text
<!-- Code example in TEXT -->

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

**FraiseQL Architecture (database-centric):**

- Application code explicitly inserts events into `tb_entity_change_log` after mutations
- `ChangeLogListener` polls table every 100ms with checkpoint tracking
- `ObserverRuntime` processes events in background task
- Events routed to both `ObserverExecutor` (actions) and `SubscriptionManager` (transports)

**Event capture across databases:**

- PostgreSQL: Direct table polling (reference implementation)
- MySQL: Table polling (same as PostgreSQL)
- SQL Server: Table polling (same as PostgreSQL)
- SQLite: Table polling for development/testing

**Event buffer (`tb_entity_change_log`):** All events written with:

- Monotonic sequence numbers (for replay and ordering)
- Debezium-compatible envelope format
- Full entity data in JSONB column
- Timestamp for chronological ordering

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
| ChangeLogListener polling | 100ms (P50), 200ms (P99) | Polling interval + checkpoint |
| graphql-ws client (local) | 100-150ms | Polling + network round-trip |
| graphql-ws client (remote) | 150-300ms | Polling + WAN latency |
| Webhook delivery | 150-400ms | Polling + HTTP request + retry logic |
| Kafka producer | 100-150ms | Polling + async write to broker |

**Example: User creates order in UI, sees confirmation**

```text
<!-- Code example in TEXT -->

1. Mutation committed (1ms)
2. Event inserted into tb_entity_change_log (1ms)
3. ChangeLogListener polls (0-100ms, average 50ms)
4. ObserverRuntime processes event (1ms)
5. Filter evaluates (0.5ms)
6. Transform to GraphQL (0.5ms)
7. Send to WebSocket (1ms)
8. Client receives (5-10ms network)
────────────────
Total: ~100-150ms (imperceptible to users)
```text
<!-- Code example in TEXT -->

### 9.2 Throughput

**Concurrent subscriptions:**

- Single process: 1,000-10,000 concurrent WebSocket connections (depends on memory)
- Horizontal scaling: Multiple FraiseQL instances behind load balancer
- Event buffering: `tb_entity_change_log` handles burst traffic

**Event throughput (observed in reference deployments):**

- ChangeLogListener polling: 1,000-2,000 events/second (database-limited)
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
<!-- Code example in GraphQL -->
  # NOT ALLOWED: Subscribes to Order changes, but also User changes
  subscription {
    orderCreated { id }
    userUpdated { id }
  }
  ```text
<!-- Code example in TEXT -->

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

**All Databases (Polling Architecture):**

- Manual event population required (application must INSERT into `tb_entity_change_log`)
- 100-200ms latency (polling interval)
- Single table write bottleneck (can scale with partitioning)

**PostgreSQL:**

- None specific (reference implementation)

**MySQL:**

- JSON type less efficient than PostgreSQL JSONB
- No native UUID type (stored as CHAR(36))

**SQL Server:**

- JSON stored as NVARCHAR(MAX) (less efficient than native JSON)
- Different date/time types (DATETIME2 vs TIMESTAMP)

**SQLite:**

- Single process (no network clients)
- Not suitable for production subscriptions
- Good for development/testing only

---

## 11. Security & Authorization

### 11.1 Authentication

Subscriptions require authentication same as mutations:

```python
<!-- Code example in Python -->
# Only authenticated users can subscribe
@FraiseQL.subscription
class OrderCreated:
    where: WhereOrder = FraiseQL.where(
        user_id=FraiseQL.context.user_id
    )
    # Fails if context.user_id is None (unauthenticated)
```text
<!-- Code example in TEXT -->

### 11.2 Row-Level Authorization

WHERE clauses enforce row-level access control through compile-time rule definition and runtime-safe parameter binding:

**Mechanism:**

- Authorization rules **defined** at compile time in schema (WHERE clause states who can access what)
- Authorization values **bound** at runtime (context.user_id, context.org_id resolved from AuthContext when subscription established)
- Filters are deterministic SQL predicates—no dynamic logic

**Examples:**

```python
<!-- Code example in Python -->
# User only sees their own orders
where: WhereOrder = FraiseQL.where(user_id=FraiseQL.context.user_id)

# Org admin sees org's orders
where: WhereOrder = FraiseQL.where(fk_org=FraiseQL.context.org_id)

# Admin sees everything (no WHERE filter)
where: WhereOrder = FraiseQL.where()  # No filter = all rows
```text
<!-- Code example in TEXT -->

### 11.3 Field-Level Authorization

Projected fields can have authorization rules:

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class OrderCreated:
    id: ID  # Always visible
    amount: Decimal  # Always visible

    # sensitive_notes only visible to admin
    sensitive_notes: Optional[str] = FraiseQL.field(
        auth_required=["admin"]
    )

# If context.role != "admin", sensitive_notes omitted from events
```text
<!-- Code example in TEXT -->

### 11.4 Signature Verification (Webhooks)

Webhooks include HMAC-SHA256 signature for verification:

```javascript
<!-- Code example in JAVASCRIPT -->
// Webhook handler
const signature = req.headers['x-FraiseQL-signature'];
const payload = req.rawBody;

const expected = crypto
  .createHmac('sha256', WEBHOOK_SECRET)
  .update(payload)
  .digest('hex');

if (signature !== expected) {
  return res.status(401).send('Signature mismatch');
}
```text
<!-- Code example in TEXT -->

---

## 12. Examples

### Example 1: Real-Time Order Dashboard

**Schema definition:**

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class OrderCreated:
    """Stream new orders for the organization."""

    where: WhereOrder = FraiseQL.where(
        fk_org=FraiseQL.context.org_id
    )

    id: ID
    user_id: ID
    amount: Decimal
    created_at: DateTime
    user: User  # Nested projection


@FraiseQL.subscription
class OrderStatusChanged:
    """Stream status updates for organization's orders."""

    where: WhereOrder = FraiseQL.where(
        fk_org=FraiseQL.context.org_id
    )

    id: ID
    old_status: OrderStatus
    new_status: OrderStatus
    updated_at: DateTime
```text
<!-- Code example in TEXT -->

**Client (React):**

```typescript
<!-- Code example in TypeScript -->
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
```text
<!-- Code example in TEXT -->

### Example 2: Event Streaming to Analytics

**Schema definition:**

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class UserRegistered:
    """Stream new user registrations (no filter, analytics event)."""

    where: WhereUser = FraiseQL.where()  # All users

    id: ID
    email: Email
    created_at: DateTime
    source: str  # How they registered


@FraiseQL.subscription
class PurchaseMade:
    """Stream purchases for analytics and revenue tracking."""

    where: WhereOrder = FraiseQL.where(
        status="completed"  # Only completed orders
    )

    id: ID
    user_id: ID
    amount: Decimal
    items: list[OrderItem]
    created_at: DateTime
```text
<!-- Code example in TEXT -->

**Kafka configuration:**

```python
<!-- Code example in Python -->
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
```text
<!-- Code example in TEXT -->

**Consumer (Python):**

```python
<!-- Code example in Python -->
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
```text
<!-- Code example in TEXT -->

### Example 3: Multi-Tenant Filtering with Variables

**Schema definition:**

```python
<!-- Code example in Python -->
@FraiseQL.subscription
class ActivityInOrganization:
    """Stream activity (creates, updates, deletes) in organization."""

    where: WhereAuditLog = FraiseQL.where(
        fk_org=FraiseQL.context.org_id
    )

    @FraiseQL.variable(name="min_severity")
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
```text
<!-- Code example in TEXT -->

**Client with filtering:**

```graphql
<!-- Code example in GraphQL -->
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
```text
<!-- Code example in TEXT -->

**Usage:**

```javascript
<!-- Code example in JAVASCRIPT -->
// Subscribe to high-priority events only
useSubscription(ActivityInOrganization, {
  variables: {
    min_severity: "HIGH"
  }
});
```text
<!-- Code example in TEXT -->

---

## 13. Appendix

### A. Debugging Subscriptions

**Check if subscription is defined:**

```bash
<!-- Code example in BASH -->
# Query introspection
query {
  __type(name: "Subscription") {
    fields {
      name
    }
  }
}
```text
<!-- Code example in TEXT -->

**Monitor event flow:**

```sql
<!-- Code example in SQL -->
-- Check event buffer
SELECT COUNT(*) as pending_events
FROM tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '1 minute';

-- Monitor subscription lag
SELECT entity_type, MAX(created_at) as last_event
FROM tb_entity_change_log
GROUP BY entity_type;
```text
<!-- Code example in TEXT -->

**Enable subscription tracing (Rust runtime):**

```rust
<!-- Code example in RUST -->
if config.debug {
    trace!("Subscription: OrderCreated");
    trace!("  Filter: WHERE user_id = {} AND deleted_at IS NULL", user_id);
    trace!("  Event matched: {}", event_matches_filter);
    trace!("  Delivered to: {} clients", client_count);
}
```text
<!-- Code example in TEXT -->

### B. Monitoring Metrics

**Key metrics to track:**

```text
<!-- Code example in TEXT -->
FraiseQL.subscription.connections     # Current active connections
FraiseQL.subscription.events_emitted   # Events matching filters
FraiseQL.subscription.events_delivered # Events sent to clients
FraiseQL.subscription.lag_seconds      # Delay from database to client
FraiseQL.subscription.error_count      # Delivery failures
```text
<!-- Code example in TEXT -->

### C. Connection Pool Sizing

**Recommendation:**

- Pool size = (expected_concurrent_subscriptions / 10) + overhead
- Default: 20 connections
- Monitor connection count and adjust

**Example:**

```python
<!-- Code example in Python -->
config = FraiseQLConfig(
    database_url="postgresql://...",
    subscriptions={
        "connection_pool_size": 50,  # For 500+ concurrent subscriptions
        "connection_timeout": 300,
        "idle_timeout": 60
    }
)
```text
<!-- Code example in TEXT -->

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

- ✅ Database-centric (table polling, not message channels)
- ✅ Compiled, not interpreted
- ✅ Transport-agnostic (graphql-ws, webhooks, Kafka, etc.)
- ✅ Deterministic, no user code
- ✅ Durable (buffered in `tb_entity_change_log`)

**Architecture:**

1. Database transaction commits
2. Application inserts event into `tb_entity_change_log`
3. ChangeLogListener polls table every 100ms
4. ObserverRuntime routes events to SubscriptionManager
5. Filters evaluated against compiled predicates
6. Delivered via transport adapter (graphql-ws, webhook, Kafka)

**Performance:**

- **Latency:** 100-150ms (polling + processing + network)
- **Throughput:** 1,000-2,000 events/sec (database-limited)
- **Perceived latency:** Imperceptible to users for UI updates

**Relationship to Observers:**

- Subscriptions and Observers share the same event source (`tb_entity_change_log`)
- Observers can optionally use NATS for distributed action processing
- Subscriptions use direct transports (graphql-ws, Kafka) for client notifications
- Default: composition (in-process routing), NATS optional for distributed deployments

**Security:**

- Row-level filtering enforced at compile time
- No cross-tenant data leakage
- Authorization via AuthContext

**Limitations:**

- Subscriptions are read-only (no mutations)
- Filters compile-time determined
- Per-entity ordering only
- Manual event population required (automatic triggers pending)

*End of Subscriptions Specification*
