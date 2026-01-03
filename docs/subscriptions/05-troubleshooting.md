# Troubleshooting Guide - GraphQL Subscriptions

Common issues, debugging techniques, and solutions.

---

## Common Issues & Solutions

### Issue 1: Client Doesn't Receive Events

**Symptoms**:
- Client connects successfully
- Events are published
- Client never receives data

**Debugging Steps**:

```python
# 1. Verify subscription was created
import logging
logging.basicConfig(level=logging.DEBUG)

# 2. Check resolver is being called
async def debug_resolver(event, variables):
    print(f"Resolver called with event: {event}")
    return {"data": event}

# 3. Verify event is being published
await manager.publish_event(
    event_type="test",
    channel="test_channel",
    data={"test": "data"}
)

# 4. Check response is being returned
response = await manager.get_next_event("subscription_id")
print(f"Response: {response}")

# 5. Parse the response
import json
if response:
    message = json.loads(response)
    print(f"Message type: {message.get('type')}")
    print(f"Payload: {message.get('payload')}")
```

**Common Causes & Solutions**:

| Cause | Solution |
|-------|----------|
| Event on wrong channel | Ensure event_type and channel match subscription |
| Resolver returns wrong shape | Ensure return dict matches GraphQL query |
| User/tenant mismatch | Check user_id and tenant_id match |
| Rate limited | Check rate limit quotas aren't exceeded |
| Subscription not created | Check create_subscription() call succeeded |

---

### Issue 2: "InvalidQuery" Error

**Error Message**:
```
SubscriptionError.InvalidQuery: GraphQL syntax error: ...
```

**Causes & Solutions**:

```python
# ❌ Wrong - Missing "subscription" keyword
query = "{ user { id name } }"

# ✅ Correct
query = "subscription { user { id name } }"

# ❌ Wrong - Multiple root fields
query = "subscription { user { id } message { text } }"

# ✅ Correct - Single root field
query = "subscription { user { id } }"

# ❌ Wrong - Invalid syntax
query = "subscription user { id }"

# ✅ Correct
query = "subscription { user { id } }"
```

**Fix**:
```python
# Validate query syntax
def validate_query(query: str) -> bool:
    # Must start with "subscription"
    if not query.strip().startswith("subscription"):
        return False
    # Must have valid syntax
    if "{" not in query or "}" not in query:
        return False
    return True
```

---

### Issue 3: "AuthorizationFailed" Error

**Error Message**:
```
SubscriptionError.AuthorizationFailed: ...
```

**Debugging**:

```python
# Check authentication setup
try:
    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="ws1",
        query="subscription { user { id } }",
        variables={},
        resolver_fn=my_resolver,
        user_id="user123",      # ← Verify this is set correctly
        tenant_id="tenant123"   # ← Verify this is set correctly
    )
except SubscriptionError.AuthorizationFailed as e:
    print(f"Auth error: {e}")
    print(f"Check user_id and tenant_id are correct")
```

**Common Causes**:

| Cause | Solution |
|-------|----------|
| user_id is None or empty | Set user_id from authenticated user |
| tenant_id is None or empty | Set tenant_id from user's organization |
| User doesn't exist | Verify user exists in system |
| User lacks permission | Check user's role/permissions for subscription |

---

### Issue 4: "RateLimited" Error

**Error Message**:
```
SubscriptionError.RateLimited: User has exceeded rate limit
```

**Meaning**: User has too many active subscriptions

**Debugging**:

```python
# Check current subscription count
# Built-in rate limiting prevents abuse:
# - Per-user subscription limit
# - Per-tenant subscription limit
# - Per-user event throughput limit

# Solution 1: Complete old subscriptions
for old_sub in inactive_subscriptions:
    await manager.complete_subscription(old_sub)

# Solution 2: Increase rate limit (config-level)
# Or set appropriate quotas for your use case
```

**Default Limits**:
- 100 subscriptions per user
- 1000 subscriptions per tenant
- 10,000 events/sec per user

Contact support to adjust limits if needed.

---

### Issue 5: Connection Drops

**Symptoms**:
- Connection established
- Works for a while
- Then disconnects

**Debugging**:

```python
# 1. Check WebSocket is staying open
@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()

    try:
        while True:
            data = await websocket.receive_text()
            # Process message
    except Exception as e:
        logger.error(f"WebSocket error: {e}", exc_info=True)
    finally:
        # Always cleanup
        await websocket.close()

# 2. Add heartbeat/ping-pong
# Keep connection alive with periodic pings
async def send_ping():
    while True:
        await asyncio.sleep(30)
        await websocket.send_json({"type": "ping"})
```

**Common Causes**:

| Cause | Solution |
|-------|----------|
| Network timeout | Implement heartbeat/ping-pong |
| Load balancer timeout | Increase load balancer timeout |
| Memory leak on server | Check resolver for unbounded memory growth |
| Server crash | Check server logs for exceptions |

---

### Issue 6: High Latency (>10ms)

**Debugging**:

```python
# Measure each component
import time
import json

# 1. Measure publish time
start = time.time()
await manager.publish_event(
    event_type="test",
    channel="test",
    data={"test": "data"}
)
publish_time = (time.time() - start) * 1000
print(f"Publish time: {publish_time}ms")  # Should be <1ms

# 2. Measure resolver execution
async def timed_resolver(event, variables):
    start = time.time()
    result = {"data": event}
    elapsed = (time.time() - start) * 1000
    if elapsed > 1:
        logger.warning(f"Slow resolver: {elapsed}ms")
    return result

# 3. Measure get_next_event time
start = time.time()
response = await manager.get_next_event("sub1")
get_time = (time.time() - start) * 1000
print(f"Get event time: {get_time}ms")  # Should be <1ms

# Total latency
print(f"Total: {publish_time + get_time}ms")  # Target: <10ms
```

**Optimization Steps**:

```python
# Step 1: Use in-memory event bus for dev
config = _fraiseql_rs.PyEventBusConfig.memory()

# Step 2: Optimize resolver
async def optimized_resolver(event, variables):
    # Only return necessary fields
    return {
        "user": {
            "id": event["id"],
            # Don't do:
            # - Network calls
            # - Database queries
            # - Heavy calculations
            # - Async operations (unless necessary)
        }
    }

# Step 3: Check network (if using Redis)
# Measure Redis latency:
import redis
r = redis.Redis(host='localhost', port=6379)
start = time.time()
r.ping()
ping_time = (time.time() - start) * 1000
print(f"Redis ping: {ping_time}ms")  # Should be <5ms
```

---

### Issue 7: Memory Leaks

**Symptoms**:
- Memory usage grows over time
- Eventually runs out of memory
- Performance degradation

**Debugging**:

```python
import tracemalloc
import asyncio

tracemalloc.start()

# Run your subscriptions
# ...

current, peak = tracemalloc.get_traced_memory()
print(f"Current: {current / 1024 / 1024}MB; Peak: {peak / 1024 / 1024}MB")

# If memory grows unbounded:
# 1. Check resolver for leaks
# 2. Check subscription cleanup
# 3. Check event queue
```

**Common Causes**:

```python
# ❌ Memory leak - unbounded list growth
class BadResolver:
    def __init__(self):
        self.events = []  # Grows forever!

    async def resolver(self, event, variables):
        self.events.append(event)  # Never cleaned up
        return {"data": event}

# ✅ Fixed - proper cleanup
class GoodResolver:
    def __init__(self, max_history=100):
        self.events = []

    async def resolver(self, event, variables):
        self.events.append(event)
        # Keep only recent events
        if len(self.events) > self.max_history:
            self.events.pop(0)
        return {"data": event}
```

**Prevention**:

```python
# 1. Bounds-check any caching
CACHE_MAX_SIZE = 1000
cache = {}

def cached_operation(key):
    if len(cache) > CACHE_MAX_SIZE:
        cache.clear()
    # ...use cache

# 2. Proper resource cleanup
@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    subscriptions = []
    try:
        # Handle subscriptions
        pass
    finally:
        # Always cleanup, even on error
        for sub_id in subscriptions:
            await manager.complete_subscription(sub_id)
```

---

## Debugging Techniques

### 1. Verbose Logging

```python
import logging

logging.basicConfig(
    level=logging.DEBUG,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

logger = logging.getLogger(__name__)

# Log subscription lifecycle
logger.info(f"Creating subscription: {subscription_id}")
logger.debug(f"Query: {query}")
logger.debug(f"Variables: {variables}")

logger.info(f"Publishing event: {event_type}")
logger.debug(f"Event data: {data}")

logger.info(f"Getting event for: {subscription_id}")
response = await manager.get_next_event(subscription_id)
logger.debug(f"Response: {response}")

logger.info(f"Completing subscription: {subscription_id}")
```

