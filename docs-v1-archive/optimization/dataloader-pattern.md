# DataLoader Pattern

**Status:** ✅ Production-ready
**Added in:** v0.5.0
**Problem:** Solves N+1 query problems

## Overview

DataLoaders eliminate the N+1 query problem by batching and caching database requests within a single GraphQL operation. FraiseQL provides built-in DataLoader integration that's easy to use and highly performant.

## The N+1 Problem

### Without DataLoaders

```python
@fraiseql.type
class Post:
    id: str
    title: str

    @fraiseql.field
    async def author(self, info) -> User:
        db = info.context["db"]
        # This executes for EVERY post!
        return await db.find_one("v_user", id=self.author_id)

# Query for 100 posts:
# 1 query for posts + 100 queries for authors = 101 total queries ❌
```

**Performance Impact:**
```
Query for 100 posts with authors:
- Without DataLoader: 101 queries, ~500ms
- With DataLoader: 2 queries, ~50ms
- Improvement: 90% faster ⚡
```

### With DataLoaders

```python
@fraiseql.type
class Post:
    id: str
    title: str

    @fraiseql.dataloader_field
    async def author(self, info) -> User:
        db = info.context["db"]
        # Batched! Only executes once for all posts
        return await db.find_one("v_user", id=self.author_id)

# Query for 100 posts:
# 1 query for posts + 1 batched query for all authors = 2 total queries ✅
```

## Basic Usage

### Step 1: Import the Decorator

```python
from fraiseql import dataloader_field, type

@type
class Post:
    id: str
    title: str
    author_id: str
```

### Step 2: Apply `@dataloader_field`

```python
@type
class Post:
    id: str
    title: str
    author_id: str

    @dataloader_field
    async def author(self, info) -> User:
        db = info.context["db"]
        return await db.find_one("v_user", id=self.author_id)
```

That's it! FraiseQL automatically:
1. **Collects** all author IDs from the current request
2. **Batches** them into a single database query
3. **Caches** results for the request lifetime
4. **Distributes** results back to each Post

## How It Works

### Request Lifecycle

```
GraphQL Request
    ↓
1. Resolve posts
   posts = [Post(id=1, author_id=10), Post(id=2, author_id=11), ...]
    ↓
2. Collect DataLoader calls
   author_ids = [10, 11, 10, 12, 11]  # Duplicates possible
    ↓
3. Deduplicate
   unique_ids = [10, 11, 12]
    ↓
4. Batch query
   SELECT * FROM v_user WHERE id IN (10, 11, 12)
    ↓
5. Cache results
   {10: User(...), 11: User(...), 12: User(...)}
    ↓
6. Distribute to fields
   Post(id=1).author = cached[10]
   Post(id=2).author = cached[11]
   ...
```

### Automatic Batching

FraiseQL waits for all field resolvers in the current "tick" to collect their requests:

```python
# Single GraphQL query
query {
  posts {
    id
    title
    author { id name }        # Batched!
    comments {
      id
      author { id name }      # Also batched with post authors!
    }
  }
}

# Results in just 3 queries:
# 1. SELECT posts
# 2. SELECT comments WHERE post_id IN (...)
# 3. SELECT users WHERE id IN (...)  ← All authors batched together!
```

## Advanced Patterns

### Custom Batch Loader

For complex loading logic, provide a custom batch function:

```python
from fraiseql import dataloader_field

async def load_users_batch(db, ids: list[str]) -> list[User]:
    """Custom batch loader with complex logic."""
    # Batch load with custom SQL
    users = await db.execute("""
        SELECT * FROM v_user_extended
        WHERE id = ANY($1)
        ORDER BY last_active DESC
    """, ids)

    # Return in same order as requested IDs
    user_map = {u.id: u for u in users}
    return [user_map.get(id) for id in ids]

@type
class Post:
    @dataloader_field(batch_loader=load_users_batch)
    async def author(self, info) -> User:
        db = info.context["db"]
        return await load_users_batch(db, [self.author_id])
```

### Nested DataLoaders

DataLoaders work seamlessly with nested relationships:

