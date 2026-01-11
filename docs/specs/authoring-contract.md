# Authoring Contract Specification

**Version:** 1.0
**Status:** Draft
**Audience:** Schema authors, framework designers, tooling developers

---

## 1. Overview

The **authoring contract** defines how schema authors declare GraphQL types, queries, mutations, and bindings. The contract is language-agnostic (Python, TypeScript, YAML, CLI, etc.) — all must produce a valid `CompiledSchema`.

**Core principle:** Authoring layer is responsible for:
1. Declaring types and queries
2. Specifying view/procedure bindings
3. Declaring authorization rules
4. Validating against database conventions
5. Producing CompiledSchema JSON

---

## 2. Minimal Example (All Languages)

### 2.1 Python (Recommended for v1)

```python
from fraiseql import Schema, Type, Query, Field, Mutation, ID, String

schema = Schema(
    name="blog-api",
    database_target="postgresql",
    description="Simple blog API"
)

@schema.type
class User:
    """A user account."""
    id: ID
    email: str
    name: str
    posts: list["Post"]

@schema.type
class Post:
    """A blog post."""
    id: ID
    title: str
    content: str
    author: User

@schema.query
def users(where: "UserWhereInput" = None, limit: int = 100) -> list[User]:
    """Get all users."""
    pass

@schema.query
def user_by_email(email: str) -> User:
    """Get user by email."""
    pass

@schema.mutation
def create_user(email: str, name: str) -> User:
    """Create a new user."""
    pass

# Export CompiledSchema
compiled = schema.compile()
```

### 2.2 YAML (Human-friendly)

```yaml
name: blog-api
database_target: postgresql
description: Simple blog API

types:
  User:
    description: A user account
    fields:
      id:
        type: ID!
        description: Unique identifier
      email:
        type: String!
      name:
        type: String!
      posts:
        type: "[Post!]!"

  Post:
    description: A blog post
    fields:
      id:
        type: ID!
      title:
        type: String!
      content:
        type: String!
      author:
        type: User!

queries:
  users:
    description: Get all users
    args:
      where:
        type: UserWhereInput
      limit:
        type: Int
        default: 100
    returns: "[User!]!"
    binding:
      view: v_user

  user_by_email:
    description: Get user by email
    args:
      email:
        type: String!
    returns: User!
    binding:
      view: v_user
      where_column: email

mutations:
  create_user:
    description: Create a new user
    input:
      email:
        type: String!
      name:
        type: String!
    returns: User!
    binding:
      procedure: fn_create_user
```

### 2.3 GraphQL SDL (Text-based)

```graphql
type User {
  id: ID!
  email: String!
  name: String!
  posts: [Post!]!
}

type Post {
  id: ID!
  title: String!
  content: String!
  author: User!
}

type Query {
  users(where: UserWhereInput, limit: Int = 100): [User!]!
  userByEmail(email: String!): User!
}

type Mutation {
  createUser(email: String!, name: String!): User!
}

input UserWhereInput {
  id: IDFilter
  email: StringFilter
  _and: [UserWhereInput!]
  _or: [UserWhereInput!]
  _not: UserWhereInput
}
```

---

## 3. Type Declaration

### 3.1 Object Types

**Python:**
```python
@schema.type(description="A user account")
class User:
    id: ID
    email: str
    name: str
    posts: list["Post"]
    created_at: datetime

    @property
    def post_count(self) -> int:
        """Computed field (not allowed in FraiseQL)."""
        # ❌ This will fail at compile time
        pass
```

**YAML:**
```yaml
types:
  User:
    description: A user account
    fields:
      id:
        type: ID!
      email:
        type: String!
      name:
        type: String!
      posts:
        type: "[Post!]!"
        description: User's posts
      createdAt:
        type: DateTime!
```

**Rules:**
- Type names are PascalCase (singular)
- Field names are camelCase
- All fields must map to view columns or JSONB paths
- No field resolvers or computed properties
- No field arguments (except on queries/mutations)

### 3.2 Input Types

Input types are generated automatically from available columns:

```python
# NOT declared by author
# Automatically generated:
# - UserWhereInput (from v_user columns + JSONB paths)
# - UserOrderByInput (from sortable columns)
# - CreateUserInput (from fn_create_user parameters)
# - UpdateUserInput (from fn_update_user parameters)
```

