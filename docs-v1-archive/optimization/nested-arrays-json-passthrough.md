# Nested Arrays with JSON Passthrough

**Complete guide to embedding arrays of objects in JSONB for maximum performance with FraiseQL.**

## Quick Reference

| Pattern | View Pattern | Python Type | Use Case |
|---------|-------------|-------------|----------|
| **Single nested object** | `'author', v_user.data` | `author: User` | 1-to-1 relationships |
| **Array of objects** | `'posts', posts_agg.data` | `posts: list[Post]` | 1-to-many relationships |
| **Multiple arrays** | Multiple aggregation views | Multiple `list[Type]` fields | Complex nested data |
| **Hierarchical** | Nested aggregation | `comments: list[Comment]` with `replies: list[Reply]` | Tree structures |

**Key Requirements:**
- ✅ Embedded type has `@fraiseql.type` (no `sql_source`)
- ✅ Parent type has `resolve_nested=False`
- ✅ View uses `jsonb_agg()` with `COALESCE(..., '[]'::jsonb)`
- ✅ Array limited in size (LIMIT in subquery)

## Overview

One of FraiseQL's most powerful but underdocumented features is **automatic deserialization of nested arrays** from JSONB. This pattern eliminates N+1 queries while maintaining sub-millisecond response times through JSON passthrough optimization.

### Performance Comparison

| Pattern | N+1 Queries | Response Time | Complexity |
|---------|-------------|---------------|------------|
| **Nested Arrays (This Guide)** | ❌ Zero | 0.5-2ms | Medium |
| DataLoader | ❌ Zero (batched) | 5-15ms | High |
| Separate Queries | ✅ N+1 | 50-500ms | Low |

## The Pattern

### Database Structure

**Step 1: Create Aggregation View**

Create a helper view that aggregates related objects into a JSONB array:

```sql
-- Helper view: Aggregate posts per user
CREATE OR REPLACE VIEW v_posts_per_user AS
WITH aggregated AS (
    SELECT
        p.user_id,
        jsonb_agg(
            jsonb_build_object(
                'id', p.id,
                'title', p.title,
                'content', LEFT(p.content, 200),
                'created_at', p.created_at::text
            )
            ORDER BY p.created_at DESC
        ) FILTER (WHERE p.id IS NOT NULL) AS posts_array
    FROM posts p
    WHERE p.is_published = true
    GROUP BY p.user_id
)
SELECT
    user_id AS id,
    COALESCE(posts_array, '[]'::jsonb) AS data
FROM aggregated;
```

**Step 2: Embed Array in Main View**

Join the aggregation view and embed the array directly:

```sql
-- Main view: User with embedded posts
CREATE OR REPLACE VIEW v_user_with_posts AS
SELECT
    u.id,
    u.email,
    u.is_active,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'is_active', u.is_active,
        'posts', COALESCE(posts.data, '[]'::jsonb)  -- ← Embedded array
    ) AS data
FROM users u
LEFT JOIN v_posts_per_user posts
    ON u.id = posts.id;
```

### Python Types

**Step 1: Define the Embedded Type**

