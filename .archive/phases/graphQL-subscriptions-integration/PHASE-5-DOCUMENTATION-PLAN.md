# Phase 5: Documentation & Examples - Implementation Plan

**Phase**: 5
**Objective**: Complete documentation and working examples for GraphQL subscriptions
**Estimated Time**: 1 week / 25 hours
**Files to Create**: 6 documentation files + 4 example files
**Success Criteria**: Comprehensive user-facing documentation with working examples
**Lead Engineer**: Senior Technical Writer / Developer Advocate

---

## Context

Phase 5 documents the complete, tested subscription system from Phases 1-4. Users need:
- Clear getting started guide
- Complete API reference
- Working examples for all frameworks
- Deployment guidance
- Troubleshooting help

**Deliverables**:
- User documentation (5 files, ~2000 lines)
- Working code examples (4 files, ~400 lines)
- Integration guides for FastAPI, Starlette
- Performance documentation
- API reference

---

## Phase 4 Completion Status

### Accomplished in Phases 1-4
- ✅ Rust core implementation (subscription executor, event bus, security)
- ✅ PyO3 bindings for Python integration
- ✅ Python high-level API (SubscriptionManager)
- ✅ Framework adapters (FastAPI, Starlette integration)
- ✅ Protocol handlers (GraphQL transport WS)
- ✅ Security integration (5 modules)
- ✅ Rate limiting enforcement
- ✅ Python resolver support
- ✅ All Clippy warnings fixed (24 warnings → 0 warnings)
- ✅ Type-safe code throughout
- ✅ ~1,700 lines of production Python code
- ✅ Phase 3 tests: 22/22 passing
- ✅ Library compilation: Clean

### Ready for Documentation
The system is complete and tested. Phase 5 is purely documentation.

---

## Files to Create/Modify

### New Documentation Files

1. **`docs/subscriptions/01-getting-started.md`** (~400 lines)
   - Installation instructions
   - Quick start example
   - Basic concept overview
   - Common use cases

2. **`docs/subscriptions/02-api-reference.md`** (~600 lines)
   - SubscriptionManager API
   - Protocol reference
   - Configuration options
   - Type definitions

3. **`docs/subscriptions/03-examples.md`** (~300 lines)
   - FastAPI integration
   - Starlette integration
   - Custom adapter
   - Real-world scenarios

4. **`docs/subscriptions/04-architecture.md`** (~200 lines)
   - System architecture overview
   - Component responsibilities
   - Data flow diagrams (ASCII)
   - Performance characteristics

5. **`docs/subscriptions/05-deployment.md`** (~200 lines)
   - Production deployment
   - Configuration
   - Scaling considerations
   - Monitoring

6. **`docs/subscriptions/06-troubleshooting.md`** (~300 lines)
   - Common issues
   - Debugging guide
   - Performance optimization
   - FAQ

### New Example Files

1. **`examples/subscriptions/fastapi_example.py`** (~150 lines)
   - Complete FastAPI app
   - Subscription resolver
   - Event publishing
   - Error handling

2. **`examples/subscriptions/starlette_example.py`** (~150 lines)
   - Complete Starlette app
   - Same functionality as FastAPI
   - Shows framework independence

3. **`examples/subscriptions/custom_adapter.py`** (~100 lines)
   - Custom WebSocket adapter
   - Template for new frameworks
   - Required interface

4. **`examples/subscriptions/real_world_chat.py`** (~100 lines)
   - Chat application example
   - Real-time messages
   - User presence
   - Error recovery

---

## Detailed Implementation Tasks

### Task 5.1: User Getting Started Guide (8 hours)

**File**: `docs/subscriptions/01-getting-started.md`

**Content Sections**:

```markdown
# Getting Started with GraphQL Subscriptions

## Installation
- pip install fraiseql (already includes subscriptions)
- Requirements: Python 3.13+, asyncio support

## Quick Start (5 minutes)

### 1. Basic Setup
```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Create manager with memory event bus
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)
```

### 2. Define Resolver
```python
async def user_subscription(event, variables):
    return {
        "user": {
            "id": event["id"],
            "name": event["name"],
            "status": "active"
        }
    }
```

### 3. Register Subscription
```python
await manager.create_subscription(
    subscription_id="sub1",
    connection_id="conn1",
    query="subscription { user { id name status } }",
    variables={},
    resolver_fn=user_subscription,
    user_id="user123",
    tenant_id="tenant1"
)
```

### 4. Publish Events
```python
await manager.publish_event(
    event_type="userOnline",
    channel="users",
    data={"id": "123", "name": "Alice"}
)
```

### 5. Receive Response
```python
response = await manager.get_next_event("sub1")
print(response)  # JSON bytes with user data
```

## Key Concepts
- Event Bus: Central pub/sub system (memory or Redis)
- Subscription: User listening for events on a channel
- Resolver: Python function that transforms events
- Channel: Named event stream (e.g., "users", "orders")
- Security: Built-in filtering by user/tenant

## Next Steps
- See FastAPI integration in examples/
- Full API reference in docs/api-reference.md
- Deployment guide in docs/deployment.md
```

