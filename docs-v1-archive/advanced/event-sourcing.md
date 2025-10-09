---
← [CQRS](cqrs.md) | [Advanced Topics](index.md) | [Next: Multi-tenancy](multi-tenancy.md) →
---

# Event Sourcing

> **In this section:** Implement event sourcing patterns with FraiseQL for audit trails and time-travel queries
> **Prerequisites:** Understanding of [CQRS patterns](cqrs.md) and [PostgreSQL functions](../mutations/postgresql-function-based.md)
> **Time to complete:** 25 minutes

Event sourcing stores all changes as a sequence of events, allowing you to reconstruct any past state and maintain a complete audit trail.

## Event Store Schema

### Core Event Table
```sql
-- Event store table
CREATE TABLE tb_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stream_id UUID NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    event_version INTEGER NOT NULL,
    event_data JSONB NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    created_by UUID,

    -- Ensure event ordering
    CONSTRAINT unique_stream_version UNIQUE (stream_id, event_version)
);

-- Indexes for performance
CREATE INDEX idx_events_stream_id ON tb_events(stream_id);
CREATE INDEX idx_events_type ON tb_events(event_type);
CREATE INDEX idx_events_created_at ON tb_events(created_at);
```

### Event Types Definition
```sql
-- Define event types for type safety
CREATE TYPE event_type AS ENUM (
    'USER_CREATED',
    'USER_UPDATED',
    'USER_DELETED',
    'POST_CREATED',
    'POST_PUBLISHED',
    'POST_UPDATED',
    'COMMENT_ADDED',
    'COMMENT_DELETED'
);
```

## Event Storage Functions

### Append Events
```sql
CREATE OR REPLACE FUNCTION append_event(
    p_stream_id UUID,
    p_event_type TEXT,
    p_event_data JSONB,
    p_metadata JSONB DEFAULT '{}',
    p_created_by UUID DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    next_version INTEGER;
    event_id UUID;
BEGIN
    -- Get next version for this stream
    SELECT COALESCE(MAX(event_version), 0) + 1
    INTO next_version
    FROM tb_events
    WHERE stream_id = p_stream_id;

    -- Insert event
    INSERT INTO tb_events (
        stream_id,
        event_type,
        event_version,
        event_data,
        metadata,
        created_by
    ) VALUES (
        p_stream_id,
        p_event_type,
        next_version,
        p_event_data,
        p_metadata,
        p_created_by
    ) RETURNING id INTO event_id;

    RETURN event_id;
END;
$$ LANGUAGE plpgsql;
```

### Query Events
```sql
CREATE OR REPLACE FUNCTION get_events(
    p_stream_id UUID,
    p_from_version INTEGER DEFAULT 1,
    p_to_version INTEGER DEFAULT NULL
) RETURNS TABLE (
    event_type TEXT,
    event_version INTEGER,
    event_data JSONB,
    created_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        e.event_type,
        e.event_version,
        e.event_data,
        e.created_at
    FROM tb_events e
    WHERE e.stream_id = p_stream_id
        AND e.event_version >= p_from_version
        AND (p_to_version IS NULL OR e.event_version <= p_to_version)
    ORDER BY e.event_version;
END;
$$ LANGUAGE plpgsql;
```

## Aggregate Implementation

### User Aggregate
```python
from dataclasses import dataclass
from datetime import datetime
from typing import List, Dict, Any
from fraiseql import ID

@dataclass
class UserCreated:
    user_id: ID
    name: str
    email: str
    created_at: datetime

@dataclass
class UserUpdated:
    user_id: ID
    name: str | None = None
    email: str | None = None
    updated_at: datetime = None

class UserAggregate:
    def __init__(self, user_id: ID):
        self.id = user_id
        self.version = 0
        self.name = ""
        self.email = ""
        self.created_at = None
        self.updated_at = None
        self.is_deleted = False

    def apply_event(self, event_type: str, event_data: Dict[str, Any]):
        """Apply event to aggregate state"""
        if event_type == "USER_CREATED":
            self._apply_user_created(event_data)
        elif event_type == "USER_UPDATED":
            self._apply_user_updated(event_data)
        elif event_type == "USER_DELETED":
            self._apply_user_deleted(event_data)

        self.version += 1

    def _apply_user_created(self, data: Dict[str, Any]):
        self.name = data["name"]
        self.email = data["email"]
        self.created_at = datetime.fromisoformat(data["created_at"])

    def _apply_user_updated(self, data: Dict[str, Any]):
        if "name" in data:
            self.name = data["name"]
        if "email" in data:
            self.email = data["email"]
        self.updated_at = datetime.fromisoformat(data["updated_at"])

    def _apply_user_deleted(self, data: Dict[str, Any]):
        self.is_deleted = True
```

