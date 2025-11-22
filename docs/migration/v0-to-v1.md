# Migration Guide: v0.x to v1.0

This guide helps you migrate from FraiseQL v0.x to v1.0.

## Overview

FraiseQL v1.0 introduces significant improvements:

- **Rust-powered JSON processing** for 10-100x performance improvement
- **Enhanced type system** with better PostgreSQL type support
- **Improved CQRS patterns** with view-based reads
- **Better authentication** and authorization
- **Comprehensive testing** and stability improvements

## Breaking Changes

### 1. Rust Extension Required

v1.0 includes a Rust extension for performance:

```bash
# Install with Rust support
pip install fraiseql[all]
```

**Migration**: No code changes needed. The Rust extension is automatically built during installation.

### 2. Repository API Changes

The repository API has been refined:

**v0.x**:
```python
repo.query("SELECT * FROM users WHERE email = ?", email)
```

**v1.0**:
```python
repo.find("users_view", email=email)
```

**Migration**: Update repository calls to use the new `find()` and `find_one()` methods.

### 3. Type System Enhancements

Better support for PostgreSQL types:

```python
from fraiseql import type, query, mutation, input, field

@type
class User:
    id: UUID  # Now properly handled
    email: EmailStr  # Email validation
    ip_address: IPAddress  # Network types
    created_at: datetime  # Timezone-aware
```

**Migration**: Review type annotations and use the enhanced types where appropriate.

### 4. Authentication Changes

Auth providers now use a standardized interface:

**v0.x**:
```python
from fraiseql.auth import JWTAuth

auth = JWTAuth(secret="...")
```

**v1.0**:
```python
from fraiseql.auth import Auth0Provider

auth = Auth0Provider(
    domain="your-domain.auth0.com",
    audience="your-api"
)
```

**Migration**: Update auth provider initialization to use the new provider classes.

## Step-by-Step Migration

### Step 1: Update Dependencies

```bash
pip install --upgrade "fraiseql>=1.0.0"
```

### Step 2: Update Imports

Some imports have moved:

```python
# Old
from fraiseql.core import Repository

# New
from fraiseql.db import FraiseQLRepository
```

### Step 3: Update Repository Usage

Convert to view-based queries:

```python
# Old
users = await repo.query("SELECT * FROM users WHERE active = true")

# New
users = await repo.find("users_view", is_active=True)
```

### Step 4: Review Type Annotations

Add proper type hints:

```python
from fraiseql import type, query, mutation, input, field, Info

@query
def get_users(info: Info, limit: int = 10) -> list[User]:
    return info.context.repo.find("users_view", limit=limit)
```

### Step 5: Test Thoroughly

Run your test suite:

```bash
pytest
```

## New Features to Adopt

### 1. Rust JSON Processing

Automatically enabled - no changes needed:

```python
from fraiseql import type, query, mutation, input, field

# JSON responses are now 10-100x faster
@query
def get_data(info: Info) -> dict:
    return {"key": "value"}  # Fast JSON serialization
```

### 2. Enhanced Filtering

More powerful where clauses:

```python
users = await repo.find(
    "users_view",
    where={
        "age": {"gte": 18, "lt": 65},
        "status": {"in": ["active", "pending"]}
    }
)
```

### 3. Connection Types

Pagination support:

```python
from fraiseql import type, query, mutation, input, field, connection

@connection
def users(
    info: Info,
    first: int = 100
) -> Connection[User]:
    return info.context.repo.find("users_view", limit=first)
```

### 4. DataLoader Integration

Automatic N+1 query prevention:

```python
from fraiseql import type, query, mutation, input, field, dataloader

@field
@dataloader
async def posts(user: User, info: Info) -> list[Post]:
    return await info.context.repo.find("posts_view", user_id=user.id)
```

## Performance Improvements

v1.0 includes significant performance improvements:

- **10-100x faster JSON processing** with Rust
- **Optimized SQL generation** with better query planning
- **Efficient view-based reads** from PostgreSQL
- **Reduced memory usage** with zero-copy transformations

## Database Migration

### Create Optimized Views

For best performance, create views for read operations:

```sql
CREATE OR REPLACE VIEW users_view AS
SELECT
    id,
    email,
    name,
    created_at,
    is_active
FROM users;

-- Add indexes
CREATE INDEX idx_users_view_email ON users(email);
```

### Optional: pg_fraiseql_cache Extension

For additional performance:

```sql
CREATE EXTENSION IF NOT EXISTS pg_fraiseql_cache;
```

## Testing Your Migration

### 1. Unit Tests

Ensure all tests pass:

```bash
pytest tests/
```

### 2. Integration Tests

Test with real database:

```bash
pytest tests/integration/
```

### 3. Performance Tests

Benchmark performance improvements:

```bash
python benchmarks/run_benchmarks.py
```

## Common Issues

### Issue: Rust Extension Build Fails

**Solution**: Ensure Rust toolchain is installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Issue: Type Errors

**Solution**: Update type annotations:

```python
from fraiseql import type, query, mutation, input, field

from datetime import datetime

@type
class User:
    created_at: datetime  # Not 'date'
    middle_name: str | None = None  # Explicit optional
```

### Issue: Query Performance

**Solution**: Ensure views and indexes exist:

```sql
-- Check views
SELECT * FROM pg_views WHERE schemaname = 'public';

-- Check indexes
SELECT * FROM pg_indexes WHERE schemaname = 'public';
```

## Rollback Plan

If you need to rollback:

```bash
pip install "fraiseql<1.0"
```

Note: v0.11.x will continue to receive security updates for 6 months after v1.0 release.

## Getting Help

- [GitHub Discussions](../discussions)
- [Documentation](https://docs.fraiseql.com)
- [Issue Tracker](../issues)

## Next Steps

After migrating:

1. Review the [Performance Guide](../performance/PERFORMANCE_GUIDE.md)
2. Explore [Enterprise Features](../enterprise/ENTERPRISE.md)
3. Check out [Advanced Patterns](../advanced/)

---

Welcome to FraiseQL v1.0!
