# Migrating from Strawberry GraphQL

This guide will help you migrate your existing Strawberry GraphQL API to FraiseQL while maintaining GraphQL schema compatibility.

## Overview

FraiseQL is designed as a drop-in replacement for Strawberry GraphQL with significant performance and architectural improvements. The migration focuses on:

1. **Type definitions** - Converting `@strawberry.type` to `@fraiseql.type`
2. **Database integration** - Moving from dataloaders to database views
3. **Resolvers** - Simplifying from complex resolvers to view-based queries
4. **Performance** - Eliminating N+1 queries through architectural changes

## Phase 1: Assessment

### Analyze Your Current Schema

First, understand your current Strawberry implementation:

```python
# Existing Strawberry code
import strawberry
from typing import List, Optional

@strawberry.type
class User:
    id: int
    name: str
    email: str
    posts: List["Post"]

@strawberry.type
class Post:
    id: int
    title: str
    content: str
    author: User

@strawberry.type
class Query:
    @strawberry.field
    async def users(self) -> List[User]:
        # Complex dataloader logic
        return await get_users_with_posts()
```

### Identify Migration Points

- **Types**: Convert decorators and add field metadata
- **Resolvers**: Replace complex logic with view queries
- **Database**: Create views that return JSONB data
- **Authentication**: Migrate to FraiseQL auth system

## Phase 2: Database Migration

### Create Database Views

Transform your existing database schema to use FraiseQL's view-based approach:

```sql
-- Create views that return JSONB data
CREATE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at,
        'posts', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'content', p.content,
                    'created_at', p.created_at
                )
            )
            FROM posts p
            WHERE p.author_id = users.id
        )
    ) as data
FROM users;

CREATE VIEW v_posts AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author_id', p.author_id,
        'created_at', p.created_at,
        'author', (
            SELECT jsonb_build_object(
                'id', u.id,
                'name', u.name,
                'email', u.email
            )
            FROM users u
            WHERE u.id = p.author_id
        )
    ) as data
FROM posts p;
```

## Phase 3: Type Definition Migration

### Convert Strawberry Types to FraiseQL

```python
# Before: Strawberry
import strawberry
from typing import List, Optional

@strawberry.type
class User:
    id: int
    name: str
    email: str

    @strawberry.field
    async def posts(self) -> List["Post"]:
        # Complex resolver logic
        return await get_posts_for_user(self.id)

# After: FraiseQL
import fraiseql
from fraiseql import fraise_field
from datetime import datetime

@fraiseql.type
class User:
    """A user in the system."""
    id: int
    name: str = fraise_field(description="User's display name")
    email: str = fraise_field(description="Email address")
    posts: list["Post"] = fraise_field(description="User's posts")
    created_at: datetime
```

### Key Changes

1. **Import change**: `strawberry` → `fraiseql`
2. **Decorator change**: `@strawberry.type` → `@fraiseql.type`
3. **Field metadata**: Add `fraise_field()` for descriptions and configuration
4. **Remove complex resolvers**: Relationships handled by views
5. **Add timestamp fields**: Include audit fields from database

## Phase 4: Resolver Migration

### Simplify Query Resolvers

```python
# Before: Complex Strawberry resolvers
@strawberry.type
class Query:
    @strawberry.field
    async def users(self, info) -> List[User]:
        # Complex dataloader logic to avoid N+1
        async with get_db_connection() as conn:
            users = await conn.fetch("SELECT * FROM users")
            user_ids = [u["id"] for u in users]
            posts = await conn.fetch(
                "SELECT * FROM posts WHERE author_id = ANY($1)",
                user_ids
            )
            # Complex grouping and object construction
            return build_users_with_posts(users, posts)

# After: Simple FraiseQL resolvers
@fraiseql.type
class Query:
    @fraiseql.field
    async def users(self, info: fraiseql.Info) -> list[User]:
        """Get all users with their posts."""
        repo = CQRSRepository(info.context["db"])
        users_data = await repo.query("v_users")
        return [User.from_dict(data) for data in users_data]
```

### Migration Benefits

- **Eliminated N+1 queries**: Views handle relationships
- **Simpler code**: One query per resolver
- **Better performance**: Database handles optimization
- **Type safety**: Automatic validation and conversion

## Phase 5: Input and Mutation Migration

### Convert Input Types

```python
# Before: Strawberry input
@strawberry.input
class CreateUserInput:
    name: str
    email: str

# After: FraiseQL input (nearly identical)
@fraiseql.input
class CreateUserInput:
    name: str
    email: str
```

### Migrate Result Types