## Event-Sourced Commands

### Create User Command
```python
@fraiseql.mutation
async def create_user_es(info, name: str, email: str) -> User:
    """Event-sourced user creation"""
    repo = info.context["repo"]
    user_id = str(uuid4())

    # Create event
    event_data = {
        "user_id": user_id,
        "name": name,
        "email": email,
        "created_at": datetime.now().isoformat()
    }

    # Store event
    event_id = await repo.call_function(
        "append_event",
        p_stream_id=user_id,
        p_event_type="USER_CREATED",
        p_event_data=event_data,
        p_created_by=info.context.get("user", {}).get("id")
    )

    # Update read model
    await repo.call_function("update_user_projection", p_user_id=user_id)

    # Return from read model
    result = await repo.find_one("v_user", where={"id": user_id})
    return User(**result)
```

### Update User Command
```python
@fraiseql.mutation
async def update_user_es(info, user_id: ID, name: str | None = None, email: str | None = None) -> User:
    """Event-sourced user update"""
    repo = info.context["repo"]

    # Build event data with only changed fields
    event_data = {"user_id": user_id, "updated_at": datetime.now().isoformat()}
    if name is not None:
        event_data["name"] = name
    if email is not None:
        event_data["email"] = email

    # Append event
    await repo.call_function(
        "append_event",
        p_stream_id=user_id,
        p_event_type="USER_UPDATED",
        p_event_data=event_data,
        p_created_by=info.context.get("user", {}).get("id")
    )

    # Update projection
    await repo.call_function("update_user_projection", p_user_id=user_id)

    # Return updated state
    result = await repo.find_one("v_user", where={"id": user_id})
    return User(**result)
```

## Read Model Projections

### User Projection
```sql
-- Projection table
CREATE TABLE proj_user (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP,
    version INTEGER NOT NULL DEFAULT 0,
    is_deleted BOOLEAN DEFAULT FALSE
);

-- Update projection function
CREATE OR REPLACE FUNCTION update_user_projection(p_user_id UUID)
RETURNS VOID AS $$
DECLARE
    event_record RECORD;
    current_state proj_user%ROWTYPE;
BEGIN
    -- Get current projection state
    SELECT * INTO current_state FROM proj_user WHERE id = p_user_id;

    -- If projection doesn't exist, initialize it
    IF current_state.id IS NULL THEN
        current_state.id := p_user_id;
        current_state.version := 0;
        current_state.is_deleted := FALSE;
    END IF;

    -- Apply all events since last version
    FOR event_record IN
        SELECT event_type, event_data, event_version
        FROM tb_events
        WHERE stream_id = p_user_id
        AND event_version > current_state.version
        ORDER BY event_version
    LOOP
        -- Apply event based on type
        CASE event_record.event_type
            WHEN 'USER_CREATED' THEN
                current_state.name := event_record.event_data->>'name';
                current_state.email := event_record.event_data->>'email';
                current_state.created_at := (event_record.event_data->>'created_at')::timestamp;

            WHEN 'USER_UPDATED' THEN
                IF event_record.event_data ? 'name' THEN
                    current_state.name := event_record.event_data->>'name';
                END IF;
                IF event_record.event_data ? 'email' THEN
                    current_state.email := event_record.event_data->>'email';
                END IF;
                current_state.updated_at := (event_record.event_data->>'updated_at')::timestamp;

            WHEN 'USER_DELETED' THEN
                current_state.is_deleted := TRUE;
        END CASE;

        current_state.version := event_record.event_version;
    END LOOP;

    -- Upsert projection
    INSERT INTO proj_user (id, name, email, created_at, updated_at, version, is_deleted)
    VALUES (current_state.id, current_state.name, current_state.email,
            current_state.created_at, current_state.updated_at,
            current_state.version, current_state.is_deleted)
    ON CONFLICT (id) DO UPDATE SET
        name = EXCLUDED.name,
        email = EXCLUDED.email,
        created_at = EXCLUDED.created_at,
        updated_at = EXCLUDED.updated_at,
        version = EXCLUDED.version,
        is_deleted = EXCLUDED.is_deleted;
END;
$$ LANGUAGE plpgsql;
```

### Read Model View
```sql
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at,
        'updated_at', updated_at,
        'version', version
    ) AS data
FROM proj_user
WHERE is_deleted = FALSE;
```

## Time Travel Queries

