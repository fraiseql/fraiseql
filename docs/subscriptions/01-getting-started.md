# Getting Started with GraphQL Subscriptions

Welcome! This guide will help you set up and use GraphQL subscriptions in FraiseQL within 5 minutes.

---

## What Are GraphQL Subscriptions?

GraphQL subscriptions allow clients to **subscribe to real-time data changes**. When an event occurs (user comes online, message sent, data updated), all subscribed clients receive the update immediately via WebSocket.

**Key Benefits**:
- 🚀 Real-time updates without polling
- 🔒 Built-in security filtering by user/tenant
- ⚡ High performance (<10ms end-to-end latency)
- 🎯 Framework-agnostic (works with FastAPI, Starlette, custom servers)

---

## Installation

FraiseQL subscriptions are included in the base package. No additional installation needed!

```bash
pip install fraiseql
```

**Requirements**:
- Python 3.13+
- asyncio support (included in Python)
- (Optional) Redis for distributed deployments
- (Optional) PostgreSQL for LISTEN/NOTIFY-based events

---

## 5-Minute Quick Start

### Step 1: Create the Subscription Manager

```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Create manager with in-memory event bus
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)
```

### Step 2: Define a Resolver Function

A resolver transforms events into subscription responses:

```python
async def user_subscription(event, variables):
    """Transform a user event into a subscription response."""
    return {
        "user": {
            "id": str(event.get("id")),
            "name": event.get("name"),
            "status": "online"
        }
    }
```

### Step 3: Register a Subscription

When a client subscribes, register it:

```python
await manager.create_subscription(
    subscription_id="sub_user_123",          # Unique ID for this subscription
    connection_id="ws_456",                  # WebSocket connection ID
    query="subscription { user { id name status } }",  # GraphQL query
    variables={},                           # Query variables
    resolver_fn=user_subscription,          # Your resolver function
    user_id="user_123",                     # Who is subscribing
    tenant_id="tenant_abc"                  # Which organization
)
```

### Step 4: Publish Events

When something happens, publish an event:

```python
await manager.publish_event(
    event_type="userOnline",               # Type of event
    channel="users",                       # Event channel
    data={"id": "123", "name": "Alice"}   # Event data
)
```

### Step 5: Receive Updates

Get the next event for a subscription:

```python
response = await manager.get_next_event("sub_user_123")

if response:
    import json
    data = json.loads(response)
    print(data)
    # Output: {"type": "next", "id": "sub_user_123", "payload": {"data": {"user": ...}}}
```

### Step 6: Clean Up

When the subscription is done, complete it:

```python
await manager.complete_subscription("sub_user_123")
```

---

## Key Concepts

### Event Bus
The central pub/sub system that routes events to subscriptions. Options:
- **Memory**: Good for development/testing
- **Redis**: For distributed deployments
- **PostgreSQL**: Using LISTEN/NOTIFY

### Subscription
A client listening for events on a channel. Each subscription:
- Has a unique ID
- Is tied to a WebSocket connection
- Runs a resolver function for each event
- Is filtered by user/tenant security

### Resolver
A Python async function that:
- Receives raw event data
- Applies business logic
- Returns data matching the GraphQL query shape
- Runs in <100ms for best performance

### Channel
A named stream of events:
- Simple names like `"users"`, `"orders"`, `"notifications"`
- Events published to channels are distributed to all subscribers
- One subscription can listen to multiple channels

### Security
Built-in filtering ensures users only see data they're authorized for:
- Events are filtered by `user_id` and `tenant_id`
- Resolvers can apply additional security checks
- Rate limiting prevents abuse

---

## Complete Example: Real-Time Chat

Here's a complete example showing how subscriptions work end-to-end:

```python
import asyncio
import json
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Setup
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)

# Resolver for messages
async def message_resolver(event, variables):
    return {
        "message": {
            "id": event.get("id"),
            "text": event.get("text"),
            "author": event.get("author")
        }
    }

async def main():
    # User 1 subscribes to messages
    await manager.create_subscription(
        subscription_id="sub_user1",
        connection_id="ws_1",
        query="subscription { message { id text author } }",
        variables={},
        resolver_fn=message_resolver,
        user_id="user_1",
        tenant_id="tenant_1"
    )

    # User 2 subscribes to messages
    await manager.create_subscription(
        subscription_id="sub_user2",
        connection_id="ws_2",
        query="subscription { message { id text author } }",
        variables={},
        resolver_fn=message_resolver,
        user_id="user_2",
        tenant_id="tenant_1"
    )

    # User 1 sends a message
    await manager.publish_event(
        event_type="messagePosted",
        channel="messages",
        data={"id": "1", "text": "Hello!", "author": "user_1"}
    )

    # Both users receive the message
    response1 = await manager.get_next_event("sub_user1")
    response2 = await manager.get_next_event("sub_user2")

    print("User 1 received:", json.loads(response1))
    print("User 2 received:", json.loads(response2))

    # User 1 sends another message
    await manager.publish_event(
        event_type="messagePosted",
        channel="messages",
        data={"id": "2", "text": "How are you?", "author": "user_1"}
    )

    response2 = await manager.get_next_event("sub_user2")
    print("User 2 received:", json.loads(response2))

    # Cleanup
    await manager.complete_subscription("sub_user1")
    await manager.complete_subscription("sub_user2")

# Run it
asyncio.run(main())
```

---

## Framework Integration

### FastAPI

```python
from fastapi import FastAPI, WebSocket
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

app = FastAPI()
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)

@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    # Handle subscription messages...
```

