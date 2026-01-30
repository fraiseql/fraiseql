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

```
Phase 1: Parse Schema Definitions
         ↓ (Python/TypeScript files)
Phase 2: Extract Type Information
         ↓ (build schema.json)
Phase 3: Validate Relationships
         ↓ (check foreign keys, types)
Phase 4: Analyze Query Patterns
         ↓ (compute query costs, N+1 detection)
Phase 5: Optimize SQL Templates
         ↓ (generate efficient queries)
Phase 6: Generate Authorization Rules
         ↓ (compile permission checks)
Phase 7: Output Compiled Schema
         ↓ (schema.compiled.json)
Production-Ready Server
```

---

## Phase 1: Parse Schema Definitions

### Input
Your schema definitions in Python or TypeScript:

```python
# schema.py
from fraiseql import schema
from datetime import datetime

@schema.type(table="tb_users")
class User:
    user_id: int
    email: str
    created_at: datetime
    is_active: bool

@schema.type(table="tb_orders")
class Order:
    order_id: int
    user_id: int
    total: float
    created_at: datetime

@schema.query()
def get_user(user_id: int) -> User:
    pass

@schema.query()
def list_orders(user_id: int) -> List[Order]:
    pass
```

### Process
1. **Parse decorators** (`@schema.type`, `@schema.query`)
2. **Extract type definitions** (class attributes → fields)
3. **Extract query definitions** (function signatures → GraphQL queries)
4. **Read database mappings** (table names, column names from annotations)
5. **Validate syntax** (no duplicate fields, valid type annotations)

### Output
Internal representation of schema structure (parsed AST):

```
Schema {
  types: [
    Type {
      name: "User",
      source: "tb_users",
      fields: [
        Field { name: "userId", type: "Int", column: "pk_user_id" },
        Field { name: "email", type: "String", column: "email" },
        Field { name: "createdAt", type: "DateTime", column: "created_at" },
        Field { name: "isActive", type: "Boolean", column: "is_active" }
      ]
    },
    Type {
      name: "Order",
      source: "tb_orders",
      fields: [
        Field { name: "orderId", type: "Int", column: "pk_order_id" },
        Field { name: "userId", type: "Int", column: "fk_user_id" },
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
```

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
-- Phase 2 queries the database schema
SELECT
  table_name,
  column_name,
  data_type,
  is_nullable,
  column_default
FROM information_schema.columns
WHERE table_name IN ('tb_users', 'tb_orders')
ORDER BY table_name, ordinal_position;

