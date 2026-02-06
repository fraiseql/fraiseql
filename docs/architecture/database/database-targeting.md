<!-- Skip to main content -->
---

title: Database Targeting & Multi-Database Support
description: FraiseQL achieves **true multi-database support** through a single, unified mechanism:
keywords: ["design", "scalability", "performance", "patterns", "security"]
tags: ["documentation", "reference"]
---

# Database Targeting & Multi-Database Support

**Version:** 1.0
**Status:** Complete
**Audience:** Compiler developers, database architects, SDK users
**Date:** January 11, 2026

---

## Overview

FraiseQL achieves **true multi-database support** through a single, unified mechanism:

> **The database target specified at compile time drives the exact GraphQL schema that is generated, which includes database-specific WHERE types with only operators supported by that database.**

This is not a runtime abstraction layer. This is **compile-time schema specialization**.

---

## 1. The Core Principle

### Single Source of Truth: Database Target

```python
<!-- Code example in Python -->
# Compiler configuration
config = CompilerConfig(
    database_target="postgresql",  # This choice drives everything
    schema_path="schema.py",
    output_dir="build/"
)

compiled = compiler.compile(config)
```text
<!-- Code example in TEXT -->

This single configuration choice determines:

1. ✅ Which WHERE operators are available in GraphQL schema
2. ✅ How WHERE filters compile to SQL
3. ✅ Which scalar types are supported
4. ✅ Which JSONB operations are available
5. ✅ The exact CompiledSchema output

**There is no "generic" CompiledSchema.** Each CompiledSchema is database-specific.

---

## 2. Three Levels of Database-Specific Code

### Level 1: Capability Manifest (Static Definition)

The **capability manifest** declares what each database can do:

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "string": [
      { "operator": "_eq", "sql": "=" },
      { "operator": "_neq", "sql": "!=" },
      { "operator": "_like", "sql": "LIKE" },
      { "operator": "_ilike", "sql": "ILIKE" },
      { "operator": "_regex", "sql": "~" },
      { "operator": "_starts_with", "sql": "LIKE" },
      { "operator": "_icontains", "sql": "ILIKE" },
      { "operator": "_regex_flags", "sql": "~*" }
    ],
    "jsonb": [
      { "operator": "_jsonb_contains", "sql": "@>" },
      { "operator": "_jsonb_has_key", "sql": "?" },
      { "operator": "_jsonb_has_keys", "sql": "?&" },
      { "operator": "_jsonb_path", "sql": "#>>" }
    ],
    "network": [
      { "operator": "_cidr_contains", "sql": ">>" },
      { "operator": "_cidr_contained_by", "sql": "<<" },
      { "operator": "_subnet_of", "sql": "<<=" }
    ],
    "vector": [
      { "operator": "_cosine_distance_lt", "sql": "<#" },
      { "operator": "_l2_distance_lt", "sql": "<->" },
      { "operator": "_inner_product", "sql": "<#>" }
    ],
    "ltree": [
      { "operator": "_ancestor", "sql": "@>" },
      { "operator": "_descendant", "sql": "<@" },
      { "operator": "_matches", "sql": "~" }
    ]
  },

  "mysql": {
    "string": [
      { "operator": "_eq", "sql": "=" },
      { "operator": "_neq", "sql": "!=" },
      { "operator": "_like", "sql": "LIKE" }
    ],
    "json": [
      { "operator": "_json_contains", "sql": "JSON_CONTAINS" },
      { "operator": "_json_extract", "sql": "JSON_EXTRACT" }
    ]
  },

  "sqlite": {
    "string": [
      { "operator": "_eq", "sql": "=" },
      { "operator": "_neq", "sql": "!=" },
      { "operator": "_like", "sql": "LIKE" }
    ]
  },

  "sqlserver": {
    "string": [
      { "operator": "_eq", "sql": "=" },
      { "operator": "_neq", "sql": "!=" },
      { "operator": "_like", "sql": "LIKE" }
    ],
    "json": [
      { "operator": "_json_value", "sql": "JSON_VALUE" },
      { "operator": "_json_query", "sql": "JSON_QUERY" }
    ]
  }
}
```text
<!-- Code example in TEXT -->

**This manifest is:**

- ✅ Static (checked in)
- ✅ Declarative (not code)
- ✅ Extensible (add new databases easily)
- ✅ Source of truth for compiler

### Level 2: Compile-Time Schema Generation

When the compiler runs, it applies the capability manifest to generate database-specific WHERE types:

```python
<!-- Code example in Python -->
# Compilation Pipeline Phase 4: WHERE Type Generation

