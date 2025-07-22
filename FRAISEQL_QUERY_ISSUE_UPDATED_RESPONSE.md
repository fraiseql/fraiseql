# FraiseQL Query Issue - Updated Response

Thank you for clarifying that the query resolvers are properly defined. Based on the additional information, here are several potential causes and debugging steps for your issue:

## 1. Query Naming Convention Issue

FraiseQL automatically converts snake_case to camelCase when `camel_case_fields = True` (which is the default). This means:

- Your resolver `smtp_server` becomes `smtpServer` in GraphQL
- Your resolver `network_configuration` becomes `networkConfiguration` in GraphQL

**Check**: Ensure your test queries use the correct camelCase names:
```graphql
# Correct
query { smtpServer(id: "...") { id } }

# Incorrect (will fail)
query { smtp_server(id: "...") { id } }
```

## 2. Enable Detailed Error Logging

To see the actual error instead of "Internal server error", you have several options:

### Option A: Set Development Environment
```python
# In your test setup
config = FraiseQLConfig(environment="development")
```

### Option B: Enable Debug Logging
```python
import logging

# Enable FraiseQL debug logging
logging.getLogger("fraiseql").setLevel(logging.DEBUG)

# Also enable GraphQL execution logging
logging.getLogger("graphql.execution").setLevel(logging.DEBUG)
```

### Option C: Add Custom Error Handler
```python
# Wrap your test to catch the actual exception
try:
    result = await client.post("/graphql", json={...})
except Exception as e:
    print(f"Actual error: {type(e).__name__}: {e}")
    import traceback
    traceback.print_exc()
```

## 3. Transaction Isolation Issue

The most likely cause is a transaction isolation issue in your test environment. Here's what might be happening:

1. Your mutation runs in a transaction
2. The mutation completes and returns data
3. The subsequent query runs in a different transaction/connection
4. Due to isolation levels or uncommitted transactions, the query doesn't see the data

### Solution: Use Committed Data Pattern
```python
# Instead of using db_connection fixture (which rolls back)
# Use db_connection_committed fixture for integration tests

async def test_router_update(db_connection_committed):
    # This ensures data is actually committed and visible
    # across different database connections
    ...
```

## 4. Database Connection Pool Issue

If you're using connection pooling, the query might be getting a different connection than the mutation:

### Debug Connection Info
```python
@fraiseql.query
async def router(info: GraphQLResolveInfo, id: uuid.UUID) -> Router | None:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id", "...")
    
    # Add debug logging
    import logging
    logger = logging.getLogger(__name__)
    logger.debug(f"Query connection: {id(db.connection)}")
    logger.debug(f"Looking for router {id} with tenant {tenant_id}")
    
    result = await db.find_one("v_router", id=id, tenant_id=tenant_id)
    logger.debug(f"Query result: {result}")
    
    return result
```

## 5. View Definition Issue

Check if your database views have any special conditions that might filter out data:

```sql
-- Check the view definition
SELECT definition FROM pg_views WHERE viewname = 'v_router';

-- Test the view directly
SELECT * FROM v_router WHERE id = 'your-test-id';
```

## 6. Async Context Issue

Sometimes GraphQL context isn't properly propagated in async tests:

```python
# Ensure proper async context in tests
async def test_with_proper_context():
    async with async_client() as client:
        # Make sure context includes all required fields
        headers = {
            "tenant-id": "your-tenant-id",
            "contact-id": "your-contact-id",
        }
        result = await client.post("/graphql", json={...}, headers=headers)
```

## 7. Complete Debugging Example

Here's a complete example to help debug the issue:

```python
import logging
import asyncio
from contextlib import asynccontextmanager

# Enable all debug logging
logging.basicConfig(level=logging.DEBUG)
logging.getLogger("fraiseql").setLevel(logging.DEBUG)
logging.getLogger("graphql").setLevel(logging.DEBUG)

async def debug_query_issue(client, router_id):
    # 1. First verify the data exists in the database
    db = ... # get your db connection
    raw_result = await db.execute_raw(
        "SELECT * FROM v_router WHERE id = $1",
        router_id
    )
    print(f"Raw DB result: {raw_result}")
    
    # 2. Try the query with detailed error handling
    query = """
        query RouterQuery($id: ID!) {
            router(id: $id) {
                id
                ipAddress
            }
        }
    """
    
    try:
        response = await client.post(
            "/graphql",
            json={
                "query": query,
                "variables": {"id": str(router_id)}
            }
        )
        print(f"Response: {response.json()}")
    except Exception as e:
        print(f"Exception type: {type(e)}")
        print(f"Exception: {e}")
        import traceback
        traceback.print_exc()
```

## 8. Quick Test: Direct Resolver Call

To isolate the issue, try calling your resolver directly:

```python
# Test the resolver without GraphQL
from graphql import GraphQLResolveInfo

# Mock the info object
class MockInfo:
    context = {
        "db": your_db_instance,
        "tenant_id": "your-tenant-id"
    }

# Call resolver directly
result = await router(MockInfo(), id=your_router_id)
print(f"Direct resolver result: {result}")
```

## Next Steps

1. Enable debug logging to see the actual error
2. Check if you're using the correct camelCase query names
3. Verify transaction isolation in your test setup
4. Test the resolver directly to isolate GraphQL vs database issues
5. Check if the issue is specific to the test environment by trying the same query in development

The fact that mutations work but queries fail strongly suggests either a transaction isolation issue or a naming mismatch. The debug logging should reveal the exact cause.