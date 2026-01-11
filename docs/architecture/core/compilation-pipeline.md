# Compilation Pipeline Architecture

**Version:** 1.0
**Status:** Draft
**Audience:** Compiler developers, schema authors, infrastructure teams

---

## 1. Overview

The **compilation pipeline** transforms an authoring-layer schema (Python, TypeScript, YAML, etc.) into a **CompiledSchema** — a database-agnostic JSON artifact that the Rust runtime executes.

**Key phases:**
1. **Schema Parsing** — Parse authoring language syntax (queries, mutations, **subscriptions**)
2. **Database Introspection** — Discover views, columns, procedures, constraints, CDC capabilities
3. **Type Binding** — Map GraphQL types to database views/JSONB paths
4. **WHERE Type Generation** — Introspect columns; generate WHERE types based on database capabilities
5. **Subscription Compilation** — Compile subscription filters, authorization, event type mappings (v2.0+)
6. **Compilation & Validation** — Validate closure, consistency, authorization, and database capability coverage
7. **Artifact Emission** — Output CompiledSchema JSON + GraphQL SDL + subscription dispatch table

---

## 2. Phase 1: Schema Parsing

### 2.1 Input Formats

The compiler accepts schemas in multiple authoring languages:

| Format | Language | Parser | Output |
|--------|----------|--------|--------|
| **Python** | Python 3.10+ | AST inspection + type hints | CompiledSchema |
| **YAML** | YAML 1.2 | Standard YAML parser | CompiledSchema |
| **GraphQL SDL** | GraphQL | gql-core parser | CompiledSchema |
| **TypeScript** | TypeScript | ts-morph / tsc | CompiledSchema |

All converge to the same **intermediate representation (IR)** before proceeding to phase 2.

### 2.2 Intermediate Representation (IR)

```python
class SchemaIR:
    """Language-agnostic intermediate representation."""
    name: str
    description: str
    database_target: str

    types: dict[str, TypeDef]              # name -> TypeDef
    queries: dict[str, QueryDef]           # name -> QueryDef
    mutations: dict[str, MutationDef]      # name -> MutationDef
    subscriptions: dict[str, SubscriptionDef]  # name -> SubscriptionDef (v2.0+)
    auth_context: AuthContextDef
    bindings: dict[str, BindingDef]        # query/mutation/subscription name -> BindingDef
    auth_rules: list[AuthRule]
```

### 2.3 Type Definition IR

```python
class TypeDef:
    """Represents a GraphQL type."""
    name: str                              # PascalCase
    kind: "OBJECT" | "INPUT" | "ENUM" | "SCALAR"
    description: str
    fields: dict[str, FieldDef]           # field_name -> FieldDef
    directives: list[str]                 # @auth.requires_role, etc.

class FieldDef:
    """Represents a field on a GraphQL type."""
    name: str                              # camelCase
    graphql_type: str                      # e.g., "String!", "[User!]!"
    description: str
    directives: list[str]
```

### 2.4 Binding Definition IR

```python
class BindingDef:
    """Defines how a query/mutation/subscription binds to a database resource."""
    query_or_mutation_or_subscription_name: str
    binding_type: "VIEW" | "PROCEDURE" | "FUNCTION"

    # For VIEW bindings (queries, mutations, subscriptions)
    view_name: str
    where_column: str | None               # If single-entity query
    data_column: str = "data"

    # For PROCEDURE/FUNCTION bindings (mutations)
    procedure_name: str
    input_mapping: dict[str, str]         # GraphQL arg -> param name
    output_mapping: dict[str, str]        # return field -> GraphQL field

    # For SUBSCRIPTION bindings (v2.0+)
    subscription_filters: list[WhereClause]  # Compile-time filters
    event_types: list[str]                # "INSERT" | "UPDATE" | "DELETE"
    authorization: AuthRule               # Enforced at event capture time
```

### 2.5 Subscription Definition IR (v2.0+)

```python
class SubscriptionDef:
    """Defines a GraphQL subscription."""
    name: str
    description: str
    fields: dict[str, FieldDef]           # field_name -> FieldDef
    input_type: str                        # Input type for variables
    return_type: str                       # Return type (payload)

class SubscriptionBindingDef:
    """Binds subscription to database event stream."""
    subscription_name: str
    event_source: "LISTEN_NOTIFY" | "CDC"  # Database event mechanism
    entity_types: list[str]               # Which entities trigger events
    where_filters: list[WhereClause]      # Compile-time filters for events
    authorization: AuthRule               # Row-level filtering per subscriber
```

