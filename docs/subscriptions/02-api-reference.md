# API Reference - GraphQL Subscriptions

Complete reference for the SubscriptionManager API and configuration options.

---

## SubscriptionManager

The main interface for managing GraphQL subscriptions.

### Constructor

```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)
```

**Parameters**:
- `config`: Event bus configuration (see Configuration section below)

---

## Core Methods

### create_subscription()

Register a new subscription.

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

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `subscription_id` | `str` | ✅ | Unique subscription identifier. Use UUID or connection_id + hash. |
| `connection_id` | `str` | ✅ | WebSocket connection ID from your framework. |
| `query` | `str` | ✅ | GraphQL subscription query string. |
| `variables` | `dict` | ✅ | GraphQL query variables (empty dict `{}` if none). |
| `resolver_fn` | `Callable` | ✅ | Python async function `async def resolver(event, variables)`. |
| `user_id` | `str` | ✅ | User making the subscription (for security filtering). |
| `tenant_id` | `str` | ✅ | Tenant/organization ID (for multi-tenant isolation). |

**Returns**: `None`

**Raises**:
- `SubscriptionError.InvalidQuery`: GraphQL query is malformed or invalid
- `SubscriptionError.AuthorizationFailed`: User lacks permission
- `SubscriptionError.RateLimited`: User has exceeded rate limit

**Example**:

```python
async def my_resolver(event, variables):
    return {
        "user": {
            "id": event.get("id"),
            "name": event.get("name"),
            "status": "online"
        }
    }

await manager.create_subscription(
    subscription_id="sub_user_123",
    connection_id="ws_456",
    query="subscription { user { id name status } }",
    variables={},
    resolver_fn=my_resolver,
    user_id="user_123",
    tenant_id="tenant_abc"
)
```

**Notes**:
- Subscription is immediately active after creation
- Resolver function must be async
- Resolver should complete in <100ms for best performance
- Same subscription_id cannot be registered twice

---

### publish_event()

Publish an event to all subscriptions on a channel.

```python
async def publish_event(
    event_type: str,
    channel: str,
    data: dict
) -> None
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `event_type` | `str` | ✅ | Type of event (e.g., "userCreated", "messagePosted"). |
| `channel` | `str` | ✅ | Event channel name (e.g., "users", "messages", "notifications"). |
| `data` | `dict` | ✅ | Event data as dictionary. Will be passed to resolver functions. |

**Returns**: `None`

**Raises**: Generally does not raise (failures are logged)

**Example**:

```python
await manager.publish_event(
    event_type="userOnline",
    channel="users",
    data={
        "id": "123",
        "name": "Alice",
        "timestamp": datetime.now().isoformat()
    }
)
```

**Notes**:
- Event is delivered to all subscriptions on the channel
- Delivery is concurrent (fast)
- Don't put sensitive data in event - let resolver filter it
- Each event triggers the resolver function

---

### get_next_event()

Get the next event response for a subscription.

```python
async def get_next_event(subscription_id: str) -> Optional[bytes]
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `subscription_id` | `str` | ✅ | ID of subscription to get event for. |

**Returns**:
- `bytes`: JSON-encoded event response if available
- `None`: If no event is available (non-blocking)

**Raises**: None

**Example**:

```python
response = await manager.get_next_event("sub_user_123")

if response:
    import json
    data = json.loads(response)
    print(data)
    # {
    #     "type": "next",
    #     "id": "sub_user_123",
    #     "payload": {"data": {"user": {...}}}
    # }
else:
    print("No event available")
```

**Notes**:
- Non-blocking - returns immediately
- Returns `None` if no event is queued
- Use polling loop or async waiting for continuous updates
- Response is always JSON bytes (not parsed Python dict)

**Parsing the Response**:

```python
import json

response = await manager.get_next_event("sub_user_123")
if response:
    message = json.loads(response)

    # Message structure:
    # {
    #     "type": "next",                    # Message type
    #     "id": "sub_user_123",              # Subscription ID
    #     "payload": {
    #         "data": {                       # Resolver's return value
    #             "user": {...}
    #         }
    #     }
    # }
```

---

### complete_subscription()

Complete and remove a subscription.

```python
async def complete_subscription(subscription_id: str) -> None
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `subscription_id` | `str` | ✅ | ID of subscription to complete. |

**Returns**: `None`

**Raises**: None (safe to call multiple times)

**Example**:

```python
await manager.complete_subscription("sub_user_123")
```

**Notes**:
- Cleans up subscription resources
- Safe to call even if subscription doesn't exist
- After completion, no more events will be delivered
- Use when client disconnects or subscription is cancelled

---

## Configuration

### Event Bus Configuration

Choose the appropriate event bus for your deployment model:

#### Memory Event Bus

For development and single-server deployments:

```python
from fraiseql import _fraiseql_rs

config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)
```

**Characteristics**:
- ✅ Fastest (in-process)
- ✅ Simple setup
- ❌ Single server only
- ❌ Events lost on restart

---

#### Redis Event Bus

For multi-server distributed deployments:

```python
from fraiseql import _fraiseql_rs

