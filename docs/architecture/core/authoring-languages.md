# Multiple Authoring Languages Architecture

**Version:** 1.0
**Status:** Complete
**Audience:** SDK developers, schema authors, framework integrators
**Date:** January 11, 2026

---

## Overview

FraiseQL supports **schema authoring in multiple languages** through a unified intermediate representation (IR):

> **Schemas authored in any language compile to the same CompiledSchema artifact, enabling language-agnostic execution and true polyglot development.**

This is not a "language binding" pattern. This is **language-agnostic compilation**.

---

## 1. The Core Principle

### Single Intermediate Representation: The AuthoringIR

All authoring languages (Python, TypeScript, YAML, CLI) produce the same **AuthoringIR** before compilation:

```
Python Schema (decorators)     ─┐
TypeScript Schema (interfaces) ─┤
YAML Schema (structured)       ─┼─→ AuthoringIR ─→ CompiledSchema ─→ Rust Runtime
GraphQL SDL                    ─┤
CLI Schema (commands)          ─┘
```

**Key principle:** The authoring language is **just syntax**. The semantics are expressed in the AuthoringIR.

---

## 2. AuthoringIR Definition

The AuthoringIR is a language-agnostic representation of schema structure:

```python
class AuthoringIR:
    """Language-agnostic schema intermediate representation."""

    name: str                           # Schema name
    description: str                    # Schema description
    database_target: str                # PostgreSQL, MySQL, SQLite, SqlServer

    types: dict[str, TypeDefinition]    # name -> definition
    queries: dict[str, OperationDef]    # name -> operation
    mutations: dict[str, OperationDef]  # name -> operation

    bindings: dict[str, BindingDef]     # operation_name -> database binding
    auth_rules: list[AuthRule]          # authorization metadata
    scalar_types: dict[str, ScalarDef]  # custom scalar definitions

class TypeDefinition:
    """Represents a GraphQL type (object, input, enum, scalar)."""
    name: str
    kind: TypeKind  # OBJECT | INPUT | ENUM | SCALAR | INTERFACE | UNION
    description: str
    fields: dict[str, FieldDef]         # field_name -> definition
    directives: list[DirectiveDef]      # Applied directives

class FieldDef:
    """Represents a field on a GraphQL type."""
    name: str
    type_ref: str                       # Type name (resolved later)
    is_nullable: bool
    is_list: bool
    description: str
    directives: list[DirectiveDef]

class OperationDef:
    """Represents a query or mutation."""
    name: str
    description: str
    return_type: str                    # Type name
    arguments: dict[str, ArgumentDef]   # arg_name -> definition
    directives: list[DirectiveDef]

class BindingDef:
    """Defines how an operation binds to a database resource."""
    operation_name: str
    binding_type: str                   # VIEW | PROCEDURE | FUNCTION
    view_name: str                      # For VIEW bindings
    procedure_name: str                 # For PROCEDURE bindings
    where_column: str | None            # For single-entity queries
    input_mapping: dict[str, str]       # GraphQL arg -> DB param
    output_mapping: dict[str, str]      # DB response -> GraphQL field

class AuthRule:
    """Authorization metadata."""
    target: str                         # Query/mutation name or field
    requires_auth: bool
    requires_roles: list[str]
    requires_claims: list[str]
```

This is the **semantic bridge** between any authoring language and the compiler.

---

## 3. Supported Authoring Languages

### Language 1: Python (Priority 1)

**File:** `fraiseql.py` (SDK)

**Approach:** Decorators + type hints

```python
from fraiseql import schema, type, query, mutation, ID, String, auth

@schema.configure(
    name="blog-api",
    database_target="postgresql",
    version="1.0.0"
)
class BlogSchema:
    pass

@schema.type
class User:
    """A user account."""
    id: ID
    email: str
    name: str
    posts: list["Post"]

@schema.type
class Post:
    id: ID
    title: str
    content: str
    author: User

@schema.query
def users(where: "UserWhereInput" = None):
    """Fetch all users with optional filtering."""
    pass

@schema.mutation
@auth.requires_role("admin")
def create_user(email: str, name: str) -> User:
    """Create a new user (admin only)."""
    pass

# Bindings
schema.bind("users", "view", "v_user")
schema.bind("create_user", "procedure", "fn_create_user")

# Compilation
if __name__ == "__main__":
    compiled = schema.compile()
```