---

## 3. Phase 2: Database Introspection

### 3.1 Purpose

Introspection discovers what the database provides:
- View columns and JSONB paths
- Stored procedure signatures
- Column indexing and types
- Foreign key constraints
- Database capabilities (operators, functions)

### 3.2 Introspection Scope

The compiler **introspects only declared bindings**, not the entire database:

```python
# Compiler will introspect:
# - v_user (because binding references it)
# - fn_create_user (because mutation binding references it)

# Compiler will NOT introspect:
# - Internal tables like tb_posts_audit
# - Unused views
# - Helper functions
```

### 3.3 View Column Discovery

For each bound view, introspect:

```sql
-- For v_user, discover:
SELECT
    column_name,
    data_type,
    is_nullable,
    column_default,
    ordinal_position
FROM information_schema.columns
WHERE table_schema = 'public'
  AND table_name = 'v_user'
ORDER BY ordinal_position;
```

**Discovered columns:**
```
id          → UUID
email       → TEXT (nullable: false)
name        → TEXT
posts       → JSONB (array of posts)
created_at  → TIMESTAMP
items__product__category_id  → UUID (nullable: true)
```

### 3.4 JSONB Path Discovery

For JSONB columns, introspect paths:

```python
# For posts (JSONB array), discover available paths:
# posts[0].id
# posts[0].title
# posts[0].author.name
# etc.

# These become available for filtering in WHERE types
```

### 3.5 Stored Procedure Discovery

For each bound procedure, introspect signature:

```sql
-- For fn_create_user, discover:
SELECT
    proname,
    pronargs,
    proargtypes,
    proargnames,
    proargmodes,
    prorettype
FROM pg_proc
WHERE proname = 'fn_create_user';
```

**Discovered signature:**
```
fn_create_user(
    email_param: TEXT,
    name_param: TEXT
) → JSON
```

### 3.6 Column Indexing Analysis

Analyze indexing for performance warnings:

```python
# Introspect: Is column indexed?
# If not indexed and used in WHERE types, emit warning:
# ⚠ Column 'v_user.email' used in WHERE but not indexed
```

---

## 4. Phase 3: Type Binding

### 4.1 Mapping GraphQL Types to Database Views

For each GraphQL type, bind to a database view:

```python
# GraphQL
@fraiseql.type
class User:
    id: ID
    email: str
    posts: list[Post]

# Binding
schema.bind("User", view="v_user", data_column="data")

# Compiler verifies:
# ✓ View v_user exists
# ✓ Column 'data' (JSONB) exists in v_user
# ✓ All fields (id, email, posts) discoverable in view or JSONB
```

### 4.2 Field-to-Column Mapping

Each GraphQL field maps to either a **SQL column** or a **JSONB path**:

```python
User.id        → v_user.id (SQL column)
User.email     → v_user.email (SQL column)
User.posts     → v_user.data->>'posts' (JSONB path)
User.createdAt → v_user.created_at (SQL column)
```

**Discovery algorithm:**
```
for each field in GraphQL type:
    1. Check if column exists with same name
    2. If not, check JSONB paths in data column
    3. If found, record mapping
    4. If not found, error: field not discoverable
```

### 4.3 Foreign Key Inference

The compiler infers foreign key relationships from type references:

```python
@fraiseql.type
class User:
    id: ID
    posts: list[Post]  # Inferred FK: User → Post

@fraiseql.type
class Post:
    id: ID
    author: User       # Inferred FK: Post → User
```

**Verification:**
- If view declares `user_id` column, FK is explicit
- If JSONB nesting available, FK is implicit
- Compiler warns if both explicit and implicit conflict

---

## 5. Phase 4: WHERE Type Generation

### 5.1 Purpose

Automatically generate WHERE input types based on:
- Available database columns
- Column types
- **Database target** (from compiler configuration)
- Database capability manifest

This ensures **only filterable columns are exposed** and **operators match the target database's capabilities**.

**This is the core mechanism for multi-database support:** The `database_target` configuration drives which operators are available in the generated GraphQL schema.

See **`docs/architecture/database/database-targeting.md`** for comprehensive explanation of compile-time schema specialization.

### 5.2 Column Introspection for WHERE Generation