**Manual input declaration (rare):**

```python
@schema.input
class CreateUserInput:
    email: str
    name: str
    password: str  # hashed server-side
```

### 3.3 Scalar Types

Built-in scalars:

```python
from fraiseql import ID, String, Int, Float, Boolean, DateTime, Date, JSON

id: ID              # UUID
email: str          # String
count: int          # Int
price: float        # Float
active: bool        # Boolean
created: datetime   # DateTime
birth_date: date    # Date
metadata: dict      # JSON
```

Custom scalars:

```python
@schema.scalar(
    name="Email",
    coerce_input="email_validation",
    coerce_output="string"
)
class Email:
    """Valid email address."""
    pattern = "^[^@]+@[^@]+\\.[^@]+$"
```

### 3.4 Enums

```python
@schema.enum
class UserRole:
    """User role enumeration."""
    ADMIN = "admin"
    USER = "user"
    GUEST = "guest"
```

---

## 4. Query Declaration

### 4.1 Basic Queries

**Python:**
```python
@schema.query(description="Get all users")
def users(
    where: "UserWhereInput" = None,
    order_by: list["UserOrderByInput"] = None,
    limit: int = 100,
    offset: int = 0
) -> list[User]:
    """Query implementation (ignored at compile time)."""
    pass
```

**Rules:**
- Function body is IGNORED (for compile-time only)
- Parameters become GraphQL arguments
- Return type determines query return
- `where` parameter is special (generates WHERE type)
- `order_by` parameter is special (generates ORDER BY type)
- `limit`/`offset` become pagination arguments

### 4.2 Query Bindings

Each query must bind to a database view:

```python
@schema.query
def users(where: "UserWhereInput" = None) -> list[User]:
    pass

# Binding is declared separately:
schema.bind(
    query="users",
    type="view",
    view="v_user",
    data_column="data"
)
```

Or inline (if language supports):

```python
@schema.query(binding={"view": "v_user", "data_column": "data"})
def users() -> list[User]:
    pass
```

### 4.3 Single-Entity Queries

```python
@schema.query
def user_by_id(id: ID) -> User:
    pass

@schema.query
def user_by_email(email: str) -> User:
    pass
```

**Binding:**
```python
schema.bind("user_by_id", "view", "v_user", where_column="id")
schema.bind("user_by_email", "view", "v_user", where_column="email")
```

---

## 5. Mutation Declaration

### 5.1 Basic Mutations

**Python:**
```python
@schema.mutation
def create_user(email: str, name: str) -> User:
    """Create a new user."""
    pass

@schema.mutation
def update_user(id: ID, name: str) -> User:
    """Update user name."""
    pass

@schema.mutation
def delete_user(id: ID) -> DeleteUserResult:
    """Delete a user (soft delete)."""
    pass
```

**Rules:**
- Parameters become mutation input
- Return type is the mutation output
- Body is IGNORED at compile time
- Each mutation binds to a stored procedure

### 5.2 Mutation Bindings

```python
schema.bind(
    mutation="create_user",
    type="procedure",
    procedure="fn_create_user",
    input_mapping={
        "email": "email_param",
        "name": "name_param"
    },
    output_mapping={
        "id": "created_id",
        "email": "created_email"
    }
)
```

**Input mapping:** GraphQL arg → function parameter
**Output mapping:** function output → GraphQL field

### 5.3 Result Types

For mutations that don't return the entity:

```python
@schema.type
class DeleteUserResult:
    success: bool
    id: ID
    deleted_at: datetime

@schema.mutation
def delete_user(id: ID) -> DeleteUserResult:
    pass
```

---

## 6. Authorization Declaration

### 6.1 Auth Context Type

```python
@schema.auth_context
class AuthContext:
    """Authentication context provided at runtime."""
    subject: str          # User ID
    roles: list[str]      # Assigned roles
    tenant_id: str        # Multi-tenant isolation
    email: str            # Email address
```

### 6.2 Auth Rules