**Parser:** Python AST inspection + type hints
**Output:** AuthoringIR

**Advantages:**
- ✅ Native Python ecosystem
- ✅ IDE autocomplete
- ✅ Type safety via mypy
- ✅ Familiar to Python developers

---

### Language 2: TypeScript (Priority 1)

**File:** `fraiseql.ts` (SDK)

**Approach:** Interfaces + decorators (via decorators proposal)

```typescript
import {
  schema,
  type,
  query,
  mutation,
  ID,
  String,
  auth,
  Database
} from "fraiseql";

@schema.configure({
  name: "blog-api",
  database_target: "postgresql",
  version: "1.0.0"
})
export class BlogSchema {}

@schema.type()
export interface User {
  /** A user account */
  id: ID;
  email: string;
  name: string;
  posts: Post[];
}

@schema.type()
export interface Post {
  id: ID;
  title: string;
  content: string;
  author: User;
}

@schema.query()
export function users(where?: UserWhereInput): User[] {
  /** Fetch all users with optional filtering */
}

@schema.mutation()
@auth.requires_role("admin")
export function createUser(email: string, name: string): User {
  /** Create a new user (admin only) */
}

// Bindings
schema.bind("users", "view", "v_user");
schema.bind("createUser", "procedure", "fn_create_user");

// Compilation
const compiled = await schema.compile();
```

**Parser:** TypeScript AST inspection + decorator metadata
**Output:** AuthoringIR

**Advantages:**
- ✅ Native TypeScript ecosystem
- ✅ Type safety at authoring time
- ✅ Familiar to Node.js developers
- ✅ Works in Node.js and browser contexts

---

### Language 3: YAML (Priority 2)

**File:** `schema.yaml` (configuration language)

**Approach:** Structured data with clear relationships

```yaml
schema:
  name: blog-api
  version: 1.0.0
  database_target: postgresql

types:
  User:
    kind: object
    description: A user account
    fields:
      id:
        type: ID
        required: true
      email:
        type: String
        required: true
      name:
        type: String
        required: true
      posts:
        type: Post
        list: true

  Post:
    kind: object
    fields:
      id:
        type: ID
      title:
        type: String
      content:
        type: String
      author:
        type: User

queries:
  users:
    description: Fetch all users with optional filtering
    return_type: User
    list: true
    arguments:
      where:
        type: UserWhereInput
        required: false

mutations:
  createUser:
    description: Create a new user (admin only)
    return_type: User
    arguments:
      email:
        type: String
        required: true
      name:
        type: String
        required: true
    auth:
      requires_roles:
        - admin

bindings:
  users:
    type: view
    view: v_user
  createUser:
    type: procedure
    procedure: fn_create_user
```

**Parser:** YAML → structured dict → AuthoringIR
**Output:** AuthoringIR

**Advantages:**
- ✅ Language-agnostic (not tied to Python or TypeScript)
- ✅ Easy for humans to read and edit
- ✅ Can be generated by other tools
- ✅ Version-control friendly (clear diffs)

---

### Language 4: GraphQL SDL (Priority 2)

**File:** `schema.graphql` (GraphQL Schema Definition Language)

**Approach:** Standard GraphQL syntax with directives for bindings

```graphql
"""Blog API Schema"""
schema {
  query: Query
  mutation: Mutation
}

"""A user account"""
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
  """Fetch all users with optional filtering"""
  users(where: UserWhereInput): [User!]!
}

type Mutation {
  """Create a new user (admin only)"""
  createUser(email: String!, name: String!): User!
    @auth(requires_role: "admin")
}

input UserWhereInput {
  id: IDFilter
  email: StringFilter
}

directive @bind(type: String!, resource: String!) on FIELD_DEFINITION
directive @auth(requires_role: String) on FIELD_DEFINITION

# Binding directives (parsed from SDL)
extend type Query {
  users: [User!]! @bind(type: "view", resource: "v_user")
}

extend type Mutation {
  createUser(email: String!, name: String!): User!
    @bind(type: "procedure", resource: "fn_create_user")
}
```

**Parser:** GraphQL parser → AST → AuthoringIR
**Output:** AuthoringIR

