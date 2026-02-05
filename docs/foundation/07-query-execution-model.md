<!-- Skip to main content -->
---
title: 2.2: Query Execution Model
description: While Topic 2.1 explained how FraiseQL **compiles** schemas at build time, this topic explains what happens when a query **executes** at runtime.
keywords: ["query-execution", "data-planes", "graphql", "compilation", "architecture"]
tags: ["documentation", "reference"]
---

# 2.2: Query Execution Model

**Audience:** Developers implementing FraiseQL servers, operations engineers, backend architects
**Prerequisite:** Topics 1.2 (Core Concepts), 2.1 (Compilation Pipeline)
**Reading Time:** 15-20 minutes

---

## Overview

While Topic 2.1 explained how FraiseQL **compiles** schemas at build time, this topic explains what happens when a query **executes** at runtime.

**Key Insight:** Runtime execution is simple and deterministic because all the hard work (optimization, validation, authorization) was done at compile time. The server just executes pre-compiled templates.

---

## The Query Execution Model

**Diagram: Query Execution** - 8-stage runtime model with authorization and field masking

```d2
<!-- Code example in D2 Diagram -->
direction: down

Request: "Client Request\n(GraphQL Query)" {
  shape: box
  style.fill: "#e3f2fd"
}

Parse: "Parse Request" {
  shape: box
  style.fill: "#f3e5f5"
}

Lookup: "Look Up Pre-Compiled\nTemplate" {
  shape: box
  style.fill: "#ede7f6"
}

Validate: "Validate & Bind\nParameters" {
  shape: box
  style.fill: "#f1f8e9"
}

AuthCheck: "Check Authorization\nRules" {
  shape: box
  style.fill: "#fff3e0"
}

Execute: "Execute SQL\nTemplate" {
  shape: box
  style.fill: "#ffe0b2"
}

Fetch: "Fetch Results from\nDatabase" {
  shape: box
  style.fill: "#ffccbc"
}

Format: "Format Response" {
  shape: box
  style.fill: "#f8bbd0"
}

Return: "Return to Client" {
  shape: box
  style.fill: "#c8e6c9"
}

Request -> Parse
Parse -> Lookup: "JSON"
Lookup -> Validate: "template + schema"
Validate -> AuthCheck: "parameters"
AuthCheck -> Execute: "approved"
Execute -> Fetch: "SQL query"
Fetch -> Format: "raw data"
Format -> Return: "JSON response"
```text
<!-- Code example in TEXT -->

---

## Stage 1: Client Request

### Input

A GraphQL query from a client:

```graphql
<!-- Code example in GraphQL -->
query GetUserProfile($userId: Int!) {
  user(userId: $userId) {
    userId
    email
    createdAt
    orders {
      orderId
      total
    }
  }
}

# Variables: { "userId": 123 }
```text
<!-- Code example in TEXT -->

### What the Server Receives

```json
<!-- Code example in JSON -->
{
  "query": "query GetUserProfile($userId: Int!) { user(userId: $userId) { ... } }",
  "variables": { "userId": 123 },
  "operationName": "GetUserProfile"
}
```text
<!-- Code example in TEXT -->

### Server Processing

1. **Receive HTTP request** (POST /graphql)
2. **Parse JSON** (extract query, variables, operation name)
3. **Extract operation** (find GetUserProfile operation)
4. **Validate schema** (ensure query matches compiled schema)

---

## Stage 2: Look Up Pre-Compiled Template

### The Pre-Compiled Schema

At compile time, FraiseQL created schema.compiled.json with all query templates:

```json
<!-- Code example in JSON -->
{
  "queries": [
    {
      "name": "GetUserProfile",
      "parameters": [
        {
          "name": "userId",
          "type": "Int",
          "nullable": false
        }
      ],
      "returns": "User",
      "sql_template": "SELECT pk_user_id, email, created_at FROM tb_users WHERE pk_user_id = $1",
      "nested_queries": [
        {
          "field": "orders",
          "sql_template": "SELECT pk_order_id, fk_user_id, total FROM tb_orders WHERE fk_user_id = $1 ORDER BY created_at DESC"
        }
      ],
      "complexity": "O(1) + O(N)",
      "estimated_cost_ms": 50
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Runtime Lookup

```python
<!-- Code example in Python -->
# Server loads schema at startup
schema = load_compiled_schema("schema.compiled.json")

