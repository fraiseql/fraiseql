# Error Handling: Common Issues & Debugging

This guide explains what can go wrong in federation and how to fix it.

## Common Errors

### Error 1: Entity Not Found

**Symptom**: Resolver returns `None` for an entity that should exist.

```python
user = await loader.load("User", "id", "user-123")
assert user is not None  # ← Fails!
```

**Cause 1: Entity doesn't exist in database**

```sql
SELECT * FROM users WHERE id = 'user-123'
-- Returns: (empty result set)
```

**Solution**:
```python
user = await loader.load("User", "id", "user-123")
if user is None:
    # Entity not found. Handle gracefully:
    # Option 1: Return error in GraphQL
    raise GraphQLError(f"User user-123 not found")

    # Option 2: Skip in results
    continue

    # Option 3: Return partial data
    return User(id="user-123", name=None)
```

**Cause 2: Key is wrong**

```python
# Wrong:
user = await loader.load("User", "id", 123)  # Integer, not string

# Right:
user = await loader.load("User", "id", "123")  # String key
```

**Debugging**:
```python
import logging
logging.getLogger("fraiseql.federation").setLevel(logging.DEBUG)

# You'll see:
# DEBUG: load("User", "id", "123") - cache miss
# DEBUG: Pending batch: [("User", "id", "123")]
# DEBUG: Executing: SELECT * FROM users WHERE id = '123'
# DEBUG: Result: empty
# DEBUG: Future resolved with None
```

---

### Error 2: Missing Key Field

**Symptom**: `ValueError` when defining entity.

```python
@entity  # Auto-key detection fails!
class User:
    name: str
    email: str
    # No 'id' field!
```

**Error message**:
```
ValueError: User has no 'id' field. Specify key explicitly: @entity(key='field_name')
```

**Solution**: Specify key explicitly.

```python
@entity(key="user_id")
class User:
    user_id: str
    name: str
    email: str
```

**Debugging**: Read the error message—it tells you exactly what to do.

---

### Error 3: Key Field Not on Entity

**Symptom**: `ValueError` when defining composite key.

```python
@entity(key=["org_id", "user_id"])
class User:
    org_id: str
    # Missing user_id field!
    name: str
```

**Error message**:
```
ValueError: Key field 'user_id' not found in User.
Available fields: org_id, name
```

**Solution**: Add the missing field or fix the key specification.

```python
@entity(key=["org_id", "user_id"])
class User:
    org_id: str
    user_id: str  # ← Add this
    name: str
```

---

### Error 4: Batch Query Fails

**Symptom**: Database connection error during `_entities` resolution.

```python
resolver.load("User", "id", "user-1")
resolver.load("User", "id", "user-2")
[Batch window expires]
```

**Error** (PostgreSQL connection failed):
```
asyncpg.exceptions.InterfaceError: Cannot connect to server
```

**What happens**:
```python
# In DataLoader.flush():
try:
    rows = await conn.fetch(sql, *key_values)
except Exception as e:
    # Mark all futures as failed
    for future in request_map.get(cache_key, []):
        future.set_exception(e)
```

**Response to client**:
```json
{
  "errors": [{
    "message": "PostgreSQL error: Cannot connect to server"
  }],
  "data": null
}
```

**Solution**:

1. **Check database connectivity**:
```bash
psql postgresql://localhost/mydb
# If fails: database is down or credentials wrong
```

2. **Verify connection pool**:
```python
db_pool = await asyncpg.create_pool(
    "postgresql://user:pass@localhost/db",
    min_size=5,
    max_size=20
)
# min_size: minimum connections to maintain
# max_size: maximum concurrent connections
```

3. **Add retry logic** (optional, at gateway level):
```python
@backoff(max_tries=3, wait_seconds=0.5)
async def resolve_with_retry(info, representations):
    return await resolve_entities(info, representations)
```

4. **Debug with logs**:
```python
import logging
logging.getLogger("asyncpg").setLevel(logging.DEBUG)

# You'll see:
# DEBUG: Creating connection
# DEBUG: Executing query
# DEBUG: Error: connection refused
```

---

### Error 5: Incomplete Representations

**Symptom**: Gateway sends malformed representation to `_entities`.

```python
# Gateway mistakenly sends:
{
  "__typename": "User",
  # Missing: "id" field!
}
```

**Result**: `KeyError` when accessing representation["id"]

```python
async def resolve_entities(info, representations):
    for rep in representations:
        entity_id = rep["id"]  # ← KeyError!
```

**Error message**:
```
KeyError: 'id'
```

**Solution**: Validate representations.

