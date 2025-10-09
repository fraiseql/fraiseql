# FraiseQL Glossary

**Quick reference for FraiseQL-specific terms, concepts, and patterns.**

---

## Core Concepts

### APQ (Automatic Persisted Queries)
**Definition**: A caching mechanism that stores GraphQL query results by SHA-256 hash for ultra-fast retrieval.

**Key Points**:
- First request: Query executed and result cached with hash
- Subsequent requests: Hash lookup returns cached result (0.5-2ms)
- Storage backends: Memory (development) or PostgreSQL (production)

**Related**: [APQ Storage Backends](advanced/apq-storage-backends.md), [JSON Passthrough](advanced/json-passthrough-optimization.md)

---

### CQRS (Command Query Responsibility Segregation)
**Definition**: Architectural pattern separating read operations (queries) from write operations (commands/mutations).

**In FraiseQL**:
- **Queries**: Use PostgreSQL views (`v_*` or `tv_*`)
- **Commands**: Use PostgreSQL functions (`fn_*`)
- **Benefit**: Optimized data structures for each operation type

**Example**:
```python
# Query (read) - Uses view
@fraiseql.query
async def users(info) -> list[User]:
    return await repo.find("v_user")  # PostgreSQL view

# Command (write) - Uses function
class CreateUser(FraiseQLMutation, function="fn_create_user"):
    ...  # PostgreSQL function handles business logic
```

**Related**: [CQRS Pattern](advanced/cqrs.md), [Architecture](core-concepts/architecture.md)

---

### DataLoader
**Definition**: Batching and caching mechanism that solves the N+1 query problem by collecting and deduplicating requests within a single GraphQL operation.

**Usage**:
```python
@fraiseql.type
class Post:
    @dataloader_field
    async def author(self, info) -> User:
        # Automatically batched with other author lookups!
        return await repo.find_one("v_user", id=self.author_id)
```

**Related**: [DataLoader Pattern](optimization/dataloader-pattern.md), [N+1 Elimination](advanced/eliminating-n-plus-one.md)

---

### Input Type
**Definition**: GraphQL type that defines the structure of data sent to mutations and parameterized queries.

**Usage**:
```python
@fraiseql.input
class CreateUserInput:
    name: str
    email: EmailAddress
    age: int | None = None
```

**Related**: [Type System](core-concepts/type-system.md), [Decorators](api-reference/decorators.md)

---

### JSON Passthrough
**Definition**: FraiseQL optimization that returns cached JSON directly without serialization, achieving sub-millisecond response times (0.5-2ms).

**How It Works**:
1. PostgreSQL returns JSONB data
2. APQ caches the complete JSON response
3. Subsequent requests bypass parsing and serialization
4. Result: 99% faster than traditional GraphQL

**Related**: [JSON Passthrough Guide](advanced/json-passthrough-optimization.md), [Performance](advanced/performance.md)

---

### JSONB
**Definition**: PostgreSQL's binary JSON data type, enabling flexible schema with full indexing and query capabilities.

**In FraiseQL**: Views return JSONB for optimal performance:
```sql
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', id,
    'name', name,
    'email', email
) AS data FROM users;
```

**Benefits**:
- Fast JSON operations
- Flexible schema evolution
- Full indexing support
- Direct GraphQL compatibility

---

### Materialized View
**Definition**: PostgreSQL view that stores query results physically, updated on-demand rather than computed on every access.

**Naming Convention**: `tv_*` prefix (table view)

**Example**:
```sql
CREATE MATERIALIZED VIEW tv_user_stats AS
SELECT
    user_id,
    count(*) as post_count,
    max(created_at) as last_post_at
FROM posts
GROUP BY user_id;

-- Refresh when needed
REFRESH MATERIALIZED VIEW tv_user_stats;
```

**Use When**: Complex aggregations, expensive joins, dashboard data

**Related**: [Database Views](core-concepts/database-views.md)

---

### Mutation
**Definition**: GraphQL write operation (create, update, delete). In FraiseQL, mutations typically call PostgreSQL functions for business logic.

