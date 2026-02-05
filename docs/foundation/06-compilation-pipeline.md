<!-- Skip to main content -->
---
title: 2.1: Compilation Pipeline
description: FraiseQL's core strength is its **build-time compilation**. This topic explains how FraiseQL transforms your schema definitions (Python/TypeScript) into an opti
keywords: ["query-execution", "data-planes", "graphql", "compilation", "architecture"]
tags: ["documentation", "reference"]
---

# 2.1: Compilation Pipeline

**Audience:** Architects, developers implementing custom extensions, teams deploying FraiseQL
**Prerequisite:** Topics 1.1-1.4 (foundation), 1.3 (Database-Centric Architecture)
**Reading Time:** 20-25 minutes

---

## Overview

FraiseQL's core strength is its **build-time compilation**. This topic explains how FraiseQL transforms your schema definitions (Python/TypeScript) into an optimized, production-ready GraphQL API.

**Key Insight:** Everything that can be done at build time is done at build time. This moves performance, validation, and optimization work away from request handling, leaving runtime fast and predictable.

---

## The Seven-Phase Compilation Pipeline

**Diagram: Compilation Pipeline** - 7-phase process from source code to optimized schema

```d2
<!-- Code example in D2 Diagram -->
direction: down

Parse: "Parse Schema Definitions\n(Python/TypeScript files)" {
  shape: box
  style.fill: "#e1f5ff"
}

Extract: "Extract Type Information\n(build schema.json)" {
  shape: box
  style.fill: "#f3e5f5"
}

Validate: "Validate Relationships\n(foreign keys, types)" {
  shape: box
  style.fill: "#f1f8e9"
}

Analyze: "Analyze Query Patterns\n(query costs, N+1 detection)" {
  shape: box
  style.fill: "#fff3e0"
}

Optimize: "Optimize SQL Templates\n(efficient queries)" {
  shape: box
  style.fill: "#ffe0b2"
}

Authorize: "Generate Authorization Rules\n(permission checks)" {
  shape: box
  style.fill: "#ffccbc"
}

Output: "Output Compiled Schema\n(schema.compiled.json)" {
  shape: box
  style.fill: "#ffccbc"
}

Server: "Production-Ready Server" {
  shape: box
  style.fill: "#c8e6c9"
}

Parse -> Extract: "Python/TypeScript"
Extract -> Validate: "schema.json"
Validate -> Analyze: "Type graph"
Analyze -> Optimize: "Cost metrics"
Optimize -> Authorize: "SQL templates"
Authorize -> Output: "Auth rules"
Output -> Server: "Compiled schema"
```text
<!-- Code example in TEXT -->

---

## Phase 1: Parse Schema Definitions

### Input

Your schema definitions in Python or TypeScript:

```python
<!-- Code example in Python -->
# schema.py
from FraiseQL import schema
from datetime import datetime

@schema.type(table="tb_user")
class User:
    user_id: UUID  # UUID v4 for GraphQL ID
    email: str
    created_at: datetime
    is_active: bool

@schema.type(table="tb_order")
class Order:
    order_id: UUID  # UUID v4 for GraphQL ID
    user_id: UUID  # UUID v4 for GraphQL ID
    total: float
    created_at: datetime

@schema.query()
def get_user(user_id: int) -> User:
    pass

@schema.query()
def list_orders(user_id: int) -> List[Order]:
    pass
```text
<!-- Code example in TEXT -->

### Process

1. **Parse decorators** (`@schema.type`, `@schema.query`)
2. **Extract type definitions** (class attributes → fields)
3. **Extract query definitions** (function signatures → GraphQL queries)
4. **Read database mappings** (table names, column names from annotations)
5. **Validate syntax** (no duplicate fields, valid type annotations)

### Output

Internal representation of schema structure (parsed AST):

```text
<!-- Code example in TEXT -->
Schema {
  types: [
    Type {
      name: "User",
      source: "tb_user",
      fields: [
        Field { name: "userId", type: "Int", column: "pk_user" },
        Field { name: "email", type: "String", column: "email" },
        Field { name: "createdAt", type: "DateTime", column: "created_at" },
        Field { name: "isActive", type: "Boolean", column: "is_active" }
      ]
    },
    Type {
      name: "Order",
      source: "tb_order",
      fields: [
        Field { name: "orderId", type: "Int", column: "pk_order" },
        Field { name: "userId", type: "Int", column: "fk_user" },
        Field { name: "total", type: "Float", column: "total" },
        Field { name: "createdAt", type: "DateTime", column: "created_at" }
      ]
    }
  ],
  queries: [
    Query { name: "getUser", params: { userId: "Int" }, returns: "User" },
    Query { name: "listOrders", params: { userId: "Int" }, returns: "List<Order>" }
  ]
}
```text
<!-- Code example in TEXT -->

