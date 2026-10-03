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
| `requires_role` | `str \| None` | JWT role required to execute any operation returning this type. Lowered onto every query and mutation whose return type it is, so the operation-level role gate enforces it. A gated type reachable as a field of a type that is not gated the same way is a **compile error** — the containing type's operations would carry no role, leaving the gated type ungated. Does **not** hide the type from GraphQL `__schema` introspection (only the REST `/introspection` route filters on it) |
| `embedded` | `bool` | Mark an embedded value object (no independent identity, nested under a parent). Declares no `sql_source`; exempt from the cascade `CascadeNode` `id: ID!` contract |
| `relationships` | `list[Relationship]` | Sub-resources the REST transport can embed — `?select=orders(id,total)`, `?select=orders.count`, `?orders.status=paid`. Also advertised per type in the served OpenAPI document and emitted by the client generator as `relationships.{ts,rs,go,py}` |

```python
@fraiseql.type(relay=True, requires_role="admin")
class AuditLog:
    id: str
    action: str
    created_at: str
```

**Relationships.** A relationship declares a sub-resource served by *its own* view and
list query, which is what lets the REST transport filter, count and paginate it
independently. That is different from a nested object field, which is projected out of the
parent row's JSONB and cannot be filtered server-side.

```python
@fraiseql.type(
    sql_source="v_user",
    relationships=[
        fraiseql.Relationship(
            name="orders",
            target_type="Order",
            cardinality="OneToMany",
            foreign_key="fk_user",     # column on the child table
            referenced_key="id",       # column on the parent table
        )
    ],
)
class User:
    id: str
```

`foreign_key` and `referenced_key` are SQL **column** names, not declared field names.
Which side each is read from swaps with the cardinality — `OneToMany` reads
`referenced_key` off the declaring type and filters `foreign_key` on the target;
`ManyToOne` and `OneToOne` do the reverse. Under the default `camelCase` naming convention
the column `fk_user` is published as the field `fkUser`, and the compiler resolves one to
the other.

A relationship no embed could follow is a **compile error**, not a silent empty result: an
undeclared `target_type`, a join column no field of that side publishes, a `target_type`
returned by no list query, an empty key, or one name declared twice. The compiled schema is
checked again when it loads, so a hand-edited artifact cannot carry one either.

**A to-one joins on a key that identifies one row.** `ManyToOne` and `OneToOne` answer with
a single object, so the key they filter the target on — `referenced_key` — must match at
most one target row. If it does not, "the" object is whichever row came back first, served
under a `200`. That is refused rather than answered, at compile time and again at load.

The entity identity needs no declaration: `id: ID` is unique under the identity contract
(ADR-0017), which is what Relay `Node`, federation `@key` and cache normalization already
read it as, and it is the conventional `referenced_key` for a to-one. Any **other** column
must say so:

```toml
[types.User.fields.identifier]
type = "String"
unique = true
```

A FraiseQL type is usually a view, and the catalogue reports no uniqueness for a view — so
this is your assertion about the relation, held to the same standard as a declared
pagination ordering key. Declaring `unique` on a column the relation does not enforce
reintroduces the arbitrary row it exists to prevent. If the key genuinely matches several
rows, the relationship is a `OneToMany`.

The same declaration in `fraiseql.toml`, for a TOML-declared type:

```toml
[types.User.relationships.orders]
target_type = "Order"
cardinality = "OneToMany"
foreign_key = "fk_user"
referenced_key = "id"
```

**Embedded value objects.** A type with no independent identity — a `Money` amount, a
`Dimensions` triple, an address-as-value — is *embedded* under a parent entity rather than
being a cache node of its own. Mark it `embedded=True`: it declares no `sql_source` and is
exempt from the cascade identity contract, so a schema that nests it under a `cascade=True`
mutation compiles (without the marker its synthesized `v_{name}` source makes it a cascade
entity that fails the `id: ID!` requirement).

```python
@fraiseql.type(embedded=True)
class Money:
    amount: int
    currency: str

@fraiseql.type(sql_source="v_order")
class Order:
    id: str
    total: Money  # embedded value object — no id of its own
```

`embedded=True` cannot be combined with `sql_source` (a value object has no backing view) or
with `cascade=True` (a value object cannot originate a cascade); both raise a `ValueError` at
authoring time. See [entity identity](adr/0017-entity-identity-contract.md) and the
[cascade surface](architecture/mutation-response.md#cascade-the-typed-cascade-surface).

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

**Which `fraiseql.toml` applies.** A `schema.json` compiles with the nearest
`fraiseql.toml` in its own directory or a parent directory, stopping at the repository
root (the directory holding `.git`). The working directory plays no part, so in a
repository with several subgraphs each schema gets its own subgraph's config wherever the
command runs from. `--config <path>` names the file explicitly. Every compile prints the
file it used (`Config: …`), or that none applied. `fraiseql run` reads its `[server]` and
`[database]` sections from the same file.

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

> **RLS + views: make `sql_source` views `security_invoker`.** If you enforce
> tenant isolation with PostgreSQL Row-Level Security, every `sql_source` view MUST
> be created `WITH (security_invoker = true)` (PostgreSQL 15+):
>
> ```sql
> CREATE VIEW v_document WITH (security_invoker = true) AS SELECT … FROM tb_document;
> ```
>
> A *default* view runs with the view owner's privileges and **silently bypasses the
> querying role's RLS** — a cross-tenant leak on ordinary reads (and in mutation
> cascades, which read the same views). `security_invoker` runs the view as the
> caller so the base-table policy applies. `fraiseql doctor --against-db` warns when
> a `sql_source` view lacks `security_invoker` while the database uses RLS.

### Project-wide inject defaults

Rather than repeating `inject=` on every operation, declare defaults once in
`fraiseql.toml`. Every query and mutation is then scoped unless it opts out, which is the
fail-closed shape: a new query is scoped, rather than unscoped until someone remembers.

```toml
# fraiseql.toml
[inject_defaults]
tenant_id = "jwt:tenant_id"     # queries and mutations

[inject_defaults.queries]
read_scope = "jwt:scope"        # queries only

[inject_defaults.mutations]
user_id = "jwt:sub"             # mutations only
```

How a default lands on an operation:

- An operation that declares the parameter itself keeps its own source.
- An operation that already injects the **same source under another name** gets nothing
  added. A mutation whose function takes the tenant as `p_tenant_id` already receives it,
  and a second argument would only break the function's arity.
- An operation opts out next to itself, for global reference data that has no tenant to
  filter on:

  ```python
  @fraiseql.query(sql_source="v_country", exclude_inject_defaults=["tenant_id"])
  def countries() -> list[Country]: ...
  ```

  An exclusion must name a default that would otherwise apply, and must not also be
  declared in `inject=`; either mistake is a compile error.

`compile --database` checks that every mutation function takes the arguments its defaults
add. A default that does not fit is reported once, naming the default and every mutation
it does not fit, with the two ways out (exclude it there, or move it from the base table
to `[inject_defaults.queries]`).

The SDK config loaders read the same section. When the schema document also carries
`inject_defaults` (emitted by an SDK from this file), the two must agree, or the compile
is refused.

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