config = _fraiseql_rs.PyEventBusConfig.redis(
    host="localhost",
    port=6379,
    db=0,
    password=None,  # Optional
    ssl=False       # Optional
)
manager = SubscriptionManager(config)
```

**Parameters**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `host` | `str` | `"localhost"` | Redis server hostname |
| `port` | `int` | `6379` | Redis server port |
| `db` | `int` | `0` | Redis database number |
| `password` | `str \| None` | `None` | Redis password (if auth enabled) |
| `ssl` | `bool` | `False` | Use SSL/TLS connection |

**Example**:

```python
# Production Redis with SSL and auth
config = _fraiseql_rs.PyEventBusConfig.redis(
    host="redis.prod.example.com",
    port=6380,
    db=0,
    password="secure_password",
    ssl=True
)
```

**Characteristics**:
- ✅ Multi-server support
- ✅ High throughput
- ✅ Persistent events (with persistence config)
- ❌ Requires Redis cluster
- ❌ Slightly higher latency than memory

---

#### PostgreSQL Event Bus

For multi-server deployments with persistence using LISTEN/NOTIFY:

```python
from fraiseql import _fraiseql_rs

config = _fraiseql_rs.PyEventBusConfig.postgresql(
    connection_string="postgresql://user:password@localhost/fraiseql"
)
manager = SubscriptionManager(config)
```

**Parameters**:

| Parameter | Type | Description |
|-----------|------|-------------|
| `connection_string` | `str` | PostgreSQL connection string |

**Connection String Format**:

```
postgresql://[user[:password]@][host][:port][/database][?param=value...]
```

**Examples**:

```python
# Local development
"postgresql://localhost/fraiseql"

# With authentication
"postgresql://user:pass@localhost:5432/fraiseql"

# With SSL
"postgresql://user:pass@prod.example.com/fraiseql?sslmode=require"
```

**Characteristics**:
- ✅ Multi-server support
- ✅ Built-in persistence
- ✅ No external services needed (if using existing database)
- ❌ Slightly higher latency
- ❌ Scales to ~1000 subscriptions per connection

---

## Resolver Functions

Resolver functions transform events into subscription responses.

### Function Signature

```python
async def resolver(event: dict, variables: dict) -> dict:
    """
    Transform raw event into subscription response.

    Args:
        event: Raw event data from publish_event()
        variables: GraphQL query variables

    Returns:
        Dict matching subscription query structure
    """
    # Your transformation logic here
    return {...}
```

**Requirements**:
- Must be `async def` (can use `await`)
- Accepts `event` (dict) and `variables` (dict) parameters
- Must return a dict matching the query structure
- Should complete in <100ms for best performance

### Example 1: Simple Pass-Through

```python
async def simple_resolver(event, variables):
    return {
        "user": {
            "id": event["id"],
            "name": event["name"]
        }
    }
```

### Example 2: Data Transformation

```python
async def transform_resolver(event, variables):
    return {
        "user": {
            "id": event["id"],
            "name": event["name"].upper(),  # Transform
            "verified": event.get("verified", False)
        }
    }
```

### Example 3: With Async Operations

```python
async def async_resolver(event, variables):
    # Can use await for async operations
    user = await get_user_from_database(event["id"])

    return {
        "user": {
            "id": user.id,
            "name": user.name,
            "email": user.email,
            "role": await get_user_role(user.id)
        }
    }
```

### Example 4: Conditional Logic

```python
async def conditional_resolver(event, variables):
    # Filter or modify based on variables
    include_email = variables.get("include_email", False)

    response = {
        "user": {
            "id": event["id"],
            "name": event["name"]
        }
    }

    if include_email:
        response["user"]["email"] = event.get("email")

    return response
```

### Example 5: Null Handling

```python
async def safe_resolver(event, variables):
    # Handle missing data gracefully
    return {
        "message": {
            "id": event.get("id"),
            "text": event.get("text", ""),  # Default to empty
            "author": event.get("author", "Unknown")  # Default to Unknown
        }
    }
```

### Performance Best Practices

```python
# ✅ GOOD - Fast execution
async def fast_resolver(event, variables):
    # Direct transformations only
    return {
        "data": {
            "id": event["id"],
            "value": event["value"] * 2
        }
    }

# ❌ AVOID - Blocking I/O
def blocking_resolver(event, variables):
    import time
    time.sleep(0.1)  # Blocks event loop!
    return {"data": event}

# ❌ AVOID - Synchronous operations
async def sync_operations_resolver(event, variables):
    # This blocks the event loop
    result = requests.get("https://example.com")  # Blocking!
    return {"data": result.json()}

# ✅ GOOD - Async I/O
async def async_resolver(event, variables):
    # Use async client
    async with aiohttp.ClientSession() as session:
        async with session.get("https://example.com") as resp:
            data = await resp.json()
    return {"data": data}
