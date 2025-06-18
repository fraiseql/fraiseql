# DataLoader

FraiseQL includes a powerful DataLoader implementation to solve the N+1 query problem and batch database operations efficiently.

## Overview

The DataLoader pattern batches multiple individual data loading requests into a single batch request, reducing the number of database queries and improving performance.

## Basic Usage

### Simple DataLoader

```python
from fraiseql import dataloader_field

@fraiseql.type
class User:
    id: int
    name: str
    
    @dataloader_field
    async def posts(self, info) -> list['Post']:
        """This will be automatically batched."""
        return await Post.get_by_user_id(self.id)
```

### Custom DataLoader

Define custom batch loading logic:

```python
from fraiseql.optimization import DataLoader

async def batch_load_posts(user_ids: list[int]) -> list[list['Post']]:
    """Load posts for multiple users in a single query."""
    posts_by_user = await Post.get_by_user_ids(user_ids)
    return [posts_by_user.get(user_id, []) for user_id in user_ids]

posts_loader = DataLoader(batch_load_posts)

@fraiseql.type
class User:
    id: int
    
    @fraiseql.field
    async def posts(self, info) -> list['Post']:
        return await posts_loader.load(self.id)
```

## DataLoader Patterns

### One-to-Many Relationships

Load related objects efficiently:

```python
@fraiseql.type
class Author:
    id: int
    name: str
    
    @dataloader_field
    async def books(self, info) -> list['Book']:
        return await Book.get_by_author_id(self.id)
        
@fraiseql.type  
class Book:
    id: int
    title: str
    author_id: int
    
    @dataloader_field
    async def author(self, info) -> Author:
        return await Author.get_by_id(self.author_id)
```

### Many-to-Many Relationships

Handle complex relationships:

```python
@fraiseql.type
class User:
    id: int
    
    @dataloader_field
    async def groups(self, info) -> list['Group']:
        return await Group.get_by_user_id(self.id)
        
@fraiseql.type
class Group:
    id: int
    
    @dataloader_field
    async def members(self, info) -> list['User']:
        return await User.get_by_group_id(self.id)
```

### Aggregation Queries

Batch aggregation operations:

```python
@fraiseql.type
class User:
    id: int
    
    @dataloader_field
    async def post_count(self, info) -> int:
        counts = await Post.count_by_user_ids([self.id])
        return counts.get(self.id, 0)
```

## Advanced DataLoader Features

### Custom Key Functions

Use custom keys for complex loading:

```python
from fraiseql.optimization import DataLoader

async def batch_load_user_posts(keys: list[tuple[int, str]]) -> list[list['Post']]:
    """Load posts by user_id and status."""
    user_ids, statuses = zip(*keys)
    posts = await Post.get_by_user_ids_and_statuses(user_ids, statuses)
    return [posts.get(key, []) for key in keys]

posts_by_status_loader = DataLoader(
    batch_load_user_posts,
    key_fn=lambda user_id, status: (user_id, status)
)

@fraiseql.type
class User:
    id: int
    
    @fraiseql.field
    async def published_posts(self, info) -> list['Post']:
        return await posts_by_status_loader.load((self.id, "published"))
```

### Cache Configuration

Configure caching behavior:

```python
from fraiseql.optimization import DataLoader

# Disable caching
no_cache_loader = DataLoader(batch_load_posts, cache=False)

# Custom cache size
limited_cache_loader = DataLoader(batch_load_posts, max_cache_size=100)

# Custom cache key function
custom_cache_loader = DataLoader(
    batch_load_posts,
    cache_key_fn=lambda user_id: f"posts:{user_id}"
)
```

### Error Handling

Handle errors in batch loading:

```python
async def batch_load_posts_with_errors(user_ids: list[int]) -> list[list['Post'] | Exception]:
    """Handle errors for individual keys."""
    results = []
    for user_id in user_ids:
        try:
            posts = await Post.get_by_user_id(user_id)
            results.append(posts)
        except Exception as e:
            results.append(e)  # Return error for this key
    return results
```

## Performance Optimization

### Batch Size Limits

Control batch sizes:

```python
from fraiseql.optimization import DataLoader

# Limit batch size to prevent memory issues
limited_batch_loader = DataLoader(
    batch_load_posts,
    max_batch_size=50
)
```

### Request-Scoped Loaders

Use request-scoped loaders for better cache behavior:

```python
from fraiseql.optimization import get_request_loader

@fraiseql.type
class User:
    id: int
    
    @fraiseql.field
    async def posts(self, info) -> list['Post']:
        loader = get_request_loader(info.context, 'posts', batch_load_posts)
        return await loader.load(self.id)
```

### Preloading Data

Preload data to prime the cache:

```python
async def preload_user_data(info, user_ids: list[int]):
    """Preload commonly accessed data."""
    posts_loader = get_request_loader(info.context, 'posts', batch_load_posts)
    profiles_loader = get_request_loader(info.context, 'profiles', batch_load_profiles)
    
    # Preload all data
    await posts_loader.load_many(user_ids)
    await profiles_loader.load_many(user_ids)
```

## Database Integration

### PostgreSQL Example

Efficient batch loading with PostgreSQL:

```python
async def batch_load_posts(user_ids: list[int]) -> list[list['Post']]:
    """Load posts using a single PostgreSQL query."""
    query = """
        SELECT user_id, json_agg(
            json_build_object(
                'id', id,
                'title', title,
                'content', content
            )
        ) as posts
        FROM posts 
        WHERE user_id = ANY($1)
        GROUP BY user_id
    """
    
    async with get_db_connection() as conn:
        rows = await conn.fetch(query, user_ids)
        posts_by_user = {row['user_id']: row['posts'] for row in rows}
        
        return [
            [Post(**post) for post in posts_by_user.get(user_id, [])]
            for user_id in user_ids
        ]
```

### With Raw SQL

Use raw SQL for complex queries:

```python
async def batch_load_user_stats(user_ids: list[int]) -> list[dict]:
    """Load aggregated user statistics."""
    query = """
        SELECT 
            u.id,
            COUNT(p.id) as post_count,
            COUNT(c.id) as comment_count,
            MAX(p.created_at) as last_post_date
        FROM users u
        LEFT JOIN posts p ON p.user_id = u.id
        LEFT JOIN comments c ON c.user_id = u.id
        WHERE u.id = ANY($1)
        GROUP BY u.id
    """
    
    async with get_db_connection() as conn:
        rows = await conn.fetch(query, user_ids)
        stats_by_user = {row['id']: dict(row) for row in rows}
        
        return [stats_by_user.get(user_id, {}) for user_id in user_ids]
```

## Testing DataLoaders

### Unit Testing

Test DataLoader behavior in isolation:

```python
import pytest
from fraiseql.optimization import DataLoader

@pytest.mark.asyncio
async def test_posts_dataloader():
    """Test that posts are batched correctly."""
    calls = []
    
    async def mock_batch_load(user_ids):
        calls.append(user_ids)
        return [[] for _ in user_ids]  # Empty results
    
    loader = DataLoader(mock_batch_load)
    
    # Make multiple loads
    await loader.load(1)
    await loader.load(2)
    await loader.load(3)
    
    # Should be batched into a single call
    assert len(calls) == 1
    assert calls[0] == [1, 2, 3]
```

### Integration Testing

Test DataLoaders with real data:

```python
@pytest.mark.asyncio
async def test_user_posts_integration(db_session):
    """Test user posts loading with real database."""
    # Create test data
    user1 = await User.create(name="User 1")
    user2 = await User.create(name="User 2")
    
    post1 = await Post.create(title="Post 1", user_id=user1.id)
    post2 = await Post.create(title="Post 2", user_id=user1.id)
    post3 = await Post.create(title="Post 3", user_id=user2.id)
    
    # Test DataLoader
    loader = DataLoader(batch_load_posts)
    
    posts1 = await loader.load(user1.id)
    posts2 = await loader.load(user2.id)
    
    assert len(posts1) == 2
    assert len(posts2) == 1
    assert posts1[0].title == "Post 1"
```

## Best Practices

1. **Always batch similar operations**: Group related data loading
2. **Use appropriate cache settings**: Consider request lifecycle
3. **Handle errors gracefully**: Don't let one bad key break the batch
4. **Monitor performance**: Track batch sizes and execution times
5. **Test thoroughly**: Verify batching behavior and correctness
6. **Use request-scoped loaders**: Avoid cache pollution between requests
7. **Consider memory usage**: Limit batch sizes for large datasets

## Common Pitfalls

### Cache Invalidation

Be careful with cached data:

```python
# Problem: Stale data after mutations
@fraiseql.mutation
async def update_user(info, id: int, name: str) -> User:
    user = await User.update(id, name=name)
    
    # Clear relevant caches
    users_loader = get_request_loader(info.context, 'users', batch_load_users)
    users_loader.clear(id)
    
    return user
```

### Over-batching

Don't batch everything:

```python
# Good: Batch similar operations
@dataloader_field
async def posts(self, info) -> list['Post']:
    return await Post.get_by_user_id(self.id)

# Bad: Don't batch single-item lookups
@fraiseql.field  # Use regular field instead
async def profile(self, info) -> 'UserProfile':
    return await UserProfile.get_by_user_id(self.id)
```

### Memory Leaks

Clear caches appropriately:

```python
# Clear caches at end of request
@fraiseql.middleware
async def clear_dataloader_caches(request, call_next):
    response = await call_next(request)
    
    # Clear all DataLoader caches
    if hasattr(request.state, 'dataloaders'):
        for loader in request.state.dataloaders.values():
            loader.clear_all()
    
    return response
```

## Monitoring and Debugging

### Performance Metrics

Track DataLoader performance:

```python
import time
from fraiseql.optimization import DataLoader

class MetricsDataLoader(DataLoader):
    """DataLoader with performance metrics."""
    
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.batch_count = 0
        self.total_keys = 0
        self.total_time = 0
    
    async def _dispatch_batch(self):
        start_time = time.time()
        result = await super()._dispatch_batch()
        
        self.batch_count += 1
        self.total_keys += len(self._batch)
        self.total_time += time.time() - start_time
        
        return result
```

### Debug Logging

Add logging to track DataLoader behavior:

```python
import logging

logger = logging.getLogger(__name__)

async def batch_load_posts_with_logging(user_ids: list[int]) -> list[list['Post']]:
    """Batch load posts with debug logging."""
    logger.info(f"Batch loading posts for {len(user_ids)} users: {user_ids}")
    
    start_time = time.time()
    results = await batch_load_posts(user_ids)
    elapsed = time.time() - start_time
    
    logger.info(f"Batch load completed in {elapsed:.3f}s")
    return results
```