# FraiseQL Blog Simple - Trinity Identifiers Example

This example demonstrates FraiseQL with **Trinity Identifiers** - a three-tier ID system for optimal performance and usability.

## What is Trinity?

Trinity provides three types of identifiers for every entity:

1. **`pk_*` (INT GENERATED ALWAYS AS IDENTITY)** - Internal primary key for fast database joins
   - Modern PostgreSQL 10+ syntax (replaces deprecated SERIAL)
   - 4 bytes (vs 16 bytes for UUID)
   - Integer comparison (faster than UUID)
   - Better cache locality
   - **Not exposed in GraphQL API**

2. **`id` (UUID)** - Public API identifier
   - Secure (no enumeration attacks)
   - Globally unique
   - Exposed in GraphQL API

3. **`identifier` (TEXT)** - Human-readable URL slug
   - SEO-friendly URLs (`/users/@johndoe` vs `/users/550e8400-...`)
   - Memorable and shareable
   - Optional (some entities may not have slugs)
   - Exposed in GraphQL API

## Schema Structure

### Users Table
```sql
CREATE TABLE users (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast joins)
    id UUID NOT NULL DEFAULT uuid_generate_v4(),           -- Public API
    identifier TEXT UNIQUE,                                 -- URL slug (@username)
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    ...
);
```

### Posts Table
```sql
CREATE TABLE posts (
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast joins)
    id UUID NOT NULL DEFAULT uuid_generate_v4(),           -- Public API
    identifier TEXT UNIQUE,                                 -- URL slug (post-title)
    title TEXT NOT NULL,
    pk_author INT NOT NULL REFERENCES users(pk_user),      -- INT FK (fast!)
    ...
);
```

### Foreign Keys

All foreign keys use INT IDENTITY (not UUID) for faster joins:
- `posts.pk_author` → `users.pk_user`
- `comments.pk_post` → `posts.pk_post`
- `comments.pk_author` → `users.pk_user`
- `comments.pk_parent` → `comments.pk_comment`
- `post_tags.pk_post` → `posts.pk_post`
- `post_tags.pk_tag` → `tags.pk_tag`

## Performance Benefits

### Join Performance

INT IDENTITY joins are faster than UUID joins:
- **Index size**: 4 bytes vs 16 bytes (75% smaller)
- **Comparison**: Integer comparison vs binary comparison
- **Cache**: Better CPU cache locality with sequential integers

**Note**: Actual performance gains vary by workload. Benchmark your specific queries to measure the improvement.

### Example: Complex Query

```sql
-- Query with multiple INT joins (modern IDENTITY syntax)
SELECT
    p.id, p.title, p.identifier,
    u.id as author_id, u.identifier as author_slug,
    COUNT(c.pk_comment) as comment_count
FROM posts p
JOIN users u ON u.pk_user = p.pk_author        -- INT join (fast)
LEFT JOIN comments c ON c.pk_post = p.pk_post  -- INT join (fast)
WHERE p.status = 'published'
GROUP BY p.pk_post, u.pk_user
ORDER BY p.created_at DESC;
```

This query uses INT IDENTITY foreign keys internally but returns UUID IDs for the API.

## GraphQL API

The GraphQL API exposes only `id` (UUID) and `identifier` (TEXT):

```graphql
query GetPost {
  post(identifier: "getting-started-with-fraiseql") {
    id            # UUID (e.g., "550e8400-e29b-41d4-a716-446655440000")
    identifier    # Text slug (e.g., "getting-started-with-fraiseql")
    title
    url           # Auto-generated: "/posts/getting-started-with-fraiseql"

    author {
      id          # UUID
      identifier  # @username
      username
      url         # "/users/@johndoe"
    }
  }
}
```

**Security**: The internal `pk_*` columns are never exposed in the API. Clients only see UUID and human-readable identifiers.

## Setup

### 1. Initialize Database

```bash
# Create database with Trinity schema
psql -U postgres -c "CREATE DATABASE fraiseql_blog_trinity"
psql -U postgres -d fraiseql_blog_trinity -f db/setup_trinity.sql

# Load seed data
psql -U postgres -d fraiseql_blog_trinity -f db/seed_data_trinity.sql
```

### 2. Run Application

```bash
# Install dependencies
pip install fraiseql

# Run with Trinity models
python app_trinity.py
```

## Python Models with Trinity