```python
# For User type bound to v_user, introspect columns:

# SQL columns (always filterable):
id          → IDFilter
email       → StringFilter
name        → StringFilter
created_at  → DateTimeFilter

# JSONB paths (if data column exists):
posts[0].id        → IDFilter
posts[0].title     → StringFilter
posts[0].author.id → IDFilter

# Flattened foreign key columns (if exist):
items__product__category_id  → IDFilter  # For efficient filtering
```

### 5.3 Capability Manifest Application

The database capability manifest defines which operators are available:

```json
{
  "capabilities": {
    "postgresql": {
      "String": [
        { "operator": "_eq", "sql": "=" },
        { "operator": "_neq", "sql": "!=" },
        { "operator": "_like", "sql": "LIKE" },
        { "operator": "_ilike", "sql": "ILIKE" },
        { "operator": "_regex", "sql": "~" }
      ],
      "ID": [
        { "operator": "_eq", "sql": "=" },
        { "operator": "_neq", "sql": "!=" },
        { "operator": "_in", "sql": "IN" }
      ]
    }
  }
}
```

### 5.4 WHERE Type Generation Algorithm

```python
def generate_where_type(type_name: str, bound_view: str, capabilities: dict):
    """Generate WHERE input type for a GraphQL type."""

    # Introspect columns
    columns = introspect_view(bound_view)

    # Build WHERE fields
    where_fields = {}
    for col_name, col_type in columns.items():
        # Look up operators for this column type
        graphql_type = map_sql_to_graphql(col_type)
        operators = capabilities[database_target][graphql_type]

        # Create filter input type
        filter_type = create_filter_type(col_name, operators)
        where_fields[col_name] = filter_type

    # Add logical operators
    where_fields["_and"] = f"[{type_name}WhereInput!]"
    where_fields["_or"] = f"[{type_name}WhereInput!]"
    where_fields["_not"] = f"{type_name}WhereInput"

    # Create WHERE input type
    return InputType(
        name=f"{type_name}WhereInput",
        fields=where_fields
    )
```

### 5.5 Generated WHERE Type Example

For `User` bound to `v_user` with PostgreSQL capabilities:

```graphql
input UserWhereInput {
  id: IDFilter
  email: StringFilter
  name: StringFilter
  createdAt: DateTimeFilter
  posts: PostsWhereInput              # Nested, from JSONB
  items__product__categoryId: IDFilter # Flattened FK for performance
  _and: [UserWhereInput!]
  _or: [UserWhereInput!]
  _not: UserWhereInput
}

input StringFilter {
  _eq: String
  _neq: String
  _like: String
  _ilike: String
  _regex: String
}

input IDFilter {
  _eq: ID
  _neq: ID
  _in: [ID!]
}
```

### 5.6 Capability-Driven Operator Inclusion

If targeting SQLite (no regex support):

```json
{
  "capabilities": {
    "sqlite": {
      "String": [
        { "operator": "_eq", "sql": "=" },
        { "operator": "_neq", "sql": "!=" },
        { "operator": "_like", "sql": "LIKE" }
        // No _regex or _ilike
      ]
    }
  }
}
```

The generated `StringFilter` for SQLite would **omit** `_regex` and `_ilike`.

### 5.7 Complete WHERE Operator Reference

FraiseQL supports **150+ WHERE clause operators** across 15 categories, automatically applied based on column types and database capabilities:

**Operator Categories:**
1. **Basic Comparison** (all types)
   - `_eq`, `_neq`, `_gt`, `_gte`, `_lt`, `_lte`, `_in`, `_nin`

2. **String/Text Operators**
   - `_like`, `_ilike` (case-insensitive), `_regex`, `_iregex`, `_starts_with`, `_istarts_with`, `_ends_with`, `_iends_with`, `_contains`, `_icontains`

3. **Array Operators**
   - `_array_contains`, `_array_contained_by`, `_array_overlaps`, `_array_length`, `_array_index`

4. **JSONB Operators**
   - `_jsonb_contains`, `_jsonb_has_key`, `_jsonb_has_keys`, `_jsonb_keys`, `_jsonb_values`, `_jsonb_extract`

5. **Date/Time Operators**
   - `_before`, `_after`, `_between`, `_year`, `_month`, `_day`, `_quarter`, `_day_of_week`, `_hour`, `_minute`

6. **Network Operators** (IP/CIDR)
   - `_cidr_contains`, `_cidr_contained_by`, `_subnet_of`, `_overlaps`, `_netmask`

7. **Geographic/Spatial Operators**
   - `_distance_lt`, `_distance_lte`, `_distance_gt`, `_distance_gte`, `_within_distance`, `_overlaps`, `_contains`, `_inside`

