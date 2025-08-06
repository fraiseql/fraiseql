# FraiseQL

**Build high-performance GraphQL APIs with PostgreSQL views and Python type safety.**

FraiseQL is a Python framework that combines the power of PostgreSQL views with GraphQL, giving you complete control over your API while leveraging your database's query optimization capabilities.

## What Makes FraiseQL Different?

### Before FraiseQL
```python
# Complex ORM queries, N+1 problems, manual optimization
users = db.query(User).options(
    joinedload(User.posts).joinedload(Post.comments)
).filter(User.active == True).all()

# Manual GraphQL resolvers for each field
@strawberry.field
async def posts(self, info) -> list[Post]:
    return await get_posts_for_user(self.id)  # N+1 query problem
```

### With FraiseQL
```sql
-- Create a PostgreSQL view that returns JSONB
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', u.id,
    'name', u.name,
    'posts', COALESCE(jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.title,
            'comments', p.comment_count
        )
    ) FILTER (WHERE p.id IS NOT NULL), '[]'::jsonb)
) AS data
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
GROUP BY u.id;
```

```python
# Define your GraphQL schema with Python types
from fraiseql import ID, FraiseQL

@fraiseql.type
class User:
    id: ID
    name: str
    posts: list[Post]

@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    return await repo.find("v_user")  # One query, fully optimized
```

## Key Benefits

### ⚡ 50-1000x Faster with Lazy Caching
- **TurboRouter**: Bypass GraphQL parsing for registered queries
- **Lazy Caching**: Pre-computed responses stored in PostgreSQL
- **Sub-millisecond**: Cache hits return instantly
- **Automatic invalidation**: Version tracking by bounded contexts

### 🎯 Type Safety Throughout
Modern Python typing (3.10+) ensures type safety from database to GraphQL schema.

### 🏗️ True CQRS Architecture
- **Queries**: Read from optimized views (`v_` prefix) and table views (`tv_` prefix)
- **Mutations**: Call PostgreSQL functions (`fn_` prefix)
- **Separation**: Storage model evolves independently from API model
- **Bounded contexts**: Clear domain boundaries with version tracking

### 🔒 Enterprise Security Built-in
Field-level authorization, rate limiting, CSRF protection, and SQL injection prevention out of the box.

### 📦 Database-Native Caching
- **No Redis/Memcached**: Cache lives in PostgreSQL
- **Historical data**: Cache becomes valuable audit trail
- **Version tracking**: Automatic invalidation on data changes
- **Multi-tenant**: Built-in cache isolation

## Quick Example

Let's build a blog API in under 5 minutes:

```python
# 1. Define your GraphQL types
from fraiseql import FraiseQL, ID
from datetime import datetime
from dataclasses import dataclass

app = FraiseQL(database_url="postgresql://localhost/blog")

@fraiseql.type
class Post:
    id: ID
    title: str
    content: str
    author: str
    created_at: datetime
    comment_count: int = 0

# 2. Create a PostgreSQL view
"""
CREATE VIEW v_post AS
SELECT jsonb_build_object(
    'id', p.id,
    'title', p.title,
    'content', p.content,
    'author', u.name,
    'created_at', p.created_at,
    'comment_count', COUNT(c.id)
) AS data
FROM posts p
JOIN users u ON u.id = p.user_id
LEFT JOIN comments c ON c.post_id = p.id
GROUP BY p.id, u.name;
"""

# 3. Define your GraphQL query
@app.query
async def posts(info, limit: int = 10) -> list[Post]:
    repo = info.context["repo"]
    return await repo.find("v_post", limit=limit)

# 4. Run it!
# uvicorn app:app --reload
```

That's it! You now have a GraphQL API with:
- Type-safe schema definition
- Optimized database queries
- GraphQL playground for testing
- Production-ready performance

## Core Philosophy

FraiseQL embraces **Database Domain-Driven Design**:

1. **PostgreSQL is your domain model** - Business logic lives in functions and views
2. **Views are your API projection** - Denormalized, optimized for reading
3. **Python defines your schema** - Explicit type definitions and GraphQL operations
4. **One source of truth** - PostgreSQL handles all data transformations

This means:
- ✅ No more N+1 queries
- ✅ No more complex ORM queries  
- ✅ No more manual query optimization
- ✅ Full control over your GraphQL schema

## Who Uses FraiseQL?

FraiseQL is perfect for teams who:
- Want to leverage PostgreSQL's full power
- Need high-performance GraphQL APIs
- Value type safety and explicit schema definition
- Are building multi-tenant SaaS applications
- Want to reduce infrastructure complexity

## Ready to Start?

<div class="grid cards" markdown>

-   :material-clock-fast:{ .lg .middle } **5-Minute Quickstart**

    ---

    Get your first API running in minutes

    [:octicons-arrow-right-24: Quick Start](getting-started/quickstart.md)

-   :material-book-open-variant:{ .lg .middle } **Learn Core Concepts**

    ---

    Understand the FraiseQL philosophy

    [:octicons-arrow-right-24: Core Concepts](core-concepts/index.md)

-   :material-school:{ .lg .middle } **Build a Blog API**

    ---

    Complete tutorial from scratch

    [:octicons-arrow-right-24: Tutorial](tutorials/blog-api.md)

-   :material-rocket:{ .lg .middle } **Deploy to Production**

    ---

    Best practices for production deployment

    [:octicons-arrow-right-24: Deployment](deployment/index.md)

</div>

## Community & Support

- **GitHub**: [github.com/fraiseql/fraiseql](https://github.com/fraiseql/fraiseql)
- **Documentation**: You're reading it!
- **Issues**: [Report bugs or request features](https://github.com/fraiseql/fraiseql/issues)

## License

FraiseQL is open source under the MIT license. Built with ❤️ for the PostgreSQL and GraphQL communities.