```python
from fraiseql.patterns import TrinityMixin

@fraiseql.type(sql_source="users")
class User(TrinityMixin):
    """User with Trinity identifiers."""

    # Public IDs (exposed in GraphQL)
    id: UUID
    identifier: str | None  # @username

    # User data
    username: str
    email: str

    # Internal pk_user is hidden (not exposed in GraphQL)
    # Accessible via self.get_internal_pk()

    @fraiseql.field
    async def posts(self, info: GraphQLResolveInfo) -> list[Post]:
        """User's posts via SERIAL FK."""
        db = info.context["db"]
        pk_user = self.get_internal_pk()  # Get SERIAL pk
        return await db.find("posts", pk_author=pk_user)  # Fast SERIAL join
```

## Querying by ID or Identifier

GraphQL queries support both UUID and text identifier:

```graphql
# Query by UUID
query {
  user(id: "550e8400-e29b-41d4-a716-446655440000") {
    username
  }
}

# Query by identifier (human-readable)
query {
  user(identifier: "johndoe") {  # Same as @johndoe
    username
  }
}

# Query post by slug
query {
  post(identifier: "getting-started-with-fraiseql") {
    title
  }
}
```

## URL Generation

Trinity models include a `url` field for SEO-friendly URLs:

```python
@fraiseql.field
async def url(self, info: GraphQLResolveInfo) -> str:
    """User profile URL."""
    return f"/users/{self.identifier}" if self.identifier else f"/users/{self.id}"
```

Example URLs:
- `/users/@johndoe` (human-readable)
- `/posts/getting-started-with-fraiseql` (SEO-friendly)
- `/tags/python` (memorable)

## Mutations with Trinity

When creating/updating entities, clients use UUIDs but the database uses SERIAL FKs internally:

```graphql
mutation CreateComment {
  createComment(input: {
    postId: "550e8400-e29b-41d4-a716-446655440000"  # UUID input
    content: "Great post!"
  }) {
    success {
      comment {
        id
        content
        post {
          identifier  # post-slug
        }
      }
    }
  }
}
```

Internally, the mutation:
1. Converts `postId` (UUID) to `pk_post` (SERIAL)
2. Uses `pk_post` for the foreign key insert (fast)
3. Returns the comment with UUID in the response

## Migration from UUID-only Schema

If you have an existing UUID-based schema:

1. **Add Trinity columns**:
```sql
ALTER TABLE users
    ADD COLUMN pk_user SERIAL UNIQUE,
    ADD COLUMN identifier TEXT UNIQUE;
```

2. **Add SERIAL foreign keys** (keep UUID FKs temporarily):
```sql
ALTER TABLE posts
    ADD COLUMN pk_author INT REFERENCES users(pk_user);

UPDATE posts p
SET pk_author = u.pk_user
FROM users u
WHERE p.author_id = u.id;  -- Backfill from UUID FK
```

3. **Update queries** to use SERIAL FKs

4. **Drop UUID FKs** after verification:
```sql
ALTER TABLE posts DROP COLUMN author_id;  -- Old UUID FK
```

## Files in This Example

- `db/setup_trinity.sql` - Trinity schema with SERIAL FKs
- `db/seed_data_trinity.sql` - Sample data
- `models_trinity.py` - Trinity-enabled Python models
- `app_trinity.py` - Application with Trinity support
- `README_TRINITY.md` - This file

## Benchmarking

To measure performance gains in your workload:

```python
import time
from statistics import mean

# Benchmark UUID join
uuid_times = []
for _ in range(100):
    start = time.time()
    await db.execute("SELECT * FROM posts p JOIN users u ON u.id = p.author_id_uuid LIMIT 100")
    uuid_times.append(time.time() - start)

# Benchmark SERIAL join
serial_times = []
for _ in range(100):
    start = time.time()
    await db.execute("SELECT * FROM posts p JOIN users u ON u.pk_user = p.pk_author LIMIT 100")
    serial_times.append(time.time() - start)

print(f"UUID avg: {mean(uuid_times)*1000:.2f}ms")
print(f"SERIAL avg: {mean(serial_times)*1000:.2f}ms")
print(f"Improvement: {mean(uuid_times)/mean(serial_times):.2f}x")
```

## Learn More

- [Trinity Pattern Documentation](/docs/patterns/trinity_identifiers.md)
- [POST_V1_ENHANCEMENTS.md](../../archive/planning/POST_V1_ENHANCEMENTS.md) - Full Trinity implementation guide
- [FraiseQL Documentation](https://github.com/anthropics/fraiseql)

## Summary

Trinity Identifiers provide:
- ✅ **Faster joins** (SERIAL vs UUID)
- ✅ **Smaller indexes** (4 bytes vs 16 bytes)
- ✅ **Better cache locality**
- ✅ **Secure public API** (UUID)
- ✅ **SEO-friendly URLs** (text identifiers)
- ✅ **No breaking changes** (GraphQL API unchanged)

The best of all worlds: performance, security, and usability!
