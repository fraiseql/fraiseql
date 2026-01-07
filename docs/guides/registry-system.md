# FraiseQL Registry System Guide

**Version**: Phase D (Registry System)
**Updated**: 2026-01-07
**Status**: Production Ready

## Table of Contents

1. [Overview](#overview)
2. [Core Concepts](#core-concepts)
3. [Quick Start](#quick-start)
4. [Architecture](#architecture)
5. [Usage Patterns](#usage-patterns)
6. [Advanced Topics](#advanced-topics)
7. [Best Practices](#best-practices)
8. [Troubleshooting](#troubleshooting)

---

## Overview

The FraiseQL Registry System is a centralized type and query management system that works with the Axum-based server. It provides:

- **Centralized Storage** - Single source of truth for all GraphQL types
- **Type Safety** - Explicit registration ensures clarity
- **Testing Support** - Custom registries for test isolation
- **Discovery Integration** - Can find types if needed
- **Decorator Hooks** - Decorators auto-register when used

### Key Components

| Component | Purpose | Status |
|-----------|---------|--------|
| **AxumRegistry** | Centralized singleton registry | ✅ Phase D.1 |
| **Discovery System** | Find types in packages | ✅ Phase D.2 |
| **Decorator Hooks** | Auto-register via decorators | ✅ Phase D.3 |
| **App Factory** | Explicit registration API | ✅ Phase D.4 |

---

## Core Concepts

### 1. AxumRegistry (Singleton)

A thread-safe singleton that stores all GraphQL items:

```python
from fraiseql.axum import AxumRegistry

# Get the singleton instance
registry = AxumRegistry.get_instance()

# Register items
registry.register_type(User)
registry.register_query(get_users)
registry.register_mutation(create_user)
registry.register_subscription(on_user_created)

# Inspect registered items
types = registry.get_registered_types()
queries = registry.get_registered_queries()

# Get summary
print(registry.summary())
```

### 2. Explicit Registration (Recommended)

Register types via the app factory:

```python
from fraiseql.axum import create_axum_fraiseql_app

app = create_axum_fraiseql_app(
    database_url="postgresql://user:pass@localhost/db",
    types=[User, Post, Comment],
    queries=[get_users, get_posts],
    mutations=[create_user, delete_post],
    subscriptions=[on_user_created],
)
```

**Why explicit is better:**
- Clear and auditable
- Fast (no scanning)
- Debuggable (can trace imports)
- Testable (deterministic)
- Secure (explicit whitelist)

### 3. Decorator Hooks

Decorators auto-register when you use them:

```python
from fraiseql import type as fraise_type, query

@fraise_type
class User:
    id: ID
    name: str
    email: str

@query
async def get_users() -> list[User]:
    """Query to fetch all users."""
    # Implementation
    pass
```

When `auto_register=True` (default):
- `@fraise_type` registers to registry
- `@query` registers to registry
- `@mutation` registers to registry
- `@subscription` registers to registry

### 4. Discovery System

Can find types in packages (useful for exploration):

```python
from fraiseql.axum.discovery import discover_from_package, DiscoveryResult

# Discover items in a package
result: DiscoveryResult = discover_from_package("myapp.graphql")

# Check what was found
print(f"Found {result.count_total()} items")
print(result.summary())

# Register discovered items
result.register_to_registry()
```

---

## Quick Start

### Basic Setup (Recommended)

```python
# types.py
from fraiseql import type as fraise_type
from fraiseql.fields import ID
from dataclasses import dataclass

@fraise_type(sql_source="users")
@dataclass
class User:
    id: ID
    name: str
    email: str

# queries.py
from fraiseql import query
from typing import list

@query
async def get_users() -> list[User]:
    """Fetch all users from database."""
    # Implementation
    pass

# main.py
from fraiseql.axum import create_axum_fraiseql_app
from .types import User
from .queries import get_users

app = create_axum_fraiseql_app(
    database_url="postgresql://user:pass@localhost/db",
    types=[User],
    queries=[get_users],
)

if __name__ == "__main__":
    app.start(host="0.0.0.0", port=8000)
```

### Verify Registration

```python
# After creating app, check what's registered
registry = app.get_registry()

print("Registered Types:")
for name in registry.get_registered_types():
    print(f"  - {name}")

print("Registered Queries:")
for name in registry.get_registered_queries():
    print(f"  - {name}")

print("\nRegistry Summary:")
print(registry.summary())
```

---

## Architecture

### System Components

```
┌─────────────────────────────────────────┐
│     User Code (types, queries, etc)     │
└──────────────┬──────────────────────────┘
               │
               │ explicit registration
               ▼
┌─────────────────────────────────────────┐
│   create_axum_fraiseql_app()            │
│   (App Factory)                         │
└──────────────┬──────────────────────────┘
               │
               │ creates
               ▼
┌─────────────────────────────────────────┐
│   AxumServer                            │
│   - stores types locally                │
│   - registers to registry               │
└──────────────┬──────────────────────────┘
               │
               │ integrates
               ▼
┌─────────────────────────────────────────┐
│   AxumRegistry (Singleton)              │
│   - Central storage                     │
│   - Types, Queries, Mutations, etc.     │
│   - Thread-safe                         │
└─────────────────────────────────────────┘
```

### Data Flow

1. **Define Types** - Use `@fraiseql.type` decorator
2. **Define Queries** - Use `@fraiseql.query` decorator
3. **Explicit Registration** - Pass to `create_axum_fraiseql_app()`
4. **AxumServer** - Registers to both local dict and registry
5. **AxumRegistry** - Stores centrally for access

### Singleton Pattern

AxumRegistry uses thread-safe singleton:

```python
# Always returns the same instance
reg1 = AxumRegistry.get_instance()
reg2 = AxumRegistry.get_instance()
assert reg1 is reg2  # True

# Useful for testing: pass custom registry
test_registry = AxumRegistry()
app = create_axum_fraiseql_app(
    database_url="...",
    registry=test_registry,
)
```

---

## Usage Patterns

### Pattern 1: Simple Single-File App

```python
from fraiseql import type as fraise_type, query
from fraiseql.axum import create_axum_fraiseql_app
from dataclasses import dataclass

# Define types
@fraise_type(sql_source="users")
@dataclass
class User:
    id: str
    name: str

# Define queries
@query
async def get_users() -> list[User]:
    pass

# Create app
app = create_axum_fraiseql_app(
    database_url="postgresql://localhost/db",
    types=[User],
    queries=[get_users],
)

app.start(host="0.0.0.0", port=8000)
```

### Pattern 2: Organized Multi-Module App

```
myapp/
├── main.py              # Entry point
├── config.py            # Config
├── types.py             # All @fraise_type definitions
├── queries.py           # All @query definitions
├── mutations.py         # All @mutation definitions
└── subscriptions.py     # All @subscription definitions
```

**types.py:**
```python
from fraiseql import type as fraise_type
from dataclasses import dataclass

@fraise_type(sql_source="users")
@dataclass
class User:
    id: str
    name: str

@fraise_type(sql_source="posts")
@dataclass
class Post:
    id: str
    title: str
    author_id: str
```

**queries.py:**
```python
from fraiseql import query
from .types import User, Post

@query
async def get_users() -> list[User]:
    pass

@query
async def get_posts() -> list[Post]:
    pass
```

**main.py:**
```python
from fraiseql.axum import create_axum_fraiseql_app
from .types import User, Post
from .queries import get_users, get_posts
from .mutations import create_user
from .subscriptions import on_user_created

app = create_axum_fraiseql_app(
    database_url="postgresql://localhost/db",
    types=[User, Post],
    queries=[get_users, get_posts],
    mutations=[create_user],
    subscriptions=[on_user_created],
)

app.start(host="0.0.0.0", port=8000)
```

### Pattern 3: Testing with Isolated Registry

```python
import pytest
from fraiseql.axum import AxumRegistry, create_axum_fraiseql_app

@pytest.fixture
def test_app():
    """Create app with isolated registry for testing."""
    test_registry = AxumRegistry()
    test_registry.clear()

    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/test_db",
        types=[TestUser],
        registry=test_registry,
    )

    yield app

    # Cleanup
    test_registry.clear()

def test_user_query(test_app):
    """Test that user query works."""
    result = test_app.execute_query("{ users { id name } }")
    assert result["data"] is not None
```

### Pattern 4: Hybrid Registration (Explicit + Decorator)

Types registered via `@fraise_type` decorator auto-register, but you can still pass explicit lists:

```python
from fraiseql import type as fraise_type, query
from fraiseql.axum import create_axum_fraiseql_app

# Auto-register via decorator
@fraise_type(sql_source="users")
class User:
    id: str
    name: str

# Explicit registration for custom query
@query
async def custom_get_users() -> list[User]:
    pass

# Create app - both decorator-registered and explicit items
app = create_axum_fraiseql_app(
    database_url="postgresql://localhost/db",
    types=[User],  # Explicit
    queries=[custom_get_users],  # Explicit
)
```

---

## Advanced Topics

### Accessing Registry at Runtime

```python
# Get registry from app
registry = app.get_registry()

# Check registered types
types = registry.get_registered_types()
print(f"Types: {list(types.keys())}")

# Check registered queries
queries = registry.get_registered_queries()
print(f"Queries: {list(queries.keys())}")

# Get counts
counts = registry.count_registered()
print(f"Total items: {counts['total']}")

# Get summary for logging
print(registry.summary())
```

### Manual Registration (Advanced)

```python
from fraiseql.axum import AxumRegistry

registry = AxumRegistry.get_instance()

# Manual registration (rarely needed)
registry.register_type(User)
registry.register_query(get_users)
registry.register_mutation(create_user)
registry.register_subscription(on_user_created)
registry.register_input(CreateUserInput)
registry.register_enum(UserRole)
registry.register_interface(Node)
```

### Using Discovery for Exploration

```python
from fraiseql.axum.discovery import discover_from_package

# Find all FraiseQL items in a package
result = discover_from_package("myapp")

print(f"Found {result.count_total()} items:")
print(result.summary())

# Register them if needed
result.register_to_registry()
```

### Custom Registry for Multi-Tenant Apps

```python
# Each tenant gets its own registry
tenant_registries = {}

def get_tenant_app(tenant_id: str):
    """Get or create app for tenant."""
    if tenant_id not in tenant_registries:
        registry = AxumRegistry()
        registry.clear()

        app = create_axum_fraiseql_app(
            database_url=f"postgresql://localhost/tenant_{tenant_id}",
            types=[User, Post],
            queries=[get_users],
            registry=registry,
        )
        tenant_registries[tenant_id] = app

    return tenant_registries[tenant_id]
```

---

## Best Practices

### 1. Keep Schema Explicit

✅ **Good** - Clear and auditable:
```python
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User, Post, Comment],
    queries=[get_users, get_posts],
    mutations=[create_user],
)
```

❌ **Avoid** - Hidden dependencies:
```python
# Don't rely on implicit discovery
# Always be explicit about what's in your schema
```

### 2. Organize by Concern

```
myapp/
├── types/
│   ├── user.py
│   ├── post.py
│   └── comment.py
├── queries/
│   ├── user_queries.py
│   └── post_queries.py
├── mutations/
│   └── user_mutations.py
└── main.py
```

### 3. Import Explicitly

✅ **Good:**
```python
from myapp.types import User, Post
from myapp.queries import get_users, get_posts
from myapp.mutations import create_user

app = create_axum_fraiseql_app(
    database_url="...",
    types=[User, Post],
    queries=[get_users, get_posts],
    mutations=[create_user],
)
```

❌ **Avoid:**
```python
# Don't use wildcard imports
# from myapp.types import *

# Explicit is better than implicit (Zen of Python)
```

### 4. Test with Isolated Registries

```python
@pytest.fixture
def test_registry():
    """Clean registry for each test."""
    registry = AxumRegistry()
    registry.clear()
    yield registry
    registry.clear()

@pytest.fixture
def test_app(test_registry):
    """App with isolated registry."""
    return create_axum_fraiseql_app(
        database_url="postgresql://localhost/test",
        types=[TestUser],
        registry=test_registry,
    )
```

### 5. Document Your Schema

```python
"""
GraphQL Schema for MyApp
=======================

Types:
- User: User account information
- Post: Blog post content
- Comment: Comments on posts

Queries:
- get_users(limit: Int!): list[User]
- get_posts(limit: Int!): list[Post]

Mutations:
- create_user(input: CreateUserInput!): User
- create_post(input: CreatePostInput!): Post

Subscriptions:
- on_user_created: User
- on_post_created: Post
"""
```

### 6. Version Your API

```python
# Keep track of schema changes
__version__ = "1.0.0"
__api_version__ = "2024-01-01"

# Document breaking changes
# Version 1.0.0 - Initial release
# Version 1.1.0 - Added Comment type
# Version 2.0.0 - Removed deprecated fields
```

---

## Troubleshooting

### Issue: Type Not in Schema

**Problem**: A type you defined isn't appearing in GraphQL schema.

**Solution**:
```python
# Check 1: Is it registered?
registry = app.get_registry()
if "MyType" not in registry.get_registered_types():
    print("Type not registered!")

# Check 2: Is it in the app factory?
app = create_axum_fraiseql_app(
    database_url="...",
    types=[MyType],  # Make sure it's here
)

# Check 3: Use registry summary
print(registry.summary())
```

### Issue: Tests Interfering with Each Other

**Problem**: Registry has items from other tests.

**Solution**: Use isolated registries:
```python
@pytest.fixture(autouse=True)
def isolated_registry():
    """Clear registry before and after each test."""
    registry = AxumRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()
```

### Issue: Custom Registry Not Used

**Problem**: Still using singleton instead of custom registry.

**Solution**:
```python
# Explicitly pass registry to app factory
custom_registry = AxumRegistry()

app = create_axum_fraiseql_app(
    database_url="...",
    registry=custom_registry,  # Must be explicit
)

# Verify
assert app.get_registry() is custom_registry
```

### Issue: Circular Import Problems

**Problem**: Importing types causes circular dependencies.

**Solution**: Use TYPE_CHECKING:
```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from myapp.types import User

# Use as annotation only, import at runtime in main.py
```

---

## Summary

The FraiseQL Registry System provides:

✅ **Centralized Storage** - Single source of truth
✅ **Type Safety** - Explicit registration
✅ **Testing Support** - Custom registries
✅ **Clarity** - No hidden magic
✅ **Performance** - No startup overhead
✅ **Security** - Explicit whitelist

**Next**: See [Migration Guide from FastAPI](./fastapi-migration.md) for upgrading existing apps.