**Advantages:**
- ✅ Familiar to GraphQL developers
- ✅ Can use standard GraphQL tooling
- ✅ Portable between tools
- ✅ Easy to visualize with GraphQL explorers

---

### Language 5: CLI (Priority 3)

**Commands:** Interactive CLI for schema generation

```bash
# Initialize schema
fraiseql init --name blog-api --database postgresql

# Add type
fraiseql add:type User \
  --field id:ID \
  --field email:String \
  --field name:String

# Add query
fraiseql add:query users \
  --returns User \
  --list \
  --binding view:v_user

# Add mutation
fraiseql add:mutation createUser \
  --argument email:String \
  --argument name:String \
  --returns User \
  --binding procedure:fn_create_user \
  --auth admin

# Compile
fraiseql compile
```

**Parser:** CLI arguments → structured data → AuthoringIR
**Output:** AuthoringIR

**Advantages:**
- ✅ No file required (schema in database/API)
- ✅ Scriptable for automation
- ✅ Interactive for learning
- ✅ Good for small schemas

---

## 4. Compilation Pipeline: Language-Agnostic

```
Python Schema (decorators)
TypeScript Schema (interfaces)
YAML Schema (structured)
GraphQL SDL
CLI Commands
    ↓
    ├─→ Python Parser → AuthoringIR
    ├─→ TypeScript Parser → AuthoringIR
    ├─→ YAML Parser → AuthoringIR
    ├─→ GraphQL Parser → AuthoringIR
    └─→ CLI Parser → AuthoringIR
        ↓
        All converge to same AuthoringIR
        ↓
Compilation Pipeline (Phases 1-6)
    ├─ Phase 1: Schema Parsing (from AuthoringIR)
    ├─ Phase 2: Database Introspection
    ├─ Phase 3: Type Binding
    ├─ Phase 4: WHERE Type Generation (database-target-aware)
    ├─ Phase 5: Validation
    └─ Phase 6: Artifact Emission
        ↓
        CompiledSchema.json
        schema.graphql
        validation-report.txt
        ↓
        Rust Runtime Execution
```

**Key insight:** The compiler doesn't care which language produced the AuthoringIR. It's all the same from Phase 1 onward.

---

## 5. Real-World Example: Same Schema, Multiple Languages

### Python Version

```python
from fraiseql import schema, type, query, ID, String

@schema.configure(name="shop", database_target="postgresql")
class ShopSchema:
    pass

@schema.type
class Product:
    id: ID
    name: str
    price: float

@schema.query
def products(where: "ProductWhereInput" = None):
    pass

schema.bind("products", "view", "v_product")
```

### TypeScript Version

```typescript
import { schema, type, query, ID, String } from "fraiseql";

@schema.configure({ name: "shop", database_target: "postgresql" })
export class ShopSchema {}

@schema.type()
export interface Product {
  id: ID;
  name: string;
  price: number;
}

@schema.query()
export function products(where?: ProductWhereInput): Product[] {}

schema.bind("products", "view", "v_product");
```

### YAML Version

```yaml
schema:
  name: shop
  database_target: postgresql

types:
  Product:
    kind: object
    fields:
      id: { type: ID }
      name: { type: String }
      price: { type: Float }

queries:
  products:
    return_type: Product
    list: true

bindings:
  products:
    type: view
    view: v_product
```

### GraphQL SDL Version

```graphql
type Product {
  id: ID!
  name: String!
  price: Float!
}

type Query {
  products: [Product!]!
    @bind(type: "view", resource: "v_product")
}
```

### CLI Version

```bash
fraiseql init --name shop --database postgresql
fraiseql add:type Product --field id:ID --field name:String --field price:Float
fraiseql add:query products --returns Product --list --binding view:v_product
fraiseql compile
```

**Result:** All five approaches produce **identical CompiledSchema.json**

The generated GraphQL is identical, the SQL lowering is identical, the execution is identical.

---

## 6. Real-World Usage Patterns

### Pattern 1: Canonical Language + Ecosystem Projections

```
Canonical source-of-truth schema (one language)
    ↓
Python version  (for Python developers / Django teams)
TypeScript version  (for TypeScript/Node.js teams)
YAML version  (for ops/DevOps)
GraphQL SDL  (for tools/explorers)
    ↓
All compile to same CompiledSchema
    ↓
Single Rust runtime
```