See `examples/subscriptions/fastapi_example.py` for a complete working app.

### Starlette

```python
from starlette.applications import Starlette
from starlette.routing import WebSocketRoute
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

async def websocket_endpoint(websocket):
    await websocket.accept()
    # Handle subscription messages...

routes = [
    WebSocketRoute("/graphql/subscriptions", websocket_endpoint)
]

app = Starlette(routes=routes)
```

See `examples/subscriptions/starlette_example.py` for a complete working app.

### Custom Framework

If you're using a different framework, implement a simple WebSocket adapter:

```python
class MyFrameworkAdapter:
    async def handle_subscription_message(self, websocket, message):
        # Parse incoming message
        # Call manager.create_subscription() or manager.publish_event()
        # Send response back through websocket
        pass
```

See `examples/subscriptions/custom_adapter.py` for a template.

---

## Common Patterns

### Pattern 1: Real-Time Notifications

```python
# Client subscribes
await manager.create_subscription(
    subscription_id="notifications",
    connection_id=websocket_id,
    query="subscription { notification { id message } }",
    variables={},
    resolver_fn=notification_resolver,
    user_id=current_user.id,
    tenant_id=current_user.tenant_id
)

# Server publishes when something happens
await manager.publish_event(
    event_type="userMentioned",
    channel="notifications",
    data={"id": "n1", "message": "You were mentioned"}
)
```

### Pattern 2: Live Data Updates

```python
# Subscribe to inventory changes
await manager.create_subscription(
    subscription_id=f"inventory_{user_id}",
    connection_id=websocket_id,
    query="subscription { inventory { sku quantity } }",
    variables={},
    resolver_fn=inventory_resolver,
    user_id=user_id,
    tenant_id=tenant_id
)

# Publish when inventory changes
await manager.publish_event(
    event_type="inventoryUpdated",
    channel="inventory",
    data={"sku": "ABC123", "quantity": 42}
)
```

### Pattern 3: Multi-Channel Subscriptions

```python
# Subscribe to multiple event types
await manager.create_subscription(
    subscription_id="all_events",
    connection_id=websocket_id,
    query="subscription { event { type data } }",
    variables={},
    resolver_fn=multi_event_resolver,
    user_id=user_id,
    tenant_id=tenant_id
)

# Publish to different channels
await manager.publish_event(event_type="a", channel="events", data={...})
await manager.publish_event(event_type="b", channel="events", data={...})
```

---

## Deployment Options

### Development
Use in-memory event bus:
```python
config = _fraiseql_rs.PyEventBusConfig.memory()
```

### Production (Single Server)
Use in-memory with persistence:
```python
config = _fraiseql_rs.PyEventBusConfig.memory()
# Or with PostgreSQL for persistence
```

### Production (Multiple Servers)
Use Redis for distributed deployments:
```python
config = _fraiseql_rs.PyEventBusConfig.redis(
    host="localhost",
    port=6379,
    db=0
)
```

Or PostgreSQL with LISTEN/NOTIFY:
```python
config = _fraiseql_rs.PyEventBusConfig.postgresql(
    connection_string="postgresql://user:pass@host/db"
)
```

See `04-deployment.md` for detailed deployment guide.

---

## Error Handling

All errors inherit from `SubscriptionError`:

```python
from fraiseql.subscriptions.manager import SubscriptionError

try:
    await manager.create_subscription(...)
except SubscriptionError.InvalidQuery as e:
    logger.error(f"GraphQL error: {e}")
except SubscriptionError.AuthorizationFailed as e:
    logger.error(f"User not authorized: {e}")
except SubscriptionError.RateLimited as e:
    logger.warning(f"Rate limited: {e}")
```

---

## Performance Tips

1. **Keep resolvers fast** (<100ms)
   - Avoid blocking I/O
   - Use async for everything
   - Cache computed values

2. **Use appropriate event bus**
   - Memory for dev/testing
   - Redis for distributed
   - PostgreSQL for persistence

3. **Monitor performance**
   - Track resolver execution time
   - Monitor memory usage
   - Check event throughput

4. **Optimize resolver functions**
   ```python
   # ✅ Good - fast and async
   async def fast_resolver(event, variables):
       return {"data": event.get("data")}

   # ❌ Bad - blocks the event loop
   def slow_resolver(event, variables):
       time.sleep(0.1)  # Blocks!
       return {"data": event.get("data")}
   ```

---

## Next Steps

- **Full API Reference**: See `02-api-reference.md`
- **FastAPI Example**: See `examples/subscriptions/fastapi_example.py`
- **Starlette Example**: See `examples/subscriptions/starlette_example.py`
- **Deployment Guide**: See `05-deployment.md`
- **Troubleshooting**: See `06-troubleshooting.md`

---

## FAQ

**Q: Do I need WebSockets?**
A: Yes, WebSockets enable bidirectional real-time communication. GraphQL subscriptions over HTTP polling is not recommended.

**Q: Can I use subscriptions with REST APIs?**
A: Subscriptions are GraphQL-specific. Use webhooks or polling for REST APIs.

**Q: How many concurrent subscriptions can I handle?**
A: With memory event bus: 1000+. With Redis: scales to 100,000+ across multiple servers.

**Q: Are subscriptions secure?**
A: Yes! Built-in filtering by user_id and tenant_id. Resolvers can apply additional checks.

**Q: What happens when a client disconnects?**
A: Call `complete_subscription()` to clean up. The subscription is removed and resources freed.

---

## Feedback

Have questions or found issues? Check `06-troubleshooting.md` or file a GitHub issue.

Happy building! 🚀