**Sections to Include**:
- Installation and setup
- 5-minute quick start
- Core concepts explained
- Framework choices (FastAPI vs Starlette vs custom)
- Common patterns
- Next steps link

---

### Task 5.2: API Reference Documentation (12 hours)

**File**: `docs/subscriptions/02-api-reference.md`

**Content Structure**:

```markdown
# API Reference

## SubscriptionManager

The main interface for managing subscriptions.

### Methods

#### create_subscription()
```python
async def create_subscription(
    subscription_id: str,
    connection_id: str,
    query: str,
    variables: dict,
    resolver_fn: Callable,
    user_id: str,
    tenant_id: str
) -> None
```

**Parameters**:
- `subscription_id`: Unique subscription identifier
- `connection_id`: WebSocket connection ID
- `query`: GraphQL subscription query
- `variables`: Query variables
- `resolver_fn`: Python async function that transforms events
- `user_id`: User making the subscription
- `tenant_id`: Tenant/organization ID

**Returns**: None

**Raises**:
- `SubscriptionError.InvalidQuery`: If query is malformed
- `SubscriptionError.AuthorizationFailed`: If user lacks permission

**Example**:
```python
await manager.create_subscription(
    subscription_id="sub_user_123",
    connection_id="ws_456",
    query="subscription { users { id name } }",
    variables={},
    resolver_fn=my_resolver,
    user_id="user_123",
    tenant_id="tenant_abc"
)
```

#### publish_event()
```python
async def publish_event(
    event_type: str,
    channel: str,
    data: dict
) -> None
```

**Parameters**:
- `event_type`: Type of event (e.g., "userCreated")
- `channel`: Event channel name (e.g., "users")
- `data`: Event data as dict

**Returns**: None

**Raises**: None (failures logged)

**Example**:
```python
await manager.publish_event(
    event_type="userOnline",
    channel="users",
    data={"id": "123", "name": "Alice"}
)
```

#### get_next_event()
```python
async def get_next_event(
    subscription_id: str
) -> Optional[bytes]
```

**Parameters**:
- `subscription_id`: Subscription to get event for

**Returns**: JSON bytes with event data, or None if no event

**Example**:
```python
response = await manager.get_next_event("sub_user_123")
if response:
    data = json.loads(response)
    print(data["payload"]["data"])
```

#### complete_subscription()
```python
async def complete_subscription(
    subscription_id: str
) -> None
```

**Parameters**:
- `subscription_id`: Subscription to complete

**Returns**: None

**Example**:
```python
await manager.complete_subscription("sub_user_123")
```

## Error Handling

All errors inherit from `SubscriptionError`:

```python
class SubscriptionError(Exception):
    InvalidQuery(msg: str)
    AuthorizationFailed(msg: str)
    RateLimited(msg: str)
    ConnectionClosed(msg: str)
```

**Example**:
```python
try:
    await manager.create_subscription(...)
except SubscriptionError.InvalidQuery as e:
    logger.error(f"GraphQL error: {e}")
except SubscriptionError.AuthorizationFailed as e:
    logger.error(f"User not authorized: {e}")
```

## Configuration

### Event Bus Configuration

**Memory Event Bus**:
```python
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)
```

**Redis Event Bus** (distributed):
```python
config = _fraiseql_rs.PyEventBusConfig.redis(
    host="localhost",
    port=6379,
    db=0
)
manager = SubscriptionManager(config)
```

**PostgreSQL Event Bus** (LISTEN/NOTIFY):
```python
config = _fraiseql_rs.PyEventBusConfig.postgresql(
    connection_string="postgresql://user:pass@host/db"
)
manager = SubscriptionManager(config)
```

## Resolver Functions

Resolvers are Python async functions that transform events:

```python
async def my_resolver(event: dict, variables: dict) -> dict:
    # event: Raw event data
    # variables: GraphQL query variables
    # return: Dict matching subscription query shape

    return {
        "user": {
            "id": event["id"],
            "name": event["name"].upper(),  # Transform
            "timestamp": time.time()
        }
    }