**Key:** One team maintains the canonical schema. Other languages are **projections** (generated or hand-maintained equivalents) for different ecosystems.

This avoids the maintenance nightmare of truly multi-language schemas.

### Pattern 2: Generated + Configuration

```
Base schema generated from database introspection
    ↓
YAML overrides applied (bindings, auth rules)
    ↓
Produces canonical CompiledSchema
```

**Key:** Machine-generated base + human-maintained config in YAML.

### Pattern 3: Language-Specific Organization

```
Core GraphQL Schema Layer (canonical truth)
    ↓
┌───────────────────────────┬──────────────────┐
│                           │                  │
Python SDK wrapper          TypeScript SDK     YAML config
(Python decorators)         (interfaces)       (deployment)

All expose same underlying CompiledSchema
```

**Key:** Different organizations might prefer different authoring languages, but they all reference the same underlying schema.

### Pattern 4: Gradual Migration

```
v1: All schemas in Python
    ↓
v2: Evaluate TypeScript for new schemas
    ↓
v3: Decide on canonical language (e.g., GraphQL SDL)
    ↓
v4: One-time migration of existing schemas to canonical language
    ↓
v5: Use canonical language going forward
```

**Key:** Not continuous polyglotism, but ability to migrate at organization boundaries without runtime changes.

---

## 7. AuthoringIR Design Benefits

### Benefit 1: Easy to Add Languages

To support a new language:

1. Write parser: `Language → AuthoringIR`
2. Validate AuthoringIR structure
3. Done

The rest of the compiler pipeline works automatically.

**Example:** Add Kotlin support
```kotlin
class KotlinAuthoringIRBuilder {
    fun fromKotlinFile(source: String): AuthoringIR {
        // Parse Kotlin AST → AuthoringIR
    }
}
```

### Benefit 2: Easy to Generate Schemas

Tools can generate schemas by producing AuthoringIR:

```python
def generate_from_database(db_connection) -> AuthoringIR:
    """Introspect database → AuthoringIR"""
    ir = AuthoringIR()
    for table in db_connection.list_tables():
        ir.types[table.name] = table_to_type_definition(table)
    return ir
```

Then compile normally. Or save to YAML/GraphQL/etc.

### Benefit 3: Easy to Transform

Transform schemas before compilation:

```python
def add_audit_fields(ir: AuthoringIR) -> AuthoringIR:
    """Add created_at, updated_at to all types"""
    for type_def in ir.types.values():
        type_def.fields["created_at"] = FieldDef("created_at", "DateTime")
        type_def.fields["updated_at"] = FieldDef("updated_at", "DateTime")
    return ir

# Usage
ir = parse_python_schema(schema_file)
ir = add_audit_fields(ir)
compiled = compile(ir)
```

### Benefit 4: Easy to Validate

Validation happens on AuthoringIR (language-agnostic):

```python
def validate_authoring_ir(ir: AuthoringIR) -> list[ValidationError]:
    """Check IR invariants (same for all languages)"""
    errors = []

    # Check type closure
    for type_def in ir.types.values():
        for field_def in type_def.fields.values():
            if field_def.type_ref not in ir.types:
                errors.append(f"Type {field_def.type_ref} not defined")

    # Check bindings reference existing operations
    for op_name in ir.bindings:
        if op_name not in ir.queries and op_name not in ir.mutations:
            errors.append(f"Binding references unknown operation {op_name}")

    return errors
```

---

## 8. Language-Specific Features

### Features Unique to Each Language

#### Python: Native Decorators
```python
@schema.type
@cache(ttl=3600)
@auth.requires_role("admin")
class User:
    pass
```

#### TypeScript: Interface Inheritance
```typescript
@schema.type()
export interface User extends Identifiable {
  email: string;
}
```

#### YAML: Clear Documentation
```yaml
types:
  User:
    description: A user account
    # Easy to read, easy to version control
```

#### GraphQL SDL: Directive Composition
```graphql
type User @cache(ttl: 3600) @auth(requires_role: "admin") {
  id: ID!
}
```

#### CLI: Interactive Learning
```bash
$ fraiseql add:type --help
$ fraiseql add:mutation --interactive
```

