# ADVANCED Example: Composite Keys & Custom Batching

⚠️ **WARNING**: This example is for the **5% of users** with complex federation needs. If you're reading this, you probably don't need it yet.

**Complexity Level**: ADVANCED (5% of federation users)
**Setup Time**: 45+ minutes
**Prerequisites**: Solid understanding of LITE + STANDARD

## When You Actually Need This

You need ADVANCED federation if **all three** apply:

1. **Composite keys**: Entity identity requires multiple fields
   - Example: `(org_id, user_id)` instead of just `id`
2. **Custom batch timing**: Default 1ms batch window doesn't fit your needs
3. **Complex resolution logic**: Multiple entity types with interdependencies

**If you only have one of these, use STANDARD or LITE.**

## Example: Multi-Tenant Organization Users

### Problem

You have a multi-tenant system where users are scoped to organizations:

```
Organization A
├─ User (org_id=A, user_id=1)
├─ User (org_id=A, user_id=2)

Organization B
├─ User (org_id=B, user_id=1)  ← Same user_id, different org!
├─ User (org_id=B, user_id=2)
```

You **cannot** uniquely identify a user with just `user_id`. You need **both** `org_id` and `user_id`.

### Solution: Composite Key

```python
# users/models.py
from fraiseql import entity

@entity(key=["org_id", "user_id"])  # Composite key
class OrgUser:
    org_id: str
    user_id: str
    name: str
    email: str
    permissions: list[str]
```

## Resolver with Custom Batching

### Standard Approach (Usually Sufficient)

```python
# users/resolvers.py
from fraiseql.federation import (
    EntitiesResolver,
    PerRequestBatchExecutor,
    Presets
)

resolver = EntitiesResolver()
executor = PerRequestBatchExecutor(
    batch_window_ms=5.0  # ← Slightly longer window for larger batches
)

async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        results = []

        for rep in representations:
            typename = rep["__typename"]

            if typename == "OrgUser":
                org_id = rep["org_id"]
                user_id = rep["user_id"]

                # Load with BOTH key fields
                user = await loader.load(
                    typename,
                    "org_id",  # ← First key field
                    f"{org_id}#{user_id}"  # ← Composite as string
                )
                results.append(user)

        return results

    return await executor.execute_request(handle_request, resolver, db_pool)
```

### Advanced Approach: Custom BatchExecutor

For more control, create a custom executor:

```python
# users/custom_batch_executor.py
from fraiseql.federation import BatchExecutor, EntityDataLoader
import asyncio

class TenantAwareBatchExecutor(BatchExecutor):
    """Custom executor that groups batches by tenant."""

    async def batch_execute(self, requests, resolver, db_pool):
        """
        Override to group requests by org_id before batching.
        This optimizes database queries for multi-tenant systems.
        """
        # Group by organization
        org_requests = {}

        for typename, key_field, key_value in requests:
            org_id = key_value.split("#")[0]  # Extract org_id from composite
            if org_id not in org_requests:
                org_requests[org_id] = []
            org_requests[org_id].append((typename, key_field, key_value))

        # Execute batches per org (parallel)
        tasks = []
        for org_id, org_reqs in org_requests.items():
            task = self._execute_org_batch(org_reqs, resolver, db_pool)
            tasks.append(task)

        results_by_org = await asyncio.gather(*tasks)

        # Flatten back to original order
        results = []
        for org_id in org_requests:
            results.extend(results_by_org[org_id])

        return results

    async def _execute_org_batch(self, requests, resolver, db_pool):
        """Execute batch for single organization."""
        loader = EntityDataLoader(resolver, db_pool, batch_window_ms=1.0)

        try:
            results = await loader.load_many(requests)
            return results
        finally:
            await loader.close()
```

### Using Custom Executor

```python
# users/resolvers.py
from users.custom_batch_executor import TenantAwareBatchExecutor

executor = TenantAwareBatchExecutor(batch_window_ms=5.0)

async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    # Prepare requests in composite format
    requests = []
    for rep in representations:
        if rep["__typename"] == "OrgUser":
            org_id = rep["org_id"]
            user_id = rep["user_id"]
            requests.append(("OrgUser", "org_id", f"{org_id}#{user_id}"))

    # Use custom executor
    results = await executor.batch_execute(requests, resolver, db_pool)
    return results
```

## Database Schema for Composite Keys

```sql
CREATE TABLE org_users (
    org_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    email VARCHAR(255),
    permissions JSONB,
    PRIMARY KEY (org_id, user_id),
    INDEX (org_id)
);
```

**Query optimization**:
```sql
-- Good: Uses primary key
SELECT * FROM org_users
WHERE org_id = 'org-A'
  AND user_id IN ('user-1', 'user-2', 'user-3')

-- Bad: Inefficient
SELECT * FROM org_users
WHERE (org_id, user_id) IN (('org-A', 'user-1'), ('org-A', 'user-2'))
```

## Composite Key in Federation

From the gateway's perspective:

```graphql
query {
  _entities(representations: [
    { __typename: "OrgUser", org_id: "org-A", user_id: "user-1" },
    { __typename: "OrgUser", org_id: "org-A", user_id: "user-2" },
    { __typename: "OrgUser", org_id: "org-B", user_id: "user-1" }
  ]) {
    __typename
    ... on OrgUser {
      org_id
      user_id
      name
      email
    }
  }
}
```

