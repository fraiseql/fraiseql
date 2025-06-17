# N+1 Query Detection

FraiseQL includes automatic N+1 query detection to help identify performance issues during development. This feature warns you when field resolvers are called repeatedly, indicating a potential N+1 query problem.

## How It Works

In development mode, FraiseQL automatically tracks field resolver executions. When a field is resolved more than a configurable threshold of times within a request, it logs a warning with suggestions to use DataLoaders.

## Basic Usage

N+1 detection is automatically enabled in development mode:

```python
from fraiseql.fastapi import create_fraiseql_app

app = create_fraiseql_app(
    database_url="postgresql://localhost/db",
    types=[User, Post],
    production=False  # Development mode enables N+1 detection
)
```

When an N+1 pattern is detected, you'll see warnings in your logs:

```
WARNING: N+1 query pattern detected in request abc-123:
  - Field 'author' on type 'Post' was resolved 15 times. Consider using a DataLoader to batch these queries.
```

## Configuration

You can customize N+1 detection behavior:

```python
from fraiseql.optimization import configure_detector

configure_detector(
    threshold=10,           # Warn after 10 similar queries (default)
    time_window=1.0,       # Within 1 second (default)
    enabled=True,          # Enable detection (default in dev)
    raise_on_detection=False  # Just warn, don't raise (default)
)
```

## Strict Mode

For stricter enforcement during testing or CI, you can configure the detector to raise exceptions:

```python
configure_detector(
    threshold=5,
    raise_on_detection=True  # Raise exception on N+1 detection
)
```

This will cause queries with N+1 patterns to fail with detailed error information:

```json
{
  "errors": [{
    "message": "N+1 query pattern detected: 1 patterns found",
    "extensions": {
      "code": "N1_QUERY_DETECTED",
      "patterns": [{
        "field": "author",
        "type": "Post",
        "count": 15
      }]
    }
  }]
}
```

## Example: Detecting N+1

Here's an example that would trigger N+1 detection:

```python
@fraiseql.type
class Post:
    id: UUID
    title: str
    author_id: UUID
    
    @fraiseql.field
    async def author(self, info) -> User:
        # This executes a separate query for each post!
        db = info.context["db"]
        return await db.get_user(self.author_id)

@fraiseql.query
async def get_posts(info) -> List[Post]:
    db = info.context["db"]
    return await db.get_all_posts()
```

When querying:

```graphql
query {
  get_posts {
    title
    author {  # N+1 query here!
      name
    }
  }
}
```

## Fixing N+1 with DataLoaders

The solution is to use DataLoaders to batch database queries:

```python
from fraiseql.optimization import DataLoader

class UserLoader(DataLoader[UUID, User]):
    def __init__(self, db):
        super().__init__()
        self.db = db
    
    async def batch_load(self, user_ids: List[UUID]) -> List[Optional[User]]:
        # Fetch all users in one query
        users = await self.db.get_users_by_ids(user_ids)
        # Return in same order as requested
        user_map = {u.id: u for u in users}
        return [user_map.get(uid) for uid in user_ids]

@fraiseql.type
class Post:
    id: UUID
    title: str
    author_id: UUID
    
    @fraiseql.field
    async def author(self, info) -> User:
        # Use DataLoader instead of direct query
        loader = get_loader(UserLoader)
        return await loader.load(self.author_id)
```

## Disabling for Specific Fields

Some fields might legitimately need to execute multiple times. You can disable N+1 tracking for specific fields:

```python
@fraiseql.type
class Report:
    id: UUID
    
    @fraiseql.field(track_n1=False)
    async def complex_calculation(self, info) -> float:
        # This won't trigger N+1 warnings
        return await expensive_computation()
```

## Production Behavior

N+1 detection is automatically disabled in production mode to avoid performance overhead:

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/db",
    types=[User, Post],
    production=True  # N+1 detection disabled
)
```

## Testing with N+1 Detection

You can use N+1 detection in your tests to ensure queries are optimized:

```python
import pytest
from fraiseql.optimization import configure_detector, N1QueryDetected

def test_posts_query_uses_dataloader():
    # Configure strict detection for tests
    configure_detector(
        threshold=5,
        raise_on_detection=True
    )
    
    # This test will fail if N+1 is detected
    response = client.post("/graphql", json={
        "query": "{ get_posts { author { name } } }"
    })
    
    assert response.status_code == 200
    assert "errors" not in response.json()
```

## Best Practices

1. **Enable in Development**: Always run with N+1 detection during development
2. **Use DataLoaders**: Implement DataLoaders for all relationships
3. **Test Coverage**: Include N+1 detection in your test suite
4. **Monitor Thresholds**: Adjust thresholds based on your use case
5. **CI Integration**: Consider enabling strict mode in CI pipelines

## Performance Impact

N+1 detection has minimal overhead in development mode:
- Tracks resolver execution counts and timing
- Logs warnings asynchronously
- Automatically disabled in production

This helps catch performance issues early without impacting production performance.