```python
@type
class User:
    @dataloader_field
    async def posts(self, info) -> list[Post]:
        db = info.context["db"]
        return await db.find("v_post", author_id=self.id)

@type
class Post:
    @dataloader_field
    async def author(self, info) -> User:
        db = info.context["db"]
        return await db.find_one("v_user", id=self.author_id)

    @dataloader_field
    async def comments(self, info) -> list[Comment]:
        db = info.context["db"]
        return await db.find("v_comment", post_id=self.id)

@type
class Comment:
    @dataloader_field
    async def author(self, info) -> User:
        db = info.context["db"]
        return await db.find_one("v_user", id=self.author_id)

# Query with 3 levels of nesting:
# users { posts { author comments { author } } }
#
# Without DataLoaders: 1 + N + N*M + N*M*P queries
# With DataLoaders: ~4 queries (users, posts, comments, all authors batched)
```

### Conditional Loading

Load data conditionally while maintaining batching:

```python
@type
class Post:
    @dataloader_field
    async def author(self, info) -> User | None:
        if not self.author_id:
            return None  # No database call

        db = info.context["db"]
        return await db.find_one("v_user", id=self.author_id)
```

### Multi-Field Batching

Batch multiple related fields together:

```python
@type
class Post:
    @dataloader_field
    async def author(self, info) -> User:
        db = info.context["db"]
        return await db.find_one("v_user", id=self.author_id)

    @dataloader_field
    async def editor(self, info) -> User | None:
        if not self.editor_id:
            return None
        db = info.context["db"]
        # Batched together with 'author' field!
        return await db.find_one("v_user", id=self.editor_id)

# Both fields use the same DataLoader instance
# Result: Single batched query for all users
```

## PostgreSQL Optimization

### Use `ANY()` for Batch Queries

```sql
-- ✅ GOOD: Efficient batch query with ANY
SELECT * FROM v_user
WHERE id = ANY($1::uuid[]);

-- ❌ BAD: Inefficient with IN
SELECT * FROM v_user
WHERE id IN (?, ?, ?, ...);  -- Variable parameter count
```

### Create Batch-Optimized Views

```sql
-- View optimized for batch loading
CREATE VIEW v_user_with_stats AS
SELECT
    u.id,
    u.name,
    u.email,
    count(p.id) as post_count,
    max(p.created_at) as last_post_at
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
GROUP BY u.id;

-- Index for batch queries
CREATE INDEX idx_user_batch ON users USING btree (id);
```

### Batch Size Limits

Handle large batch sizes gracefully:

```python
async def load_users_batch(db, ids: list[str]) -> list[User]:
    # PostgreSQL performs well up to ~1000 parameters
    if len(ids) > 1000:
        # Split into chunks if needed
        chunks = [ids[i:i+1000] for i in range(0, len(ids), 1000)]
        results = []
        for chunk in chunks:
            results.extend(await db.find("v_user", id_in=chunk))
        return results

    return await db.find("v_user", id_in=ids)
```

## Performance Monitoring

### Enable DataLoader Logging

```python
import logging

logging.getLogger("fraiseql.optimization.dataloader").setLevel(logging.DEBUG)

# Logs show:
# DEBUG: DataLoader[User]: Batched 45 IDs into 1 query
# DEBUG: DataLoader[User]: Query took 12ms, cache hit rate: 23%
```

### Track Batch Efficiency

```python
from fraiseql.monitoring import dataloader_stats

stats = dataloader_stats()
print(f"Average batch size: {stats['avg_batch_size']}")
print(f"Cache hit rate: {stats['cache_hit_rate']:.1%}")
print(f"Total queries saved: {stats['queries_avoided']}")
```

### Prometheus Metrics

```python
# Available metrics
fraiseql_dataloader_batch_size{loader="User"}
fraiseql_dataloader_cache_hits_total{loader="User"}
fraiseql_dataloader_query_duration_seconds{loader="User"}
```

## Common Patterns

### 1. One-to-Many Relationships

```python
@type
class User:
    @dataloader_field
    async def posts(self, info) -> list[Post]:
        db = info.context["db"]
        return await db.find("v_post", author_id=self.id)
```