```python
# Require authentication
@schema.query
@auth.requires_auth()
def me() -> User:
    pass

# Require specific role
@schema.mutation
@auth.requires_role("admin")
def create_user(email: str) -> User:
    pass

# Require claim to match
@schema.query
@auth.requires_claim("tenant_id")
def my_org_users() -> list[User]:
    pass

# Type-level auth
@schema.type
@auth.requires_role("admin")
class AdminPanel:
    users: list[User]
    logs: list[AuditLog]

# Field-level auth
@schema.type
class User:
    id: ID
    email: str

    @auth.requires_role("admin")
    password_hash: str
```

---

## 7. Database Introspection

The compiler must introspect the target database:

```python
# Pseudo-code: compiler discovers schema
schema.compile(
    database_url="postgresql://...",
    introspect=True  # Discover views, functions, columns
)

# Or provide manually:
schema.compile(
    bindings={
        "User": {
            "view": "v_user",
            "columns": ["id", "email", "user_id", "items__product__category_id"]
        }
    }
)
```

**Compiler checks:**
1. Does view exist? ✓
2. Does `data` column exist? ✓
3. Do filter columns exist? ✓
4. Are columns indexed? (warning if not)
5. Do procedures exist? ✓
6. Do procedure parameters match input? ✓

---

## 8. Validation Rules

The compiler must validate:

### 8.1 Type Closure
```
All referenced types must be defined.

❌ INVALID:
@schema.query
def user() -> UndefinedType:  # Error: UndefinedType not defined
    pass
```

### 8.2 Binding Existence
```
All types with queries/mutations must have bindings.

❌ INVALID:
@schema.query
def users() -> list[User]:
    pass
# Error: No binding specified for User

✓ VALID:
schema.bind("users", "view", "v_user")
```

### 8.3 View Column Validation
```
All fields must exist as columns or JSONB paths.

❌ INVALID:
@schema.type
class User:
    id: ID
    email: str
    undefined_field: str  # Error: not in v_user

✓ VALID:
@schema.type
class User:
    id: ID
    email: str
    # Must exist as: tb_user.email or v_user.data->>'email'
```

### 8.4 Operator Support
```
Used filters must be in capability manifest.

❌ INVALID (on SQLite):
where: {
    email: { _regex: "^test" }  # Error: SQLite doesn't support regex
}

✓ VALID:
where: {
    email: { _like: "test%" }   # OK: all DBs support LIKE
```

### 8.5 Authorization Validity
```
Auth rules must reference valid auth context fields.

❌ INVALID:
@auth.requires_claim("invalid_field")  # Error: not in AuthContext

✓ VALID:
@auth.requires_claim("tenant_id")      # OK: in AuthContext
```

---

## 9. Compilation Output

Compilation produces:

1. **CompiledSchema.json** — Complete executable schema
2. **schema.graphql** — Standard GraphQL SDL (for clients)
3. **validation-report.txt** — Warnings and information

```bash
$ fraiseql compile schema.py
✓ Compiled successfully
├── CompiledSchema.json (15 KB)
├── schema.graphql (8 KB)
└── validation-report.txt
    ℹ Column 'v_user.email' not indexed
    ⚠ View 'v_user_stats' is materialized, may be stale
```

---

## 3.4 Complete Custom Scalar Library

FraiseQL provides a comprehensive library of **56 custom scalar types** beyond the GraphQL standard scalars. These are organized into 18 domain-specific categories:

**Core Temporal Types:**
- `Date`, `DateTime`, `Time`, `Duration`, `DateRange`, `Timezone`

**Geographic & Spatial:**
- `Coordinate`, `Latitude`, `Longitude`, `Point`, `Polygon`, `Box`

**Network & Connectivity:**
- `IpAddress`, `CIDR`, `MacAddress`, `Hostname`, `DomainName`, `URL`

**Financial & Monetary:**
- `Money`, `CurrencyCode`, `Percentage`, `ExchangeRate`, `ISIN`, `CUSIP`, `SEDOL`, `LEI`

**Vector & Embeddings (pgvector):**
- `Vector`, `HalfVector`, `SparseVector`, `QuantizedVector`

**Content & Markup:**
- `Markdown`, `RichText`, `HTML`, `EmailAddress`, `PhoneNumber`, `JSON`, `JSONB`

**Identifiers & Codes:**
- `UUID`, `ULID`, `Snowflake`, `Slug`, `VIN`, `GTIN`, `ISBN`