```

**Requirements**:
- Must be async
- Accept (event, variables) parameters
- Return dict matching query structure
- Handle None/missing data gracefully

**Performance**:
- Should complete in <100ms
- Avoid blocking operations
- Use async for I/O

## Protocol Reference

### Connection Flow

1. Client connects via WebSocket
2. Send `{"type": "connection_init", "payload": {}}`
3. Server responds with `{"type": "connection_ack"}`
4. Client sends subscription: `{"type": "subscribe", "id": "1", "payload": {...}}`
5. Server sends events: `{"type": "next", "id": "1", "payload": {...}}`
6. Complete: `{"type": "complete", "id": "1"}`

### Message Types

- `connection_init`: Initialize connection
- `connection_ack`: Connection accepted
- `subscribe`: Subscribe to query
- `next`: Event data
- `error`: Error message
- `complete`: Subscription complete

---

## Types and Constants

### SubscriptionId
Type: `str`
Unique subscription identifier. Use UUID or connection_id + query hash.

### ChannelName
Type: `str`
Event channel for pub/sub. Keep simple (e.g., "users", "orders", "chat").

### EventType
Type: `str`
Event classification (e.g., "userCreated", "orderUpdated").

### TenantId
Type: `str`
Multi-tenant identifier. Required for security filtering.

## Best Practices

1. **Resolver Functions**
   - Keep them fast (<100ms)
   - Avoid external API calls
   - Cache if needed
   - Handle null gracefully

2. **Event Publishing**
   - Use clear channel names
   - Include all relevant data in event
   - Don't publish sensitive data (filtered later)

3. **Error Handling**
   - Catch SubscriptionError exceptions
   - Log all errors for debugging
   - Return sensible defaults

4. **Performance**
   - Reuse SubscriptionManager instance
   - Use connection pooling for Redis/PostgreSQL
   - Monitor memory usage

5. **Security**
   - Always set user_id and tenant_id
   - Let framework handle WebSocket validation
   - Resolvers can do additional filtering
```

---

### Task 5.3: Working Code Examples (7 hours)

**Files**: 4 example implementations

#### Example 1: FastAPI Integration

**File**: `examples/subscriptions/fastapi_example.py`