---

## Phase 2: Extract Type Information & Build schema.json

### Input

Parsed schema from Phase 1

### Process

1. **Introspect database** (connect to schema database to verify tables/columns exist)
2. **Extract column types** (INT, VARCHAR, TIMESTAMP, BOOLEAN, etc.)
3. **Build GraphQL type definitions** (convert database types to GraphQL types)
4. **Establish relationships** (detect foreign keys)
5. **Create schema.json** (portable schema format, database-agnostic)

### Example: Database Introspection

```sql
<!-- Code example in SQL -->
-- Phase 2 queries the database schema
SELECT
  table_name,
  column_name,
  data_type,
  is_nullable,
  column_default
FROM information_schema.columns
WHERE table_name IN ('tb_user', 'tb_order')
ORDER BY table_name, ordinal_position;

-- Result:
table_name  | column_name  | data_type | is_nullable | column_default
------------|--------------|-----------|-------------|----------------
tb_user    | pk_user   | integer   | false       | nextval(...)
tb_user    | email        | character | false       | NULL
tb_user    | created_at   | timestamp | false       | now()
tb_order   | pk_order  | integer   | false       | nextval(...)
tb_order   | fk_user   | integer   | false       | NULL
tb_order   | total        | numeric   | false       | NULL
tb_order   | created_at   | timestamp | false       | now()
```text
<!-- Code example in TEXT -->

### Output: schema.json

```json
<!-- Code example in JSON -->
{
  "version": "1.0",
  "database": "postgresql",
  "types": [
    {
      "name": "User",
      "source": "tb_user",
      "fields": [
        {
          "name": "userId",
          "type": "Int",
          "nullable": false,
          "column": "pk_user",
          "database_type": "integer"
        },
        {
          "name": "email",
          "type": "String",
          "nullable": false,
          "column": "email",
          "database_type": "varchar(255)"
        },
        {
          "name": "createdAt",
          "type": "DateTime",
          "nullable": false,
          "column": "created_at",
          "database_type": "timestamp without time zone"
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "getUser",
      "parameters": [
        {
          "name": "userId",
          "type": "Int",
          "nullable": false
        }
      ],
      "returns": "User"
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## Phase 3: Validate Relationships

### Input

schema.json with all type and query information

### Process

1. **Validate foreign keys** (fk_user in orders table → references tb_user)
2. **Verify type compatibility** (parameter types match column types)
3. **Check for circular references** (detect and warn about cycles)
4. **Validate view relationships** (for database views: verify dependencies)
5. **Detect N+1 query potential** (warn if relationships might cause N+1)

### Example: Foreign Key Validation

```text
<!-- Code example in TEXT -->
Validating relationships...
✅ tb_order.fk_user → tb_user.pk_user (valid)
✅ Query getUser expects Int, tb_user.pk_user is Int (compatible)
⚠️  N+1 risk detected: listOrders(userId) will load orders for each user
    Suggestion: Use database-level join or batch loading
✅ All relationships valid
```text
<!-- Code example in TEXT -->

### Output

Validation report (warnings + errors):

```text
<!-- Code example in TEXT -->
Relationship Validation Results:

- Errors: 0
- Warnings: 1
- N+1 Risks Detected: 1

Warnings:

1. Relationship: User → Orders
   Pattern: Many-to-one (1 user, N orders)
   Recommendation: Use view or batch query to avoid N+1
```text
<!-- Code example in TEXT -->

---

## Phase 4: Analyze Query Patterns

### Input

Validated schema.json with relationship information

### Process

1. **Compute query complexity** (how many joins, filters, aggregations)
2. **Estimate data volume** (based on database statistics)
3. **Detect potential N+1 patterns** (accessing related objects in loops)
4. **Identify missing indexes** (recommend indexes for WHERE clauses)
5. **Calculate query costs** (estimated execution time)

### Example: Query Complexity Analysis

```text
<!-- Code example in TEXT -->
Analyzing query patterns...

Query: getUser(userId: Int!) -> User
- Complexity: O(1) simple lookup
- Expected joins: 0
- Expected filters: 1 (WHERE pk_user = ?)
- Estimated cost: 1-5ms
- Recommended indexes: [pk_user] (already primary key)
✅ Query is efficient