### 2. Tracing Events

```python
# Track event flow through system
class TracingResolver:
    async def resolver(self, event, variables):
        print(f"[TRACE] Event received: {event}")

        # Process
        result = {"data": event}

        print(f"[TRACE] Returning: {result}")
        return result

# In WebSocket handler
@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    client_id = f"{websocket.client[0]}:{websocket.client[1]}"
    logger.info(f"[TRACE] Client connected: {client_id}")

    # ... handle subscriptions

    logger.info(f"[TRACE] Client disconnected: {client_id}")
```

### 3. Unit Testing Resolvers

```python
import pytest

@pytest.mark.asyncio
async def test_resolver():
    # Test resolver in isolation
    event = {"id": "123", "name": "Alice"}
    variables = {}

    result = await my_resolver(event, variables)

    assert result["user"]["id"] == "123"
    assert result["user"]["name"] == "Alice"

@pytest.mark.asyncio
async def test_resolver_with_missing_data():
    # Test null handling
    event = {"id": "123"}  # name is missing
    variables = {}

    result = await my_resolver(event, variables)

    # Should not crash
    assert result is not None
    assert result["user"]["id"] == "123"
```

### 4. Performance Profiling

```python
import cProfile
import pstats

# Profile resolver
profiler = cProfile.Profile()
profiler.enable()

# Run resolver
asyncio.run(my_resolver({"id": "123"}, {}))

profiler.disable()
stats = pstats.Stats(profiler)
stats.print_stats(10)  # Top 10 functions
```

---

## Performance Optimization

### 1. Resolver Optimization

```python
# ❌ Slow - Multiple function calls
async def slow_resolver(event, variables):
    result = {
        "user": {
            "id": event.get("id"),
            "name": event.get("name"),
            "email": event.get("email"),
            # ... 20+ fields
        }
    }
    return result

# ✅ Fast - Only needed fields
async def fast_resolver(event, variables):
    return {
        "user": {
            "id": event["id"],
            "name": event["name"]
        }
    }
```

### 2. Query Optimization

```python
# ❌ Slow - Requesting unneeded data
query = """
subscription {
    user {
        id
        name
        email
        phone
        address
        # ... 50 fields
    }
}
"""

# ✅ Fast - Only needed fields
query = """
subscription {
    user {
        id
        name
    }
}
"""
```

### 3. Event Bus Selection

```python
# Development: Use memory (fast)
config = _fraiseql_rs.PyEventBusConfig.memory()

# Production single-server: Use memory (fast)
config = _fraiseql_rs.PyEventBusConfig.memory()

# Production multi-server: Use Redis
config = _fraiseql_rs.PyEventBusConfig.redis(
    host="redis.example.com",
    port=6379
)
```

---

## FAQ

**Q: How do I debug in production?**

A: Enable structured logging and use log aggregation:

```python
import json
import logging

class JSONFormatter(logging.Formatter):
    def format(self, record):
        log_obj = {
            "timestamp": self.formatTime(record),
            "level": record.levelname,
            "message": record.getMessage(),
            "subscription_id": getattr(record, "subscription_id", None)
        }
        return json.dumps(log_obj)

handler = logging.StreamHandler()
handler.setFormatter(JSONFormatter())
logger.addHandler(handler)
```

**Q: What's the expected latency?**

A: <10ms end-to-end:
- Publish: <1ms
- Security check: <0.1ms
- Resolver: <1ms
- Serialize: <0.1ms
- Return: <1ms

If exceeding, profile each step.

**Q: How many subscriptions can I handle?**

A: Per instance:
- Memory: 10,000+
- With Redis: 100,000+ (across cluster)

Depends on resolver complexity and resource allocation.

**Q: Should I use Redis or PostgreSQL?**

A: Use Redis for:
- High throughput needs
- Multi-server distributed deployments
- No persistence requirement

Use PostgreSQL for:
- Existing database deployment
- Persistence required
- Audit logging needed

---

See the examples (`examples/subscriptions/`) for more debugging patterns and best practices.