-- Result:
table_name  | column_name  | data_type | is_nullable | column_default
------------|--------------|-----------|-------------|----------------
tb_users    | pk_user_id   | integer   | false       | nextval(...)
tb_users    | email        | character | false       | NULL
tb_users    | created_at   | timestamp | false       | now()
tb_orders   | pk_order_id  | integer   | false       | nextval(...)
tb_orders   | fk_user_id   | integer   | false       | NULL
tb_orders   | total        | numeric   | false       | NULL
tb_orders   | created_at   | timestamp | false       | now()
```

### Output: schema.json

```json
{
  "version": "1.0",
  "database": "postgresql",
  "types": [
    {
      "name": "User",
      "source": "tb_users",
      "fields": [
        {
          "name": "userId",
          "type": "Int",
          "nullable": false,
          "column": "pk_user_id",
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
```

---

## Phase 3: Validate Relationships

### Input
schema.json with all type and query information

### Process
1. **Validate foreign keys** (fk_user_id in orders table → references tb_users)
2. **Verify type compatibility** (parameter types match column types)
3. **Check for circular references** (detect and warn about cycles)
4. **Validate view relationships** (for database views: verify dependencies)
5. **Detect N+1 query potential** (warn if relationships might cause N+1)

### Example: Foreign Key Validation

```
Validating relationships...
✅ tb_orders.fk_user_id → tb_users.pk_user_id (valid)
✅ Query getUser expects Int, tb_users.pk_user_id is Int (compatible)
⚠️  N+1 risk detected: listOrders(userId) will load orders for each user
    Suggestion: Use database-level join or batch loading
✅ All relationships valid
```

### Output
Validation report (warnings + errors):

```
Relationship Validation Results:
- Errors: 0
- Warnings: 1
- N+1 Risks Detected: 1

Warnings:
1. Relationship: User → Orders
   Pattern: Many-to-one (1 user, N orders)
   Recommendation: Use view or batch query to avoid N+1
```

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

```
Analyzing query patterns...

Query: getUser(userId: Int!) -> User
- Complexity: O(1) simple lookup
- Expected joins: 0
- Expected filters: 1 (WHERE pk_user_id = ?)
- Estimated cost: 1-5ms
- Recommended indexes: [pk_user_id] (already primary key)
✅ Query is efficient

Query: listOrders(userId: Int!) -> List<Order>
- Complexity: O(N) where N = number of orders
- Expected joins: 0
- Expected filters: 1 (WHERE fk_user_id = ?)
- Estimated cost: 10-50ms (depends on data volume)
- Recommended indexes: [fk_user_id]
⚠️ Missing index on fk_user_id - consider adding
```

### Output
Query analysis report with optimization recommendations:

```
Query Pattern Analysis:
- Simple lookups (O(1)): 5 queries
- List queries (O(N)): 3 queries
- Aggregations (O(N)): 2 queries

Recommendations:
1. Add index on tb_orders(fk_user_id)
2. Consider materialized view for user order totals
3. Add column tb_orders(total_count) for common aggregation
```

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
# Input: getUser query definition
# Output: SQL template

sql_template = """
SELECT
  pk_user_id,
  email,
  created_at,
  is_active
FROM tb_users
WHERE pk_user_id = $1
LIMIT 1
"""

# With optimization hints (PostgreSQL):
sql_template_optimized = """
SELECT
  pk_user_id,
  email,
  created_at,
  is_active
FROM tb_users
WHERE pk_user_id = $1  -- Uses primary key index
LIMIT 1
"""

# For list query with potential N+1:
sql_template_list = """
SELECT
  pk_order_id,
  fk_user_id,
  total,
  created_at
FROM tb_orders
WHERE fk_user_id = $1  -- Should use index on fk_user_id
ORDER BY created_at DESC
"""
```

### Output
Compiled SQL templates (one per query):

```
Compiled SQL Templates:
- getUser: 1 template (simple lookup)
- listOrders: 1 template (list with filter)
- getUserWithOrders: 3 templates (user + orders with pagination)
  * Template 1: Get user
  * Template 2: Get order count
  * Template 3: Get paginated orders

Total templates: 5
Memory footprint: ~50KB
```

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
# Input: Schema with permissions
@schema.permission("user_role = 'admin'")
@schema.query()
def delete_user(user_id: int) -> bool:
    pass

@schema.permission("user_id = authenticated_user_id")
@schema.type(table="tb_users")
class UserProfile:
    user_id: int
    email: str
    phone: str  # Only visible to self or admin

    @schema.permission("user_role = 'admin'")
    phone: str
```

### Compiled Permissions

```
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
```

### Output
Compiled authorization bytecode:

```
Authorization Bytecode:
- Byte size: ~2KB
- Runtime checks: 12
- Pre-execution filters: 5
- Post-fetch filters: 7
- Performance: <1ms per request
```

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
{
  "version": "2.1.0",
  "fraiseql_version": "2.0.0",
  "database": "postgresql",
  "checksum": "sha256:abc123...",
  "compiled_at": "2026-01-29T19:30:00Z",

  "types": [
    {
      "name": "User",
      "source": "tb_users",
      "fields": [
        {
          "name": "userId",
          "type": "Int",
          "column": "pk_user_id",
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
      "sql_template": "SELECT pk_user_id, email, ... FROM tb_users WHERE pk_user_id = $1",
      "complexity": "O(1)",
      "estimated_cost_ms": 5,
      "required_indexes": ["tb_users(pk_user_id)"]
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
```

### Output: Production-Ready Schema
```
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
```

---

## The Complete Pipeline in Action

### Example: E-Commerce Query

**Input: Python Schema**

```python
@schema.type(table="v_products")
class Product:
    product_id: int
    name: str
    price: float
    in_stock: bool

@schema.query()
def search_products(query: str, limit: int = 10) -> List[Product]:
    pass
```

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
SELECT product_id, name, price, in_stock
FROM v_products
WHERE name ILIKE $1 || '%'  -- PostgreSQL ILIKE with leading wildcard
ORDER BY popularity DESC
LIMIT $2
```

**Phase 6: Authorize**
→ No special permissions (public query)

**Phase 7: Output**
→ schema.compiled.json includes optimized template

**Runtime: GraphQL Request**
```graphql
query {
  searchProducts(query: "laptop", limit: 5) {
    productId
    name
    price
    inStock
  }
}
```

→ Server looks up pre-compiled template
→ Binds parameters ($1="laptop%", $2=5)
→ Executes pre-optimized SQL
→ Returns results in <10ms

---

## Benefits of Multi-Phase Compilation

### 1. Early Error Detection
```
❌ Error caught at compile time (Phase 3):
   Column 'users_id' not found in tb_users

✅ Not discovered in production
```

### 2. Performance Optimization
```
Phase 4 detects missing index:
→ Recommendation: Add index on tb_orders(fk_user_id)
→ DBA adds index before deployment
→ Queries automatically use it (Phase 5 already generated optimal SQL)
```

### 3. Security Verification
```
Phase 6 compiles authorization:
→ All permission rules checked for logic errors
→ Impossible conditions detected
→ Authorization always evaluated consistently
```

### 4. Deterministic Behavior
```
All query optimization happens at build time.
At runtime: just execute pre-compiled template.
Result: Predictable performance, no surprises.
```

---

## What the Compilation Pipeline Enables

### 1. Query Plan Transparency
```
Every query has a pre-computed plan:
SELECT statement, expected cost, required indexes
All visible before serving requests
```

### 2. Automatic Optimization
```
Index missing? Phase 4 detects and recommends
Index added? Phase 5 generates optimal SQL using it
No code changes needed
```

### 3. Schema Versioning
```
Each compiled schema is versioned:
schema.compiled.json version 1.2
Can run multiple versions, gradually migrate clients
```

### 4. Deployment Safety
```
Compile server before deploying:
- All queries validated
- All permissions verified
- All indexes recommended
- If anything fails, don't deploy
Result: Zero surprises in production
```

---

## When Compilation Happens

### Development
```bash
# Watch for changes and recompile
fraiseql-cli watch schema.py --output schema.compiled.json

# Or one-time compilation
fraiseql-cli compile schema.py --output schema.compiled.json
```

### CI/CD Pipeline
```bash
# Automated compilation in build step
fraiseql-cli compile schema.py \
  --database-url $DATABASE_URL \
  --output schema.compiled.json \
  --strict  # Fail on any warning
```

### Production Deployment
```bash
# 1. Compile schema (all validations run)
fraiseql-cli compile schema.py --database-url prod_db

# 2. Run tests with compiled schema
fraiseql-server --schema schema.compiled.json &
pytest tests/

# 3. Deploy if tests pass
docker build .
docker push registry/fraiseql-server:latest
```

---

## Performance Impact of Compilation

### Compilation Time

```
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
```

### Runtime Performance (Per Request)

```
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
```

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