# When query arrives, look up template
query_template = schema.queries["GetUserProfile"]

# Result: Pre-optimized SQL template + metadata
# sql_template: "SELECT pk_user_id, email, created_at FROM tb_users WHERE pk_user_id = $1"
# parameters: [{ name: "userId", type: "Int" }]
# nested_queries: [...]
```text
<!-- Code example in TEXT -->

**Performance:** O(1) lookup (hash table), <1ms

---

## Stage 3: Validate & Bind Parameters

### Parameter Validation

```python
<!-- Code example in Python -->
# Request variables: { "userId": 123 }
# Template expects: { "userId": Int }

parameters = request.variables
template = query_template

for param in template.parameters:
    if param.name not in parameters:
        # Required parameter missing
        raise GraphQLError(f"Missing required parameter: {param.name}")

    value = parameters[param.name]

    # Type validation (compile-time guarantees catch this too, but validate at runtime)
    if not isinstance(value, int):  # Expected Int
        raise GraphQLError(f"Parameter {param.name} must be Int, got {type(value)}")

    # Bind to SQL template
    sql_bindings.append(value)
```text
<!-- Code example in TEXT -->

### SQL Parameter Binding

```python
<!-- Code example in Python -->
# Template: "SELECT * FROM tb_users WHERE pk_user_id = $1"
# Bindings: [123]

sql = template.sql_template
bindings = [request.variables["userId"]]

# Database driver (e.g., psycopg2, pymysql) handles safe binding
# Result: "SELECT * FROM tb_users WHERE pk_user_id = 123"
# BUT: parameters are NEVER concatenated into SQL string
#      Database receives: statement + separate bindings
#      This prevents SQL injection
```text
<!-- Code example in TEXT -->

### Output

- ✅ All parameters validated against types
- ✅ All required parameters present
- ✅ SQL statement ready for execution
- ✅ Bindings safely separated from SQL

**Performance:** <1ms

---

## Stage 4: Check Authorization Rules

### Permission Evaluation

```python
<!-- Code example in Python -->
# Compiled authorization rule for GetUserProfile:
# "authenticated_user_id = userId"

auth_rule = template.permissions["GetUserProfile"]
context = {
    "authenticated_user_id": request.user.id,  # From JWT/session
    "user_role": request.user.role,
    "userId": request.variables["userId"]
}

# Evaluate rule
if not evaluate_permission(auth_rule, context):
    raise GraphQLError("Unauthorized", status_code=403)
```text
<!-- Code example in TEXT -->

### Pre-Execution vs Post-Fetch Checks

**Pre-Execution (Fast Path)**

```python
<!-- Code example in Python -->
# Rule evaluated before SQL execution
# Example: "user_role = 'admin'"

if context["user_role"] != "admin":
    raise GraphQLError("Unauthorized")  # Fail fast, no database query

# Only if permission passes, execute SQL
execute_sql(sql, bindings)
```text
<!-- Code example in TEXT -->

**Post-Fetch (Filter Results)**

```python
<!-- Code example in Python -->
# Rule evaluated after fetching results
# Example: "user_id = authenticated_user_id OR user_role = 'admin'"

results = execute_sql(sql, bindings)