8. **Vector Distance Operators** (pgvector)
   - `_cosine_distance_lt`, `_l2_distance_lt`, `_l1_distance_lt`, `_hamming_distance_lt`, `_jaccard_distance_lt`, `_inner_product`

9. **LTree (Hierarchical) Operators**
   - `_ancestor`, `_descendant`, `_matches`, `_is_ancestor`, `_is_descendant`, `_first`, `_last`

10. **Full-Text Search Operators**
    - `_tsquery`, `_tsvector_match`, `_plainto_tsquery`

11. **Type-Specific Numeric**
    - `_is_even`, `_is_odd`, `_is_prime`, `_divisible_by`

12. **UUID Operators**
    - `_nil` (check if nil), `_version` (match UUID version)

13. **Enum Operators**
    - `_eq`, `_neq`, `_in` (plus all logical operators)

14. **Boolean Operators**
    - `_is_true`, `_is_false`, `_is_null`

15. **Logical Operators** (all types)
    - `_and`, `_or`, `_not`

**Complete Reference:**
See [`docs/reference/where-operators.md`](../reference/where-operators.md) for:
- Full operator specifications by category
- SQL equivalents for each operator
- Performance characteristics and indexing recommendations
- Example queries for each operator
- Database compatibility matrix

**Example: Complex WHERE Using Multiple Operators**
```graphql
query {
  users(where: {
    # String operators
    email: { _ilike: "%@example.com" }
    name: { _contains: "John" }

    # Date operators
    createdAt: { _gte: "2025-01-01T00:00:00Z" }

    # Array operators
    tags: { _array_contains: ["vip"] }

    # JSONB operators
    metadata: { _jsonb_has_key: "verified" }

    # Vector operators (AI/RAG)
    embedding: { _cosine_distance_lt: 0.1 }

    # Logical operators
    _and: [
      { role: { _eq: ADMIN } }
      { isActive: { _is_true } }
    ]
  }) {
    id
    email
  }
}
```

**Capability Detection:**
At compilation time, the compiler:
1. Introspects column types from database view
2. Maps column types to available operators (from capability manifest)
3. Generates WHERE input types only for supported operators
4. Validates at runtime that only supported operators are used

This ensures **type safety** — you cannot use unsupported operators that the database cannot execute.

---

## 6. Phase 5: Subscription Compilation

Subscriptions compile through the same pipeline as queries and mutations, with identical field resolution, authorization, and type-safety rules.

### 6.1 Subscription Parsing

Parse `@fraiseql.subscription` declarations from all authoring languages:

```python
@fraiseql.subscription
class OrderCreated:
    """Event fired when order is created."""
    id: ID
    user_id: ID
    total: float
    created_at: Timestamp

    # Compile-time filter
    where: WhereClause = WhereClause(user_id=current_user_id)
```

### 7.2 Subscription Binding

Bind each subscription to database event streams:

```python
schema.bind("OrderCreated",
    event_source="LISTEN_NOTIFY",  # PostgreSQL
    entity_types=["Order"],
    operation_types=["INSERT"]
)
```

**Binding validates:**
- Event source is supported by database target
- Entity types exist in schema
- Operation types are valid (`INSERT`, `UPDATE`, `DELETE`)
- Authorization rules are enforceable

### 6.3 Filter Compilation

Subscription WHERE clauses compile to SQL predicates evaluated at event capture:

```sql
-- For: subscription OrderCreated where user_id = current_user_id
-- Compiles to:
SELECT * FROM tb_entity_change_log
WHERE entity_type = 'Order'
  AND operation = 'INSERT'
  AND (data->>'user_id')::UUID = $1  -- Runtime-bound user_id
```

### 6.4 Authorization Binding

Row-level authorization rules applied at event capture:

```python
# If schema defines:
@fraiseql.subscription
@fraiseql.auth(requires_role="user")
class OrderCreated:
    ...

# Compiler generates:
# - RLS policy enforcement at event capture
# - subscription_matchers per authenticated user
# - Auth context binding for runtime variable resolution
```

### 6.5 Subscription Dispatch Table Generation

The compiler generates a dispatch table for runtime:

```json
{
  "subscriptions": {
    "OrderCreated": {
      "entity_type": "Order",
      "operation_types": ["INSERT"],
      "event_source": "LISTEN_NOTIFY",
      "fields": ["id", "user_id", "total", "created_at"],
      "where_filters": [
        {"field": "user_id", "operator": "_eq", "value_type": "VARIABLE:current_user_id"}
      ],
      "authorization": {
        "requires_role": "user",
        "entity_acl": []
      }
    }
  }
}
```

