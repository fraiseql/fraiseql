# Change Data Capture (CDC) Format Specification

**Version:** 1.0
**Status:** Draft
**Audience:** Database architects, subscription implementers, real-time systems

---

## 1. Overview

The **CDC Format** defines how database mutations (INSERT, UPDATE, DELETE) are captured and emitted as events for real-time subscriptions, audit logging, and external system synchronization.

**Key characteristics:**
- **Universal format** — all databases produce same event structure
- **Deterministic** — every mutation generates exactly one CDC event
- **Rich** — includes before/after state, changed fields, cascade information
- **Optional** — CDC is opt-in; can be disabled for write-heavy workloads
- **Database-agnostic** — same event structure whether PostgreSQL, MySQL, SQL Server, or SQLite

---

## 2. CDC Event Structure

### 2.1 Top-Level Event

```json
{
  "version": "1.0",
  "event_id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "event_type": "entity:updated|entity:deleted|entity:created",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "schema_name": "acme-api",
  "schema_version": "2.1.0",
  "source": {
    "database": "postgresql",
    "instance": "prod-primary-1",
    "transaction_id": "1234567890",
    "session_id": "sess_xyz"
  },
  "entity": {
    "entity_type": "User",
    "entity_id": "550e8400-e29b-41d4-a716-446655440000",
    "tenant_id": "tenant-123"
  },
  "operation": { ... },
  "cascade": { ... },
  "metadata": { ... }
}
```

### 2.2 Event Metadata

| Field | Type | Required | Purpose |
|-------|------|----------|---------|
| `version` | string | ✓ | CDC format version ("1.0") |
| `event_id` | uuid | ✓ | Unique event identifier |
| `event_type` | enum | ✓ | `entity:created`, `entity:updated`, `entity:deleted` |
| `timestamp` | iso8601 | ✓ | When mutation occurred (database server time) |
| `sequence_number` | int64 | ✓ | Monotonic sequence for ordering |
| `schema_name` | string | ✓ | Schema name (from CompiledSchema) |
| `schema_version` | string | ✓ | Schema version (from CompiledSchema) |
| `source` | object | ✓ | Database source information |
| `entity` | object | ✓ | Entity information |
| `operation` | object | ✓ | Operation details (before/after state) |
| `cascade` | object | ✓ | Related entity changes |
| `metadata` | object | ✓ | Custom metadata |

**Related Specifications:**
- **docs/specs/schema-conventions.md section 6.2** — Debezium envelope format used in `tb_entity_change_log` matches the `operation` structure here
- **docs/guides/observability.md section 9** — CDC event streaming patterns and consumption strategies

---

## 3. Source Information

### 3.1 Source Structure

```json
{
  "source": {
    "database": "postgresql|sqlite|mysql|sqlserver",
    "instance": "prod-primary-1",
    "database_name": "acme_production",
    "host": "pg.internal.example.com",
    "region": "us-east-1",
    "transaction_id": "1234567890",
    "session_id": "sess_xyz",
    "user": "api_service",
    "schema": "public"
  }
}
```

### 3.2 Transaction ID

The `transaction_id` uniquely identifies the database transaction:

```python
# PostgreSQL: txid_current()
# MySQL: @@global.gtid_executed
# SQL Server: @@TRANCOUNT
# SQLite: Not available (use timestamp)
```

Used for idempotency — multiple events from same transaction share same ID.

### 3.3 Session ID

Optional session identifier for correlation:

```json
{
  "session_id": "sess_550e8400-e29b-41d4-a716-446655440000"
}
```

Can trace all mutations from single client session.

---

## 4. Entity Information

### 4.1 Entity Structure

```json
{
  "entity": {
    "entity_type": "User",
    "entity_id": "550e8400-e29b-41d4-a716-446655440000",
    "entity_identifier": "alice@example.com",
    "tenant_id": "tenant-123",
    "version": 3,
    "is_soft_deleted": false
  }
}
```

### 4.2 Entity Fields