# Filter results based on permission rule
filtered_results = [
    row for row in results
    if row.user_id == context.authenticated_user_id or context.user_role == "admin"
]
```text
<!-- Code example in TEXT -->

### Compiled Permissions

From Topic 2.1 Phase 6, these are already compiled into efficient bytecode:

```text
<!-- Code example in TEXT -->
Permission Bytecode:
├─ Query: GetUserProfile
│  └─ Rule: authenticated_user_id = userId
│     Type: Pre-execution
│     Cost: <0.1ms
│
├─ Field: User.email
│  └─ Rule: authenticated_user_id = userId OR user_role = 'admin'
│     Type: Post-fetch
│     Cost: ~0.5ms (for result filtering)
│
└─ Field: User.phone
   └─ Rule: user_role = 'admin'
      Type: Post-fetch
      Cost: ~0.5ms
```text
<!-- Code example in TEXT -->

**Performance:** Pre-execution <0.1ms, Post-fetch ~0.5ms per result

---

## Stage 5: Execute SQL Template

### Database Connection

```python
<!-- Code example in Python -->
# Connection pool (pre-established at server startup)
db_pool = get_connection_pool()
connection = db_pool.get_connection()

try:
    # Execute pre-optimized SQL with bound parameters
    results = connection.execute(
        sql,           # "SELECT pk_user_id, email, created_at FROM tb_users WHERE pk_user_id = $1"
        bindings       # [123]
    )
except DatabaseError as e:
    raise GraphQLError(f"Database error: {e}")
finally:
    db_pool.return_connection(connection)
```text
<!-- Code example in TEXT -->

### Query Execution Example

```sql
<!-- Code example in SQL -->
-- Pre-compiled template (from schema.compiled.json)
SELECT pk_user_id, email, created_at
FROM tb_users
WHERE pk_user_id = $1  -- Uses primary key index (very fast)
LIMIT 1

-- Result:

-- pk_user_id | email            | created_at
-- 123        | user@example.com | 2026-01-01 10:00:00
```text
<!-- Code example in TEXT -->

### Nested Queries (Relationships)

```python
<!-- Code example in Python -->
# Initial query fetches user
user_result = {
    "userId": 123,
    "email": "user@example.com",
    "createdAt": "2026-01-01T10:00:00Z"
}

# Nested query: fetch user's orders
orders_template = query_template.nested_queries["orders"]
orders_sql = orders_template.sql_template  # Pre-compiled!
orders_bindings = [user_result["userId"]]

orders_results = execute_sql(orders_sql, orders_bindings)

# Result:
orders_results = [
    {"orderId": 456, "total": 99.99},
    {"orderId": 789, "total": 149.99}
]
```text
<!-- Code example in TEXT -->

### SQL Optimization Enabled by Compilation

```sql
<!-- Code example in SQL -->
-- FraiseQL queries are optimized at compile time
-- Database sees well-structured queries

-- Good: Uses indexes effectively
SELECT pk_user_id, email, created_at
FROM tb_users
WHERE pk_user_id = $1  -- ← Primary key (indexed)
LIMIT 1

-- Good: Batch loading pattern (avoid N+1)
SELECT pk_order_id, fk_user_id, total
FROM tb_orders
WHERE fk_user_id = ANY($1)  -- ← IN clause, uses index
ORDER BY created_at DESC

-- Good: Indexes recommended at compile time
-- Query uses: tb_users(pk_user_id), tb_orders(fk_user_id, created_at)
-- Missing indexes detected and recommended in compilation report
```text
<!-- Code example in TEXT -->

**Performance:** Depends on data size

- Simple lookup: 5-20ms
- List query (100 items): 20-50ms
- Complex join: 50-200ms

---

## Stage 6: Format Response

### Fetch Results

```python
<!-- Code example in Python -->
# Raw database results
rows = [
    {pk_user_id: 123, email: "user@example.com", created_at: datetime(2026, 1, 1)},
    # ... (from orders query)
]

# Include nested results
orders = [
    {pk_order_id: 456, fk_user_id: 123, total: 99.99},
    {pk_order_id: 789, fk_user_id: 123, total: 149.99}
]
```text
<!-- Code example in TEXT -->

### Transform to GraphQL Response

```python
<!-- Code example in Python -->
# Convert database column names to GraphQL field names
# pk_user_id → userId
# created_at → createdAt (camelCase)