### 6.6 Subscription Schema Validation

Validate subscription schema consistency:

**Subscription Return Type Check:**
- Subscription fields must be projectable from event data
- Same field resolution as queries (SQL columns + JSONB paths)

**Event Type Coverage:**
- Subscription fields must exist in all operation types
- Example: `ORDER_CANCELLED` event cannot project `ORDER_SHIPPED` fields

**Authorization Validity:**
- Rules must be decidable at compile time
- Dynamic fields disallowed (use only static roles and entity attributes)

---

## 7. Phase 6: Compilation & Validation

### 7.1 Validation Rules

The compiler enforces these invariants:

#### 7.1.1 Type Closure

Every referenced type must be defined:

```python
@fraiseql.query
def users() -> list[User]:  # User must be @fraiseql.type
    pass
```

**Error if violated:**
```
Error: Type closure violation
  Query 'users' returns 'list[User]'
  Type 'User' not defined
  → Define @fraiseql.type class User
```

#### 7.1.2 Binding Existence

Every type returned by queries/mutations must have a binding:

```python
@fraiseql.query
def users() -> list[User]:
    pass

# Must have:
schema.bind("users", view="v_user")
```

**Error if violated:**
```
Error: Missing binding
  Query 'users' returns 'list[User]'
  → schema.bind("users", "view", "v_user")
```

#### 7.1.3 View Existence

Bound views must exist in database:

```python
schema.bind("users", view="v_user_missing")  # ❌ Doesn't exist
```

**Error if violated:**
```
Error: View not found
  Binding 'users' references 'v_user_missing'
  → View does not exist in database
  → Check with: \dv v_user*
```

#### 7.1.4 Column Existence

All GraphQL fields must map to discoverable columns:

```python
@fraiseql.type
class User:
    id: ID
    undefined_field: str  # ❌ Not in v_user
```

**Error if violated:**
```
Error: Column not found
  Type 'User' field 'undefinedField'
  → Not found in view 'v_user' or JSONB paths
  → Check view schema: \d v_user
```

#### 7.1.5 Procedure Signature Match

Mutation input must match procedure parameters:

```python
@fraiseql.mutation
def create_user(email: str, name: str) -> User:
    pass

schema.bind("create_user", procedure="fn_create_user")

# Compiler checks:
# ✓ fn_create_user exists
# ✓ Has parameters matching: email, name
# ✓ Returns JSON
```

**Error if violated:**
```
Error: Procedure signature mismatch
  Mutation 'createUser' declares inputs: email, name
  Procedure 'fn_create_user' has params: email_param, name_param
  → Use input_mapping: {"email": "email_param", ...}
```

#### 7.1.6 Operator Support

All used filters must be in capability manifest:

```python
# If query uses _regex filter on SQLite:
where: {
  email: { _regex: "^test" }  # ❌ SQLite doesn't support regex
}
```

**Error if violated:**
```
Error: Operator not supported
  Filter uses '_regex' on 'email' field
  → Database 'sqlite' does not support regex operator
  → Use '_like' instead or target 'postgresql'
```

#### 7.1.7 Authorization Validity

Auth rules must reference valid auth context fields:

```python
@fraiseql.query
@auth.requires_claim("invalid_field")  # ❌ Not in AuthContext
def secure_query() -> User:
    pass
```

**Error if violated:**
```
Error: Auth context mismatch
  Rule requires claim 'invalidField'
  → Field not in AuthContext
  → Add field to @fraiseql.auth_context
```

### 7.2 Validation Output

After validation, compiler emits a **validation report**:

```
✓ Compilation successful (2026-01-11T15:35:00Z)

Schema: acme-api v2.1.0
Target: postgresql
Types: 12 | Queries: 8 | Mutations: 5

Validation Results:
  ✓ Type closure: 12/12 valid
  ✓ Bindings: 13/13 found
  ✓ Views: 8/8 exist
  ✓ Columns: 64/64 discoverable
  ✓ Procedures: 5/5 exist
  ✓ Operators: All supported
  ✓ Auth rules: 6/6 valid

Warnings:
  ⚠ Column 'v_user.email' used in WHERE but not indexed
  ⚠ View 'v_user_posts' is materialized; may become stale
  ⚠ Mutation 'deleteUser' performs soft delete; ensure cascades handled

Generated Artifacts:
  ✓ CompiledSchema.json (24 KB)
  ✓ schema.graphql (12 KB)
  ✓ capability-manifest.json (8 KB)
```