| Field | Type | Purpose |
|-------|------|---------|
| `entity_type` | string | Type name from CompiledSchema |
| `entity_id` | uuid | Primary identifier (UUID) |
| `entity_identifier` | string | Human-readable identifier (optional) |
| `tenant_id` | string | Multi-tenant isolation key |
| `version` | int | Optimistic lock version (if applicable) |
| `is_soft_deleted` | bool | True if soft delete (deleted_at not null) |

### 4.3 Tenant ID

For multi-tenant systems, always include tenant:

```json
{
  "entity": {
    "tenant_id": "tenant-123"
  }
}
```

Used for:
- Event filtering in multi-tenant systems
- Audit log partitioning
- Cache invalidation scoping

---

## 5. Operation Details

### 5.1 CREATE Operation

When entity is inserted:

```json
{
  "operation": {
    "type": "CREATE",
    "trigger": "api_create|api_import|batch_import|admin_action|data_migration",
    "timestamp": "2026-01-11T15:35:00.123456Z",
    "after": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "alice@example.com",
      "name": "Alice Smith",
      "status": "active",
      "created_at": "2026-01-11T15:35:00Z",
      "updated_at": "2026-01-11T15:35:00Z"
    },
    "changed_fields": [
      "id", "email", "name", "status", "created_at", "updated_at"
    ]
  }
}
```

### 5.2 UPDATE Operation

When entity is modified:

```json
{
  "operation": {
    "type": "UPDATE",
    "trigger": "api_update|admin_action",
    "timestamp": "2026-01-11T15:40:00.123456Z",
    "before": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "alice@example.com",
      "name": "Alice Smith",
      "status": "active",
      "updated_at": "2026-01-11T15:35:00Z"
    },
    "after": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "alice.smith@example.com",  // Changed
      "name": "Alice Smith",
      "status": "active",
      "updated_at": "2026-01-11T15:40:00Z"
    },
    "changed_fields": [
      "email",       // Actually changed
      "updated_at"   // Updated by trigger
    ],
    "changed_field_values": {
      "email": {
        "before": "alice@example.com",
        "after": "alice.smith@example.com"
      },
      "updated_at": {
        "before": "2026-01-11T15:35:00Z",
        "after": "2026-01-11T15:40:00Z"
      }
    }
  }
}
```

### 5.3 DELETE Operation

When entity is deleted:

```json
{
  "operation": {
    "type": "DELETE",
    "trigger": "api_delete|admin_action|cascade_delete",
    "timestamp": "2026-01-11T15:45:00.123456Z",
    "before": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "alice.smith@example.com",
      "name": "Alice Smith",
      "status": "active",
      "deleted_at": null
    },
    "after": null,  // Entity no longer exists (hard delete)
    "changed_fields": [
      "deleted_at"  // Or entire record if hard delete
    ]
  }
}
```

### 5.4 Soft Delete

For soft deletes (deleted_at):

```json
{
  "operation": {
    "type": "UPDATE",  // Soft delete is UPDATE operation
    "trigger": "api_delete|admin_action",
    "after": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "deleted_at": "2026-01-11T15:45:00Z"
    },
    "changed_fields": ["deleted_at"]
  }
}
```

### 5.5 Operation Triggers

The `trigger` field indicates what caused the mutation:

| Trigger | Meaning | Example |
|---------|---------|---------|
| `api_create` | Created via GraphQL mutation | `createUser` |
| `api_update` | Updated via GraphQL mutation | `updateUser` |
| `api_delete` | Deleted via GraphQL mutation | `deleteUser` |
| `admin_action` | Admin console action | Direct SQL |
| `batch_import` | Bulk import operation | CSV import |
| `data_migration` | Migration script | Schema migration |
| `cascade_delete` | Deleted due to FK cascade | Delete user → delete posts |

---

## 6. Cascade Information

### 6.1 Cascade Structure

The `cascade` field describes related entity changes:

```json
{
  "cascade": {
    "updated": [
      {
        "entity_type": "User",
        "entity_id": "550e8400-e29b-41d4-a716-446655440000",
        "changes": ["last_post_count", "updated_at"],
        "reason": "related_entity_updated"
      }
    ],
    "deleted": [
      {
        "entity_type": "Post",
        "entity_id": "660e8400-e29b-41d4-a716-446655440001",
        "reason": "parent_deleted"
      }
    ],
    "invalidations": [
      {
        "query": "userPosts",
        "reason": "post_deleted"
      },
      {
        "query": "users",
        "reason": "user_updated"
      }
    ]
  }
}
```

