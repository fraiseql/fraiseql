# FraiseQL Compilation Phases: Detailed Specifications

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Compiler engineers, schema designers, framework architects

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Phase 1: Schema Parsing & Validation](#1-phase-1-schema-parsing--validation)
3. [Phase 2: Type Resolution & Linking](#2-phase-2-type-resolution--linking)
4. [Phase 3: Field Binding & Authorization](#3-phase-3-field-binding--authorization)
5. [Phase 4: Federation Analysis & Validation](#4-phase-4-federation-analysis--validation)
6. [Phase 5: Query/Mutation/Subscription Compilation](#5-phase-5-querymutationsubscription-compilation)
7. [Phase 6: Code Generation & Optimization](#6-phase-6-code-generation--optimization)
8. [Validation & Error Reporting](#7-validation--error-reporting)
9. [Compilation Examples](#8-compilation-examples)
10. [Performance Characteristics](#9-performance-characteristics)
11. [Summary & Checklist](#10-summary--checklist)

---

## Executive Summary

FraiseQL's compiler transforms user-defined schemas into deterministic, database-optimized execution plans through six sequential phases. Each phase builds on previous phases, with clearly defined inputs, outputs, and validation rules.

**Core principle**: Compile-time certainty. Everything that can be determined at compile time is determined; nothing is left to runtime interpretation.

**Compilation flow**:

```text
User Schema (Python/YAML)
    ↓ Phase 1: Schema Parsing & Validation
SchemaAST (Abstract Syntax Tree)
    ↓ Phase 2: Type Resolution & Linking
ResolvedSchema (with type references resolved)
    ↓ Phase 3: Field Binding & Authorization
BoundSchema (with field mappings and auth rules)
    ↓ Phase 4: Federation Analysis & Validation
FederationSchema (with federation metadata)
    ↓ Phase 5: Query/Mutation/Subscription Compilation
OperationPlan (executable query/mutation/subscription plans)
    ↓ Phase 6: Code Generation & Optimization
CompiledSchema (final executable IR)
    ↓
Runtime executes CompiledSchema
```text

---

## 1. Phase 1: Schema Parsing & Validation

### 1.1 Overview

**Input**: User-defined schema (Python class definitions, YAML, or SDL)
**Output**: SchemaAST with all entities, fields, relationships, and decorators
**Purpose**: Parse and validate schema syntax; identify semantic structure

### 1.2 Input Format: Python

```python
@fraiseql.type
class User:
    """A user in the system"""
    id: ID
    name: str
    email: str | None = None

    @fraiseql.field
    def profile(self) -> 'UserProfile':
        """User's extended profile"""
        pass

@fraiseql.type
@fraiseql.key(fields=["id"])  # Federation key
class Post:
    """A blog post"""
    id: ID
    title: str
    content: str
    author_id: ID
    author: User  # Relationship
    created_at: datetime

    @fraiseql.authorize(rule="owner_only")
    def delete(self) -> bool:
        """Delete this post (owner only)"""
        pass

@fraiseql.enum
class Role:
    """User roles"""
    ADMIN = "admin"
    USER = "user"
    GUEST = "guest"
```text

### 1.3 Input Format: YAML

```yaml
types:
  User:
    description: "A user in the system"
    fields:
      id:
        type: ID
        required: true
      name:
        type: String
        required: true
      email:
        type: String
        required: false
      profile:
        type: UserProfile
        resolver: user_profile_resolver

  Post:
    description: "A blog post"
    federation:
      key: ["id"]
    fields:
      id:
        type: ID
        required: true
      title:
        type: String
        required: true
      author:
        type: User
        required: true
      created_at:
        type: DateTime
        required: true

enums:
  Role:
    ADMIN: "admin"
    USER: "user"
    GUEST: "guest"
```text

### 1.4 Input Format: SDL (GraphQL Schema Definition Language)

```graphql
"""A user in the system"""
type User {
  id: ID!
  name: String!
  email: String
  profile: UserProfile
}

"""A blog post"""
type Post @key(fields: "id") {
  id: ID!
  title: String!
  content: String!
  author: User!
  created_at: DateTime!
}

enum Role {
  ADMIN
  USER
  GUEST
}
```text

### 1.5 Parsing Rules

**Type definitions:**

- Extract all `@fraiseql.type` decorated classes
- Extract all `@fraiseql.enum` enums
- Extract all `@fraiseql.interface` interfaces
- Extract all `@fraiseql.scalar` custom scalars
- Extract all `@fraiseql.union` union types

**Field extraction:**

- From each type, extract all public fields (not starting with `_`)
- Determine field type (scalar, enum, object, list, union)
- Identify field modifiers (required `!`, list `[]`, nullable)
- Extract field decorators (`@field`, `@authorize`, `@cache`, etc.)

**Relationship detection:**

- When field type is another defined type, mark as relationship
- Identify foreign key relationships (e.g., `author_id` → `author: User`)
- Mark one-to-one, one-to-many, many-to-many relationships

**Decorator extraction:**

- Extract all decorators: `@fraiseql.type`, `@fraiseql.key`, `@fraiseql.authorize`, `@fraiseql.cache`, `@fraiseql.requires`, etc.
- Preserve decorator arguments for later phases

### 1.6 Validation Rules

**Type naming:**

- ✅ Type names must be PascalCase (User, UserProfile, Post)
- ✅ Enum names must be PascalCase (Role, Status, Priority)
- ✅ Field names must be snake_case (user_id, created_at, author_email)
- ❌ Reserved type names: Query, Mutation, Subscription, _Any,_Entity

**Field definitions:**

- ✅ Must have a type annotation
- ✅ Field type must be defined or scalar
- ❌ Circular non-nullable relationships (User.best_friend: User! creates infinite depth)
- ❌ Self-referential without proper nesting control

**Decorator usage:**

- ✅ `@fraiseql.key(fields=[...])` only on types marked for federation
- ✅ `@fraiseql.external()` only on `@fraiseql.type(extend=True)` types
- ✅ `@fraiseql.authorize(rule=...)` on queries, mutations, subscriptions, or individual fields
- ❌ Multiple `@fraiseql.type` decorators on same class

### 1.7 SchemaAST Structure

Output SchemaAST:

```python
class SchemaAST:
    types: dict[str, TypeDef]          # All type definitions
    enums: dict[str, EnumDef]          # All enum definitions
    scalars: dict[str, ScalarDef]      # All scalar definitions
    unions: dict[str, UnionDef]        # All union types
    interfaces: dict[str, InterfaceDef]  # All interfaces
    queries: dict[str, QueryDef]       # Query operations
    mutations: dict[str, MutationDef]  # Mutation operations
    subscriptions: dict[str, SubscriptionDef]  # Subscription operations

class TypeDef:
    name: str
    description: str | None
    fields: dict[str, FieldDef]
    decorators: dict[str, Any]  # e.g., {"key": ["id"], "extend": False}
    source_location: SourceLocation  # File, line, column

class FieldDef:
    name: str
    type: TypeReference  # ID, String, [Post], Post!, etc.
    required: bool       # True for ! modifier
    list: bool          # True for [] modifier
    description: str | None
    decorators: dict[str, Any]  # e.g., {"authorize": "owner_only"}
    default_value: Any | None
    source_location: SourceLocation
```text

---

## 2. Phase 2: Type Resolution & Linking

### 2.1 Overview

**Input**: SchemaAST with unresolved type references
**Output**: ResolvedSchema with all type references verified and linked
**Purpose**: Resolve forward references, check type existence, build type dependency graph

### 2.2 Type Resolution Process

**Step 1: Build type registry**

```python
type_registry = {
    "User": TypeDef(...),
    "Post": TypeDef(...),
    "Role": EnumDef(...),
    # Built-in scalars
    "ID": SCALAR_ID,
    "String": SCALAR_STRING,
    "Int": SCALAR_INT,
    "Float": SCALAR_FLOAT,
    "Boolean": SCALAR_BOOLEAN,
    "DateTime": SCALAR_DATETIME,
    "JSON": SCALAR_JSON,
    "UUID": SCALAR_UUID,
}
```text

**Step 2: Resolve all type references**

For each field with type reference:

```python
# Field definition: author: User
field.type = TypeReference("User")  # Unresolved

# Resolve to actual type:
field.type_def = type_registry["User"]  # Resolved

# If type not found:
raise CompilationError(
    f"Type 'User' not defined. Line {field.source_location.line}",
    code="E_SCHEMA_UNKNOWN_TYPE_101"
)
```text

**Step 3: Resolve list and nullable modifiers**

```python
# Field: tags: [String!]!
# Breakdown:
#   - List of: String!
#   - Required: true
#   - Element required: true

field.list = True
field.element_required = True
field.required = True
```text

**Step 4: Handle forward references**

```python
# Field: posts: [Post]  (defined before Post type)
# In Python: class User -> field posts: [Post] (string forward reference)
# In Phase 2: Resolve "Post" string to actual Post type

# Convert:
field.type = TypeReference("Post")  # String reference
# To:
field.type_def = type_registry["Post"]  # Resolved
```text

### 2.3 Dependency Analysis

Build type dependency graph:

```text
User
├─ depends on: UserProfile, Role
└─ no dependencies on Post

Post
├─ depends on: User, DateTime
└─ no dependencies on User (author_id is scalar)

Comment
├─ depends on: Post, User, DateTime
└─ depends on: Post (circular with Post.comments)
```text

**Circular dependency detection:**

```python
# Circular but safe:
User.posts: [Post]  # One-to-many
Post.author: User   # Many-to-one

# Circular but problem (infinite nesting):
User.best_friend: User!  # Can be nested infinitely
# Solution: Mark with depth limit @fraiseql.depth(max=2)

# Circular but allowed if nullable:
User.profile: UserProfile
UserProfile.user: User | None  # Nullable, can be null at leaf
```text

### 2.4 Validation Rules

**Type existence:**

- ✅ All field types must be defined or built-in scalar
- ❌ Reference to undefined type (e.g., `author: User` but User not defined)

**Forward references:**

- ✅ Can reference types defined later in schema
- ✅ Can use string forward references in Python (e.g., `'User'`)

**Circular dependencies:**

- ✅ Allowed (User → Post → User)
- ✅ If all cycles are through nullable fields
- ✅ If marked with depth limit
- ❌ If creates infinite non-nullable cycle (Post.self: Post!)

**Generic types:**

- ✅ List types (e.g., `[Post]`)
- ✅ Nullable types (e.g., `Post | None`)
- ✅ Non-nullable types (e.g., `Post!`)
- ❌ Nested generics (e.g., `[[String]]` - not allowed)

### 2.5 ResolvedSchema Structure

```python
class ResolvedSchema:
    types: dict[str, ResolvedTypeDef]
    dependency_graph: Dict[str, Set[str]]  # Type -> dependencies

class ResolvedTypeDef:
    name: str
    fields: dict[str, ResolvedFieldDef]
    decorators: dict[str, Any]

class ResolvedFieldDef:
    name: str
    type_def: TypeDef | EnumDef | ScalarDef  # Resolved
    required: bool
    list: bool
    decorators: dict[str, Any]
```text

---

## 3. Phase 3: Field Binding & Authorization

### 3.1 Overview

**Input**: ResolvedSchema with types and fields
**Output**: BoundSchema with field bindings to database columns and authorization rules applied
**Purpose**: Map GraphQL fields to database columns; apply authorization rules; validate data access

### 3.2 Field Binding Process

**Step 1: Identify database mapping**

```python
# GraphQL type: User
# Database table: tb_user
# Database view: v_user

# Field mappings:
User.id → tb_user.pk_user (primary key)
User.email → tb_user.email (column)
User.name → tb_user.name (column)
User.created_at → tb_user.created_at (column)
User.profile → v_user_profile (via join or subquery)
```text

**Step 2: Resolve database column names**

```python
# User.email → lookup in tb_user columns
# If column not found:
raise CompilationError(
    f"Field 'email' has no database mapping. "
    f"Define mapping: @fraiseql.column('user_email')",
    code="E_BINDING_NO_COLUMN_201"
)

# If explicit mapping exists:
@fraiseql.type
class User:
    @fraiseql.column("email_address")  # Maps to column 'email_address'
    email: str
```text

**Step 3: Handle relationships**

```python
# Post.author: User
# Resolve to: JOIN tb_user ON tb_post.author_id = tb_user.pk_user

# Foreign key detection:
# If field ends with "_id" → scalar foreign key
# If field has same name as type (lowercase) → relationship field

Post.author_id: ID  # Foreign key (scalar)
Post.author: User   # Relationship (object)
```text

**Step 4: Apply field-level authorization**

```python
# Field with authorization:
@fraiseql.type
class User:
    @fraiseql.authorize(rule="owner_or_admin")
    ssn: str

# Authorization binding:
# User.ssn → apply "owner_or_admin" rule at query time
# Rule means: Only owner of user or admin can access ssn
```text

### 3.3 Authorization Rule Compilation

**Rule types:**

```python
# 1. Public (no rule, accessible to everyone)
@fraiseql.type
class Post:
    title: str  # No @authorize, public

# 2. Owner-only
@fraiseql.type
class User:
    @fraiseql.authorize(rule="owner_only")
    email: str

# 3. Role-based
@fraiseql.type
class AdminPanel:
    @fraiseql.authorize(rule="role:admin")
    api_keys: [str]

# 4. Custom rule
@fraiseql.type
class Post:
    @fraiseql.authorize(rule="is_published_or_author")
    content: str

# 5. Field-level masking
@fraiseql.type
class User:
    @fraiseql.mask(
        show_to=["owner", "admin"],
        hide_from=["public"],
        masked_value=None
    )
    ssn: str
```text

**Rule resolution:**

```python
# "owner_only" →
# Built-in rule: Check if current_user.id == resource.id

# "role:admin" →
# Built-in rule: Check if "admin" in current_user.roles

# "is_published_or_author" →
# Custom rule: Compile from rule definition in schema
```text

### 3.4 Masking & Filtering

**Field-level masking:**

```python
# Rule: "owner_or_admin" on User.ssn
# Current user: Guest
# Result: Field returns NULL

# Rule: "role:admin" on AdminPanel.api_keys
# Current user: Admin
# Result: Field returns actual data

# If field is list and user unauthorized:
# Result: Return empty list []

# If field is required and user unauthorized:
# Result: GraphQL null error (cannot return null for non-null field)
```text

**Row-level security (applied in Phase 5):**

```python
# Query: users { id email }
# RLS rule: "current_user.department == user.department"
# Result: Only return users in same department

# RLS rule: "current_user.id == user.id OR current_user.role == 'admin'"
# Result: Only return own user + admin can see all
```text

### 3.5 BoundSchema Structure

```python
class BoundSchema:
    types: dict[str, BoundTypeDef]
    authorization_rules: dict[str, AuthorizationRule]
    database_mappings: dict[str, DatabaseMapping]

class BoundTypeDef:
    name: str
    database_table: str
    database_view: str | None
    fields: dict[str, BoundFieldDef]

class BoundFieldDef:
    name: str
    database_column: str | None  # For scalars
    relationship: Relationship | None  # For objects
    authorization_rule: AuthorizationRule | None
    masking_rule: MaskingRule | None

class AuthorizationRule:
    rule_type: str  # "public", "owner_only", "role:X", "custom"
    custom_rule: str | None  # SQL WHERE clause if custom

class MaskingRule:
    show_to: list[str]  # Roles/users who can see
    hide_from: list[str]  # Roles/users who cannot see
    masked_value: Any  # What to show if masked (None, 0, "", etc.)
```text

---

## 4. Phase 4: Federation Analysis & Validation

### 4.1 Overview

**Input**: BoundSchema with field bindings
**Output**: FederationSchema with federation metadata, entity resolution functions, and foreign table definitions
**Purpose**: Validate federation contracts; generate entity resolution logic; prepare database linking

### 4.2 Federation Contract Validation

**Step 1: Extract federation decorators**

```python
@fraiseql.type
@fraiseql.key(fields=["id"])  # Primary key
@fraiseql.key(fields=["email"])  # Alternative key
class User:
    id: ID
    email: str
    name: str
```text

**Step 2: Validate key fields**

```python
# Validate key field exists:
# @key(fields=["id"]) → field "id" must exist ✅
# @key(fields=["nonexistent"]) → Error ❌

# Validate key field is selectable (not just JSONB):
# Fields must map to database columns for efficient lookup

# Validate key field is indexed:
# Fields should have database index for performance
```text

**Step 3: Validate extended types**

```python
@fraiseql.type(extend=True)  # This type extends another subgraph's type
@fraiseql.key(fields=["id"])  # Must have same key as original
class Post:
    id: ID = fraiseql.external()  # Mark external field

    # New field owned by this subgraph:
    comments: [Comment]
```text

**Validation rules:**

- ✅ Extended types must have `@key` matching original type's `@key`
- ✅ `@external()` fields must be in original type
- ✅ New fields must not conflict with original type
- ❌ Extended type changes key definition
- ❌ External field not in original type

### 4.3 Federation Entity Resolution

**Step 1: Generate entity resolution functions**

For each `@key` on each type:

```python
# User @key(fields=["id"])
# Generate SQL function:
CREATE FUNCTION resolve_user_by_id(keys UUID[]) RETURNS JSONB[] AS $$
  SELECT array_agg(data ORDER BY idx)
  FROM unnest(keys) WITH ORDINALITY AS t(key, idx)
  JOIN v_user ON v_user.id = t.key
$$ LANGUAGE sql STABLE;

# User @key(fields=["email"])
# Generate SQL function:
CREATE FUNCTION resolve_user_by_email(keys TEXT[]) RETURNS JSONB[] AS $$
  SELECT array_agg(data ORDER BY idx)
  FROM unnest(keys) WITH ORDINALITY AS t(key, idx)
  JOIN v_user ON v_user.email = t.key
$$ LANGUAGE sql STABLE;
```text

**Step 2: Generate dispatch metadata**

```python
federation_metadata = {
    "entities": {
        "User": {
            "keys": [
                {
                    "fields": ["id"],
                    "db_function": "resolve_user_by_id",
                    "arg_types": ["UUID"]
                },
                {
                    "fields": ["email"],
                    "db_function": "resolve_user_by_email",
                    "arg_types": ["TEXT"]
                }
            ]
        }
    }
}
```text

### 4.4 Database Linking Configuration (PostgreSQL FDW)

**Step 1: Detect federation targets**

```python
# FraiseQL schema references external types:
@fraiseql.type(extend=True)
@fraiseql.key(fields=["id"])
class Product:  # Extended from Products subgraph
    id: ID = fraiseql.external()
    vendor: Vendor = fraiseql.requires(fields=["id"])  # Requires external field
```text

**Step 2: Generate foreign table definitions**

If Products subgraph is also FraiseQL on PostgreSQL:

```sql
-- Create FDW server (one per external subgraph)
CREATE SERVER products_fdw FOREIGN DATA WRAPPER postgres_fdw
  OPTIONS (host 'products-db', dbname 'products', port '5432');

-- Create foreign table (schema mapped from FraiseQL view)
CREATE FOREIGN TABLE products_schema_v_product (
    pk_product INTEGER,
    id UUID,
    vendor_id UUID,
    data JSONB
) SERVER products_fdw OPTIONS (schema_name 'products_schema', table_name 'v_product');

-- Create user mapping
CREATE USER MAPPING FOR current_user SERVER products_fdw
  OPTIONS (user 'fdw_user', password 'secret');
```text

**Step 3: Generate entity resolution with FDW joins**

```sql
-- Entity resolution for Product with vendor relationship
CREATE FUNCTION resolve_product_with_vendor(keys UUID[]) RETURNS JSONB[] AS $$
  SELECT array_agg(
    p.data || jsonb_build_object(
      'vendor', v.data
    ) ORDER BY idx
  )
  FROM unnest(keys) WITH ORDINALITY AS t(key, idx)
  JOIN products_schema_v_product p ON p.id = t.key
  LEFT JOIN vendors_schema_v_vendor v ON v.id = p.vendor_id
$$ LANGUAGE sql STABLE;
```text

### 4.5 FederationSchema Structure

```python
class FederationSchema:
    entities: dict[str, EntityDefinition]
    federation_functions: dict[str, FunctionDefinition]
    database_links: dict[str, DatabaseLink]

class EntityDefinition:
    name: str
    keys: list[KeyDefinition]
    is_extended: bool
    external_fields: list[str]

class KeyDefinition:
    fields: list[str]  # ["id"] or ["email"] or ["id", "email"]
    db_function: str  # "resolve_user_by_id"
    arg_types: list[str]  # ["UUID"] or ["TEXT"]

class DatabaseLink:
    name: str  # "products_fdw"
    db_type: str  # "postgresql", "sqlserver", "mysql"
    connection_string: str
    foreign_tables: dict[str, ForeignTableDef]
```text

---

## 5. Phase 5: Query/Mutation/Subscription Compilation

### 5.1 Overview

**Input**: FederationSchema with federation metadata
**Output**: OperationPlan with executable plans for queries, mutations, subscriptions
**Purpose**: Compile GraphQL operations into database queries, apply authorization, optimize execution

### 5.2 Query Compilation

**Step 1: Parse query structure**

```graphql
query GetPosts($published: Boolean) {
  posts(where: { published: $published }, first: 20) {
    id
    title
    author {
      id
      name
    }
    comments(first: 5) {
      id
      content
    }
  }
}
```text

**Step 2: Build execution plan**

```text
QueryPlan:
  ├─ Resolve root: posts
  │  └─ Database query: SELECT * FROM v_post WHERE published = $1 LIMIT 20
  │  └─ Apply authorization: Filter posts by user's view rules
  │
  ├─ Resolve field: id
  │  └─ No join needed (already in v_post)
  │
  ├─ Resolve field: title
  │  └─ No join needed (already in v_post)
  │
  ├─ Resolve field: author
  │  └─ Database join: JOIN v_user ON v_post.author_id = v_user.id
  │
  ├─ Resolve field: author.id
  │  └─ Already available from join
  │
  ├─ Resolve field: author.name
  │  └─ Already available from join
  │
  ├─ Resolve field: comments
  │  └─ Database join: JOIN v_comment ON v_post.id = v_comment.post_id LIMIT 5
  │
  └─ Resolve field: comments.id, comments.content
     └─ Already available from comment join
```text

**Step 3: Optimize and generate SQL**

```sql
-- Compiled SQL plan:
SELECT
  p.id AS "id",
  p.title AS "title",
  jsonb_build_object(
    'id', u.id,
    'name', u.name
  ) AS "author",
  (
    SELECT jsonb_agg(jsonb_build_object(
      'id', c.id,
      'content', c.content
    ) ORDER BY c.created_at DESC LIMIT 5)
    FROM v_comment c
    WHERE c.post_id = p.id
  ) AS "comments"
FROM v_post p
JOIN v_user u ON p.author_id = u.id
WHERE p.published = true
LIMIT 20
```text

**Step 4: Apply authorization**

```sql
-- Add row-level security WHERE clause:
SELECT ...
FROM v_post p
JOIN v_user u ON p.author_id = u.id
WHERE p.published = true
  AND (
    -- Authorization: User can see own posts or published posts
    p.author_id = $current_user_id OR p.published = true
  )
LIMIT 20
```text

### 5.3 Mutation Compilation

**Step 1: Parse mutation structure**

```graphql
mutation CreatePost($title: String!, $content: String!) {
  createPost(input: {
    title: $title,
    content: $content
  }) {
    id
    title
  }
}
```text

**Step 2: Build execution plan**

```text
MutationPlan:
  ├─ Validate input: title required, content required
  ├─ Apply authorization: Check if user can create posts
  ├─ Serialize input: Convert GraphQL input to database values
  ├─ Execute: INSERT INTO tb_post (title, content, author_id) VALUES (...)
  ├─ Return: SELECT * FROM v_post WHERE id = ...
  └─ Apply authorization: Check if user can read created post
```text

**Step 3: Compile to SQL**

```sql
-- Insert + return in single operation (RETURNING clause):
INSERT INTO tb_post (title, content, author_id, created_at)
VALUES ($1, $2, $current_user_id, NOW())
RETURNING (
  SELECT jsonb_build_object(
    'id', id,
    'title', title
  ) FROM v_post WHERE tb_post.id = v_post.id
)
```text

### 5.4 Subscription Compilation

**Step 1: Parse subscription structure**

```graphql
subscription OnPostCreated {
  postCreated {
    id
    title
    author {
      name
    }
  }
}
```text

**Step 2: Build event subscription plan**

```text
SubscriptionPlan:
  ├─ Event trigger: PostgreSQL LISTEN "post_created"
  ├─ Event handler: PostgreSQL NOTIFY with entity ID
  ├─ Entity resolution: Query entity by ID (same as query resolution)
  ├─ Apply authorization: Only notify if user can see post
  ├─ Transform: Convert entity to subscription response format
  └─ Transport: Send via WebSocket/webhook/message queue
```text

**Step 3: Generate event trigger SQL**

```sql
-- Trigger function:
CREATE FUNCTION notify_post_created() RETURNS trigger AS $$
BEGIN
  PERFORM pg_notify(
    'post_created',
    json_build_object(
      'id', NEW.id,
      'author_id', NEW.author_id,
      'title', NEW.title
    )::text
  );
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Attach trigger to table:
CREATE TRIGGER post_created_trigger
AFTER INSERT ON tb_post
FOR EACH ROW EXECUTE FUNCTION notify_post_created();
```text

**Step 4: Runtime subscription handler**

```rust
// At runtime, when subscription created:
// 1. LISTEN "post_created"
// 2. On NOTIFY event:
//    - Parse event payload (entity ID)
//    - Check user authorization
//    - Query entity (same SQL as query resolution)
//    - Send to client
```text

### 5.5 Field Resolution Strategies

**Strategy 1: Inline (no join needed)**

```graphql
# Field value is in current row
query {
  post(id: "1") {
    id      # Already have from query
    title   # Already have from query
  }
}
```text

**Strategy 2: Join (direct relationship)**

```graphql
# Field requires join to related table
query {
  post(id: "1") {
    author {  # Requires JOIN v_user
      id
      name
    }
  }
}
```text

**Strategy 3: Subquery (filtered relationship)**

```graphql
# Field requires filtered subquery
query {
  post(id: "1") {
    comments(first: 5) {  # Requires subquery with LIMIT
      id
      content
    }
  }
}
```text

**Strategy 4: Federation (external type)**

```graphql
# Field requires federation resolution
query {
  post(id: "1") {
    product {  # From external Products subgraph
      id
      name
    }
  }
}
```text

### 5.6 OperationPlan Structure

```python
class OperationPlan:
    operation_type: str  # "query", "mutation", "subscription"
    root_field: str
    field_plans: dict[str, FieldResolutionPlan]
    sql_plan: str | None  # For queries/mutations
    authorization: AuthorizationPlan
    parameters: dict[str, ParameterDef]

class FieldResolutionPlan:
    field_name: str
    strategy: str  # "inline", "join", "subquery", "federation"
    sql_join: str | None
    nested_fields: dict[str, FieldResolutionPlan]
    authorization: AuthorizationPlan

class AuthorizationPlan:
    rules: list[AuthorizationRule]
    sql_where_clause: str  # SQL WHERE for row-level security
    field_masks: dict[str, MaskingRule]
```text

---

## 6. Phase 6: Code Generation & Optimization

### 6.1 Overview

**Input**: OperationPlan with execution plans
**Output**: CompiledSchema (final executable IR)
**Purpose**: Generate optimized code; prepare for runtime execution; compress for distribution

### 6.2 SQL Generation & Optimization

**Step 1: Generate optimal SQL**

```python
# From OperationPlan, generate SQL that:
# 1. Minimizes joins (use JSONB aggregation where possible)
# 2. Pushes authorization down to WHERE clause
# 3. Limits result sets early (push LIMIT down)
# 4. Uses prepared statements (parameterized queries)
# 5. Enables query plan caching
```text

**Step 2: Query optimization techniques**

```sql
-- Technique 1: JSONB aggregation (avoids JOIN overhead)
SELECT
  jsonb_build_object(
    'id', id,
    'title', title,
    'author', jsonb_build_object('id', author_id)
  ) AS result
FROM v_post
WHERE ...

-- Technique 2: Window functions (for pagination)
SELECT * FROM (
  SELECT *,
    ROW_NUMBER() OVER (ORDER BY created_at DESC) as rn
  FROM v_post
  WHERE ...
) sub
WHERE rn BETWEEN $1 AND $2

-- Technique 3: CTE for complex filters
WITH filtered_posts AS (
  SELECT * FROM v_post
  WHERE published = true AND author_id IN (...)
)
SELECT ... FROM filtered_posts

-- Technique 4: Lateral joins (for dependent subqueries)
SELECT p.*, a.all_comments
FROM v_post p
LEFT JOIN LATERAL (
  SELECT jsonb_agg(data) as all_comments
  FROM v_comment
  WHERE post_id = p.id
  LIMIT 5
) a ON true
```text

### 6.3 Prepared Statement Generation

**Step 1: Identify parameters**

```graphql
query GetPosts($published: Boolean!, $limit: Int) {
  posts(where: { published: $published }, first: $limit) {
    id
  }
}
```text

**Step 2: Generate prepared statement**

```sql
PREPARE get_posts (BOOLEAN, INTEGER) AS
  SELECT jsonb_build_object('id', id)
  FROM v_post
  WHERE published = $1
  LIMIT COALESCE($2, 20);
```text

**Step 3: Parameter binding at runtime**

```rust
// At runtime:
let params = (published_value, limit_value);
db.execute_prepared("get_posts", params).await?
```text

### 6.4 Caching Metadata Generation

**Step 1: Identify cacheable operations**

```python
# Queries that can be cached:
# 1. Side-effect free (SELECT only)
# 2. Deterministic (same input = same output)
# 3. Not sensitive (not returning personal data)

# Mark for caching:
CacheMetadata {
    operation_name: "GetPosts",
    operation_type: "query",
    cacheable: true,
    cache_key: "GetPosts:$published:$limit",
    ttl_seconds: 300  # Cache 5 minutes
}
```text

**Step 2: Authorization-aware cache keys**

```python
# Different users see different results (row-level security)
# Cache key must include user context:

cache_key = f"GetPosts:$published:$limit:user_{user_id}"
# Now:
# - User 1 sees User 1's posts
# - User 2 sees User 2's posts
# - Cache keeps both separate
```text

### 6.5 Error Handling Code Generation

**Step 1: Generate error cases**

```rust
// From compilation, generate error handling:
// 1. Parse errors (query syntax invalid)
// 2. Binding errors (field not found)
// 3. Authorization errors (user not allowed)
// 4. Database errors (query timeout, deadlock)
// 5. Type errors (wrong argument type)
```text

**Step 2: Error code mapping**

```python
error_cases = {
    "unknown_field": "E_BINDING_UNKNOWN_FIELD_202",
    "missing_argument": "E_VALIDATION_MISSING_ARGUMENT_102",
    "invalid_type": "E_VALIDATION_INVALID_TYPE_103",
    "unauthorized": "E_AUTH_PERMISSION_401",
    "query_timeout": "E_DB_QUERY_TIMEOUT_302",
}
```text

### 6.6 CompiledSchema Structure

**Final output:**

```json
{
  "framework_version": "2.0.0",
  "compiled_schema_version": 1,
  "schema_version": "1.0.0",

  "types": {
    "User": {
      "name": "User",
      "fields": {
        "id": { "type": "ID", "database_column": "pk_user" },
        "name": { "type": "String", "database_column": "name" },
        "email": { "type": "String", "database_column": "email" }
      }
    }
  },

  "queries": {
    "posts": {
      "sql_plan": "SELECT ... FROM v_post WHERE ...",
      "parameters": ["published", "limit"],
      "authorization": { "rules": [...] },
      "cache_key": "GetPosts:$published:$limit:user_{user_id}",
      "cache_ttl": 300
    }
  },

  "mutations": {
    "createPost": {
      "sql_plan": "INSERT INTO tb_post ... RETURNING ...",
      "parameters": ["title", "content"],
      "authorization": { "rules": [...] }
    }
  },

  "subscriptions": {
    "postCreated": {
      "event_trigger": "post_created",
      "entity_resolution": "query_posts",
      "authorization": { "rules": [...] }
    }
  },

  "federation": {
    "entities": {
      "User": {
        "keys": [
          { "fields": ["id"], "db_function": "resolve_user_by_id" }
        ]
      }
    }
  },

  "error_codes": {
    "E_SCHEMA_UNKNOWN_TYPE_101": "Type not found",
    "E_BINDING_UNKNOWN_FIELD_202": "Field not found",
    ...
  }
}
```text

### 6.7 Optimization Techniques

**1. Dead code elimination:**

- Remove unreachable fields
- Remove unused joins

**2. Query plan merging:**

- Combine multiple subqueries when possible
- Flatten nested queries

**3. Join order optimization:**

- Order joins by selectivity (most filtering first)
- Use statistics to determine best join order

**4. Index utilization:**

- Identify WHERE clauses that can use indexes
- Prefer indexed columns in filters

**5. Memory optimization:**

- Avoid loading large JSONB objects unnecessarily
- Use streaming for large result sets

**6. Parallelization hints:**

- Mark queries that can execute in parallel
- Identify independent subqueries

---

## 7. Validation & Error Reporting

### 7.1 Compilation Error Categories

**Syntax Errors:**

```text
E_SCHEMA_SYNTAX_ERROR_001: Invalid schema syntax
E_SCHEMA_DUPLICATE_TYPE_002: Type defined twice
E_SCHEMA_INVALID_NAME_003: Invalid type/field name
```text

**Resolution Errors:**

```text
E_SCHEMA_UNKNOWN_TYPE_101: Type reference not found
E_SCHEMA_CIRCULAR_DEPENDENCY_102: Circular non-nullable reference
E_SCHEMA_INVALID_MODIFIER_103: Invalid type modifier
```text

**Binding Errors:**

```text
E_BINDING_NO_COLUMN_201: Field has no database mapping
E_BINDING_UNKNOWN_FIELD_202: Field not found in type
E_BINDING_AMBIGUOUS_MAPPING_203: Multiple possible mappings
E_BINDING_NO_RELATIONSHIP_204: Cannot resolve relationship
```text

**Federation Errors:**

```text
E_FED_NO_KEY_301: Extended type missing @key
E_FED_KEY_MISMATCH_302: @key doesn't match original type
E_FED_EXTERNAL_NOT_FOUND_303: External field not in original type
E_FED_INVALID_REQUIRES_304: @requires field not found
```text

**Query Errors:**

```text
E_QUERY_UNKNOWN_FIELD_401: Field doesn't exist in type
E_QUERY_INVALID_ARGUMENT_402: Argument doesn't exist or wrong type
E_QUERY_AUTHORIZATION_DENIED_403: Query not allowed by authorization rules
E_QUERY_AMBIGUOUS_FRAGMENT_404: Fragment definition ambiguous
```text

**Code Generation Errors:**

```text
E_CODEGEN_INVALID_SQL_501: Generated SQL is invalid
E_CODEGEN_OPTIMIZATION_FAILED_502: Optimization produced wrong result
E_CODEGEN_MEMORY_LIMIT_503: Generated code too large
```text

### 7.2 Error Reporting Format

```json
{
  "error": {
    "message": "Type 'User' not defined",
    "code": "E_SCHEMA_UNKNOWN_TYPE_101",
    "phase": 2,
    "location": {
      "file": "schema.py",
      "line": 15,
      "column": 8,
      "snippet": "    author: User  # ← Undefined type"
    },
    "context": {
      "type": "Post",
      "field": "author"
    },
    "suggestions": [
      "Define type User: @fraiseql.type class User: ...",
      "Import User from another module",
      "Check spelling: Did you mean 'UserProfile'?"
    ]
  }
}
```text

### 7.3 Validation Rules Matrix

| Phase | Input | Validation | Output |
|-------|-------|-----------|--------|
| 1 | Python/YAML | Syntax, structure | SchemaAST |
| 2 | SchemaAST | Type existence, circular refs | ResolvedSchema |
| 3 | ResolvedSchema | Database mappings, auth | BoundSchema |
| 4 | BoundSchema | Federation contracts | FederationSchema |
| 5 | FederationSchema | Operation validity | OperationPlan |
| 6 | OperationPlan | SQL generation | CompiledSchema |

---

## 8. Compilation Examples

### 8.1 Simple Type Compilation

**Input:**

```python
@fraiseql.type
class User:
    id: ID
    name: str
    email: str | None = None
```text

**Phase 1 (Parsing):**

```python
SchemaAST {
    types: {
        "User": TypeDef {
            name: "User",
            fields: {
                "id": FieldDef(type="ID", required=True),
                "name": FieldDef(type="String", required=True),
                "email": FieldDef(type="String", required=False)
            }
        }
    }
}
```text

**Phase 2 (Resolution):**

```python
ResolvedSchema {
    types: {
        "User": ResolvedTypeDef {
            type_def: <ID type>,
            type_def: <String type>,
            type_def: <String type>
        }
    }
}
```text

**Phase 3 (Binding):**

```python
BoundSchema {
    types: {
        "User": BoundTypeDef {
            database_table: "tb_user",
            fields: {
                "id": BoundFieldDef(database_column: "pk_user"),
                "name": BoundFieldDef(database_column: "name"),
                "email": BoundFieldDef(database_column: "email")
            }
        }
    }
}
```text

**Phase 6 (Final):**

```json
{
  "types": {
    "User": {
      "fields": {
        "id": { "type": "ID", "column": "pk_user" },
        "name": { "type": "String", "column": "name" },
        "email": { "type": "String", "column": "email" }
      }
    }
  }
}
```text

### 8.2 Query Compilation with Authorization

**Input schema:**

```python
@fraiseql.type
class Post:
    id: ID
    title: str

    @fraiseql.authorize(rule="published_or_author")
    content: str

@fraiseql.type
class User:
    id: ID

    @fraiseql.authorize(rule="owner_only")
    email: str
```text

**Phase 3 (Authorization binding):**

```python
# content field: Apply "published_or_author" rule
# email field: Apply "owner_only" rule
```text

**Phase 5 (Query compilation):**

```graphql
query GetPost($id: ID!) {
  post(id: $id) {
    id
    title
    content     # ← Will have authorization rule
    author {
      id
      email     # ← Will have authorization rule
    }
  }
}
```text

**Phase 6 (SQL generation):**

```sql
SELECT
  p.id,
  p.title,
  CASE
    -- Show content if published OR user is author
    WHEN p.published OR p.author_id = $current_user_id
    THEN p.content
    ELSE NULL  -- Masked for non-authorized users
  END AS content,
  jsonb_build_object(
    'id', u.id,
    CASE
      -- Show email only if user is owner
      WHEN u.id = $current_user_id
      THEN u.email
      ELSE NULL  -- Masked for non-owners
    END AS email
  ) AS author
FROM v_post p
JOIN v_user u ON p.author_id = u.id
WHERE p.id = $1
```text

---

## 9. Performance Characteristics

### 9.1 Compilation Time

**Typical timings:**

| Schema Size | Phases 1-3 | Phase 4 | Phase 5 | Phase 6 | Total |
|------------|-----------|---------|---------|---------|-------|
| Small (10 types) | 50ms | 10ms | 20ms | 15ms | ~95ms |
| Medium (50 types) | 150ms | 30ms | 80ms | 50ms | ~310ms |
| Large (200 types) | 400ms | 100ms | 250ms | 150ms | ~900ms |
| Enterprise (1000 types) | 1500ms | 400ms | 1000ms | 600ms | ~3500ms |

### 9.2 Compiled Schema Size

**Typical sizes (after compression):**

| Schema Complexity | JSON Size | Compressed |
|------------------|-----------|------------|
| 10 types | 50KB | 12KB |
| 50 types | 250KB | 60KB |
| 200 types | 1.2MB | 280KB |
| 1000 types | 6MB | 1.4MB |

### 9.3 SQL Query Performance

**Optimization impact:**

| Query Type | Without Optimization | With Optimization | Improvement |
|-----------|------------------|------------------|------------|
| Simple SELECT | 10ms | 8ms | 20% |
| Join (1 level) | 15ms | 12ms | 20% |
| Nested query (3 levels) | 50ms | 25ms | 50% |
| Federation query | 100ms | 60ms | 40% |

---

## 10. Summary & Checklist

### 10.1 Compilation Phases Summary

| Phase | Input | Output | Key Tasks |
|-------|-------|--------|-----------|
| 1 | Schema | SchemaAST | Parse, validate syntax |
| 2 | SchemaAST | ResolvedSchema | Resolve types, check circular refs |
| 3 | ResolvedSchema | BoundSchema | Bind fields to DB, apply auth |
| 4 | BoundSchema | FederationSchema | Federation validation, entity resolution |
| 5 | FederationSchema | OperationPlan | Compile queries/mutations/subscriptions |
| 6 | OperationPlan | CompiledSchema | Generate SQL, optimize, compress |

### 10.2 Compilation Validation Checklist

- [ ] Phase 1: All types parse without syntax errors
- [ ] Phase 2: All type references resolve successfully
- [ ] Phase 3: All fields bind to database columns
- [ ] Phase 3: All authorization rules compile
- [ ] Phase 4: All federation contracts validate
- [ ] Phase 5: All operations compile to valid SQL
- [ ] Phase 6: Generated SQL executes successfully
- [ ] All error codes are valid and documented
- [ ] Compiled schema is valid JSON
- [ ] Compiled schema fits size budget (<5MB)

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL's six-phase compilation process ensures deterministic, optimized, and secure execution plans. Each phase adds semantic understanding, transforms data structures, and validates constraints. By the time execution begins, the runtime knows exactly what to do.