def generate_where_type(
    type_name: str,
    bound_view: str,
    database_target: str,  # ← Key parameter
    capabilities: dict
):
    """Generate WHERE input type for this type + database combination."""

    columns = introspect_view(bound_view)
    where_fields = {}

    for col_name, sql_type in columns.items():
        graphql_type = map_sql_to_graphql(sql_type)

        # THIS IS THE KEY DECISION POINT:
        # Look up operators for THIS database target
        operators = capabilities[database_target][graphql_type]

        # Only these operators are included in the generated GraphQL type
        filter_type = create_filter_type(col_name, operators)
        where_fields[col_name] = filter_type

    return InputType(name=f"{type_name}WhereInput", fields=where_fields)
```text
<!-- Code example in TEXT -->

**Result:** The generated GraphQL schema is different for each database target.

### Level 3: Runtime Backend Lowering (Execution)

The Rust runtime lowers SDL predicates to backend-specific SQL:

```rust
<!-- Code example in RUST -->
// Each database has a lowering module

mod postgresql {
    fn lower_filter(filter: &Filter, args: &mut Vec<Value>) -> String {
        match filter {
            Filter::Eq(Field::Json { column, path }, value) => {
                args.push(value);
                format!("{} #>> '{}' = ${}", column, path, args.len())
            },
            Filter::Regex(Field::Column(col), value) => {
                args.push(value);
                format!("{} ~ ${}", col, args.len())
            },
            // PostgreSQL-specific operators
            Filter::LTreeAncestor(Field::Column(col), value) => {
                args.push(value);
                format!("{} @> ${}", col, args.len())
            },
            // ... etc
        }
    }
}

mod mysql {
    fn lower_filter(filter: &Filter, args: &mut Vec<Value>) -> String {
        match filter {
            Filter::Eq(Field::Json { column, path }, value) => {
                args.push(value);
                format!("JSON_EXTRACT({}, '{}') = ?", column, path)
            },
            // MySQL only has basic string operators
            Filter::Regex(..) => panic!("Regex not supported in MySQL"),
            // ... etc
        }
    }
}

mod sqlite {
    fn lower_filter(filter: &Filter, args: &mut Vec<Value>) -> String {
        match filter {
            Filter::Eq(Field::Json { column, path }, value) => {
                args.push(value);
                format!("json_extract({}, '{}') = ?", column, path)
            },
            // SQLite only has basic string operators
            Filter::Regex(..) => panic!("Regex not supported in SQLite"),
            // ... etc
        }
    }
}
```text
<!-- Code example in TEXT -->

---

## 3. Example: The Same Schema, Three Different GraphQL APIs

### Input Schema (Same for All)

```python
<!-- Code example in Python -->
from FraiseQL import schema, type, query, ID, String

@schema.type
class User:
    id: ID
    email: str
    name: str
    bio: str
    tags: list[str]
    status: str

@schema.query
def users(where: "UserWhereInput" = None):
    pass

schema.bind("users", "view", "v_user")
```text
<!-- Code example in TEXT -->

### Compilation: PostgreSQL Target

```bash
<!-- Code example in BASH -->
FraiseQL compile schema.py --database postgresql
```text
<!-- Code example in TEXT -->

Generated GraphQL schema includes:

```graphql
<!-- Code example in GraphQL -->
input UserWhereInput {
  email: EmailStringFilter
  bio: StringFilter
  status: StringFilter
}

input EmailStringFilter {
  _eq: String
  _neq: String
  _like: String
  _ilike: String              # ✅ PostgreSQL only
  _regex: String              # ✅ PostgreSQL only
  _starts_with: String
  _icontains: String          # ✅ PostgreSQL only
}

input StringFilter {
  _eq: String
  _neq: String
  _like: String
  _ilike: String              # ✅ PostgreSQL only
  _regex: String              # ✅ PostgreSQL only
  _starts_with: String
}
```text
<!-- Code example in TEXT -->

### Compilation: MySQL Target

```bash
<!-- Code example in BASH -->
FraiseQL compile schema.py --database mysql
```text
<!-- Code example in TEXT -->

Generated GraphQL schema includes:

```graphql
<!-- Code example in GraphQL -->
input UserWhereInput {
  email: EmailStringFilter
  bio: StringFilter
  status: StringFilter
}

input EmailStringFilter {
  _eq: String
  _neq: String
  _like: String
  # ❌ No _ilike (MySQL doesn't have ILIKE)
  # ❌ No _regex (MySQL doesn't have regex operator)
  # ❌ No _starts_with (would have to be simulated with LIKE)
}

input StringFilter {
  _eq: String
  _neq: String
  _like: String
  # ❌ No _ilike
  # ❌ No _regex
}
```text
<!-- Code example in TEXT -->

### Compilation: SQLite Target

```bash
<!-- Code example in BASH -->
FraiseQL compile schema.py --database sqlite
```text
<!-- Code example in TEXT -->

Generated GraphQL schema includes:

```graphql
<!-- Code example in GraphQL -->
input UserWhereInput {
  email: EmailStringFilter
  bio: StringFilter
  status: StringFilter
}