```python
async def resolve_entities(info, representations):
    for rep in representations:
        typename = rep.get("__typename")

        if typename == "User":
            entity_id = rep.get("id")
            if entity_id is None:
                raise GraphQLError(
                    f"Missing 'id' in representation for {typename}"
                )

            user = await loader.load(typename, "id", entity_id)
            results.append(user)
        else:
            raise GraphQLError(f"Unknown type: {typename}")

    return results
```

---

### Error 6: Type Mismatch on Extend

**Symptom**: Extension in one service doesn't match original entity.

```python
# Users Service:
@entity(key="id")
class User:
    id: str

# Reviews Service (WRONG):
@extend_entity(key="id")
class User:
    id: int  # ← Type mismatch!
```

**Result**: Federation fails silently or with cryptic error.

**Solution**: Ensure key types match.

```python
# Users Service:
@entity(key="id")
class User:
    id: str  # String

# Reviews Service (CORRECT):
@extend_entity(key="id")
class User:
    id: str  # Must be string
    reviews: list[Review]
```

**Verification**:
```python
# Check at startup
user_meta = get_entity_metadata("User")
print(f"User key: {user_meta.resolved_key}")
print(f"User fields: {user_meta.fields}")
# Verify manually across services
```

---

### Error 7: Missing External Field Marker

**Symptom**: Extension doesn't properly mark external fields.

```python
@extend_entity(key="id")
class User:
    id: str  # Should be: id: str = external()
    name: str  # Should be: name: str = external()
    reviews: list[Review]
```

**Result**: Confusion about which fields come from which service.

**Solution**: Mark all external fields.

```python
@extend_entity(key="id")
class User:
    id: str = external()  # ← From Users Service
    name: str = external()  # ← From Users Service
    email: str = external()  # ← From Users Service

    reviews: list[Review]  # ← New, from Reviews Service
```

**Check at startup**:
```python
user_meta = get_entity_metadata("User")
print(f"External fields: {user_meta.external_fields}")
# Should include: id, name, email
```

---

### Error 8: Batch Window Timeout Issues

**Symptom**: Requests seem to hang or timeout.

```python
loader = EntityDataLoader(resolver, db_pool, batch_window_ms=10000)  # 10 seconds!
# Requests now take 10+ seconds to complete
```

**Solution**: Tune batch window appropriately.

```python
# Real-time (low latency, fewer batches)
loader = EntityDataLoader(resolver, db_pool, batch_window_ms=0.1)

# Balanced (1ms is default, recommended)
loader = EntityDataLoader(resolver, db_pool, batch_window_ms=1.0)

# Aggressive batching (higher latency, more batching)
loader = EntityDataLoader(resolver, db_pool, batch_window_ms=50.0)
```

**How to choose**:
```
If typical batch size is:
  < 10 entities → use 0.1-1.0ms (requests are fast)
  10-100 entities → use 1-5ms (good batching window)
  > 100 entities → use 5-100ms (you want maximum batching)
```

---

## Testing Error Cases

### Test 1: Missing Entity

```python
@pytest.mark.asyncio
async def test_entity_not_found(db_pool):
    """Verify None is returned for missing entity."""
    executor = PerRequestBatchExecutor()

    async def load_missing(loader):
        user = await loader.load("User", "id", "nonexistent-user")
        return [user]

    results = await executor.execute_request(load_missing, resolver, db_pool)
    assert results[0] is None
```

### Test 2: Database Failure

```python
@pytest.mark.asyncio
async def test_database_failure(mocker):
    """Verify exception is propagated on DB failure."""
    executor = PerRequestBatchExecutor()

    # Mock database to fail
    mocker.patch("asyncpg.Pool.fetch", side_effect=Exception("DB error"))

    async def load_with_failure(loader):
        user = await loader.load("User", "id", "user-1")
        return [user]

    # Should raise exception
    with pytest.raises(Exception, match="DB error"):
        await executor.execute_request(load_with_failure, resolver, db_pool)
```

### Test 3: Partial Failure

```python
@pytest.mark.asyncio
async def test_partial_batch_failure(db_pool):
    """Some entities exist, some don't."""
    executor = PerRequestBatchExecutor()

    async def load_partial(loader):
        user1 = await loader.load("User", "id", "user-1")  # Exists
        user2 = await loader.load("User", "id", "user-2")  # Missing
        return [user1, user2]

    results = await executor.execute_request(load_partial, resolver, db_pool)
    assert results[0] is not None
    assert results[1] is None
```

---

## Debugging Strategies

### Strategy 1: Enable Debug Logging