response = {
    "data": {
        "user": {
            "userId": 123,
            "email": "user@example.com",
            "createdAt": "2026-01-01T10:00:00Z",  # ISO format
            "orders": [
                {
                    "orderId": 456,
                    "total": 99.99
                },
                {
                    "orderId": 789,
                    "total": 149.99
                }
            ]
        }
    }
}
```text
<!-- Code example in TEXT -->

### Serialize to JSON

```python
<!-- Code example in Python -->
import json

# Convert Python objects to JSON
json_response = json.dumps(response)

# Result:
# {"data": {"user": {"userId": 123, "email": "user@example.com", ...}}}
```text
<!-- Code example in TEXT -->

**Performance:** <5ms for typical response

---

## Stage 7: Return to Client

### HTTP Response

```http
<!-- Code example in HTTP -->
HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: 342

{
  "data": {
    "user": {
      "userId": 123,
      "email": "user@example.com",
      "createdAt": "2026-01-01T10:00:00Z",
      "orders": [
        {
          "orderId": 456,
          "total": 99.99
        },
        {
          "orderId": 789,
          "total": 149.99
        }
      ]
    }
  }
}
```text
<!-- Code example in TEXT -->

### Error Response (If Something Failed)

```http
<!-- Code example in HTTP -->
HTTP/1.1 400 Bad Request
Content-Type: application/json