All translate to the same AuthoringIR semantics.

---

## 9. Tooling & Ecosystem

### IDE Support

- **Python:** mypy type checking, autocomplete in PyCharm/VS Code
- **TypeScript:** Full TypeScript support, strict mode
- **YAML:** Schema validation, linting
- **GraphQL SDL:** GraphQL extension, GraphQL playground
- **CLI:** Tab completion, help text

### Version Control

- **Python:** `.py` files, native diff
- **TypeScript:** `.ts` files, native diff
- **YAML:** `.yaml` files, easy to review diffs
- **GraphQL SDL:** `.graphql` files, familiar format
- **CLI:** Commit compiled artifacts, not CLI history

### CI/CD Integration

```yaml
# ci/compile.yml
jobs:
  compile:
    - If using Python: pip install fraiseql && fraiseql compile schema.py
    - If using TypeScript: npm install && fraiseql compile schema.ts
    - If using YAML: fraiseql compile schema.yaml
    - If using GraphQL: fraiseql compile schema.graphql

    # All produce identical CompiledSchema.json
    - git diff build/CompiledSchema.json
```

---

## 10. Backward Compatibility & Evolution

### Adding a New Language

When adding Kotlin support:

1. **Write Kotlin parser**
   ```kotlin
   fun parseKotlinSchema(file: File): AuthoringIR { }
   ```

2. **Register with compiler**
   ```rust
   match file_extension {
       "py" => parse_python(content),
       "ts" => parse_typescript(content),
       "yaml" => parse_yaml(content),
       "graphql" => parse_graphql(content),
       "kt" => parse_kotlin(content),  // New!
   }
   ```

3. **No breaking changes**
   - Existing schemas continue to work
   - CompiledSchema format unchanged
   - Rust runtime unchanged

### Evolving AuthoringIR

If adding new feature (e.g., directives):

1. **Extend AuthoringIR**
   ```python
   class FieldDef:
       directives: list[DirectiveDef]  # New field
   ```

2. **Update all parsers**
   - Python: Parse `@cache` decorator
   - TypeScript: Parse `@cache()` decorator
   - YAML: Parse `directives` field
   - GraphQL: Parse `@cache` directive
   - CLI: Add `--directive` flag

3. **Backward compatible**
   - Old schemas without directives still compile
   - New schemas with directives compile
   - No breaking changes

---

## 11. Success Criteria

✅ **Multiple authoring languages achieved when:**

1. **Each language has ergonomic syntax** for its ecosystem (Python decorators, TypeScript interfaces, YAML, GraphQL SDL)
2. **All languages compile to identical AuthoringIR** (language doesn't matter to compiler)
3. **All AuthoringIRs produce identical CompiledSchema** (canonical single source, regardless of input language)
4. **Easy to translate between languages** (tooling to convert Python→YAML, TypeScript→GraphQL SDL, etc.)
5. **Adding new language requires only a parser** (one module per language)
6. **Zero runtime differences** based on authoring language (no "Python mode" vs "TypeScript mode")
7. **Support gradual migration** (can switch canonical language without breaking execution)
8. **IDE support per language** (each language gets appropriate tooling)

---

## 12. Related Specifications

- **Authoring Contract** (`docs/specs/authoring-contract.md`) — What can be authored
- **Compilation Pipeline** (`docs/architecture/core/compilation-pipeline.md`) — How compilation works
- **Database Targeting** (`docs/architecture/database/database-targeting.md`) — How database target influences compilation
- **Execution Model** (`docs/architecture/core/execution-model.md`) — How CompiledSchema is executed

---

## 13. Roadmap

### Phase 1 (Launch)
- ✅ Python SDK with decorators
- ✅ YAML parser
- ✅ AuthoringIR definition

### Phase 2 (Expand)
- ⏳ TypeScript SDK
- ⏳ GraphQL SDL parser
- ⏳ Enhanced tooling

### Phase 3 (Scale)
- ⏳ CLI schema generator
- ⏳ Kotlin support
- ⏳ Java support

### Phase 4+ (Ecosystem)
- ⏳ Go support
- ⏳ Rust support
- ⏳ Generated schema tools

---

*End of Multiple Authoring Languages Architecture*