input EmailStringFilter {
  _eq: String
  _neq: String
  _like: String
  # ❌ No _ilike
  # ❌ No _regex
}

input StringFilter {
  _eq: String
  _neq: String
  _like: String
}
```text
<!-- Code example in TEXT -->

---

## 4. The SQL Generation Consequence

When a client queries with an available operator, the Rust backend lowers it to appropriate SQL:

### PostgreSQL (Regex Available)

```graphql
<!-- Code example in GraphQL -->
query {
  users(where: {
    email: { _regex: "^admin@" }
  }) {
    id email
  }
}
```text
<!-- Code example in TEXT -->

Lowering → SQL:

```sql
<!-- Code example in SQL -->
SELECT id, email FROM v_user
WHERE email ~ $1
PARAMETERS: ["^admin@"]
```text
<!-- Code example in TEXT -->

### MySQL (Regex NOT Available)

The same query **cannot be issued** because `_regex` doesn't exist in the GraphQL schema generated for MySQL.

**Compile error if you try to reuse the PostgreSQL schema with MySQL:**

```text
<!-- Code example in TEXT -->
Error: Field '_regex' is not available in MySQL-targeted schema.
The following operators are available for String fields:
  - _eq
  - _neq
  - _like

Consider:
  - Using _like instead: WHERE email LIKE ?
  - Recompiling schema for MySQL to update available operators
```text
<!-- Code example in TEXT -->

This error happens **at compile time**, not at runtime.

---

## 5. Advanced PostgreSQL Features: Still Available

PostgreSQL gets the full power of its operators. The capability manifest captures all of them:

```graphql
<!-- Code example in GraphQL -->
input UserWhereInput {
  # ... basic operators ...

  # PostgreSQL JSONB operators
  metadata: JsonbFilter

  # PostgreSQL network operators
  ip_address: NetworkFilter

  # PostgreSQL vector operators (pgvector)
  embedding: VectorFilter

  # PostgreSQL LTree operators
  category_path: LTreeFilter
}

input JsonbFilter {
  _eq: JSON
  _jsonb_contains: JSON
  _jsonb_has_key: String
  _jsonb_has_keys: [String!]
  _jsonb_path: String
}

input NetworkFilter {
  _eq: IpAddress
  _cidr_contains: CIDR
  _cidr_contained_by: CIDR
  _subnet_of: CIDR
}

input VectorFilter {
  _eq: Vector
  _cosine_distance_lt: Float
  _l2_distance_lt: Float
  _inner_product: Float
  # ... 3 more vector distance operators ...
}

input LTreeFilter {
  _eq: LTree
  _ancestor: LTree
  _descendant: LTree
  _matches: String
}
```text
<!-- Code example in TEXT -->

All 60+ PostgreSQL operators are available because they're all in the capability manifest.

---

## 6. Adding a New Database: Implementation Path

To support a new database (e.g., DuckDB), you implement:

### Step 1: Add Capability Manifest Entry

```json
<!-- Code example in JSON -->
{
  "duckdb": {
    "string": [
      { "operator": "_eq", "sql": "=" },
      { "operator": "_neq", "sql": "!=" },
      { "operator": "_like", "sql": "LIKE" },
      { "operator": "_regex", "sql": "~" },
      { "operator": "_json_extract_path", "sql": "->" }
    ]
  }
}
```text
<!-- Code example in TEXT -->

### Step 2: Add Backend Lowering Module

```rust
<!-- Code example in RUST -->
// src/runtime/backends/duckdb.rs

pub fn lower_filter(filter: &Filter, args: &mut Vec<Value>) -> String {
    match filter {
        Filter::Eq(field, value) => { /* DuckDB-specific SQL */ },
        Filter::Regex(field, pattern) => { /* DuckDB regex syntax */ },
        // ... etc
    }
}
```text
<!-- Code example in TEXT -->

### Step 3: No Changes to Schema

Your schema authoring, compiler phases 1-3, or GraphQL type system changes. Only:

- Capability manifest gains new database entry
- One new lowering module added

**That's it.** Clients automatically get a DuckDB-targeted schema with the exact operators DuckDB supports.

---

## 7. Comparison to "Fake Abstraction" Approach

### ❌ Bad: Runtime Adapter (What NOT to do)

```rust
<!-- Code example in RUST -->
// Anti-pattern: Trying to unify at runtime

trait DbAdapter {
    async fn execute_regex_filter(&self, col: &str, pattern: &str) -> Result<Rows>;
}