**Enterprise & Hierarchical:**
- `LTree` (PostgreSQL hierarchies), `ApiKey`, `Signature`

**Complete Reference:**
See [`docs/reference/scalars.md`](../reference/scalars.md) for detailed documentation of all 56 custom scalar types, including:
- Type definitions and validation rules
- GraphQL representation (as strings or JSON)
- Example values for each scalar
- SQL column type mappings
- Performance characteristics
- Use cases and best practices

**Quick Example:**
```graphql
type User {
  id: UUID!
  email: EmailAddress!
  phone: PhoneNumber
  coordinates: Coordinate
  metadata: JSON
  embeddingVector: Vector
  signature: Signature
}
```

Each scalar type is validated at:
1. **Compile-time** — Schema validation ensures scalar is recognized
2. **Runtime** — Input validation checks value conforms to scalar's rules
3. **Database** — SQL column type matches scalar's storage format

---

## 10. Supported Authoring Formats (Priority Order)

| Format | Language | Status | Best For |
|--------|----------|--------|----------|
| **Python** | Python 3.10+ | Priority 1 | Framework, type safety |
| **YAML** | YAML | Priority 1 | Config, manual editing |
| **GraphQL SDL** | GraphQL | Priority 2 | Familiar to frontend devs |
| **TypeScript** | TypeScript | Priority 2 | Node.js projects |
| **CLI** | CLI Tool | Priority 3 | Scripting, CI/CD |

---

## 11. Example: Complete Schema (Python)

```python
from fraiseql import (
    Schema, Type, Query, Mutation, Field,
    ID, String, Int, DateTime, List,
    auth
)

# Create schema
schema = Schema(
    name="blog-api",
    version="1.0.0",
    database_target="postgresql",
    description="Simple blog API"
)

# Define types
@schema.type
class User:
    """A user account."""
    id: ID
    identifier: str
    email: str
    name: str
    created_at: DateTime
    posts: List["Post"]

@schema.type
class Post:
    """A blog post."""
    id: ID
    identifier: str
    title: str
    content: str
    created_at: DateTime
    author: User

# Define queries
@schema.query
def users(
    where: "UserWhereInput" = None,
    limit: int = 100,
    offset: int = 0
) -> List[User]:
    """Get all users with optional filtering."""
    pass

@schema.query
def user_by_email(email: str) -> User:
    """Get user by email address."""
    pass

# Define mutations
@schema.mutation
def create_user(email: str, name: str) -> User:
    """Create a new user."""
    pass

@schema.mutation
def update_user(id: ID, name: str) -> User:
    """Update user information."""
    pass

# Define authorization
@schema.auth_context
class AuthContext:
    """Authentication context at runtime."""
    subject: str          # user_id
    roles: List[str]      # ["user", "admin"]
    tenant_id: str        # multi-tenant

# Apply auth rules
@schema.query
@auth.requires_auth()
def me() -> User:
    """Get current authenticated user."""
    pass

@schema.mutation
@auth.requires_role("admin")
def create_user(email: str, name: str) -> User:
    """Create a new user (admin only)."""
    pass

# Define bindings
schema.bind("users", "view", "v_user")
schema.bind("user_by_email", "view", "v_user", where_column="email")
schema.bind("create_user", "procedure", "fn_create_user")
schema.bind("update_user", "procedure", "fn_update_user")
schema.bind("me", "view", "v_user", where_column="id")

# Compile
if __name__ == "__main__":
    compiled = schema.compile()
    print("Schema compiled successfully!")
```

---

## 12. Errors & Validation Messages

**Compilation errors should be clear:**

```
Error: Type closure violation
  Query 'users' returns 'list[User]'
  Type 'User' not defined
  → Define @schema.type class User

Error: Binding missing
  Query 'users' has no binding
  → schema.bind("users", "view", "v_user")

Error: View not found
  Binding references 'v_user_missing'
  → View does not exist in database
  → Check database schema: \dv v_user*

Error: Operator not supported
  Filter uses '_regex' on 'email' field
  → Database 'sqlite' does not support regex
  → Use '_like' instead or target 'postgresql'

Error: Auth context mismatch
  Rule requires claim 'invalid_field'
  → Field not in AuthContext
  → Add field to @schema.auth_context
```

---

*End of Authoring Contract Specification*