### 6.2 Cascade Entry Types

#### 6.2.1 Updated

Related entities that were updated as side effect:

```json
{
  "updated": [
    {
      "entity_type": "User",
      "entity_id": "550e8400-e29b-41d4-a716-446655440000",
      "changes": ["post_count", "updated_at"],
      "reason": "child_entity_added"
    }
  ]
}
```

#### 6.2.2 Deleted

Related entities that were deleted as side effect:

```json
{
  "deleted": [
    {
      "entity_type": "Post",
      "entity_id": "660e8400-e29b-41d4-a716-446655440001",
      "reason": "parent_deleted"
    }
  ]
}
```

#### 6.2.3 Invalidations

Queries/lists that should be cache-invalidated:

```json
{
  "invalidations": [
    {
      "query": "userPosts",
      "reason": "post_deleted",
      "affected_entities": ["User:550e8400-e29b-41d4-a716-446655440000"]
    }
  ]
}
```

---

## 7. Custom Metadata

### 7.1 Metadata Structure

```json
{
  "metadata": {
    "request_id": "req_550e8400-e29b-41d4-a716-446655440000",
    "user_id": "550e8400-e29b-41d4-a716-446655440002",
    "user_roles": ["user", "admin"],
    "ip_address": "192.0.2.1",
    "user_agent": "Mozilla/5.0...",
    "api_version": "2.1.0",
    "custom_fields": {
      "import_batch_id": "batch_123",
      "workflow_step": "approval_pending"
    }
  }
}
```

### 7.2 Metadata Fields

| Field | Purpose |
|-------|---------|
| `request_id` | Trace ID for correlation |
| `user_id` | Which user triggered mutation |
| `user_roles` | User roles at time of mutation |
| `ip_address` | Source IP (if available) |
| `user_agent` | Client user agent |
| `api_version` | API version that made request |
| `custom_fields` | Application-specific metadata |

---

## 8. Complete Event Examples

### 8.1 User Created

```json
{
  "version": "1.0",
  "event_id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "event_type": "entity:created",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "schema_name": "acme-api",
  "schema_version": "2.1.0",
  "source": {
    "database": "postgresql",
    "instance": "prod-primary-1",
    "transaction_id": "1234567890",
    "session_id": "sess_abc123"
  },
  "entity": {
    "entity_type": "User",
    "entity_id": "550e8400-e29b-41d4-a716-446655440000",
    "tenant_id": "tenant-123"
  },
  "operation": {
    "type": "CREATE",
    "trigger": "api_create",
    "timestamp": "2026-01-11T15:35:00.123456Z",
    "after": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "alice@example.com",
      "name": "Alice Smith",
      "status": "active",
      "created_at": "2026-01-11T15:35:00Z",
      "updated_at": "2026-01-11T15:35:00Z"
    },
    "changed_fields": ["id", "email", "name", "status", "created_at", "updated_at"]
  },
  "cascade": {
    "updated": [],
    "deleted": [],
    "invalidations": [
      {
        "query": "users",
        "reason": "entity_created"
      }
    ]
  },
  "metadata": {
    "request_id": "req_550e8400-e29b-41d4-a716-446655440001",
    "user_id": "550e8400-e29b-41d4-a716-446655440002",
    "user_roles": ["admin"],
    "api_version": "2.1.0"
  }
}
```

### 8.2 User Updated

