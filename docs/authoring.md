# FraiseQL Authoring Guide

## Overview

FraiseQL uses a **compile-time schema authoring** model. You define your GraphQL types,
queries, mutations, and subscriptions in Python or TypeScript using decorators. The
`fraiseql-cli compile` command converts these definitions into a `schema.compiled.json`
file that the FraiseQL runtime loads at startup.

No Python or TypeScript code runs at request time — the runtime is pure Rust.

```
Python/TypeScript decorators
         ↓  fraiseql generate-schema
    schema.json
         ↓  fraiseql compile
  schema.compiled.json
         ↓  fraiseql-server loads
    GraphQL API (Rust)
```

---

## Python Quick Start

### 1. Install

```bash
pip install fraiseql
```

### 2. Define types and operations

```python
# schema.py
import fraiseql
from typing import Annotated

@fraiseql.type
class User:
    id: str
    email: str
    name: str | None
    created_at: str

@fraiseql.query(
    sql_source="v_user",
    entity_type="User",
)
def users(email: str | None = None, limit: int = 10) -> list[User]:
    """List all users with optional filtering."""

@fraiseql.query(
    sql_source="v_user",
    entity_type="User",
)
def user(id: str) -> User | None:
    """Fetch a single user by ID."""

@fraiseql.mutation(
    sql_source="fn_create_user",
    operation="insert",
    invalidates=["v_user"],
)
def create_user(email: str, name: str | None = None) -> User:
    """Create a new user account."""
```

### 3. Generate and compile

```bash
# Generate schema.json from Python definitions
fraiseql generate-schema schema.py > schema.json

# Compile to optimized schema.compiled.json
fraiseql compile schema.json
```

### 4. Run the server

```bash
fraiseql-server --schema schema.compiled.json --database-url postgres://...
```

---

## Decorator Reference

### `@fraiseql.type`

Marks a Python class as a GraphQL object type.

```python
@fraiseql.type
class Post:
    id: str
    title: str
    body: str | None
    author_id: str
    published: bool
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `implements` | `list[str] \| None` | Interface names this type implements |
| `relay` | `bool` | Enable Relay cursor pagination for this type |
| `requires_role` | `str \| None` | JWT role required to access any field on this type |

```python
@fraiseql.type(relay=True, requires_role="admin")
class AuditLog:
    id: str
    action: str
    created_at: str
```

---

### `@fraiseql.query`

Marks a function as a GraphQL query backed by a SQL view.

```python
@fraiseql.query(
    sql_source="v_post",
    entity_type="Post",
)
def posts(author_id: str | None = None, limit: int = 20) -> list[Post]:
    """List all posts with optional filtering."""
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `sql_source` | `str` | SQL view name (e.g., `"v_post"`) |
| `entity_type` | `str` | Return type name (e.g., `"Post"`) |
| `operation` | `str \| None` | SQL operation hint for the compiler |
| `inject` | `dict[str, str] \| None` | JWT claim injections (e.g., `{"user_id": "jwt:sub"}`) |

---

### `@fraiseql.mutation`

Marks a function as a GraphQL mutation backed by a SQL function.

```python
@fraiseql.mutation(
    sql_source="fn_create_post",
    operation="insert",
    invalidates=["v_post"],
)
def create_post(title: str, body: str | None = None) -> Post:
    """Create a new post."""
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `sql_source` | `str` | SQL function name (e.g., `"fn_create_post"`) |
| `operation` | `str` | Operation type: `"insert"`, `"update"`, `"delete"`, `"custom"` |
| `invalidates` | `list[str] \| None` | Cache views to invalidate on success |
| `inject` | `dict[str, str] \| None` | JWT claim injections |

---

### `@fraiseql.subscription`

Marks a function as a GraphQL subscription (real-time updates via WebSocket).

```python
@fraiseql.subscription(
    entity_type="Post",
    topic="posts",
    operation="created",
)
def post_created(author_id: str | None = None) -> Post:
    """Subscribe to new post creation events."""
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `entity_type` | `str` | Return type name |
| `topic` | `str` | Event topic (e.g., channel name in Redis or NATS subject) |
| `operation` | `str` | Event operation: `"created"`, `"updated"`, `"deleted"`, `"custom"` |

---

### `fraiseql.field()`

Adds metadata to individual fields — access control, deprecation, description.

```python
from typing import Annotated

@fraiseql.type
class Employee:
    id: str
    name: str
    # Requires scope to read — rejects the query if unauthorized
    salary: Annotated[float, fraiseql.field(requires_scope="hr:read_salary")]
    # Mask mode — returns null instead of rejecting
    ssn: Annotated[str, fraiseql.field(
        requires_scope="hr:view_pii",
        on_deny="mask",
    )]
    # Deprecated field
    legacy_id: Annotated[str, fraiseql.field(
        deprecated="Use id instead. Will be removed in v3.",
    )]
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `requires_scope` | `str \| None` | `None` | JWT scope required (e.g., `"read:User.salary"`) |
| `on_deny` | `"reject" \| "mask" \| None` | `"reject"` | Policy when scope is missing |
| `deprecated` | `str \| None` | `None` | Deprecation reason |
| `description` | `str \| None` | `None` | Field description for schema docs |

---

### `@fraiseql.enum`

Marks a Python `Enum` as a GraphQL enum type.

```python
import fraiseql
from enum import Enum

@fraiseql.enum
class UserRole(Enum):
    ADMIN = "ADMIN"
    EDITOR = "EDITOR"
    VIEWER = "VIEWER"