**Also called**: Command (in CQRS context)

**Pattern**:
```python
class CreateUser(FraiseQLMutation, function="fn_create_user"):
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError
```

**Related**: [Mutations](api-reference/decorators.md#mutation), [CQRS](advanced/cqrs.md)

---

### Object Type
**Definition**: GraphQL type representing an entity with fields. The primary building block of your GraphQL schema.

**Usage**:
```python
@fraiseql.type
class User:
    id: str
    name: str
    email: EmailAddress
```

**Related**: [Type System](core-concepts/type-system.md)

---

### Query
**Definition**: GraphQL read operation that fetches data without side effects.

**Usage**:
```python
@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    return await repo.find("v_user", limit=limit)
```

**Related**: [Queries](api-reference/decorators.md#query), [CQRS](advanced/cqrs.md)

---

### Repository
**Definition**: Data access layer implementing the repository pattern. In FraiseQL, the `CQRSRepository` provides methods for database operations.

**Common Methods**:
- `find()` - Query multiple records
- `find_one()` - Query single record
- `insert()` - Create record
- `update()` - Modify record
- `delete()` - Remove record
- `execute()` - Run custom SQL

**Usage**:
```python
repo = info.context["repo"]
users = await repo.find("v_user", where={"active": True})
```

**Also called**: Data layer, database access layer

**Not called**: DAO (Data Access Object) - FraiseQL uses "repository" consistently

**Related**: [Repository API](api-reference/repository.md), [CQRS](advanced/cqrs.md)

---

### Scalar
**Definition**: GraphQL primitive type representing leaf values (strings, numbers, booleans, custom types).

**Built-in Scalars**:
- `ID` - Unique identifier
- `String` - Text
- `Int` - Integer number
- `Float` - Decimal number
- `Boolean` - True/false

**FraiseQL Custom Scalars**:
- `EmailAddress` - Validated email
- `UUID` - Universally unique identifier
- `JSON` - Arbitrary JSON data
- `Date` - Date type
- `IPv4`, `IPv6` - IP addresses
- `CIDR`, `MACAddress` - Network types
- And more...

**Related**: [Type System](core-concepts/type-system.md), [Custom Scalars](advanced/custom-scalars.md)

---

### Schema
**Definition**: The complete GraphQL type system definition describing all queries, mutations, types, and their relationships.

**In FraiseQL**: Automatically generated from Python type hints:
```python
# Python types
@fraiseql.type
class User:
    id: str
    name: str

# Generates GraphQL schema
type User {
  id: String!
  name: String!
}
```

**Related**: [Schema Generation](core-concepts/type-system.md)

---

### TurboRouter
**Definition**: FraiseQL's query pre-compilation system that caches parsed GraphQL queries for 4-10x faster execution.

**How It Works**:
1. First request: Parse GraphQL → Compile to SQL → Cache
2. Subsequent requests: Hash lookup → Pre-compiled SQL (1-2ms)

**Combined with APQ**: Achieves sub-millisecond responses

**Related**: [TurboRouter Guide](advanced/turbo-router.md), [Performance](advanced/performance.md)

---

### View
**Definition**: PostgreSQL virtual table defined by a SELECT query, computed on-demand.

**Naming Convention**: `v_*` prefix

**Types**:
- **Regular View** (`v_*`): Computed on each query, always up-to-date
- **Materialized View** (`tv_*`): Stored results, requires refresh

**Example**:
```sql
CREATE VIEW v_user AS
SELECT
    id,
    name,
    email,
    created_at
FROM users
WHERE deleted_at IS NULL;
```

**In FraiseQL**: Primary mechanism for exposing data to GraphQL queries

**Related**: [Database Views](core-concepts/database-views.md)

---

## Patterns & Best Practices

### N+1 Problem
**Definition**: Performance anti-pattern where fetching N items triggers N+1 database queries (1 for items, N for related data).

**Example**:
```python
# N+1 Problem (BAD)
posts = await repo.find("v_post")  # 1 query
for post in posts:
    author = await repo.find_one("v_user", id=post.author_id)  # N queries!
```

**Solution**: Use DataLoaders to batch requests

**Related**: [DataLoader Pattern](optimization/dataloader-pattern.md), [N+1 Elimination](advanced/eliminating-n-plus-one.md)

---

### Repository Pattern
**Definition**: Software design pattern abstracting data access behind a repository interface, allowing business logic to remain database-agnostic.

**In FraiseQL**:
```python
# Business logic uses repository abstraction
users = await repo.find("v_user")

# Repository handles actual database operations
# Implementation can change without affecting business logic
```

**Related**: [CQRS Repository](api-reference/repository.md)

---

### Type-Safe
**Definition**: Using Python type hints to ensure compile-time type checking and automatic GraphQL schema generation.

**FraiseQL Approach**:
```python
@fraiseql.type
class User:
    id: str  # Type hints drive schema
    name: str
    age: int | None  # Optional fields

# Python type checker validates this
# GraphQL schema generated automatically
```

**Benefits**:
- Catch errors before runtime
- IDE autocomplete
- Automatic schema generation
- Self-documenting code

---

## Naming Conventions

### Database Objects

| Pattern | Meaning | Example |
|---------|---------|---------|
| `v_*` | Regular view | `v_user`, `v_post` |
| `tv_*` | Materialized view | `tv_user_stats` |
| `fn_*` | PostgreSQL function | `fn_create_user` |
| `tb_*` | Table | `tb_users`, `tb_posts` |
| `pk_*` | Primary key column | `pk_user` |
| `fk_*` | Foreign key column | `fk_author_id` |

### Python Naming

| Pattern | Usage |
|---------|-------|
| `PascalCase` | Type names, class names |
| `snake_case` | Function names, variable names |
| `UPPER_CASE` | Constants |

---

## Common Abbreviations

| Abbreviation | Full Term |
|--------------|-----------|
| **APQ** | Automatic Persisted Queries |
| **CQRS** | Command Query Responsibility Segregation |
| **DDD** | Domain-Driven Design |
| **JSONB** | JSON Binary (PostgreSQL type) |
| **ORM** | Object-Relational Mapping |
| **SQL** | Structured Query Language |
| **UUID** | Universally Unique Identifier |
| **CIDR** | Classless Inter-Domain Routing |
| **RLS** | Row Level Security (PostgreSQL) |

---

## Storage & Caching

### Cache
**Definition**: Temporary storage of frequently accessed data for faster retrieval.

**FraiseQL Caching Layers**:
1. **APQ Cache**: Stores query results by hash
2. **TurboRouter Cache**: Stores pre-compiled SQL
3. **DataLoader Cache**: Per-request batching cache

**Related**: [Performance Optimization](advanced/performance-optimization-layers.md)

---

### Storage Backend
**Definition**: Underlying system storing APQ cache data.

**Options**:
- **Memory**: In-process cache (development, simple apps)
- **PostgreSQL**: Persistent database cache (production, multi-instance)
- **Redis**: External cache server (high-scale systems)

**Configuration**:
```python
config = FraiseQLConfig(
    apq_storage_backend="postgresql"  # or "memory" or "redis"
)
```

**Related**: [APQ Storage Backends](advanced/apq-storage-backends.md)

---

## Development & Tooling

### Hot Reload
**Definition**: Automatic application restart when code changes are detected during development.

**Usage**:
```bash
fraiseql dev  # Starts with hot reload
# or
uvicorn app:app --reload
```

---

### Introspection
**Definition**: GraphQL feature allowing clients to query the schema itself, powering tools like GraphQL Playground.

**Example**:
```graphql
{
  __schema {
    types {
      name
      description
    }
  }
}
```

---

## See Also

- **[Core Concepts](core-concepts/index.md)** - Fundamental FraiseQL concepts
- **[API Reference](api-reference/index.md)** - Complete API documentation
- **[Advanced Topics](advanced/index.md)** - Deep dives into FraiseQL features
- **[Examples](../examples/)** - Real-world code examples

---

**Need a term added?** [Open an issue](https://github.com/fraiseql/fraiseql/issues) or submit a PR!