### 2. Many-to-Many Relationships

```python
@type
class Post:
    @dataloader_field
    async def tags(self, info) -> list[Tag]:
        db = info.context["db"]
        # Uses junction table
        tag_ids = await db.execute("""
            SELECT tag_id FROM post_tags WHERE post_id = $1
        """, self.id)
        return await db.find("v_tag", id_in=[t['tag_id'] for t in tag_ids])
```

### 3. Computed Fields

```python
@type
class User:
    @dataloader_field
    async def post_count(self, info) -> int:
        db = info.context["db"]
        result = await db.execute("""
            SELECT count(*) as cnt FROM posts WHERE author_id = $1
        """, self.id)
        return result[0]['cnt']
```

## Best Practices

### 1. Always Use DataLoaders for Relations

```python
# ✅ GOOD: Uses DataLoader
@dataloader_field
async def author(self, info) -> User:
    ...

# ❌ BAD: Direct database call (N+1 problem)
@fraiseql.field
async def author(self, info) -> User:
    return await db.find_one("v_user", id=self.author_id)
```

### 2. Keep Batch Functions Pure

```python
# ✅ GOOD: Pure function, predictable
async def load_users(db, ids):
    return await db.find("v_user", id_in=ids)

# ❌ BAD: Side effects, unpredictable
async def load_users(db, ids):
    await log_access(ids)  # Side effect!
    return await db.find("v_user", id_in=ids)
```

### 3. Handle Missing Data

```python
async def load_users_batch(db, ids: list[str]) -> list[User | None]:
    users = await db.find("v_user", id_in=ids)
    user_map = {u.id: u for u in users}
    # Return None for missing users (maintains order)
    return [user_map.get(id) for id in ids]
```

### 4. Use Type Hints

```python
from typing import List

@dataloader_field
async def posts(self, info) -> List[Post]:  # Clear return type
    ...
```

## Troubleshooting

### DataLoader Not Batching

**Symptom:** Still seeing N+1 queries

**Solution:** Check decorator is `@dataloader_field`, not `@field`:

```python
# ✅ CORRECT
@dataloader_field
async def author(self, info) -> User:
    ...

# ❌ WRONG
@fraiseql.field
async def author(self, info) -> User:
    ...
```

### Incorrect Result Order

**Symptom:** Wrong data returned to fields

**Cause:** Batch function not maintaining order

**Solution:** Return results in same order as IDs:

```python
async def load_batch(db, ids):
    items = await db.find("v_item", id_in=ids)
    item_map = {item.id: item for item in items}
    # CRITICAL: Return in same order as input IDs
    return [item_map.get(id) for id in ids]
```

### Memory Issues with Large Batches

**Symptom:** High memory usage

**Solution:** Implement batch size limits:

```python
MAX_BATCH_SIZE = 1000

async def load_batch(db, ids):
    if len(ids) > MAX_BATCH_SIZE:
        # Process in chunks
        ...
    return await db.find("v_item", id_in=ids)
```

## Performance Comparison

### Real-World Example

```python
# Query: 100 blog posts with authors, comments, and tags

# Without DataLoaders:
# - 1 query for posts
# - 100 queries for post authors
# - 100 queries for post comment lists
# - ~500 queries for comment authors (5 comments per post avg)
# - 100 queries for post tags
# Total: 801 queries, ~4000ms

# With DataLoaders:
# - 1 query for posts
# - 1 batched query for all post authors
# - 1 batched query for all comments
# - 1 batched query for all comment authors
# - 1 batched query for all tags
# Total: 5 queries, ~50ms

# Improvement: 99% fewer queries, 98.75% faster! 🚀
```

## See Also

- [Eliminating N+1 Queries](eliminating-n-plus-one.md)
- [Performance Optimization](performance.md)
- [Database View Optimization](../core-concepts/database-views.md)
- [GraphQL Field Resolvers](../api-reference/decorators.md#field)

---

**DataLoaders are essential for production GraphQL APIs. Use `@dataloader_field` for all relationship fields to eliminate N+1 queries and achieve optimal performance.**