Define the nested type WITHOUT `sql_source` (it's embedded, not queried separately):

```python
import fraiseql
from datetime import datetime

@fraiseql.type  # No sql_source - this is an embedded type
class EmbeddedPost:
    """Post embedded in user's JSONB data."""
    id: int
    title: str
    content: str
    created_at: datetime
```

**Step 2: Define the Parent Type**

The parent type has `sql_source` and includes the array field:

```python
@fraiseql.type(
    sql_source="v_user_with_posts",
    jsonb_column="data",
    resolve_nested=False  # Data is embedded, don't query separately
)
class User:
    """User with embedded posts (zero N+1 queries)."""
    id: int
    name: str
    email: str
    is_active: bool
    posts: list[EmbeddedPost]  # ← Automatically deserialized!
```

**Step 3: Use in Query**

Simple query - FraiseQL handles all the deserialization:

```python
@fraiseql.query
async def user_with_posts(info, id: int) -> User:
    """Get user with all their posts (zero N+1 queries)."""
    repo = info.context["repo"]
    return await repo.find_one("v_user_with_posts", id=id)
```

### GraphQL Query

```graphql
{
  userWithPosts(id: 1) {
    id
    name
    email
    posts {
      id
      title
      content
      createdAt
    }
  }
}
```

**Response (sub-millisecond with APQ):**
```json
{
  "data": {
    "userWithPosts": {
      "id": 1,
      "name": "Jane Doe",
      "email": "jane@example.com",
      "posts": [
        {
          "id": 101,
          "title": "My First Post",
          "content": "Hello world...",
          "createdAt": "2025-01-15T10:30:00Z"
        },
        {
          "id": 102,
          "title": "Second Post",
          "content": "More content...",
          "createdAt": "2025-01-16T14:20:00Z"
        }
      ]
    }
  }
}
```

## How It Works

### Automatic Deserialization Flow

1. **Database Query** executes and returns JSONB:
   ```sql
   SELECT data FROM v_user_with_posts WHERE id = 1;
   -- Returns: {"id": 1, "name": "Jane", "posts": [{"id": 101, "title": "..."}]}
   ```

2. **FraiseQL's `from_dict()`** method:
   - Detects `posts: list[EmbeddedPost]` type hint
   - Sees `posts` field in JSONB is an array
   - Iterates through array items
   - Calls `EmbeddedPost.from_dict()` for each item
   - Returns fully typed Python objects

3. **GraphQL** serializes the Python objects to GraphQL response

### Source Code Reference

The automatic deserialization is handled in `constructor.py`:

```python
def _process_field_value(value: Any, field_type: Any) -> Any:
    """Process field value based on type hint."""

    # Extract actual type from Optional
    actual_type = _extract_type(field_type)
    origin = typing.get_origin(actual_type)

    # Handle lists (THIS IS THE MAGIC)
    if origin is list:
        args = typing.get_args(actual_type)
        if args:
            item_type = args[0]
            if isinstance(value, list):
                # Recursively process each item
                return [_process_field_value(item, item_type) for item in value]

    # Handle FraiseQL types
    if hasattr(actual_type, "__fraiseql_definition__") and isinstance(value, dict):
        # Recursively instantiate nested object
        return actual_type.from_dict(value)

    return value
```

## Production Example: Network Configuration

This pattern is used in production in printoptim_backend:

### Database Views

```sql
-- Aggregation view: Print servers per network configuration
CREATE OR REPLACE VIEW v_print_servers_per_network_configuration AS
WITH combined AS (
    SELECT
        nc.pk_network_configuration AS id,
        jsonb_agg(ps.data) FILTER (WHERE ps.data IS NOT NULL) AS data_list
    FROM tb_network_configuration nc
    LEFT JOIN tb_network_configuration_print_server ncps
        ON nc.pk_network_configuration = ncps.fk_network_configuration
    LEFT JOIN v_print_server ps
        ON ncps.fk_print_server = ps.id
    GROUP BY nc.pk_network_configuration
)
SELECT
    id,
    COALESCE(data_list, '[]'::jsonb) AS data
FROM combined;

-- Main view: Network configuration with embedded print servers
CREATE OR REPLACE VIEW v_network_configuration AS
SELECT
    nc.pk_network_configuration AS id,
    nc.ip_address,
    nc.is_dhcp,
    jsonb_build_object(
        'id', nc.pk_network_configuration,
        'identifier', nc.identifier,
        'ip_address', host(nc.ip_address),
        'is_dhcp', nc.is_dhcp,
        'gateway', gateway.data,
        'router', router.data,
        'print_servers', print_servers.data  -- ← Embedded array
    ) AS data
FROM tb_network_configuration nc
LEFT JOIN v_gateway gateway ON nc.fk_gateway = gateway.id
LEFT JOIN v_router router ON nc.fk_router = router.id
LEFT JOIN v_print_servers_per_network_configuration print_servers
    ON nc.pk_network_configuration = print_servers.id;
```

### Python Types

```python
import fraiseql
from uuid import UUID

@fraiseql.type(sql_source="v_print_server")
class PrintServer:
    """Print server (can be queried independently OR embedded)."""
    id: UUID
    identifier: str
    hostname: str
    ip_address: str | None = None
    operating_system: str | None = None

@fraiseql.type(
    sql_source="v_network_configuration",
    jsonb_column="data",
    resolve_nested=False
)
class NetworkConfiguration:
    """Network configuration with embedded print servers."""
    id: UUID
    identifier: str
    ip_address: str | None = None
    is_dhcp: bool | None = None
    gateway: Gateway | None = None
    router: Router | None = None
    print_servers: list[PrintServer] | None = None  # ← Works automatically!
```

### GraphQL Query

```graphql
{
  networkConfiguration(id: "550e8400-e29b-41d4-a716-446655440000") {
    id
    identifier
    ipAddress
    isDhcp
    gateway {
      id
      hostname
    }
    printServers {
      id
      hostname
      ipAddress
    }
  }
}
```

**Performance**: 0.8-2ms with APQ cache hit

## Common Patterns

### Pattern 1: User with Posts and Comments

```sql
-- Aggregation views
CREATE VIEW v_posts_per_user AS
WITH agg AS (
    SELECT user_id, jsonb_agg(data ORDER BY created_at DESC) AS posts
    FROM v_post WHERE is_published = true
    GROUP BY user_id
)
SELECT user_id AS id, COALESCE(posts, '[]'::jsonb) AS data FROM agg;

CREATE VIEW v_comments_per_user AS
WITH agg AS (
    SELECT user_id, jsonb_agg(data ORDER BY created_at DESC) AS comments
    FROM v_comment
    GROUP BY user_id
)
SELECT user_id AS id, COALESCE(comments, '[]'::jsonb) AS data FROM agg;

-- Main view
CREATE VIEW v_user_full AS
SELECT
    u.id,
    u.email,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'posts', COALESCE(posts.data, '[]'::jsonb),
        'comments', COALESCE(comments.data, '[]'::jsonb)
    ) AS data
FROM users u
LEFT JOIN v_posts_per_user posts ON u.id = posts.id
LEFT JOIN v_comments_per_user comments ON u.id = comments.id;
```

```python
@fraiseql.type
class EmbeddedPost:
    id: int
    title: str
    excerpt: str

@fraiseql.type
class EmbeddedComment:
    id: int
    content: str
    post_id: int

@fraiseql.type(sql_source="v_user_full", resolve_nested=False)
class UserFull:
    id: int
    name: str
    email: str
    posts: list[EmbeddedPost]
    comments: list[EmbeddedComment]
```

### Pattern 2: Post with Author and Tags

```sql
-- Tags aggregation
CREATE VIEW v_tags_per_post AS
WITH agg AS (
    SELECT pt.post_id, jsonb_agg(
        jsonb_build_object('id', t.id, 'name', t.name, 'slug', t.slug)
        ORDER BY t.name
    ) AS tags
    FROM post_tags pt
    JOIN tags t ON pt.tag_id = t.id
    GROUP BY pt.post_id
)
SELECT post_id AS id, COALESCE(tags, '[]'::jsonb) AS data FROM agg;

-- Post with author and tags
CREATE VIEW v_post_full AS
SELECT
    p.id,
    p.author_id,
    p.is_published,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', author.data,  -- Single nested object
        'tags', COALESCE(tags.data, '[]'::jsonb)  -- Nested array
    ) AS data
FROM posts p
LEFT JOIN v_user author ON p.author_id = author.id
LEFT JOIN v_tags_per_post tags ON p.id = tags.id;
```

```python
@fraiseql.type
class EmbeddedTag:
    id: int
    name: str
    slug: str

@fraiseql.type
class Author:
    id: int
    name: str
    email: str

@fraiseql.type(sql_source="v_post_full", resolve_nested=False)
class PostFull:
    id: int
    title: str
    content: str
    author: Author  # Single nested object
    tags: list[EmbeddedTag]  # Nested array
```

### Pattern 3: Hierarchical Comments

```sql
-- Replies aggregation (one level deep)
CREATE VIEW v_replies_per_comment AS
WITH agg AS (
    SELECT parent_id, jsonb_agg(
        jsonb_build_object(
            'id', id,
            'content', content,
            'author_id', author_id,
            'created_at', created_at::text
        )
        ORDER BY created_at ASC
    ) AS replies
    FROM comments
    WHERE parent_id IS NOT NULL
    GROUP BY parent_id
)
SELECT parent_id AS id, COALESCE(replies, '[]'::jsonb) AS data FROM agg;

-- Comment with nested replies
CREATE VIEW v_comment_with_replies AS
SELECT
    c.id,
    c.post_id,
    c.author_id,
    jsonb_build_object(
        'id', c.id,
        'content', c.content,
        'author', author.data,
        'replies', COALESCE(replies.data, '[]'::jsonb)
    ) AS data
FROM comments c
LEFT JOIN v_user author ON c.author_id = author.id
LEFT JOIN v_replies_per_comment replies ON c.id = replies.id
WHERE c.parent_id IS NULL;  -- Only top-level comments
```

```python
@fraiseql.type
class EmbeddedReply:
    id: int
    content: str
    author_id: int
    created_at: datetime

@fraiseql.type(sql_source="v_comment_with_replies", resolve_nested=False)
class CommentWithReplies:
    id: int
    content: str
    author: User
    replies: list[EmbeddedReply]
```

## Best Practices

### ✅ DO: Use Aggregation Views

```sql
-- ✅ GOOD: Separate aggregation view
CREATE VIEW v_posts_per_user AS
SELECT user_id AS id,
    jsonb_agg(v_post.data) AS data
FROM v_post
GROUP BY user_id;

-- Then join in main view
SELECT u.id, 'posts', posts.data AS data
FROM users u
LEFT JOIN v_posts_per_user posts ON u.id = posts.id;
```

```sql
-- ❌ BAD: Inline aggregation (harder to maintain, test, reuse)
SELECT u.id,
    'posts', (
        SELECT jsonb_agg(jsonb_build_object(...))
        FROM posts WHERE user_id = u.id
    ) AS data
FROM users u;
```

### ✅ DO: Use COALESCE for Empty Arrays

```sql
-- ✅ GOOD: Returns [] not null
'posts', COALESCE(posts.data, '[]'::jsonb)

-- ❌ BAD: Returns null if no posts
'posts', posts.data
```

### ✅ DO: Use FILTER for Conditional Aggregation

```sql
-- ✅ GOOD: Excludes NULL rows from aggregation
jsonb_agg(v_post.data) FILTER (WHERE v_post.data IS NOT NULL)

-- ❌ BAD: Includes NULL as array element
jsonb_agg(v_post.data)
```

### ✅ DO: Limit Array Size

```sql
-- ✅ GOOD: Limit to recent items
CREATE VIEW v_recent_posts_per_user AS
WITH limited AS (
    SELECT *,
        ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY created_at DESC) AS rn
    FROM posts
)
SELECT user_id AS id,
    jsonb_agg(data) AS data
FROM limited
WHERE rn <= 10  -- Limit to 10 most recent
GROUP BY user_id;
```

### ✅ DO: Order Arrays Consistently

```sql
-- ✅ GOOD: Explicit ordering
jsonb_agg(v_post.data ORDER BY v_post.created_at DESC)

-- ❌ BAD: Undefined order
jsonb_agg(v_post.data)
```

### ✅ DO: Define Embedded Types Without sql_source

```python
# ✅ GOOD: Embedded type (no sql_source)
@fraiseql.type
class EmbeddedPost:
    id: int
    title: str

# ❌ BAD: sql_source on embedded type
@fraiseql.type(sql_source="v_post")  # Wrong! This is embedded, not queried
class EmbeddedPost:
    id: int
    title: str
```

### ✅ DO: Use resolve_nested=False on Parent

```python
# ✅ GOOD: Data is embedded, don't query separately
@fraiseql.type(
    sql_source="v_user_with_posts",
    resolve_nested=False  # Important!
)
class User:
    posts: list[EmbeddedPost]

# ❌ BAD: resolve_nested=True causes N+1 queries
@fraiseql.type(
    sql_source="v_user_with_posts",
    resolve_nested=True  # Will try to query posts separately!
)
class User:
    posts: list[EmbeddedPost]
```

## Performance Tuning

### Index the Aggregation Join

```sql
-- Index the foreign key used in aggregation
CREATE INDEX idx_posts_user_id ON posts(user_id);

-- Composite index for filtered aggregations
CREATE INDEX idx_posts_user_published ON posts(user_id, created_at DESC)
    WHERE is_published = true;
```

### Use Materialized Views for Expensive Aggregations

```sql
-- For expensive aggregations, use materialized view
CREATE MATERIALIZED VIEW mv_user_with_posts AS
SELECT /* expensive aggregation here */;

CREATE UNIQUE INDEX idx_mv_user_with_posts_id
    ON mv_user_with_posts(id);

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_with_posts;
```

### Monitor Query Performance

```sql
-- Check query plan
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user_with_posts WHERE id = 1;

-- Look for:
-- ✅ Index Scan on aggregation join
-- ❌ Seq Scan (needs index)
-- ✅ Execution Time < 10ms (good)
-- ❌ Execution Time > 50ms (needs optimization)
```

## Troubleshooting

### Problem: "Type registry lookup not implemented. Registry size: 0"

**Symptoms:**
```json
{
  "data": { "user": null },
  "errors": [{
    "message": "Type registry lookup for mv_user_with_posts not implemented. Available views: []. Registry size: 0"
  }]
}
```

**Root Cause:** Mode configuration conflict between `environment="development"` and JSON passthrough settings.

**Why This Happens:**

FraiseQL has two execution modes:
1. **Development mode**: Instantiates Python objects from database rows (requires type registry)
2. **Production mode**: Returns JSONB directly without instantiation (no type registry needed)

When you configure:
```python
config = FraiseQLConfig(
    environment="development",  # ← Repository runs in development mode
    json_passthrough_enabled=True,  # ← Only applies in PRODUCTION mode!
)
```

The repository runs in development mode and tries to instantiate types, but JSON passthrough is NOT enabled because it only activates in production mode.

**Solution 1 (RECOMMENDED):** Use Production Mode

Change `environment` to `"production"`:

```python
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="production",  # ← Use production mode
    json_passthrough_enabled=True,
    json_passthrough_in_production=True,
    apq_storage_backend="memory",
    enable_turbo_router=True,
)
```

**Why this works:**
- Repository runs in production mode
- Returns JSONB directly → GraphQL handles deserialization
- No type registry lookup needed
- Achieves 0.5-2ms response time (the intended performance)

**Solution 2:** Enable Debug Logging to Verify Registration

If Solution 1 doesn't work, verify types are being registered:

```python
import logging

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger("fraiseql")
logger.setLevel(logging.DEBUG)

# After decorating types
print(f"User has __fraiseql_definition__: {hasattr(User, '__fraiseql_definition__')}")
print(f"User sql_source: {User.__fraiseql_definition__.sql_source}")

# After creating app
from fraiseql.db import _type_registry
app = create_fraiseql_app(...)

print(f"Registry size: {len(_type_registry)}")
print(f"Registered views: {list(_type_registry.keys())}")
```

**Expected output:**
```
User has __fraiseql_definition__: True
User sql_source: mv_user_with_posts
Registry size: 1
Registered views: ['mv_user_with_posts']
```

**Solution 3:** Manual Registration (Development Mode Only)

If you MUST use development mode for debugging:

```python
from fraiseql.db import register_type_for_view

# Manually register the type
register_type_for_view(
    "mv_user_with_posts",
    User,
    table_columns={"id", "name", "email", "age", "city", "created_at", "data"},
    has_jsonb_data=True,
)

app = create_fraiseql_app(
    config=config,
    types=[User, EmbeddedPost],
    queries=[user],
)
```

**NOTE**: This should NOT be necessary! Types with `sql_source` are automatically registered during schema building. If manual registration is required, there may be an import order issue.

**Common Pitfalls:**

1. **Import Order**: Ensure types are fully decorated before passing to `create_fraiseql_app()`
2. **Multiple Installations**: Check `import fraiseql; print(fraiseql.__file__)` to verify you're using the correct installation
3. **Registry Cleared**: Check if `SchemaRegistry.clear()` is being called somewhere in your code

**Performance Note:** Production mode is the RECOMMENDED configuration for nested arrays as it provides the best performance (0.5-2ms) through JSON passthrough optimization.

---

### Problem: Nested Array Returns NULL Instead of Objects

**Symptoms:**
```json
{
  "user": {
    "posts": [null, null, null]  // Wrong!
  }
}
```

**Cause:** The embedded type doesn't have `@fraiseql.type` decorator

**Solution:**
```python
# ❌ WRONG: No decorator
class EmbeddedPost:
    id: int
    title: str

# ✅ CORRECT: Add decorator
@fraiseql.type
class EmbeddedPost:
    id: int
    title: str
```

### Problem: Empty Array Returns NULL

**Symptoms:**
```json
{
  "user": {
    "posts": null  // Should be []
  }
}
```

**Solution:** Use `COALESCE` in view:
```sql
-- ✅ CORRECT
'posts', COALESCE(posts.data, '[]'::jsonb)
```

### Problem: Field Missing from Nested Objects

**Symptoms:**
```json
{
  "posts": [
    {"id": 1, "title": null}  // title should have value
  ]
}
```

**Cause:** Field name mismatch between JSONB and Python type

**Solution:** Check field names match exactly:
```sql
-- View must match Python field names
jsonb_build_object(
    'id', p.id,
    'title', p.title,  -- Must match Python field name
    'created_at', p.created_at::text  -- Snake case, FraiseQL converts
)
```

```python
@fraiseql.type
class EmbeddedPost:
    id: int
    title: str  # Must match JSONB key
    created_at: datetime  # Matches 'created_at' from JSONB
```

### Problem: Slow Performance with Large Arrays

**Solution 1:** Limit array size in view:
```sql
WHERE row_number <= 10
```

**Solution 2:** Use pagination:
```python
@fraiseql.query
async def user_posts(
    info,
    user_id: int,
    limit: int = 20,
    offset: int = 0
) -> list[Post]:
    # Query posts separately for large datasets
    repo = info.context["repo"]
    return await repo.find(
        "v_post",
        where={"user_id": user_id},
        limit=limit,
        offset=offset
    )
```

## When NOT to Use This Pattern

### Use DataLoader Instead When:

1. **Arrays are very large** (> 100 items)
2. **Need pagination** on nested arrays
3. **Filtering nested items** by user input
4. **Multiple queries need same nested data**

### Use Separate Queries When:

1. **Nested data rarely needed**
2. **Client requests specific fields**
3. **Authorization varies per nested item**

## See Also

- [JSON Passthrough Optimization](json-passthrough-optimization.md) - Overview of JSON passthrough
- [Eliminating N+1 Queries](../performance/eliminating-n-plus-one.md) - DataLoader pattern
- [Database Views](../core-concepts/database-views.md) - View design patterns
- [Repository API](../api-reference/repository.md) - Query methods

---

**Key Takeaway**: Nested arrays with `list[EmbeddedType]` work automatically in FraiseQL when using the aggregation view pattern. This provides zero-N+1 performance with sub-millisecond response times.