```python
# Before: Strawberry union/errors
@strawberry.type
class CreateUserSuccess:
    user: User

@strawberry.type
class CreateUserError:
    message: str

CreateUserResult = strawberry.union("CreateUserResult", [
    CreateUserSuccess,
    CreateUserError
])

# After: FraiseQL result pattern
@fraiseql.result
class CreateUserResult:
    """Result of creating a user."""

@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@fraiseql.failure
class CreateUserError:
    message: str
    code: str
```

## Phase 6: Authentication Migration

### From Strawberry Extensions to FraiseQL Auth

```python
# Before: Strawberry extensions
from strawberry.extensions import Extension

class AuthExtension(Extension):
    async def resolve(self, next_, root, info, **kwargs):
        # Custom auth logic
        if not is_authenticated(info.context["request"]):
            raise Exception("Unauthorized")
        return await next_(root, info, **kwargs)

# After: FraiseQL decorators
from fraiseql.auth import requires_auth

@fraiseql.type
class Query:
    @fraiseql.field
    @requires_auth
    async def me(self, info: fraiseql.Info) -> User:
        """Get current user."""
        user_context = info.context["user"]
        repo = CQRSRepository(info.context["db"])
        user_data = await repo.get_by_id("v_users", user_context.user_id)
        return User.from_dict(user_data)
```

## Phase 7: Application Migration

### FastAPI Integration

```python
# Before: Strawberry with FastAPI
from strawberry.fastapi import GraphQLRouter

app = FastAPI()
graphql_app = GraphQLRouter(schema)
app.include_router(graphql_app, prefix="/graphql")

# After: FraiseQL application
import fraiseql

app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post, Query, Mutation],
    auto_camel_case=True,
    production=False  # Development mode
)
```

## Phase 8: Testing Migration

### Update Test Suite

```python
# Before: Strawberry testing
import strawberry
from strawberry.test import BaseGraphQLTestClient

client = BaseGraphQLTestClient(schema)
result = client.query("""
    query {
        users {
            id
            name
        }
    }
""")

# After: FraiseQL testing
import pytest
from httpx import AsyncClient

@pytest.mark.asyncio
async def test_users_query():
    async with AsyncClient(app=app, base_url="http://test") as client:
        response = await client.post("/graphql", json={
            "query": """
                query {
                    users {
                        id
                        name
                    }
                }
            """
        })
        assert response.status_code == 200
        data = response.json()
        assert "users" in data["data"]
```

## Migration Checklist

### Pre-Migration
- [ ] **Audit current schema** and identify all types
- [ ] **Document current resolvers** and their complexity
- [ ] **Plan database views** for each type
- [ ] **Set up FraiseQL development environment**

### Database Migration
- [ ] **Create JSONB views** for all types
- [ ] **Test view performance** with realistic data
- [ ] **Add appropriate indexes** for query optimization
- [ ] **Verify data integrity** in views

### Code Migration
- [ ] **Convert type decorators** from Strawberry to FraiseQL
- [ ] **Add field metadata** with `fraise_field()`
- [ ] **Simplify resolvers** to use repository pattern
- [ ] **Convert input types** and mutations
- [ ] **Migrate authentication** to FraiseQL decorators

### Testing and Validation
- [ ] **Update test suite** for new patterns
- [ ] **Verify GraphQL schema** compatibility
- [ ] **Performance test** with production data
- [ ] **Load test** critical endpoints

### Deployment
- [ ] **Deploy to staging** environment
- [ ] **Monitor performance** and errors
- [ ] **Gradual rollout** to production
- [ ] **Monitor and optimize** post-migration

## Common Migration Challenges

### Complex Resolvers
**Problem**: Existing resolvers have complex business logic
**Solution**: Move business logic to PostgreSQL functions or separate service layer

### Custom Dataloaders
**Problem**: Heavy investment in dataloader infrastructure
**Solution**: Views eliminate need for dataloaders; complex cases can use materialized views

### Authentication Integration
**Problem**: Custom authentication system
**Solution**: Implement FraiseQL `AuthProvider` interface for existing auth

### Schema Breaking Changes
**Problem**: Concern about GraphQL schema changes
**Solution**: FraiseQL maintains schema compatibility; field names auto-convert

## Performance Comparison

Typical improvements after migration:

| Metric | Strawberry | FraiseQL | Improvement |
|--------|------------|----------|-------------|
| Simple queries | 1000 req/s | 5000 req/s | 5x faster |
| Complex queries | 100 req/s | 1000 req/s | 10x faster |
| Database queries | N queries | 1 query | N+1 eliminated |
| Memory usage | High | Low | 50-70% reduction |

## Support

If you encounter issues during migration:

1. **Review documentation** for specific patterns
2. **Check examples** in the repository
3. **Search issues** on GitHub
4. **Create issue** for migration-specific problems

FraiseQL's migration benefits - eliminated N+1 queries, simplified code, and better performance - make the migration effort worthwhile for most applications.