Your resolver receives all 3 representations and batches:

```python
# Batched query
SELECT * FROM org_users
WHERE org_id = 'org-A' AND user_id IN ('user-1', 'user-2')
UNION ALL
SELECT * FROM org_users
WHERE org_id = 'org-B' AND user_id IN ('user-1')
```

## Advanced: Conditional Batch Flushing

Sometimes you want to flush the batch based on conditions other than time:

```python
class SmartBatchExecutor(BatchExecutor):
    """Flushes batch when:
    - Time expires (default)
    - Batch size reaches threshold
    - Batch spans multiple tenants
    """

    def __init__(self, batch_window_ms=1.0, max_batch_size=100):
        super().__init__(batch_window_ms)
        self.max_batch_size = max_batch_size

    async def batch_execute(self, requests, resolver, db_pool):
        loader = EntityDataLoader(resolver, db_pool, batch_window_ms=self.batch_window_ms)

        # Flush if batch size exceeds threshold
        if len(requests) >= self.max_batch_size:
            await asyncio.sleep(0)  # Yield control
            await loader.flush()

        try:
            results = await loader.load_many(requests)
            return results
        finally:
            await loader.close()
```

## When ADVANCED Goes Wrong

### ❌ Mistake 1: Composite Keys Without Understanding

```python
# WRONG: Concatenating keys without delimiter
@entity(key=["org_id", "user_id"])
class OrgUser:
    org_id: str = "123"   # What if it contains a space?
    user_id: str = "abc"

# In representations: org_id="123", user_id="abc"
# Composite: "123abc" ← ambiguous!
```

**Solution**: Use a guaranteed delimiter or separate fields.

### ❌ Mistake 2: Custom Executor Without Monitoring

```python
class CustomExecutor(BatchExecutor):
    async def batch_execute(self, requests, resolver, db_pool):
        # Complex logic here
        # ...
        # But now you have no visibility into batching behavior
        pass
```

**Solution**: Log batch sizes, flush reasons, and statistics.

### ❌ Mistake 3: Composite Key Mismatch

Users Service says:
```python
@entity(key=["org_id", "user_id"])
class User:
    org_id: str
    user_id: str
```

Orders Service extends and says:
```python
@extend_entity(key="user_id")  # WRONG! Should include org_id
class User:
    user_id: str = external()
    ...
```

**Result**: Federation fails because keys don't match.

## Performance Characteristics

### Composite Key Batching

```
Without composite:
- 50 requests → 50 unique user_ids
- 1 query: SELECT * FROM users WHERE user_id IN (...)

With composite (multi-tenant):
- 50 requests → 5 orgs × 10 users each
- Custom executor batches per org
- 5 queries (vs. 50 without batching)
- Maintains isolation
```

### Cache Behavior

```python
loader = EntityDataLoader(
    resolver,
    db_pool,
    cache_size=10000  # Larger cache for composite keys
)

# Request 1
load("OrgUser", "org_id", "org-A#user-1")  # Cache miss
load("OrgUser", "org_id", "org-A#user-2")  # Cache miss

# Request 2
load("OrgUser", "org_id", "org-A#user-1")  # Cache hit (if within TTL)
```

## Testing ADVANCED Setups

```python
@pytest.mark.asyncio
async def test_composite_key_federation(db_pool):
    """Test multi-tenant user resolution."""
    executor = TenantAwareBatchExecutor()

    requests = [
        ("OrgUser", "org_id", "org-A#user-1"),
        ("OrgUser", "org_id", "org-A#user-2"),
        ("OrgUser", "org_id", "org-B#user-1"),
    ]

    results = await executor.batch_execute(requests, resolver, db_pool)

    assert len(results) == 3
    assert results[0].org_id == "org-A"
    assert results[0].user_id == "user-1"
    assert results[2].org_id == "org-B"  # Different org
```

## Debugging ADVANCED Issues

Enable detailed logging:

```python
import logging

logging.getLogger("fraiseql.federation").setLevel(logging.DEBUG)
logging.getLogger("fraiseql.batch_executor").setLevel(logging.DEBUG)

# You'll see:
# - Batch composition
# - Custom executor decisions
# - Cache behavior
# - Query execution time
```

## When to Refactor Back to STANDARD

If you find your ADVANCED setup needs:
- Multiple conditional flush reasons
- Complex key encoding/decoding
- Custom caching strategy
- Tenant-specific optimizations

**Consider**: Is federation the right tool? Or should you use:
- Separate GraphQL services per tenant
- Shared resolver library with federation
- Different schema structure

## Next Steps

- **Diagnosing performance?** See [Performance Guide](../performance-tuning.md)
- **Finding errors in production?** See [Error Handling](../error-handling.md)
- **Scaling this further?** You're beyond FraiseQL scope; consider micro-federation

---

**Final note**: If you've reached ADVANCED federation and are implementing custom batch logic, **you know what you're doing**. Document your decisions for the next engineer. FraiseQL gives you the primitives; you're building your own patterns.
