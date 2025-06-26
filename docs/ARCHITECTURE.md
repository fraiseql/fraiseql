# FraiseQL Architecture

This document explains the core architectural decisions and patterns in FraiseQL.

## Table of Contents

1. [Overview](#overview)
2. [Core Patterns](#core-patterns)
3. [The JSONB Data Column Pattern](#the-jsonb-data-column-pattern)
4. [Query Resolution Pattern](#query-resolution-pattern)
5. [Repository Pattern](#repository-pattern)
6. [Context Management](#context-management)
7. [Dual-Mode Execution](#dual-mode-execution)
8. [Design Decisions](#design-decisions)

## Overview

FraiseQL is built on four architectural pillars:

1. **CQRS Separation**: Queries use views, mutations use functions
2. **JSONB-First**: All data flows through JSONB columns
3. **Type Safety**: Python types drive GraphQL schema
4. **Zero Magic**: Explicit patterns over implicit behavior

## Core Patterns

### 1. JSONB Data Column Pattern

Every database view in FraiseQL must follow this structure:

```sql
CREATE VIEW entity_view AS
SELECT 
    -- Filtering/access columns
    id,                      -- Primary key
    tenant_id,               -- Multi-tenancy
    status,                  -- Business filters
    
    -- Data column (REQUIRED)
    jsonb_build_object(
        -- All fields for type instantiation
        'id', id,
        'name', name,
        'nested_object', jsonb_build_object(...)
    ) as data
FROM entity;
```

#### Why This Pattern?

1. **Performance**: Single column retrieval vs multiple columns
2. **Flexibility**: Add fields without schema changes
3. **Consistency**: One pattern for all data access
4. **Type Safety**: Automatic object instantiation
5. **Caching**: JSONB is easily cacheable

#### Architecture Benefits

- **Separation of Concerns**: Filtering columns vs data columns
- **Security**: Access control at column level
- **Optimization**: Index filtering columns only
- **Denormalization**: Natural fit for read models

### 2. Query Resolution Pattern

FraiseQL uses a simplified query resolution pattern:

```python
@fraiseql.query
async def query_name(info, arg1: Type1, arg2: Type2) -> ReturnType:
    """Query documentation."""
    # Direct implementation - no resolvers, no classes
    db = info.context["db"]
    return await db.find("view_name", **filters)
```

#### Key Differences from Traditional GraphQL

| Traditional GraphQL | FraiseQL |
|-------------------|----------|
| Resolver classes | Function decorators |
| `resolve_field` naming | Direct field names |
| Complex type registry | Automatic registration |
| Manual schema building | Automatic from types |

#### Why This Pattern?

1. **Simplicity**: One function = one query
2. **Clarity**: No resolver indirection
3. **Performance**: Direct execution path
4. **Type Safety**: Full type inference

### 3. Repository Pattern

The `FraiseQLRepository` is the sole interface to the database:

```python
class FraiseQLRepository:
    def __init__(self, pool: AsyncConnectionPool, context: dict = None):
        self._pool = pool
        self.context = context or {}
        self.mode = self._determine_mode()
    
    async def find(self, view_name: str, **filters) -> list[T]:
        """Find multiple records."""
        
    async def find_one(self, view_name: str, **filters) -> T | None:
        """Find single record."""
```

#### Connection Management

```
Request → Repository → Connection Pool → Database
   ↑                        ↓
   └────── Automatic ───────┘
```

- **No manual connection management**
- **Automatic acquisition/release**
- **Connection pooling built-in**
- **Transaction support**

#### Why Repository Pattern?

1. **Abstraction**: Hide connection complexity
2. **Consistency**: Standard interface for all queries
3. **Features**: Mode switching, type instantiation
4. **Testing**: Easy to mock/stub

### 4. Context Management

Context flows through the entire request lifecycle:

```
Request Headers
    ↓
Context Builder
    ↓
GraphQL Context (info.context)
    ↓
Repository Context
    ↓
Query Execution
```

#### Standard Context Structure

```python
{
    "db": FraiseQLRepository,      # Database access
    "user": UserContext | None,    # Authenticated user
    "authenticated": bool,         # Auth flag
    "request": Request,           # FastAPI request
    "tenant_id": str | None,      # Multi-tenancy
    # ... custom values
}
```

#### Context Flow Example

```python
# 1. HTTP Request
Headers: { "tenant-id": "123", "authorization": "Bearer ..." }

# 2. Context Builder
async def get_context(request):
    return {
        "db": FraiseQLRepository(pool, {"tenant_id": "123"}),
        "tenant_id": "123",
        "user": await auth.get_user(token)
    }

# 3. Query Access
@fraiseql.query
async def my_data(info):
    tenant_id = info.context["tenant_id"]
    db = info.context["db"]
```

### 5. Dual-Mode Execution

FraiseQL supports two execution modes:

#### Development Mode
```python
# Returns fully typed Python objects
user = await db.find_one("user_view", id=1)
print(user.name)  # IDE autocomplete works
isinstance(user, User)  # True
```

#### Production Mode
```python
# Returns raw dicts for performance
user = await db.find_one("user_view", id=1)
print(user["data"]["name"])  # Dict access
isinstance(user, dict)  # True
```

#### Mode Determination

```
Priority: Context > Environment > Default

1. repo = FraiseQLRepository(pool, {"mode": "development"})
2. FRAISEQL_ENV=development
3. Default: production
```

## Design Decisions

### 1. Why JSONB Only?

**Decision**: All type data must be in a JSONB `data` column.

**Rationale**:
- Consistency across all queries
- Natural denormalization for read models
- Single instantiation pattern
- Simplified caching strategies
- PostgreSQL JSONB performance

**Trade-offs**:
- (+) Consistent pattern
- (+) Flexible schema evolution
- (-) Requires view changes
- (-) Some storage overhead

### 2. Why No Resolver Classes?

**Decision**: Use function decorators instead of resolver classes.

**Rationale**:
- Simpler mental model
- Less boilerplate
- Direct execution path
- Better type inference
- Easier testing

**Trade-offs**:
- (+) Simpler code
- (+) Better performance
- (-) Less familiar to GraphQL veterans
- (-) No resolver middleware

### 3. Why Repository Pattern?

**Decision**: All database access through FraiseQLRepository.

**Rationale**:
- Centralized connection management
- Consistent query interface
- Mode switching capability
- Feature additions (caching, logging)

**Trade-offs**:
- (+) Abstraction benefits
- (+) Testability
- (-) Learning curve
- (-) Less flexibility

### 4. Why Explicit Context?

**Decision**: Context passed explicitly through `info` parameter.

**Rationale**:
- No hidden globals
- Clear data flow
- Testable functions
- Request isolation

**Trade-offs**:
- (+) Explicit dependencies
- (+) Thread safety
- (-) More parameters
- (-) Repetitive access

## Performance Considerations

### Query Optimization

```sql
-- Index filtering columns, not JSONB
CREATE INDEX idx_users_tenant ON users(tenant_id);
CREATE INDEX idx_users_status ON users(status);

-- Use filtering columns in WHERE
SELECT * FROM user_view 
WHERE tenant_id = $1  -- Good: column filter
AND status = 'active' -- Good: column filter
-- Not: WHERE data->>'status' = 'active'
```

### Connection Pooling

```python
# Pool configuration
pool = AsyncConnectionPool(
    conninfo=database_url,
    min_size=10,
    max_size=20,
    timeout=30,
    max_lifetime=3600
)
```

### Caching Strategy

```python
# JSONB data is perfect for caching
cache_key = f"user:{user_id}"
cached = await redis.get(cache_key)
if cached:
    return json.loads(cached)

user = await db.find_one("user_view", id=user_id)
await redis.set(cache_key, json.dumps(user["data"]))
```

## Security Considerations

### Multi-Tenancy

```python
# Tenant isolation at repository level
repo = FraiseQLRepository(pool, {"tenant_id": tenant_id})

# All queries automatically scoped
await repo.find("data_view")  # Adds tenant_id filter
```

### Access Control

```sql
-- Row-level security with filtering columns
CREATE POLICY tenant_isolation ON data
FOR SELECT USING (tenant_id = current_setting('app.tenant_id'));
```

### SQL Injection Protection

```python
# Repository uses parameterized queries
await db.find("view", name=user_input)  # Safe
# Never: f"WHERE name = '{user_input}'"  # Vulnerable
```

## Error Handling

### Repository Errors

```python
try:
    user = await db.find_one("user_view", id=user_id)
except Exception as e:
    logger.error(f"Database error: {e}")
    raise GraphQLError("Failed to fetch user")
```

### Context Errors

```python
@fraiseql.query
async def protected_query(info):
    if not info.context.get("authenticated"):
        raise GraphQLError("Authentication required")
```

## Best Practices

1. **Always use filtering columns** in WHERE clauses
2. **Index filtering columns** for performance
3. **Keep data column pure** - only for instantiation
4. **Use repository methods** - never raw SQL
5. **Pass context explicitly** - no globals
6. **Handle errors gracefully** - user-friendly messages
7. **Test with both modes** - dev and production

## Summary

FraiseQL's architecture is designed for:
- **Simplicity**: One way to do things
- **Performance**: Optimized patterns
- **Safety**: Type-safe, SQL-safe
- **Clarity**: Explicit over implicit

The JSONB data column pattern is the foundation that enables all other features while maintaining consistency and performance.