```json
{
  "version": "1.0",
  "event_id": "evt_660e8400-e29b-41d4-a716-446655440000",
  "event_type": "entity:updated",
  "timestamp": "2026-01-11T15:40:00.654321Z",
  "sequence_number": 4522,
  "schema_name": "acme-api",
  "schema_version": "2.1.0",
  "source": {
    "database": "postgresql",
    "instance": "prod-primary-1",
    "transaction_id": "1234567891",
    "session_id": "sess_def456"
  },
  "entity": {
    "entity_type": "User",
    "entity_id": "550e8400-e29b-41d4-a716-446655440000",
    "tenant_id": "tenant-123"
  },
  "operation": {
    "type": "UPDATE",
    "trigger": "api_update",
    "timestamp": "2026-01-11T15:40:00.654321Z",
    "before": {
      "email": "alice@example.com",
      "status": "active",
      "updated_at": "2026-01-11T15:35:00Z"
    },
    "after": {
      "email": "alice.smith@example.com",
      "status": "active",
      "updated_at": "2026-01-11T15:40:00.654321Z"
    },
    "changed_fields": ["email", "updated_at"],
    "changed_field_values": {
      "email": {
        "before": "alice@example.com",
        "after": "alice.smith@example.com"
      },
      "updated_at": {
        "before": "2026-01-11T15:35:00Z",
        "after": "2026-01-11T15:40:00.654321Z"
      }
    }
  },
  "cascade": {
    "updated": [],
    "deleted": [],
    "invalidations": [
      {
        "query": "user",
        "reason": "entity_updated"
      },
      {
        "query": "users",
        "reason": "entity_updated"
      }
    ]
  },
  "metadata": {
    "request_id": "req_660e8400-e29b-41d4-a716-446655440001",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "user_roles": ["user"],
    "api_version": "2.1.0"
  }
}
```

### 8.3 User Deleted (Cascade)

```json
{
  "version": "1.0",
  "event_id": "evt_770e8400-e29b-41d4-a716-446655440000",
  "event_type": "entity:deleted",
  "timestamp": "2026-01-11T15:45:00.987654Z",
  "sequence_number": 4523,
  "schema_name": "acme-api",
  "schema_version": "2.1.0",
  "source": {
    "database": "postgresql",
    "instance": "prod-primary-1",
    "transaction_id": "1234567892",
    "session_id": "sess_ghi789"
  },
  "entity": {
    "entity_type": "User",
    "entity_id": "550e8400-e29b-41d4-a716-446655440000",
    "tenant_id": "tenant-123"
  },
  "operation": {
    "type": "DELETE",
    "trigger": "api_delete",
    "timestamp": "2026-01-11T15:45:00.987654Z",
    "before": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "alice.smith@example.com",
      "status": "active"
    },
    "after": null,
    "changed_fields": ["id", "email", "name", "status", "created_at", "updated_at"]
  },
  "cascade": {
    "updated": [
      {
        "entity_type": "Organization",
        "entity_id": "880e8400-e29b-41d4-a716-446655440000",
        "changes": ["member_count"],
        "reason": "parent_entity_deleted"
      }
    ],
    "deleted": [
      {
        "entity_type": "Post",
        "entity_id": "990e8400-e29b-41d4-a716-446655440001",
        "reason": "parent_deleted"
      },
      {
        "entity_type": "Post",
        "entity_id": "990e8400-e29b-41d4-a716-446655440002",
        "reason": "parent_deleted"
      }
    ],
    "invalidations": [
      {
        "query": "users",
        "reason": "entity_deleted"
      },
      {
        "query": "userPosts",
        "reason": "parent_deleted"
      }
    ]
  },
  "metadata": {
    "request_id": "req_770e8400-e29b-41d4-a716-446655440001",
    "user_id": "550e8400-e29b-41d4-a716-446655440003",
    "user_roles": ["admin"],
    "api_version": "2.1.0"
  }
}
```

---

## 9. CDC Implementation Patterns

### 9.1 PostgreSQL Implementation

PostgreSQL can emit CDC events via:

**Option 1: Triggers**
```sql
CREATE TRIGGER user_cdc_trigger
AFTER INSERT OR UPDATE OR DELETE ON tb_user
FOR EACH ROW
EXECUTE FUNCTION emit_cdc_event();
```

**Option 2: Logical Replication** (WAL)
- Use pg_logical_decode
- Consume events from replication slot
- Transform to CDC format

**Option 3: Event Table**
```sql
INSERT INTO audit_log (event_type, entity_type, entity_id, operation)
VALUES ('entity:updated', 'User', NEW.id, row_to_json(NEW));
```

