# Beta Development Log: Subscriptions Planning
**Date**: 2025-01-16  
**Time**: 19:15 UTC  
**Session**: 002  
**Author**: Backend Team Lead (under Viktor's supervision)

## Objective
Design and plan WebSocket-based GraphQL subscriptions for FraiseQL with PostgreSQL LISTEN/NOTIFY integration.

## Technical Analysis

### Current Architecture Gaps
1. No WebSocket support in FastAPI integration
2. No subscription types in schema builder
3. No PostgreSQL NOTIFY trigger system
4. No connection state management

### Proposed Architecture

```
Client <--> WebSocket <--> Subscription Manager <--> PostgreSQL LISTEN/NOTIFY
                              |
                              +-> Connection Registry
                              +-> Authorization Layer
                              +-> Event Router
```

### Implementation Phases

#### Phase 1: WebSocket Foundation (Week 1-2)
- [ ] Add WebSocket endpoint to FastAPI app
- [ ] Create subscription connection manager
- [ ] Implement basic ping/pong keepalive
- [ ] Add connection registry with TTL

#### Phase 2: GraphQL Integration (Week 2-3)
- [ ] Add `@subscription` decorator
- [ ] Extend schema builder for subscription types
- [ ] Create subscription resolver pattern
- [ ] Implement subscription context

#### Phase 3: PostgreSQL Integration (Week 3-4)
- [ ] Create LISTEN/NOTIFY wrapper
- [ ] Add trigger generation utilities
- [ ] Implement event routing system
- [ ] Handle connection recovery

#### Phase 4: Production Features (Week 4-5)
- [ ] Add subscription authorization
- [ ] Implement rate limiting
- [ ] Add monitoring/metrics
- [ ] Create debugging tools

## API Design

### Subscription Decorator
```python
from fraiseql import subscription

@subscription
async def task_updates(info, project_id: UUID):
    """Subscribe to task updates for a project."""
    # Verify access
    if not await has_project_access(info.context["user"], project_id):
        raise PermissionError("No access to project")
    
    # Return async generator
    async for event in task_event_stream(project_id):
        yield event
```

### PostgreSQL Triggers
```python
# Auto-generated triggers
CREATE OR REPLACE FUNCTION notify_task_update() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify(
        'task_updates',
        json_build_object(
            'project_id', NEW.project_id,
            'task_id', NEW.id,
            'operation', TG_OP,
            'data', row_to_json(NEW)
        )::text
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

## Technical Decisions

1. **Use GraphQL-WS Protocol** (not legacy subscriptions-transport-ws)
2. **PostgreSQL LISTEN/NOTIFY** for event bus (vs Redis pub/sub)
3. **AsyncIO-based** connection handling
4. **JWT auth** for subscription connections
5. **Automatic reconnection** with exponential backoff

## Performance Considerations
- Connection pooling separate from query pool
- Event debouncing for high-frequency updates
- Subscription query complexity limits
- Maximum connections per user

## Testing Strategy
- Unit tests for each component
- Integration tests with real PostgreSQL
- Load tests with 1000+ concurrent subscriptions
- Chaos testing for connection drops

## Viktor's Review Notes
"Subscriptions are where amateur frameworks die. One memory leak, one unhandled disconnection, and your server is toast. Build it like a tank, test it like a rocket."

## Next Steps
1. Create subscription branch
2. Implement basic WebSocket endpoint
3. Design connection registry
4. Write comprehensive tests

---
Next Log: Query optimization and DataLoader pattern