```python
"""
Complete FastAPI application with GraphQL subscriptions.

Run: uvicorn fastapi_example:app --reload
Connect: ws://localhost:8000/graphql/subscriptions
"""

import asyncio
import json
from fastapi import FastAPI, WebSocket
from fastapi.responses import HTMLResponse
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

app = FastAPI()

# Global subscription manager
config = _fraiseql_rs.PyEventBusConfig.memory()
subscription_manager = SubscriptionManager(config)


# Define resolver for user subscriptions
async def user_subscription_resolver(event, variables):
    """Transform event to subscription response."""
    return {
        "user": {
            "id": str(event.get("id")),
            "name": event.get("name"),
            "status": "online",
            "timestamp": event.get("timestamp")
        }
    }


@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    """GraphQL subscription WebSocket endpoint."""
    await websocket.accept()

    try:
        while True:
            # Receive GraphQL subscription message
            data = await websocket.receive_text()
            message = json.loads(data)

            if message["type"] == "subscribe":
                # Create subscription
                sub_id = message["id"]
                query = message["payload"]["query"]
                variables = message["payload"].get("variables", {})

                await subscription_manager.create_subscription(
                    subscription_id=sub_id,
                    connection_id=websocket.client[0],
                    query=query,
                    variables=variables,
                    resolver_fn=user_subscription_resolver,
                    user_id="user123",
                    tenant_id="tenant1"
                )

                # Send subscription acknowledgment
                await websocket.send_text(json.dumps({
                    "type": "next",
                    "id": sub_id,
                    "payload": {"data": {}}
                }))

            elif message["type"] == "complete":
                # Complete subscription
                sub_id = message["id"]
                await subscription_manager.complete_subscription(sub_id)
                await websocket.send_text(json.dumps({
                    "type": "complete",
                    "id": sub_id
                }))

    except Exception as e:
        print(f"WebSocket error: {e}")
    finally:
        await websocket.close()


@app.post("/api/events")
async def publish_event(event_data: dict):
    """REST endpoint to publish events (for testing)."""
    await subscription_manager.publish_event(
        event_type=event_data["type"],
        channel=event_data["channel"],
        data=event_data["data"]
    )
    return {"status": "published"}


@app.get("/")
async def get_html():
    """Simple HTML client to test subscriptions."""
    return HTMLResponse("""
    <!DOCTYPE html>
    <html>
    <body>
        <h1>GraphQL Subscriptions Demo</h1>
        <button onclick="startSubscription()">Start Subscription</button>
        <button onclick="publishEvent()">Publish Event</button>
        <pre id="output"></pre>

        <script>
        let ws;

        function startSubscription() {
            ws = new WebSocket("ws://localhost:8000/graphql/subscriptions");
            ws.onopen = () => {
                ws.send(JSON.stringify({
                    type: "subscribe",
                    id: "1",
                    payload: {
                        query: "subscription { user { id name status } }"
                    }
                }));
            };
            ws.onmessage = (e) => {
                const msg = JSON.parse(e.data);
                document.getElementById("output").innerText += JSON.stringify(msg, null, 2) + "\n";
            };
        }

        function publishEvent() {
            fetch("/api/events", {
                method: "POST",
                headers: {"Content-Type": "application/json"},
                body: JSON.stringify({
                    type: "userOnline",
                    channel: "users",
                    data: {id: Math.random(), name: "User " + Math.random()}
                })
            });
        }
        </script>
    </body>
    </html>
    """)


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

**Tasks**:
- FastAPI example: Complete working app
- Starlette example: Same but with Starlette
- Custom adapter: Shows how to add support for other frameworks
- Real-world: Chat app with presence

---

### Task 5.4: Architecture & Deployment Guides (5 hours)

**Files**:
- `docs/subscriptions/04-architecture.md` (200 lines)
- `docs/subscriptions/05-deployment.md` (200 lines)

**Architecture Guide Sections**:
- System diagram (ASCII)
- Component responsibilities
- Data flow
- Performance characteristics
- Scalability notes

**Deployment Guide Sections**:
- Production setup
- Redis/PostgreSQL configuration
- Horizontal scaling
- Monitoring and metrics
- Performance tuning
- Security checklist

---

### Task 5.5: Troubleshooting & FAQ (3 hours)

**File**: `docs/subscriptions/06-troubleshooting.md`

**Sections**:
- Common issues (connection failures, timeouts, etc.)
- Debugging techniques
- Performance optimization
- Memory leaks investigation
- Security troubleshooting
- Frequently asked questions

---

## Verification Checklist

### Documentation Quality
- [ ] All documentation is clear and grammatically correct
- [ ] Code examples are runnable and tested
- [ ] API reference is complete and accurate
- [ ] Architecture diagrams are clear
- [ ] Getting started guide works for new users

### Code Examples
- [ ] FastAPI example runs without errors
- [ ] Starlette example works identically
- [ ] Custom adapter template is complete
- [ ] Real-world example demonstrates best practices

### Completeness
- [ ] All major features documented
- [ ] All APIs documented with examples
- [ ] Common patterns shown
- [ ] Common issues addressed
- [ ] Performance tips included

### Accuracy
- [ ] All code examples match actual implementation
- [ ] Architecture description matches implementation
- [ ] Performance claims are verified
- [ ] Security features accurately described

---

## Success Criteria for Phase 5

When Phase 5 is complete:

**User-Facing Documentation**:
- ✅ Getting started guide usable by new developers
- ✅ Complete API reference with examples
- ✅ Architecture documented and understood
- ✅ Deployment guide covers all scenarios

**Working Examples**:
- ✅ FastAPI example runs immediately
- ✅ Starlette example works identically
- ✅ Custom adapter template is reusable
- ✅ Real-world example shows best practices

**Quality Standards**:
- ✅ Documentation is clear and complete
- ✅ Code examples are tested and verified
- ✅ No broken links or references
- ✅ Consistent formatting and style

---

## Next Steps After Phase 5

1. **Release Preparation**
   - Update main README
   - Add subscriptions to feature list
   - Create release notes

2. **User Outreach**
   - Publish blog post
   - Create tutorial videos
   - Announce on social media

3. **Monitoring**
   - Gather user feedback
   - Fix documentation issues
   - Improve examples based on usage

---

## Time Estimate Breakdown

- Task 5.1: Getting started (8 hours)
- Task 5.2: API reference (12 hours)
- Task 5.3: Code examples (7 hours)
- Task 5.4: Architecture & deployment (5 hours)
- Task 5.5: Troubleshooting (3 hours)

**Total: 35 hours (approximately 1 week)**

---

## Files Checklist

**Documentation Files**:
- [ ] `docs/subscriptions/01-getting-started.md`
- [ ] `docs/subscriptions/02-api-reference.md`
- [ ] `docs/subscriptions/04-architecture.md`
- [ ] `docs/subscriptions/05-deployment.md`
- [ ] `docs/subscriptions/06-troubleshooting.md`
- [ ] `docs/subscriptions/INDEX.md` (navigation)

**Example Files**:
- [ ] `examples/subscriptions/fastapi_example.py`
- [ ] `examples/subscriptions/starlette_example.py`
- [ ] `examples/subscriptions/custom_adapter.py`
- [ ] `examples/subscriptions/real_world_chat.py`

**Integration**:
- [ ] Update main `README.md`
- [ ] Add subscriptions to docs index
- [ ] Create examples README
- [ ] Update CHANGELOG

---

## Dependencies & Blockers

**Prerequisites**:
- Phases 1-4 complete and tested ✅
- All code working and deployable ✅
- Examples tested and verified

**Help Needed**:
- Technical writer for polish (optional)
- UX review of examples
- Performance validation

---

**Status**: Ready for Phase 5 implementation
**Timeline**: 1 week to complete
**Dependency**: Phases 1-4 must be complete (they are ✅)