```

---

## Error Types

All errors inherit from `SubscriptionError`:

```python
from fraiseql.subscriptions.manager import SubscriptionError

class SubscriptionError(Exception):
    """Base class for subscription errors"""

    class InvalidQuery(SubscriptionError):
        """GraphQL query is malformed or invalid"""
        pass

    class AuthorizationFailed(SubscriptionError):
        """User lacks permission or security check failed"""
        pass

    class RateLimited(SubscriptionError):
        """User has exceeded rate limit"""
        pass

    class ConnectionClosed(SubscriptionError):
        """Connection/subscription was closed"""
        pass
```

### Error Handling Example

```python
from fraiseql.subscriptions.manager import SubscriptionError

try:
    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="ws1",
        query="subscription { user { id } }",
        variables={},
        resolver_fn=my_resolver,
        user_id="user1",
        tenant_id="tenant1"
    )
except SubscriptionError.InvalidQuery as e:
    logger.error(f"GraphQL syntax error: {e}")
    await websocket.send_json({
        "type": "error",
        "id": "sub1",
        "payload": {"message": "Invalid subscription query"}
    })
except SubscriptionError.AuthorizationFailed as e:
    logger.error(f"Authorization failed: {e}")
    await websocket.send_json({
        "type": "error",
        "id": "sub1",
        "payload": {"message": "Not authorized"}
    })
except SubscriptionError.RateLimited as e:
    logger.warning(f"Rate limited: {e}")
    await websocket.send_json({
        "type": "error",
        "id": "sub1",
        "payload": {"message": "Too many subscriptions"}
    })
```

---

## Response Format

Events are returned as JSON bytes with this structure:

```python
{
    "type": "next",                    # Message type
    "id": "sub_user_123",              # Subscription ID
    "payload": {
        "data": {                       # Resolver's return value
            "user": {
                "id": "123",
                "name": "Alice",
                "status": "online"
            }
        }
    }
}
```

**Parsing**:

```python
import json

response = await manager.get_next_event("sub_user_123")
if response:
    message = json.loads(response)
    data = message["payload"]["data"]
    # Now use data["user"], etc.
```

---

## Best Practices

### 1. Subscription ID Strategy

```python
# ✅ Good - Unique per connection + subscription
import uuid
subscription_id = f"{connection_id}_{uuid.uuid4()}"

# ✅ Good - Hash of query
import hashlib
query_hash = hashlib.md5(query.encode()).hexdigest()
subscription_id = f"{connection_id}_{query_hash}"

# ❌ Bad - Not unique across connections
subscription_id = "sub1"  # Could conflict!
```

### 2. Resolver Error Handling

```python
# ✅ Good - Handle null gracefully
async def safe_resolver(event, variables):
    return {
        "user": {
            "id": event.get("id"),
            "name": event.get("name", "Unknown")
        }
    }

# ❌ Bad - Crashes on missing data
async def unsafe_resolver(event, variables):
    return {
        "user": {
            "id": event["id"],           # KeyError if missing!
            "name": event["name"]
        }
    }
```

### 3. Rate Limiting Awareness

```python
# Rate limiting is built-in
# Attempting to exceed limits raises SubscriptionError.RateLimited

try:
    for i in range(1000):
        await manager.create_subscription(...)
except SubscriptionError.RateLimited:
    logger.warning("User exceeded subscription limit")
```

### 4. Cleanup on Disconnect

```python
@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    subscriptions = []

    try:
        while True:
            message = await websocket.receive_text()
            # Handle message, create subscriptions
            subscriptions.append(subscription_id)
    finally:
        # Always cleanup on disconnect
        for sub_id in subscriptions:
            await manager.complete_subscription(sub_id)
```

### 5. Monitoring Performance

```python
import time

async def monitored_resolver(event, variables):
    start = time.time()

    result = {
        "user": {
            "id": event["id"],
            "name": event["name"]
        }
    }

    elapsed = (time.time() - start) * 1000
    if elapsed > 100:  # Log if slow
        logger.warning(f"Resolver took {elapsed}ms")

    return result
```

---

## Complete FastAPI Example

```python
from fastapi import FastAPI, WebSocket
import json
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

app = FastAPI()
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)

async def my_resolver(event, variables):
    return {"user": {"id": event["id"], "name": event["name"]}}

@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    subscriptions = []

    try:
        while True:
            data = await websocket.receive_text()
            message = json.loads(data)

            if message["type"] == "subscribe":
                sub_id = message["id"]
                subscriptions.append(sub_id)

                await manager.create_subscription(
                    subscription_id=sub_id,
                    connection_id=str(websocket.client),
                    query=message["payload"]["query"],
                    variables=message["payload"].get("variables", {}),
                    resolver_fn=my_resolver,
                    user_id="user1",
                    tenant_id="tenant1"
                )

            elif message["type"] == "complete":
                await manager.complete_subscription(message["id"])

    finally:
        for sub_id in subscriptions:
            await manager.complete_subscription(sub_id)
```

---

See the full examples in `examples/subscriptions/` for complete working applications.
