# FraiseQL

[![Test](https://github.com/fraiseql/fraiseql/actions/workflows/test.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/test.yml)
[![Lint](https://github.com/fraiseql/fraiseql/actions/workflows/lint.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/lint.yml)
[![Security](https://github.com/fraiseql/fraiseql/actions/workflows/security.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/security.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![PyPI version](https://badge.fury.io/py/fraiseql.svg)](https://pypi.org/project/fraiseql/)
[![Python Versions](https://img.shields.io/pypi/pyversions/fraiseql.svg)](https://pypi.org/project/fraiseql/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance GraphQL-to-PostgreSQL framework with **database-native camelCase transformation**, automatic type generation, built-in caching, and comprehensive security features. **The world's first GraphQL framework with intelligent database-native field transformation.**

## What is FraiseQL?

FraiseQL is a Python framework that generates GraphQL APIs by connecting Python type definitions to PostgreSQL database views and functions. It leverages PostgreSQL's JSONB capabilities for flexible schema evolution while providing complete type safety through Python's type system.

## Origin of the Name

FraiseQL (pronounced "fraise-QL") is a French homage to [Strawberry GraphQL](https://strawberry.rocks/), the excellent Python GraphQL library that inspired this project. "Fraise" is the French word for strawberry, reflecting both our inspiration and our goal to bring a fresh, performant approach to GraphQL in Python.

### Special Acknowledgments

A special tribute goes to **Harry Percival**, author of *Architecture Patterns with Python*. Reading his book sent me down the rabbit hole of software architecture, and everything good in FraiseQL owes much to his insights on clean architecture, domain-driven design, and the repository pattern. His work fundamentally shaped how FraiseQL approaches the separation of concerns between business logic and infrastructure.

Through Harry's book, I discovered the foundational texts that influenced FraiseQL's design:
- **Eric Evans' "Domain-Driven Design"** (the Blue Book) - which inspired our database-centric domain model
- **Vaughn Vernon's "Implementing Domain-Driven Design"** (the Red Book) - which guided our CQRS implementation and bounded contexts approach

These works collectively shaped FraiseQL's philosophy: that the database can be a powerful domain layer when properly abstracted through views and functions.

## Architecture Philosophy: Database Domain-Driven Design

FraiseQL embraces **CQRS (Command Query Responsibility Segregation)** with PostgreSQL at its core:

- **Commands**: Mutations call PostgreSQL functions that encapsulate business logic
- **Queries**: PostgreSQL views expose denormalized, query-optimized projections as JSONB
- **Separation**: Your storage model (tables) evolves independently from your API model (views)

This means PostgreSQL handles all the heavy lifting - joins, aggregations, and transformations - not your application layer. Your Python code just defines types and coordinates:

```python
from fraiseql import ID

@fraiseql.type
class User:
    id: ID  # Maps to pk_user in PostgreSQL
    identifier: str | None = None
    name: str
    email: str

@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    return await repo.find("v_user")  # Reads from PostgreSQL view

@fraiseql.mutation
async def create_user(info, name: str, email: str) -> User:
    repo = info.context["repo"]
    # Calls PostgreSQL function that handles business logic
    result = await repo.execute_function("fn_create_user", name=name, email=email)
    return User(**result)
```

The framework supports both regular views (`v_` prefix) for real-time data and table views (`tv_` prefix) for materialized projections with incremental updates.

→ [Learn more about view patterns in our documentation](https://github.com/fraiseql/fraiseql/tree/main/docs)

### Key Features

#### 🌟 **CamelForge Integration (NEW in v0.4.0)**
- **Database-Native camelCase Transformation**: Field conversion happens in PostgreSQL for sub-millisecond responses
- **Intelligent Field Threshold**: Automatically uses CamelForge for small queries (≤20 fields), falls back for large queries
- **Zero Breaking Changes**: Completely backward compatible, disabled by default
- **One-Line Enablement**: `FRAISEQL_CAMELFORGE_ENABLED=true` to activate
- **Automatic Field Mapping**: GraphQL camelCase ↔ PostgreSQL snake_case (e.g., `ipAddress` ↔ `ip_address`)

#### 🚀 **Core Features**
- **Automatic GraphQL Schema Generation**: Define Python types, get a complete GraphQL API
- **PostgreSQL-First Design**: Optimized for PostgreSQL's advanced features (JSONB, views, functions)
- **Type Safety**: Full type checking with Python 3.13+ type hints
- **High Performance**: Built-in query optimization, caching, and N+1 query detection
- **Security**: Field-level authorization, rate limiting, CSRF protection
- **Developer Experience**: Hot reload, GraphQL playground, comprehensive error messages
- **Default Schema Configuration**: Configure default PostgreSQL schemas once, eliminate repetitive boilerplate

### Why FraiseQL?

- **4-100x Faster**: Pre-compiled queries outperform traditional GraphQL servers
- **Zero Network Overhead**: Built-in PostgreSQL caching eliminates external cache dependencies
- **True Multi-tenancy**: Complete isolation with per-tenant cache and domain versioning
- **Production Proven**: Powering enterprise SaaS applications in production

## Installation

```bash
pip install fraiseql
```

### Requirements

- Python 3.13+
- PostgreSQL 14+ (with JSONB support)

## Quick Start

### 1. Initialize a New Project

```bash
fraiseql init my-api
cd my-api
```

### 2. Define Your GraphQL Types

```python
# src/types.py
import fraiseql
from datetime import datetime
from fraiseql import ID, EmailAddress

@fraiseql.type
class User:
    id: ID  # Maps to pk_user in PostgreSQL
    identifier: str | None = None
    email: EmailAddress
    name: str
    created_at: datetime
    avatar_url: str | None = None
```

### 3. Create Database Views

All views must return data in a JSONB column:

```sql
-- migrations/001_create_user_view.sql
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', pk_user,  -- UUID, framework handles ID conversion
    'email', email,
    'name', name,
    'created_at', created_at,
    'avatar_url', avatar_url
) AS data
FROM tb_users;
```

### 4. Define Queries

```python
# src/queries.py
import fraiseql
from fraiseql import ID
from .types import User

@fraiseql.query
async def users(info) -> list[User]:
    """Fetch all users"""
    repo = info.context["repo"]
    return await repo.find("v_user")

@fraiseql.query
async def user(info, id: ID) -> User | None:
    """Fetch a single user by ID"""
    repo = info.context["repo"]
    return await repo.find_one("v_user", id=id)
```

### 5. Define Mutations

```python
# src/mutations.py
from fraiseql import FraiseQLMutation, FraiseQLError, EmailAddress
from .types import User

class CreateUserSuccess:
    """Success response for user creation."""
    user: User
    message: str = "User created successfully"
    errors: list[FraiseQLError] = []

class CreateUserError:
    """Error response for user creation."""
    message: str
    errors: list[FraiseQLError]
    validation_details: dict | None = None

class CreateUser(
    FraiseQLMutation,  # Clean default pattern
    function="fn_create_user",
    validation_strict=True
):
    """Create a new user with clean default patterns.

    Success and failure types are automatically decorated by FraiseQLMutation.
    """
    input: dict  # Or CreateUserInput type
    success: CreateUserSuccess  # Auto-decorated
    failure: CreateUserError    # Auto-decorated
```

### 6. Run the Development Server

```bash
fraiseql dev
```

Your GraphQL API is now available at <http://localhost:8000/graphql>

### 7. Enable CamelForge (Optional - NEW in v0.4.0)

For sub-millisecond GraphQL responses with database-native camelCase transformation:

```python
# src/config.py
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    camelforge_enabled=True,  # Enable CamelForge
)
```

Or via environment variable:
```bash
export FRAISEQL_CAMELFORGE_ENABLED=true
fraiseql dev
```

**What CamelForge does:**
- **Small queries** (≤20 fields): Converts fields in PostgreSQL for maximum performance
- **Large queries** (>20 fields): Automatically falls back to standard processing
- **Zero breaking changes**: Existing queries work identically

**Example transformation:**
```graphql
# GraphQL Query
{ users { id, createdAt, avatarUrl } }

# Without CamelForge: Python processes response
# With CamelForge: PostgreSQL returns transformed JSON directly
```

## Core Concepts

### Repository Pattern

FraiseQL uses a repository pattern for database access:

```python
from fraiseql import CQRSRepository

# Initialize repository with your database pool
async def get_context(request):
    db_pool = request.app.state.db_pool
    return {
        "repo": CQRSRepository(db_pool),
        "user": getattr(request.state, "user", None)
    }

# In your queries/mutations (via info.context)
repo = info.context["repo"]

# Query operations
users = await repo.find("user_view", limit=10)
user = await repo.find_one("user_view", id=123)

# Execute PostgreSQL functions
result = await repo.execute_function("create_user", email="test@example.com")
```

### Type System

FraiseQL provides a rich type system with built-in scalars:

- **Basic Types**: `int`, `str`, `float`, `bool`, `datetime`, `date`
- **Network Types**: `EmailAddress`, `IPv4`, `IPv6`, `CIDR`, `MACAddress`
- **Advanced Types**: `UUID`, `JSON`, `LTree`, `DateRange`
- **Custom Scalars**: Easy to define domain-specific types

### WHERE Clause Generation

FraiseQL automatically generates type-safe WHERE clauses with **intelligent type-aware SQL optimization** (v0.5.7+):

```graphql
# GraphQL query with automatic type-aware optimization
query {
  users(where: {
    email: {eq: "user@example.com"}
    created_at: {gte: "2024-01-01"}
    age: {between: [18, 65]}
  }) {
    id
    name
  }
}
```

#### Advanced Type-Aware Filtering (NEW in v0.5.7)

```graphql
# Network and special type filtering with optimized SQL generation
query {
  dnsServers(where: {
    ipAddress: { eq: "8.8.8.8" }        # → Optimized ::inet casting
    port: { gt: 1024 }                  # → Optimized ::integer casting
    createdAt: { gte: "2024-01-01" }    # → Optimized ::timestamp casting
    macAddress: { eq: "aa:bb:cc:dd:ee:ff" }  # → Optimized ::macaddr casting
  }) {
    id identifier ipAddress port createdAt
  }
}
```

**What happens behind the scenes:**
```sql
-- FraiseQL automatically generates optimized SQL based on field types:
WHERE (data->>'ip_address')::inet = '8.8.8.8'::inet
  AND (data->>'port')::integer > 1024
  AND (data->>'created_at')::timestamp >= '2024-01-01'::timestamp
  AND (data->>'mac_address')::macaddr = 'aa:bb:cc:dd:ee:ff'::macaddr
```

**Performance Impact:** Type-aware casting significantly improves query performance by allowing PostgreSQL to use specialized indexes and operators for network types, dates, and numeric fields.

### Field-Level Features

```python
@fraiseql.type
class Post:
    id: int
    title: str

    # Computed field
    @fraiseql.field
    async def comment_count(self, info) -> int:
        repo = info.context["repo"]
        return await repo.count("comment_view", post_id=self.id)

    # DataLoader for N+1 prevention
    @fraiseql.dataloader_field
    async def author(self, info) -> User:
        return await info.context["user_loader"].load(self.author_id)
```

## Advanced Features

### Subscriptions

```python
@fraiseql.subscription
async def post_updates(info, post_id: int):
    """Subscribe to updates for a specific post"""
    async for update in watch_post_updates(post_id):
        yield update
```

### Custom Context

```python
from fraiseql.fastapi import create_turbo_router

async def get_context(request):
    return {
        "repo": CQRSRepository(request.app.state.db_pool),
        "user": await get_current_user(request),
        "request": request
    }

router = create_turbo_router(
    schema=schema,
    context_getter=get_context
)
```

### Error Handling

FraiseQL provides structured error handling with extensions:

```python
{
  "errors": [{
    "message": "Email already exists",
    "extensions": {
      "code": "DUPLICATE_EMAIL",
      "field": "email",
      "details": {
        "existing_user_id": 123
      }
    }
  }]
}
```

### Performance Monitoring

Built-in query complexity analysis:

```python
from fraiseql.analysis import QueryComplexityConfig

config = QueryComplexityConfig(
    max_complexity=1000,
    field_weights={
        "posts": 10,  # Expensive field
        "id": 1       # Cheap field
    }
)
```

## CLI Commands

```bash
# Project management
fraiseql init <project-name>    # Initialize new project
fraiseql dev                     # Run development server
fraiseql check                   # Validate schema

# Code generation
fraiseql generate schema         # Generate GraphQL schema file
fraiseql generate types          # Generate TypeScript types

# Database utilities
fraiseql sql find               # Generate SQL for GraphQL queries
fraiseql sql analyze            # Analyze query performance
```

## Testing

FraiseQL provides testing utilities:

```python
import pytest
from fraiseql.testing import create_test_schema

@pytest.mark.asyncio
async def test_create_user(test_client):
    result = await test_client.execute(
        """
        mutation {
            createUser(email: "test@example.com", name: "Test") {
                ... on Success {
                    data { id email }
                }
            }
        }
        """
    )
    assert result["data"]["createUser"]["data"]["email"] == "test@example.com"
```

## Production Deployment

### Environment Variables

```bash
# Required
DATABASE_URL=postgresql://user:password@localhost/dbname

# Optional
FRAISEQL_MODE=production
FRAISEQL_LOG_LEVEL=INFO
FRAISEQL_QUERY_TIMEOUT=30
FRAISEQL_MAX_QUERY_DEPTH=10
```

### Docker Support

```dockerfile
FROM python:3.11-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install -r requirements.txt
COPY . .
CMD ["fraiseql", "dev", "--host", "0.0.0.0", "--port", "8000"]
```

## Architecture

FraiseQL follows a clean architecture pattern with two key innovations:

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│   GraphQL   │────▶│   FraiseQL   │────▶│  PostgreSQL  │
│   Client    │     │              │     │              │
└─────────────┘     │  - Types     │     │  - Views     │
                    │  - Queries   │     │  - Functions │
                    │  - Mutations │     │  - JSONB     │
                    └──────────────┘     └──────────────┘
```

### How It Works

1. **Query Registration**: GraphQL queries are pre-compiled into optimized SQL
2. **Hash Lookup**: Incoming queries are identified by SHA-256 hash (O(1) lookup)
3. **Direct Execution**: Pre-compiled SQL executes directly without parsing
4. **Smart Caching**: Results cached in PostgreSQL with automatic invalidation

### Design Philosophy: Storage for Speed

FraiseQL makes a deliberate trade-off: **invest in storage to achieve exceptional performance**. By pre-computing and storing optimized query plans, denormalized views, and cached results, FraiseQL eliminates runtime overhead. This approach means:

- **More Storage**: Pre-aggregated JSONB views, cached results, compiled queries
- **Less Compute**: No query parsing, no dynamic SQL generation, no JOINs
- **Result**: 4-100x performance improvement with predictable sub-10ms latency

## Performance

FraiseQL delivers exceptional performance through production-proven optimizations:

### Benchmarks (Real Production Data)

| Operation | Standard GraphQL | FraiseQL | FraiseQL + Cache |
|-----------|-----------------|----------|------------------|
| Simple Query | 100-200ms | 25-55ms | 2-5ms |
| Complex Query | 300-600ms | 30-60ms | 2-5ms |
| Cache Hit Rate | N/A | N/A | 85-95% |

### Key Performance Features

- **TurboRouter**: Pre-compiles GraphQL queries with SHA-256 hash lookup (4-10x faster)
- **Built-in Caching**: PostgreSQL-based caching with automatic invalidation
- **Query Optimization**: Leverages PostgreSQL's JSONB for optimal query execution
- **Pre-aggregated Views**: Eliminate JOINs at query time with tv_* tables

## Comparison with Other Frameworks

### vs PostGraphile

- **PostGraphile**: Dynamic SQL generation from database introspection
- **FraiseQL**: Pre-compiled queries with hash lookup (4-10x faster)

### vs Strawberry/Graphene

- **Traditional**: 100-300ms for typical queries
- **FraiseQL**: 25-60ms uncached, 2-5ms cached

### vs Hasura

- **Hasura**: Separate service, complex deployment
- **FraiseQL**: Embedded in your Python app, simple deployment

## Use Cases

FraiseQL excels in:

- **Multi-tenant SaaS**: Per-tenant cache isolation
- **High-traffic APIs**: Sub-10ms response times
- **Enterprise Applications**: ACID guarantees, no eventual consistency
- **Cost-sensitive Projects**: 70% reduction in infrastructure costs

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Links

- [Documentation](https://fraiseql.dev)
- [GitHub Repository](https://github.com/fraiseql/fraiseql)
- [PyPI Package](https://pypi.org/project/fraiseql/)

---
*Last verified: 2025-08-25*
