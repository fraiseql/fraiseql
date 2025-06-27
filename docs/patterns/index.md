# FraiseQL Patterns

Learn the core patterns that make FraiseQL unique and powerful.

## Core Patterns

### 🎯 [Query Patterns](./queries.md)
**The One True Pattern** - Learn how FraiseQL queries work and why they're different from traditional GraphQL.

- ✅ Using `@fraiseql.query` decorator
- ❌ Why `resolve_` methods don't work
- 📝 Complete examples with parameters, auth, and nesting
- 🔄 Migration from other GraphQL libraries

### 🗄️ [Database Patterns](./database.md)
**The JSONB Data Column Pattern** - Understand FraiseQL's unique approach to database views.

- 🎯 Why all data goes in JSONB
- 📊 Creating compliant views
- 🏗️ Nested objects and arrays
- ⚡ Performance optimization

### 🚨 [Error Handling](./error-handling.md)
**Common Errors and Solutions** - Quick fixes for frequent issues.

- 🔧 'NoneType' object has no attribute 'context'
- 🔌 Connection already closed
- 📋 View must have a 'data' column
- 🔍 Debugging techniques

## Advanced Patterns

### 🏢 [Multi-Tenant Applications](../COMMON_PATTERNS.md#multi-tenant-applications)
Build SaaS applications with tenant isolation.

### 🔐 [Authentication & Authorization](../COMMON_PATTERNS.md#authentication--authorization)
Secure your API with flexible auth patterns.

### 📄 [Pagination](../COMMON_PATTERNS.md#pagination)
Implement cursor and offset-based pagination.

### 🔍 [Filtering & Search](../COMMON_PATTERNS.md#filtering--search)
Complex queries with full-text search.

## Quick Start Example

Here's the FraiseQL pattern in action:

```python
# 1. Define your type
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str

# 2. Create your query (NOT a resolver!)
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

# 3. Create your view with JSONB
"""
CREATE VIEW user_view AS
SELECT
    id,              -- For filtering
    email,           -- For lookups
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data        -- Required!
FROM users;
"""

# 4. Run your app
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/db",
    types=[User]
)
```

## Key Principles

1. **Queries are Functions** - Not methods, not resolvers
2. **Views Return JSONB** - All data in the `data` column
3. **Repository Pattern** - Always use `info.context["db"]`
4. **Type Everything** - Full type annotations required
5. **Async by Default** - All queries should be async

## Common Mistakes to Avoid

❌ **DON'T** use `resolve_` prefix methods
❌ **DON'T** create resolver classes
❌ **DON'T** forget the `data` column in views
❌ **DON'T** create your own database connections
❌ **DON'T** put `info` anywhere but first parameter

✅ **DO** use `@fraiseql.query` decorator
✅ **DO** make `info` the first parameter
✅ **DO** return all data in JSONB
✅ **DO** use the repository from context
✅ **DO** include type annotations

## Next Steps

1. Master [Query Patterns](./queries.md) first
2. Understand [Database Patterns](./database.md)
3. Learn from [Common Errors](./error-handling.md)
4. Explore [Advanced Patterns](../COMMON_PATTERNS.md)
5. Check out [Examples](../../examples/)