impl DbAdapter for MysqlAdapter {
    async fn execute_regex_filter(&self, col: &str, pattern: &str) -> Result<Rows> {
        // Fake it! Use stored procedures or workarounds
        self.execute(format!("CALL fn_regex({}, {})", col, pattern)).await
    }
}

impl DbAdapter for SqliteAdapter {
    async fn execute_regex_filter(&self, col: &str, pattern: &str) -> Result<Rows> {
        // Fake it! Use GLOB or user-defined functions
        self.execute(format!("WHERE {} GLOB {}", col, pattern)).await
    }
}
```text
<!-- Code example in TEXT -->

**Problems:**

- GraphQL lies (says regex is available everywhere)
- Runtime surprises (query works in dev/PostgreSQL, fails in prod/SQLite)
- Maintenance nightmare (every new operator needs adapter methods)
- No compile-time safety

### ✅ Good: Compile-Time Specialization (What YOU do)

```python
<!-- Code example in Python -->
# Capability manifest declares reality

{
  "postgresql": { "string": ["_eq", "_regex"] },
  "mysql": { "string": ["_eq"] },
  "sqlite": { "string": ["_eq"] }
}

# Compiler generates different schemas for each

# PostgreSQL GraphQL: Has _regex
# MySQL GraphQL: Doesn't have _regex
# SQLite GraphQL: Doesn't have _regex

# Client using MySQL gets compile error if they try _regex
# No runtime surprises
# No fake abstractions
```text
<!-- Code example in TEXT -->

**Advantages:**

- Truth in schema (GraphQL schema matches database reality)
- Compile-time safety (errors caught during build)
- Simple implementation (no runtime adapters)
- Explicit in code (capability manifest is source of truth)

---

## 8. Database-Target-Driven Compilation Flow

```text
<!-- Code example in TEXT -->
┌──────────────────────────────────┐
│ Compiler Configuration          │
│ database_target = "postgresql"  │ ← Single decision point
└─────────────────┬────────────────┘
                  │
        ┌─────────▼──────────┐
        │ Load Capability    │
        │ Manifest           │
        └─────────┬──────────┘
                  │
        ┌─────────▼──────────────────┐
        │ Phase 4: WHERE Generation  │
        │ For each type:             │
        │ - Introspect columns       │
        │ - Filter operators by      │
        │   database_target          │
        │ - Generate WHERE type      │
        │   with ONLY available ops  │
        └─────────┬──────────────────┘
                  │
        ┌─────────▼──────────────┐
        │ Output CompiledSchema  │
        │ + schema.graphql       │
        │ (database-specific!)   │
        └───────────────────────┘
```text
<!-- Code example in TEXT -->

---

## 9. Production Deployment

### Development

```bash
<!-- Code example in BASH -->
# PostgreSQL with all features
FraiseQL compile schema.py --database postgresql
# → Gets 60+ WHERE operators, JSONB, vectors, LTree, etc.
```text
<!-- Code example in TEXT -->

### Production (Customer A: PostgreSQL)

```bash
<!-- Code example in BASH -->
# Deploy with PostgreSQL schema
FraiseQL compile schema.py --database postgresql
# Clients see full operator set
```text
<!-- Code example in TEXT -->

### Production (Customer B: MySQL)

```bash
<!-- Code example in BASH -->
# Deploy with MySQL schema
FraiseQL compile schema.py --database mysql
# Clients see only MySQL-compatible operators
# Same schema.py file, different compiled output
```text
<!-- Code example in TEXT -->

### Production (Customer C: SQLite)

```bash
<!-- Code example in BASH -->
# Deploy with SQLite schema
FraiseQL compile schema.py --database sqlite
# Clients see only basic operators
# Same schema.py file, different compiled output
```text
<!-- Code example in TEXT -->

---

## 10. Success Criteria

✅ **True multi-database support achieved when:**

1. Same schema definition works for all databases
2. Each database target generates appropriate WHERE types
3. Clients can't access unsupported operators (compile error)
4. Adding new operators only requires updating capability manifest
5. Adding new database only requires adding lowering module
6. No runtime adapters, shims, or fake abstractions
7. SQL generation is backend-isolated
8. Full PostgreSQL power is retained for PostgreSQL deployments

---

## 11. Related Specifications

- **Compilation Pipeline** (`docs/architecture/core/compilation-pipeline.md`) — Phase 4 WHERE generation
- **Execution Model** (`docs/architecture/core/execution-model.md`) — Backend lowering and execution
- **WHERE Operators Reference** (`docs/reference/where-operators.md`) — Complete operator catalog by database
- **Scalars Reference** (`docs/reference/scalars.md`) — Database-specific scalar support

---

*End of Database Targeting Specification*