### 9.2 SQLite Implementation

SQLite CDC via update hooks:

```python
def sqlite_cdc_hook(action: str, db: str, table: str, rowid: int):
    """SQLite update hook for CDC."""
    if action == sqlite3.INSERT:
        event_type = "entity:created"
    elif action == sqlite3.UPDATE:
        event_type = "entity:updated"
    elif action == sqlite3.DELETE:
        event_type = "entity:deleted"

    emit_cdc_event(event_type, table, rowid)
```

### 9.3 SQL Server Implementation

SQL Server CDC via CDC feature:

```sql
EXEC sys.sp_cdc_enable_db
EXEC sys.sp_cdc_enable_table
  @source_schema = 'dbo',
  @source_name = 'tb_user',
  @role_name = NULL
```

---

## 10. Event Delivery

### 10.1 Event Streaming Protocols

CDC events can be delivered via:

| Protocol | Use Case | Latency |
|----------|----------|---------|
| **WebSocket** | Browser subscriptions | Real-time (< 100ms) |
| **Server-Sent Events** | HTTP streaming | Real-time (< 100ms) |
| **Kafka/Pub-Sub** | Multi-service | Configurable |
| **Message Queue** | Durable delivery | Configurable |
| **Webhook** | External systems | Best-effort |
| **File/Log** | Audit trail | Batch |

### 10.2 Idempotency

To ensure exactly-once delivery:

```python
# Use event_id for deduplication
processed_event_ids = set()

for event in event_stream:
    if event.event_id in processed_event_ids:
        continue  # Skip duplicate

    process_event(event)
    processed_event_ids.add(event.event_id)
```

### 10.3 Ordering Guarantees

Events within a transaction are ordered by `sequence_number`:

```python
# Sort events before processing
events_sorted = sorted(events, key=lambda e: e.sequence_number)

# Process in order
for event in events_sorted:
    process_event(event)
```

---

## 11. CDC Filtering

### 11.1 Entity Type Filter

```python
# Only receive User events
events = subscribe(event_type="entity:*", entity_type="User")
```

### 11.2 Tenant Filter

```python
# Only receive tenant-123 events
events = subscribe(tenant_id="tenant-123")
```

### 11.3 Trigger Filter

```python
# Only API mutations (not admin/migration)
events = subscribe(trigger="api_*")
```

### 11.4 Changed Field Filter

```python
# Only if status field changed
events = subscribe(
    entity_type="User",
    changed_fields=["status"]
)
```

---

## 12. CDC Guarantees

### 12.1 Atomicity

All changes from a single mutation are in one event:

```python
# Mutation deletes user + 3 posts
# CDC emits 1 event with:
# - entity_type: User
# - cascade.deleted: [Post, Post, Post]
```

### 12.2 Ordering

Events are monotonically ordered by `sequence_number`:

```python
# Event 4520, 4521, 4522, ... guaranteed order
# No gaps (unless CDC temporarily disabled)
```

### 12.3 Completeness

Every mutation produces exactly one event:

```python
# If mutation succeeds, CDC event emitted
# If mutation fails, no CDC event
# If mutation partially succeeds, one event with error metadata
```

### 12.4 Durability

Events are persisted before acknowledging to client:

```
Client mutation request
    ↓
Execute in database
    ↓
Emit CDC event to queue/log
    ↓
Return success to client
```

---

## 13. CDC Configuration

### 13.1 Enable/Disable CDC

In CompiledSchema:

```json
{
  "cdc": {
    "enabled": true,
    "storage": "postgres_wal|kafka|event_table",
    "retention_days": 30,
    "batch_size": 100
  }
}
```

### 13.2 Selective Entities

Only emit CDC for certain entities:

```json
{
  "cdc": {
    "entities": ["User", "Post"],
    "exclude_entities": ["AuditLog"]
  }
}
```

### 13.3 Selective Operations

Only emit CDC for certain operations:

```json
{
  "cdc": {
    "operations": ["UPDATE", "DELETE"],
    "exclude_operations": ["CREATE"]
  }
}
```

---

*End of CDC Format Specification*