```

---

### `@fraiseql.input`

Marks a Python dataclass or class as a GraphQL input type.

```python
@fraiseql.input
class CreatePostInput:
    title: str
    body: str | None = None
    tags: list[str] | None = None
```

---

### `@fraiseql.interface`

Marks a Python class as a GraphQL interface.

```python
@fraiseql.interface
class Node:
    id: str

@fraiseql.type(implements=["Node"])
class User:
    id: str
    email: str
```

---

### `@fraiseql.scalar`

Registers a custom scalar type with validation logic.

```python
@fraiseql.scalar
class SlugScalar:
    """URL-safe slug (e.g., 'my-post-title')."""
    graphql_name = "Slug"
    serialize = staticmethod(str)
    parse_value = staticmethod(str)  # validation happens in SQL constraints
```

---

## TypeScript Quick Start

### 1. Install

```bash
npm install @fraiseql/sdk
```

### 2. Define types and operations

```typescript
// schema.ts
import { fraiseql } from "@fraiseql/sdk";

@fraiseql.type({ description: "A registered user" })
class User {
  id!: string;
  email!: string;
  name?: string;
}

fraiseql.registerQuery("users", {
  returnType: "User",
  returnsList: true,
  sqlSource: "v_user",
  arguments: [
    { name: "email", type: "String", nullable: true },
    { name: "limit", type: "Int",    nullable: true },
  ],
});

fraiseql.registerMutation("createUser", {
  returnType: "User",
  sqlSource: "fn_create_user",
  operation: "insert",
  invalidates: ["v_user"],
  arguments: [
    { name: "email", type: "String", nullable: false },
    { name: "name",  type: "String", nullable: true },
  ],
});
```

### 3. Generate and compile

```bash
npx fraiseql generate-schema schema.ts > schema.json
npx fraiseql compile schema.json
```

---

## Schema Compilation

The `fraiseql-cli compile` command performs:

1. **Validation** — checks type references, argument types, SQL identifier safety
2. **SQL template generation** — produces parameterized query templates per database dialect
3. **Index building** — generates O(1) lookup structures for runtime performance
4. **Config embedding** — merges `fraiseql.toml` security/caching config into the output

```bash
# Compile with custom config
fraiseql compile schema.json --config fraiseql.toml --output schema.compiled.json

# Validate without compiling
fraiseql validate schema.json
```

The compiled schema is a self-contained JSON file. Deploy it alongside the
`fraiseql-server` binary — no Python or Node.js needed at runtime.

---

## Common Patterns

### Relay Cursor Pagination

Enable Relay-compatible cursor pagination on any type:

```python
@fraiseql.type(relay=True)
class Post:
    id: str
    title: str
    created_at: str

@fraiseql.query(sql_source="v_post", entity_type="Post")
def posts(
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
) -> list[Post]:
    """Paginate posts using Relay cursor pagination."""
```

This generates `PostConnection`, `PostEdge`, and `PageInfo` types automatically.

### Field-Level Authorization

```python
@fraiseql.type
class User:
    id: str
    name: str
    # Requires 'admin:read' scope — query fails if missing
    internal_notes: Annotated[str, fraiseql.field(requires_scope="admin:read")]
    # Returns null if user lacks 'hr:view_salary' scope
    salary: Annotated[float, fraiseql.field(
        requires_scope="hr:view_salary",
        on_deny="mask",
    )]
```

### JWT Claim Injection

Inject JWT claims as SQL parameters — useful for row-level security:

```python
@fraiseql.query(
    sql_source="v_document",
    entity_type="Document",
    inject={"tenant_id": "jwt:org_id"},  # injects JWT "org_id" claim as $tenant_id
)
def documents(status: str | None = None) -> list[Document]:
    """List documents for the current user's organization."""
```

The SQL view receives `$tenant_id` as a parameter, enabling database-level tenant isolation.

### Cache Invalidation

```python
@fraiseql.mutation(
    sql_source="fn_update_post",
    operation="update",
    invalidates=["v_post", "v_post_summary"],  # clears these views from cache
)
def update_post(id: str, title: str | None = None) -> Post:
    """Update a post and invalidate related caches."""
```

---

## Troubleshooting

### `ValueError: sql_source is not a valid SQL identifier`

The `sql_source` parameter only accepts ASCII letters, digits, underscores, and an
optional schema prefix. Spaces, hyphens, and SQL keywords are rejected:

```python
# ❌ Wrong
@fraiseql.query(sql_source="my-view")

# ✅ Correct
@fraiseql.query(sql_source="v_my_view")
@fraiseql.query(sql_source="public.v_my_view")
```

### `ScopeValidationError: requires_scope format is invalid`

Scopes must follow the `namespace:resource` format:

```python
# ❌ Wrong
fraiseql.field(requires_scope="readUserSalary")

# ✅ Correct
fraiseql.field(requires_scope="read:User.salary")
fraiseql.field(requires_scope="hr:view_pii")
```

### `on_deny has no effect without requires_scope`

`on_deny` only applies when `requires_scope` is also set:

```python
# ❌ Wrong
fraiseql.field(on_deny="mask")

# ✅ Correct
fraiseql.field(requires_scope="hr:view_pii", on_deny="mask")
```

### Compilation errors: `unknown type 'MyType'`

Ensure all types referenced in queries/mutations are defined with `@fraiseql.type`
before running `fraiseql compile`. The compiler resolves all cross-references and
reports missing types with their location.

### Schema format version mismatch at runtime

If the server logs:
```
Schema format version mismatch: compiled schema has version X, but this runtime expects version Y.
```

Recompile your schema with the matching `fraiseql-cli` version:

```bash
pip install --upgrade fraiseql
fraiseql compile schema.json
```