### Point-in-Time Reconstruction
```python
@fraiseql.query
async def user_at_time(info, user_id: ID, timestamp: datetime) -> User | None:
    """Get user state at specific point in time"""
    repo = info.context["repo"]

    # Get events up to timestamp
    events = await repo.execute(
        """
        SELECT event_type, event_data, event_version
        FROM tb_events
        WHERE stream_id = $1 AND created_at <= $2
        ORDER BY event_version
        """,
        user_id, timestamp
    )

    if not events:
        return None

    # Reconstruct state
    aggregate = UserAggregate(user_id)
    for event in events:
        aggregate.apply_event(event["event_type"], event["event_data"])

    if aggregate.is_deleted:
        return None

    return User(
        id=aggregate.id,
        name=aggregate.name,
        email=aggregate.email,
        created_at=aggregate.created_at,
        updated_at=aggregate.updated_at
    )
```

### Audit Trail Query
```python
@fraiseql.query
async def user_audit_trail(info, user_id: ID, limit: int = 50) -> list[AuditEvent]:
    """Get complete audit trail for user"""
    repo = info.context["repo"]

    events = await repo.execute(
        """
        SELECT
            event_type,
            event_data,
            created_at,
            created_by,
            metadata
        FROM tb_events
        WHERE stream_id = $1
        ORDER BY event_version DESC
        LIMIT $2
        """,
        user_id, limit
    )

    return [
        AuditEvent(
            event_type=event["event_type"],
            data=event["event_data"],
            timestamp=event["created_at"],
            user_id=event["created_by"],
            metadata=event["metadata"]
        )
        for event in events
    ]
```

## Snapshot Optimization

### Snapshot Table
```sql
-- For performance optimization
CREATE TABLE tb_snapshots (
    stream_id UUID NOT NULL,
    snapshot_version INTEGER NOT NULL,
    snapshot_data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),

    PRIMARY KEY (stream_id, snapshot_version)
);
```

### Create Snapshots
```sql
CREATE OR REPLACE FUNCTION create_snapshot(
    p_stream_id UUID,
    p_version INTEGER,
    p_data JSONB
) RETURNS VOID AS $$
BEGIN
    INSERT INTO tb_snapshots (stream_id, snapshot_version, snapshot_data)
    VALUES (p_stream_id, p_version, p_data)
    ON CONFLICT (stream_id, snapshot_version) DO UPDATE
    SET snapshot_data = EXCLUDED.snapshot_data;

    -- Clean old snapshots (keep last 5)
    DELETE FROM tb_snapshots
    WHERE stream_id = p_stream_id
    AND snapshot_version < p_version - 5;
END;
$$ LANGUAGE plpgsql;
```

## Event Sourcing Benefits

### Complete Audit Trail

- Every change is recorded with timestamp and user
- Full history available for compliance and debugging
- Immutable event log prevents data tampering

### Time Travel Capabilities

- Reconstruct any past state
- Debug issues by examining historical states
- Temporal queries and analysis

### Flexible Read Models

- Multiple projections from same events
- Add new read models without data migration
- Optimized views for different use cases

## Best Practices

### Event Design
```python
# ✅ Good: Immutable events with all necessary data
@dataclass
class PostPublished:
    post_id: ID
    author_id: ID
    title: str
    published_at: datetime
    tags: list[str]

# ❌ Bad: Mutable or incomplete events
@dataclass
class PostChanged:
    post_id: ID
    # Missing: what changed? when? by whom?
```

### Versioning Strategy
```python
# Handle event schema evolution
def apply_event(self, event_type: str, event_data: dict, version: int = 1):
    if event_type == "USER_CREATED":
        if version == 1:
            self._apply_user_created_v1(event_data)
        elif version == 2:
            self._apply_user_created_v2(event_data)
```

### Performance Considerations

- Use snapshots for long event streams
- Index events by stream_id and created_at
- Consider event archival for old streams
- Batch projection updates when possible

## See Also

### Related Concepts

- [**CQRS Implementation**](cqrs.md) - Command Query Responsibility Segregation
- [**Audit Logging**](../security.md#audit-logging) - Security audit trails
- [**Database Views**](../core-concepts/database-views.md) - Read model patterns

### Implementation

- [**PostgreSQL Functions**](../mutations/postgresql-function-based.md) - Command implementation
- [**Testing Event Sourced Systems**](../testing/integration-testing.md) - Testing strategies
- [**Performance Tuning**](performance.md) - Event store optimization

### Advanced Topics

- [**Bounded Contexts**](bounded-contexts.md) - Context boundaries
- [**Domain-Driven Design**](database-api-patterns.md) - DDD patterns
- [**Multi-tenancy**](multi-tenancy.md) - Multi-tenant event stores