```python
import logging

# Enable FraiseQL federation logging
logging.basicConfig(level=logging.DEBUG)
logging.getLogger("fraiseql.federation").setLevel(logging.DEBUG)

# Now you'll see all batching operations
```

### Strategy 2: Inspect DataLoader Stats

```python
async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        results = []
        for rep in representations:
            user = await loader.load("User", "id", rep["id"])
            results.append(user)

        # Before returning, inspect stats
        stats = loader.stats
        print(f"Stats: {stats}")
        print(f"  - Total requests: {stats.total_requests}")
        print(f"  - Cache hits: {stats.cache_hits}")
        print(f"  - Dedup hits: {stats.dedup_hits}")
        print(f"  - Batch count: {stats.batch_count}")
        print(f"  - Dedup rate: {stats.dedup_rate:.1%}")

        return results

    return await batch_executor.execute_request(
        handle_request, resolver, db_pool
    )
```

### Strategy 3: Check Representations in GraphQL Query

```graphql
# Debug query to verify gateway sends correct representations
query {
  _entities(representations: [
    { __typename: "User", id: "user-1" },
    { __typename: "User", id: "user-2" }
  ]) {
    __typename
    ... on User {
      id
      name
    }
  }
}
```

### Strategy 4: Add Validation Middleware

```python
async def validate_and_resolve_entities(info, representations):
    # Validate representations
    for rep in representations:
        if "__typename" not in rep:
            raise GraphQLError("Missing __typename in representation")
        if "id" not in rep:
            raise GraphQLError(
                f"Missing id for {rep['__typename']}"
            )

    # Then proceed with normal resolution
    return await resolve_entities(info, representations)
```

---

## Error Recovery

### Pattern 1: Graceful Degradation

```python
async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        results = []
        for rep in representations:
            try:
                user = await loader.load("User", "id", rep["id"])
                if user is None:
                    # Return stub with ID but no other data
                    results.append(User(id=rep["id"], name="Unknown"))
                else:
                    results.append(user)
            except Exception as e:
                logger.error(f"Failed to load user {rep['id']}: {e}")
                # Return partial data instead of failing entire request
                results.append(User(id=rep["id"], name="Unknown"))

        return results

    return await batch_executor.execute_request(
        handle_request, resolver, db_pool
    )
```

### Pattern 2: Circuit Breaker

```python
from functools import lru_cache
import time

class CircuitBreaker:
    def __init__(self, failure_threshold=5, timeout=60):
        self.failure_count = 0
        self.failure_threshold = failure_threshold
        self.timeout = timeout
        self.last_failure_time = None

    async def call(self, coro):
        if self.is_open():
            raise Exception("Circuit breaker is open")

        try:
            result = await coro
            self.failure_count = 0
            return result
        except Exception as e:
            self.failure_count += 1
            self.last_failure_time = time.time()
            if self.failure_count >= self.failure_threshold:
                logger.error("Circuit breaker opened")
            raise

    def is_open(self):
        if self.failure_count < self.failure_threshold:
            return False
        if time.time() - self.last_failure_time > self.timeout:
            self.failure_count = 0
            return False
        return True

# Usage
breaker = CircuitBreaker()

async def resolve_entities(info, representations):
    try:
        return await breaker.call(
            batch_executor.execute_request(...)
        )
    except Exception:
        # Return empty results if circuit open
        return []
```

---

## Production Checklist

- [ ] Validate all representations in `_entities` resolver
- [ ] Log all batch operations (batch size, entity types, duration)
- [ ] Monitor cache hit rates (target >30% for typical workloads)
- [ ] Set appropriate batch window (start with 1ms, tune based on metrics)
- [ ] Test entity not found scenario
- [ ] Test database connection failure
- [ ] Implement error recovery (graceful degradation or circuit breaker)
- [ ] Alert on excessive batch sizes (>1000 entities per batch)
- [ ] Alert on database latency (queries taking >100ms)
- [ ] Document key types for all entities across services

---

## Summary

| Error | Cause | Solution |
|-------|-------|----------|
| Entity is None | Doesn't exist in DB | Validate, handle gracefully |
| KeyError on field | Auto-key detection failed | Specify key explicitly |
| ValueError on key | Key field not on entity | Add field or fix key spec |
| Database exception | Connection failed | Check DB, verify pool config |
| Representation invalid | Gateway bug | Validate representations |
| Type mismatch on extend | Key type differs | Match key types across services |
| Missing external() | Confusion about field ownership | Mark external fields |
| Slow requests | Batch window too long | Tune window based on batch size |

---

**Key principle**: Most errors are discoverable at startup (schema validation, type checking). The rest are data issues (missing entities, connection problems). Log early, validate always.