{
  "errors": [
    {
      "message": "Missing required parameter: userId",
      "extensions": {
        "code": "GRAPHQL_PARSE_FAILED"
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## Complete Execution Timeline

### Example Request

```graphql
<!-- Code example in GraphQL -->
query GetUserProfile($userId: Int!) {
  user(userId: $userId) {
    userId
    email
    orders {
      orderId
      total
    }
  }
}

Variables: { "userId": 123 }
```text
<!-- Code example in TEXT -->

### Execution Timeline (Detailed)

```text
<!-- Code example in TEXT -->
Timeline: Request → Response

T+0ms:    Client sends request
T+1ms:    ├─ Parse JSON request
T+1ms:    ├─ Look up "GetUserProfile" template
T+2ms:    ├─ Validate parameter userId=123 (Int)
T+2ms:    ├─ Check permission: authenticated_user_id = 123
T+2ms:    ├─ Execute user query
T+15ms:   │  └─ Database returns: { userId: 123, email: "...", ... }
T+16ms:   ├─ Authorization check passed
T+16ms:   ├─ Execute nested orders query
T+25ms:   │  └─ Database returns: [{ orderId: 456, ... }, { orderId: 789, ... }]
T+26ms:   ├─ Format GraphQL response
T+26ms:   ├─ Serialize to JSON
T+27ms:   └─ Send HTTP response

Total Time: 27ms
Breakdown:

- Parsing & lookup: 2ms (4%)
- Authorization: <1ms (2%)
- Database queries: 20ms (74%)
- Formatting & serialization: 5ms (20%)
```text
<!-- Code example in TEXT -->

---

## Error Handling During Execution

### Type Errors

```python
<!-- Code example in Python -->
# Request: { "userId": "not-a-number" }
# Expected: Int

try:
    value = int(request.variables["userId"])
except ValueError:
    raise GraphQLError(
        "Variable $userId of type Int! was not provided a valid Int value",
        extensions={"code": "BAD_USER_INPUT"}
    )
```text
<!-- Code example in TEXT -->

### Missing Required Parameters

```python
<!-- Code example in Python -->
if "userId" not in request.variables:
    raise GraphQLError(
        "Variable $userId of required type Int! was not provided",
        extensions={"code": "BAD_USER_INPUT"}
    )
```text
<!-- Code example in TEXT -->

### Authorization Failures

```python
<!-- Code example in Python -->
if not evaluate_permission(auth_rule, context):
    raise GraphQLError(
        "Unauthorized",
        extensions={"code": "FORBIDDEN"}
    )
```text
<!-- Code example in TEXT -->

### Database Errors

```python
<!-- Code example in Python -->
try:
    results = execute_sql(sql, bindings)
except DatabaseError as e:
    if "connection refused" in str(e):
        raise GraphQLError(
            "Database connection failed",
            extensions={"code": "INTERNAL_SERVER_ERROR"}
        )
    else:
        raise GraphQLError(
            "Database query failed",
            extensions={"code": "INTERNAL_SERVER_ERROR"}
        )
```text
<!-- Code example in TEXT -->

### Error Response Format

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Variable $userId of type Int! was not provided a valid Int value",
      "extensions": {
        "code": "BAD_USER_INPUT"
      },
      "locations": [
        {
          "line": 1,
          "column": 23
        }
      ]
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## Key Characteristics of FraiseQL Execution

### 1. Deterministic Performance

```text
<!-- Code example in TEXT -->
Every query has predictable performance:

- Lookup time: O(1)
- Parameter validation: O(P) where P = number of parameters
- Authorization: O(1) or O(1) per result
- SQL execution: Depends on database, but query is optimized
- Response formatting: O(N) where N = number of results

Total: Predictable and reproducible
```text
<!-- Code example in TEXT -->

### 2. No Query Interpretation

```text
<!-- Code example in TEXT -->
Traditional GraphQL:

- Parse query (5ms)
- Validate against schema (3ms)
- Determine resolver chain (2ms)
- Execute resolvers (20-100ms)
Total: Variable

FraiseQL:

- Look up pre-compiled template (0.1ms)
- Execute SQL template (20-50ms)
Total: Fast and consistent
```text
<!-- Code example in TEXT -->

### 3. Automatic N+1 Prevention

```text
<!-- Code example in TEXT -->
Compile-time query analysis detects potential N+1 patterns:
❌ Bad pattern (would cause N+1):
   for user in users:
       orders = query_orders(user.id)  # N queries!

✅ Good pattern (batch loading):
   users = query_users()
   orders = query_orders_for_users([user.id for user in users])  # 1 query!

FraiseQL uses compiled templates that are already optimized.
```text
<!-- Code example in TEXT -->

### 4. Authorization Integrated

```text
<!-- Code example in TEXT -->
Permission checks happen at two points:

1. Pre-execution: Fast fail if not authorized
   Example: "user_role = 'admin'" → Check before SQL
2. Post-fetch: Filter results based on permission
   Example: "user_id = authenticated_user_id" → Filter after SQL

All compiled and optimized at build time.
```text
<!-- Code example in TEXT -->

---

## Comparison: FraiseQL vs Traditional GraphQL

### Apollo Server

```text
<!-- Code example in TEXT -->
Request → Parse (5ms) → Validate (3ms) →
Resolve User (2ms) → Execute query (20ms) →
Resolve Orders field (2ms) → Execute query (15ms) →
Format (3ms) → Total: ~50ms
```text
<!-- Code example in TEXT -->

### FraiseQL

```text
<!-- Code example in TEXT -->
Request → Lookup template (0.1ms) →
Validate params (0.5ms) → Check auth (<0.1ms) →
Execute SQL (35ms) → Format (1ms) → Total: ~37ms
```text
<!-- Code example in TEXT -->

**Result:** FraiseQL is ~26% faster because it skips interpretation overhead

---

## Real-World Example: E-Commerce Query

### Query

```graphql
<!-- Code example in GraphQL -->
query GetProductReviews($productId: Int!, $limit: Int = 10) {
  product(productId: $productId) {
    productId
    name
    price
    reviews(limit: $limit) {
      reviewId
      rating
      content
      author {
        userId
        username
      }
    }
  }
}

Variables: { "productId": 42, "limit": 5 }
```text
<!-- Code example in TEXT -->

### Execution

```python
<!-- Code example in Python -->
# 1. Look up template (pre-compiled)
template = schema.queries["GetProductReviews"]

# 2. Validate & bind
bindings = [42, 5]  # productId, limit

# 3. Check authorization
# Rule: "product is public OR (user_role = 'seller' AND user owns product)"
if not check_auth(template, request.user, bindings):
    return error("Unauthorized")

# 4. Execute initial query
product = db.execute(
    "SELECT pk_product_id, name, price FROM tb_products WHERE pk_product_id = $1",
    [42]
)

# 5. Execute nested query
reviews = db.execute(
    """
    SELECT rv.pk_review_id, rv.fk_product_id, rv.rating, rv.content,
           u.pk_user_id, u.username
    FROM tb_reviews rv
    JOIN tb_users u ON rv.fk_user_id = u.pk_user_id
    WHERE rv.fk_product_id = $1
    ORDER BY rv.created_at DESC
    LIMIT $2
    """,
    [42, 5]
)

# 6. Filter results by post-fetch authorization
# (only show reviews author's username to reviewer or seller)
filtered_reviews = [
    review for review in reviews
    if (request.user.id == review.author_id or
        request.user.role == "seller" and request.user.id == product.seller_id)
]

# 7. Format response
return {
    "data": {
        "product": {
            "productId": 42,
            "name": product.name,
            "price": product.price,
            "reviews": [
                {
                    "reviewId": review.pk_review_id,
                    "rating": review.rating,
                    "content": review.content,
                    "author": {
                        "userId": review.pk_user_id,
                        "username": review.username
                    }
                }
                for review in filtered_reviews
            ]
        }
    }
}
```text
<!-- Code example in TEXT -->

---

## Performance Characteristics

### Query Latency

```text
<!-- Code example in TEXT -->
Typical Query Performance:

Simple lookup (1 result):

- Latency: 10-20ms
- Database time: 5-15ms
- Server overhead: 2-5ms

List query (100 results):

- Latency: 30-50ms
- Database time: 20-40ms
- Server overhead: 5-10ms

Complex join (1000 results):

- Latency: 100-200ms
- Database time: 80-180ms
- Server overhead: 10-20ms

Factors:

- Database server performance (latency, index tuning)
- Network latency (distance to database)
- Result size (more results = slower formatting)
- Authorization complexity (post-fetch filtering)
```text
<!-- Code example in TEXT -->

### Throughput

```text
<!-- Code example in TEXT -->
Server Capacity (with connection pooling):

For single FraiseQL server (4 CPUs, 8GB RAM):

- Simple queries: 1000-2000 QPS
- Average queries: 500-1000 QPS
- Complex queries: 100-500 QPS

Limiting factors:

- Database connection pool size
- Database capacity
- Network bandwidth
- Authorization check complexity
```text
<!-- Code example in TEXT -->

---

## Related Topics

- **Topic 1.2:** Core Concepts & Terminology (understanding terms)
- **Topic 2.1:** Compilation Pipeline (what happens at compile time)
- **Topic 2.3:** Data Planes Architecture (JSON vs Arrow execution)
- **Topic 2.5:** Error Handling & Validation (detailed error handling)
- **Topic 2.7:** Performance Characteristics (optimization strategies)
- **Topic 5.1:** Performance Optimization (tuning execution)

---

## Summary

FraiseQL's query execution model is simple and deterministic because all the hard work (optimization, validation, authorization) was done at compile time:

1. **Stage 1: Parse Request** - Extract query and variables
2. **Stage 2: Look Up Template** - O(1) hash lookup
3. **Stage 3: Validate & Bind** - Type check and SQL bind parameters
4. **Stage 4: Check Authorization** - Pre-execution and post-fetch rules
5. **Stage 5: Execute SQL** - Pre-optimized template with indexes
6. **Stage 6: Format Response** - Convert to GraphQL response format
7. **Stage 7: Return** - Send HTTP response

**Result:** Predictable, fast, auditable query execution with built-in security.

**Typical Performance:**

- Simple query: 10-20ms
- Average query: 30-50ms
- Complex query: 100-200ms

**Throughput:**

- Simple queries: 1000-2000 QPS per server
- Average queries: 500-1000 QPS per server
- Complex queries: 100-500 QPS per server