Query: listOrders(userId: Int!) -> List<Order>
- Complexity: O(N) where N = number of orders
- Expected joins: 0
- Expected filters: 1 (WHERE fk_user = ?)
- Estimated cost: 10-50ms (depends on data volume)
- Recommended indexes: [fk_user]
⚠️ Missing index on fk_user - consider adding
```text
<!-- Code example in TEXT -->

### Output

Query analysis report with optimization recommendations:

```text
<!-- Code example in TEXT -->
Query Pattern Analysis:

- Simple lookups (O(1)): 5 queries
- List queries (O(N)): 3 queries
- Aggregations (O(N)): 2 queries

Recommendations:

1. Add index on tb_order(fk_user)
2. Consider materialized view for user order totals
3. Add column tb_order(total_count) for common aggregation
```text
<!-- Code example in TEXT -->

---

## Phase 5: Optimize SQL Templates

### Input

Query patterns and complexity analysis from Phase 4

### Process

1. **Generate optimal SQL** (write efficient base queries)
2. **Determine join strategies** (inner join vs. left join vs. subquery)
3. **Optimize WHERE clauses** (order conditions, use indexes)
4. **Plan aggregations** (GROUP BY optimization)
5. **Add query hints** (PostgreSQL-specific optimizations)

### Example: SQL Template Generation

```python
<!-- Code example in Python -->
# Input: getUser query definition
# Output: SQL template

sql_template = """
SELECT
  pk_user,
  email,
  created_at,
  is_active
FROM tb_user
WHERE pk_user = $1
LIMIT 1
"""

# With optimization hints (PostgreSQL):
sql_template_optimized = """
SELECT
  pk_user,
  email,
  created_at,
  is_active
FROM tb_user
WHERE pk_user = $1  -- Uses primary key index
LIMIT 1
"""

# For list query with potential N+1:
sql_template_list = """
SELECT
  pk_order,
  fk_user,
  total,
  created_at
FROM tb_order
WHERE fk_user = $1  -- Should use index on fk_user
ORDER BY created_at DESC
"""
```text
<!-- Code example in TEXT -->

### Output

Compiled SQL templates (one per query):

```text
<!-- Code example in TEXT -->
Compiled SQL Templates:

- getUser: 1 template (simple lookup)
- listOrders: 1 template (list with filter)
- getUserWithOrders: 3 templates (user + orders with pagination)
  * Template 1: Get user
  * Template 2: Get order count
  * Template 3: Get paginated orders

Total templates: 5
Memory footprint: ~50KB
```text
<!-- Code example in TEXT -->

---

## Phase 6: Generate Authorization Rules

### Input

Query definitions with permission annotations

### Process

1. **Parse permission decorators** (`@schema.permission()`)
2. **Generate permission checks** (compile into efficient runtime checks)
3. **Create permission context** (what user info needed, when evaluated)
4. **Validate permission rules** (catch impossible conditions at compile time)
5. **Output permission bytecode** (fast binary format for runtime)

### Example: Permission Compilation

```python
<!-- Code example in Python -->
# Input: Schema with permissions
@schema.permission("user_role = 'admin'")
@schema.query()
def delete_user(user_id: int) -> bool:
    pass

@schema.permission("user_id = authenticated_user_id")
@schema.type(table="tb_user")
class UserProfile:
    user_id: UUID  # UUID v4 for GraphQL ID
    email: str
    phone: str  # Only visible to self or admin

    @schema.permission("user_role = 'admin'")
    phone: str
```text
<!-- Code example in TEXT -->

### Compiled Permissions

```text
<!-- Code example in TEXT -->
Permission Rules (Compiled):

1. delete_user:
   - Type: Mutation permission
   - Rule: user_role = 'admin'
   - Evaluation: Pre-execution (fail fast)
   - Context needed: user_role

2. UserProfile.phone:
   - Type: Field permission
   - Rules:
     a) user_role = 'admin' (override)
     b) user_id = authenticated_user_id (default)
   - Evaluation: Post-fetch (apply to results)
   - Context needed: user_id, user_role, authenticated_user_id
```text
<!-- Code example in TEXT -->

### Output

Compiled authorization bytecode:

```text
<!-- Code example in TEXT -->
Authorization Bytecode:

- Byte size: ~2KB
- Runtime checks: 12
- Pre-execution filters: 5
- Post-fetch filters: 7
- Performance: <1ms per request
```text
<!-- Code example in TEXT -->

---

## Phase 7: Output Compiled Schema

### Input

All previous phases' outputs (SQL templates, auth rules, type definitions)

### Process

1. **Merge all metadata** (types, queries, mutations, permissions)
2. **Create runtime-ready format** (binary optimized for speed)
3. **Add version information** (schema version, FraiseQL version)
4. **Generate checksums** (detect tampering, corruption)
5. **Output schema.compiled.json** (ready for production)

### Example: schema.compiled.json Structure

```json
<!-- Code example in JSON -->
{
  "version": "2.1.0",
  "fraiseql_version": "2.0.0",
  "database": "postgresql",
  "checksum": "sha256:abc123...",
  "compiled_at": "2026-01-29T19:30:00Z",

  "types": [
    {
      "name": "User",
      "source": "tb_user",
      "fields": [
        {
          "name": "userId",
          "type": "Int",
          "column": "pk_user",
          "nullable": false,
          "permissions": []
        },
        {
          "name": "email",
          "type": "String",
          "column": "email",
          "nullable": false,
          "permissions": ["read"]
        }
      ]
    }
  ],

  "queries": [
    {
      "name": "getUser",
      "parameters": [
        {
          "name": "userId",
          "type": "Int",
          "nullable": false
        }
      ],
      "returns": "User",
      "permissions": [],
      "sql_template": "SELECT pk_user, email, ... FROM tb_user WHERE pk_user = $1",
      "complexity": "O(1)",
      "estimated_cost_ms": 5,
      "required_indexes": ["tb_user(pk_user)"]
    }
  ],

  "mutations": [],

  "authorization": {
    "global_rules": [],
    "field_rules": {
      "UserProfile.phone": "user_role = 'admin' OR user_id = authenticated_user_id"
    },
    "query_rules": {
      "deleteUser": "user_role = 'admin'"
    }
  }
}
```text
<!-- Code example in TEXT -->

### Output: Production-Ready Schema

```text
<!-- Code example in TEXT -->
✅ Compilation Complete

Compiled Schema Statistics:

- Types: 12
- Queries: 24
- Mutations: 8
- SQL templates: 45
- Authorization rules: 18
- File size: 285KB
- Checksum: sha256:abc123def456...

Ready for deployment!
```text
<!-- Code example in TEXT -->

---

## The Complete Pipeline in Action

### Example: E-Commerce Query

**Input: Python Schema**

```python
<!-- Code example in Python -->
@schema.type(table="v_products")
class Product:
    product_id: UUID  # UUID v4 for GraphQL ID
    name: str
    price: float
    in_stock: bool

@schema.query()
def search_products(query: str, limit: int = 10) -> List[Product]:
    pass
```text
<!-- Code example in TEXT -->

**Phase 1: Parse**
→ Extract type information, recognize search_products query

**Phase 2: Extract Types**
→ Introspect v_products view, build schema.json

**Phase 3: Validate**
→ Verify all columns exist, types are compatible

**Phase 4: Analyze**
→ Detect: search_products uses LIKE operator (needs index on name)

**Phase 5: Optimize**
→ Generate SQL:

```sql
<!-- Code example in SQL -->
SELECT product_id, name, price, in_stock
FROM v_products
WHERE name ILIKE $1 || '%'  -- PostgreSQL ILIKE with leading wildcard
ORDER BY popularity DESC
LIMIT $2
```text
<!-- Code example in TEXT -->

**Phase 6: Authorize**
→ No special permissions (public query)

**Phase 7: Output**
→ schema.compiled.json includes optimized template

**Runtime: GraphQL Request**

```graphql
<!-- Code example in GraphQL -->
query {
  searchProducts(query: "laptop", limit: 5) {
    productId
    name
    price
    inStock
  }
}
```text
<!-- Code example in TEXT -->

→ Server looks up pre-compiled template
→ Binds parameters ($1="laptop%", $2=5)
→ Executes pre-optimized SQL
→ Returns results in <10ms

---

## Benefits of Multi-Phase Compilation

### 1. Early Error Detection

```text
<!-- Code example in TEXT -->
❌ Error caught at compile time:
   Column 'users_id' not found in tb_user

✅ Not discovered in production
```text
<!-- Code example in TEXT -->

### 2. Performance Optimization

```text
<!-- Code example in TEXT -->
 detects missing index:
→ Recommendation: Add index on tb_order(fk_user)
→ DBA adds index before deployment
→ Queries automatically use it
```text
<!-- Code example in TEXT -->

### 3. Security Verification

```text
<!-- Code example in TEXT -->
 compiles authorization:
→ All permission rules checked for logic errors
→ Impossible conditions detected
→ Authorization always evaluated consistently
```text
<!-- Code example in TEXT -->

### 4. Deterministic Behavior

```text
<!-- Code example in TEXT -->
All query optimization happens at build time.
At runtime: just execute pre-compiled template.
Result: Predictable performance, no surprises.
```text
<!-- Code example in TEXT -->

---

## What the Compilation Pipeline Enables

### 1. Query Plan Transparency

```text
<!-- Code example in TEXT -->
Every query has a pre-computed plan:
SELECT statement, expected cost, required indexes
All visible before serving requests
```text
<!-- Code example in TEXT -->

### 2. Automatic Optimization

```text
<!-- Code example in TEXT -->
Index missing? Phase 4 detects and recommends
Index added? Phase 5 generates optimal SQL using it
No code changes needed
```text
<!-- Code example in TEXT -->

### 3. Schema Versioning

```text
<!-- Code example in TEXT -->
Each compiled schema is versioned:
schema.compiled.json version 1.2
Can run multiple versions, gradually migrate clients
```text
<!-- Code example in TEXT -->

### 4. Deployment Safety

```text
<!-- Code example in TEXT -->
Compile server before deploying:

- All queries validated
- All permissions verified
- All indexes recommended
- If anything fails, don't deploy
Result: Zero surprises in production
```text
<!-- Code example in TEXT -->

---

## When Compilation Happens

### Development

```bash
<!-- Code example in BASH -->
# Watch for changes and recompile
FraiseQL-cli watch schema.py --output schema.compiled.json

# Or one-time compilation
FraiseQL-cli compile schema.py --output schema.compiled.json
```text
<!-- Code example in TEXT -->

### CI/CD Pipeline

```bash
<!-- Code example in BASH -->
# Automated compilation in build step
FraiseQL-cli compile schema.py \
  --database-url $DATABASE_URL \
  --output schema.compiled.json \
  --strict  # Fail on any warning
```text
<!-- Code example in TEXT -->

### Production Deployment

```bash
<!-- Code example in BASH -->
# 1. Compile schema (all validations run)
FraiseQL-cli compile schema.py --database-url prod_db

# 2. Run tests with compiled schema
FraiseQL-server --schema schema.compiled.json &
pytest tests/

# 3. Deploy if tests pass
docker build .
docker push registry/FraiseQL-server:latest
```text
<!-- Code example in TEXT -->

---

## Performance Impact of Compilation

### Compilation Time

```text
<!-- Code example in TEXT -->
Typical project:

- Schema definitions: 50 types, 30 queries
- Compilation time: 2-5 seconds
- Breakdown:
  * Phase 1-2 (Parse + Extract): 0.5s
  * Phase 3 (Validate): 0.3s
  * Phase 4 (Analyze): 0.8s
  * Phase 5 (Optimize): 1.2s
  * Phase 6 (Authorize): 0.1s
  * Phase 7 (Output): 0.1s

Total: ~3 seconds for full recompilation
```text
<!-- Code example in TEXT -->

### Runtime Performance (Per Request)

```text
<!-- Code example in TEXT -->
Query execution with pre-compiled schema:

Traditional GraphQL Server:

1. Parse query (5ms)
2. Validate against schema (3ms)
3. Resolve fields (10ms)
4. Execute database query (50ms)
Total: 68ms

FraiseQL:

1. Look up query template (0.1ms)
2. Bind parameters (0.5ms)
3. Execute pre-optimized SQL (50ms)
4. Format response (0.4ms)
Total: 51ms ← 25% faster

Difference: 17ms per request = 8x faster on median case
```text
<!-- Code example in TEXT -->

---

## Related Topics

- **Topic 1.2:** Core Concepts & Terminology (understanding terms used here)
- **Topic 1.3:** Database-Centric Architecture (philosophy behind compilation)
- **Topic 2.2:** Query Execution Model (what happens at runtime)
- **Topic 2.4:** Type System (types used in compilation)
- **Topic 2.6:** Compiled Schema Structure (detailed schema.json format)
- **Topic 5.1:** Performance Optimization (using compilation output for optimization)

---

## Summary

The FraiseQL compilation pipeline is a seven-phase process that transforms your schema definitions into a production-ready, optimized GraphQL API:

1. **Phase 1: Parse** - Read Python/TypeScript schema definitions
2. **Phase 2: Extract** - Introspect database, build schema.json
3. **Phase 3: Validate** - Check relationships, types, constraints
4. **Phase 4: Analyze** - Compute query costs, detect N+1 risks, recommend indexes
5. **Phase 5: Optimize** - Generate optimal SQL templates
6. **Phase 6: Authorize** - Compile permission rules
7. **Phase 7: Output** - Create schema.compiled.json

**Key Benefits:**

- Early error detection (compile time, not runtime)
- Performance optimization (optimal SQL pre-generated)
- Deterministic behavior (no query plan surprises)
- Deployment safety (all validations before production)

**Result:** A predictable, fast, auditable GraphQL API built on a solid foundation of compile-time guarantees.