---

## 8. Phase 7: Artifact Emission

### 7.1 Output Artifacts

The compiler produces three files:

#### 7.1.1 CompiledSchema.json

The executable schema consumed by Rust runtime:

```json
{
  "version": "1.0",
  "metadata": { ... },
  "types": [ ... ],
  "queries": [ ... ],
  "mutations": [ ... ],
  "bindings": { ... },
  "authorization": { ... },
  "capabilities": { ... }
}
```

See: `specs/compiled-schema.md` for full structure.

#### 7.1.2 schema.graphql

Standard GraphQL SDL for client tooling:

```graphql
type User {
  id: ID!
  email: String!
  name: String!
  posts: [Post!]!
}

type Query {
  users(where: UserWhereInput, limit: Int = 100): [User!]!
  userByEmail(email: String!): User!
}
```

#### 7.1.3 capability-manifest.json

Database-specific capabilities applied during compilation:

```json
{
  "databaseTarget": "postgresql",
  "capabilities": {
    "String": {
      "operators": ["_eq", "_neq", "_like", "_ilike", "_regex"],
      "sortable": true,
      "indexable": true
    },
    "ID": {
      "operators": ["_eq", "_neq", "_in"],
      "sortable": false,
      "indexable": true
    }
  }
}
```

### 7.2 File Organization

```
my-api/
├── schema.py           # Authoring input
├── build/
│   ├── CompiledSchema.json
│   ├── schema.graphql
│   ├── capability-manifest.json
│   └── validation-report.txt
```

---

## 9. Compilation Commands

### 8.1 Python Compiler

```bash
# Compile schema
fraiseql compile schema.py \
  --database-url postgresql://... \
  --output build/

# Or with environment variable
export DATABASE_URL=postgresql://...
fraiseql compile schema.py
```

### 8.2 Validation-Only Mode

```bash
# Validate without database connection (use cached schema)
fraiseql compile schema.py --validate-only
```

### 8.3 Dry-Run Mode

```bash
# Show what would be compiled, no changes
fraiseql compile schema.py --dry-run
```

---

## 10. Compiler Error Handling

### 9.1 Error Categories

| Category | Severity | Action |
|----------|----------|--------|
| **Fatal** | Compilation stops | Invalid schema, missing binding, broken view |
| **Error** | Compilation stops | Type closure violation, operator unsupported |
| **Warning** | Compilation succeeds | Unindexed column, stale materialized view |
| **Info** | Compilation succeeds | Generated 5 WHERE types, 3 mutations compiled |

### 9.2 Error Messages

All errors include:

1. **Problem** — What went wrong
2. **Location** — File, line, field involved
3. **Suggestion** — How to fix

```
Error: View not found
  File: schema.py, line 35
  Binding 'users' references 'v_user_missing'

  Reason: View does not exist in target database 'postgresql'

  Suggestions:
    → Check view exists: \dv v_user*
    → Use correct view name in binding
    → Or create view in database
```

---

## 11. Compilation Performance

### 10.1 Typical Timings

| Phase | Duration | Notes |
|-------|----------|-------|
| Parsing | < 100ms | Python AST inspection |
| Introspection | 500ms - 2s | Database queries |
| Type binding | < 100ms | Local mappings |
| WHERE generation | 100-500ms | Per-type capability matching |
| Validation | < 200ms | Graph traversal |
| Artifact emission | < 100ms | JSON serialization |
| **Total** | **~1-3 seconds** | End-to-end with introspection |

### 10.2 Optimization: Cached Introspection

For fast iteration, cache database introspection:

```bash
# First compile: full introspection
fraiseql compile schema.py --database-url postgresql://...

# Subsequent compiles: use cache
fraiseql compile schema.py  # Skips DB queries if schema unchanged
```

---

## 12. Future Extensions

### 11.1 Multi-Database Compilation

```bash
# Compile for multiple targets
fraiseql compile schema.py \
  --targets postgresql,sqlite,mysql
```

Each target produces:
- `CompiledSchema-postgresql.json`
- `CompiledSchema-sqlite.json`
- `CompiledSchema-mysql.json`

### 11.2 Schema Versioning

```bash
# Generate migration metadata
fraiseql compile schema.py --version 2.1.0 --prev-version 2.0.0
```

Produces schema diff for documentation and client versioning.

---

*End of Compilation Pipeline Architecture*
