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

```
Client Request (GraphQL Query)
         ↓
Parse Request
         ↓
Look Up Pre-Compiled Template
         ↓
Validate & Bind Parameters
         ↓
Check Authorization Rules
         ↓
Execute SQL Template
         ↓
Fetch Results from Database
         ↓
Format Response
         ↓
Return to Client
```

---

## Stage 1: Client Request

### Input
A GraphQL query from a client:

```graphql
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
```

### What the Server Receives
```json
{
  "query": "query GetUserProfile($userId: Int!) { user(userId: $userId) { ... } }",
  "variables": { "userId": 123 },
  "operationName": "GetUserProfile"
}
```

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
```

### Runtime Lookup
```python
# Server loads schema at startup
schema = load_compiled_schema("schema.compiled.json")

# When query arrives, look up template
query_template = schema.queries["GetUserProfile"]

# Result: Pre-optimized SQL template + metadata
# sql_template: "SELECT pk_user_id, email, created_at FROM tb_users WHERE pk_user_id = $1"
# parameters: [{ name: "userId", type: "Int" }]
# nested_queries: [...]
```

**Performance:** O(1) lookup (hash table), <1ms

---

## Stage 3: Validate & Bind Parameters

### Parameter Validation
```python
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
```

### SQL Parameter Binding
```python
# Template: "SELECT * FROM tb_users WHERE pk_user_id = $1"
# Bindings: [123]

sql = template.sql_template
bindings = [request.variables["userId"]]

# Database driver (e.g., psycopg2, pymysql) handles safe binding
# Result: "SELECT * FROM tb_users WHERE pk_user_id = 123"
# BUT: parameters are NEVER concatenated into SQL string
#      Database receives: statement + separate bindings
#      This prevents SQL injection
```

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
```

### Pre-Execution vs Post-Fetch Checks

**Pre-Execution (Fast Path)**
```python
# Rule evaluated before SQL execution
# Example: "user_role = 'admin'"

if context["user_role"] != "admin":
    raise GraphQLError("Unauthorized")  # Fail fast, no database query

# Only if permission passes, execute SQL
execute_sql(sql, bindings)
```

**Post-Fetch (Filter Results)**
```python
# Rule evaluated after fetching results
# Example: "user_id = authenticated_user_id OR user_role = 'admin'"

results = execute_sql(sql, bindings)

# Filter results based on permission rule
filtered_results = [
    row for row in results
    if row.user_id == context.authenticated_user_id or context.user_role == "admin"
]
```

### Compiled Permissions
From Topic 2.1 Phase 6, these are already compiled into efficient bytecode:

```
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
```

**Performance:** Pre-execution <0.1ms, Post-fetch ~0.5ms per result

---

## Stage 5: Execute SQL Template

### Database Connection
```python
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
```

### Query Execution Example

```sql
-- Pre-compiled template (from schema.compiled.json)
SELECT pk_user_id, email, created_at
FROM tb_users
WHERE pk_user_id = $1  -- Uses primary key index (very fast)
LIMIT 1

-- Result:

-- pk_user_id | email            | created_at
-- 123        | user@example.com | 2026-01-01 10:00:00
```

### Nested Queries (Relationships)
```python
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
```

### SQL Optimization Enabled by Compilation
```sql
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
```

**Performance:** Depends on data size
- Simple lookup: 5-20ms
- List query (100 items): 20-50ms
- Complex join: 50-200ms

---

## Stage 6: Format Response

### Fetch Results
```python
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
```

### Transform to GraphQL Response
```python
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
```

### Serialize to JSON
```python
import json

# Convert Python objects to JSON
json_response = json.dumps(response)

# Result:
# {"data": {"user": {"userId": 123, "email": "user@example.com", ...}}}
```

**Performance:** <5ms for typical response

---

## Stage 7: Return to Client

### HTTP Response
```http
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
```

### Error Response (If Something Failed)
```http
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
```

---

## Complete Execution Timeline

### Example Request
```graphql
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
```

### Execution Timeline (Detailed)

```
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
```

---

## Error Handling During Execution

### Type Errors
```python
# Request: { "userId": "not-a-number" }
# Expected: Int

try:
    value = int(request.variables["userId"])
except ValueError:
    raise GraphQLError(
        "Variable $userId of type Int! was not provided a valid Int value",
        extensions={"code": "BAD_USER_INPUT"}
    )
```

### Missing Required Parameters
```python
if "userId" not in request.variables:
    raise GraphQLError(
        "Variable $userId of required type Int! was not provided",
        extensions={"code": "BAD_USER_INPUT"}
    )
```

### Authorization Failures
```python
if not evaluate_permission(auth_rule, context):
    raise GraphQLError(
        "Unauthorized",
        extensions={"code": "FORBIDDEN"}
    )
```

### Database Errors
```python
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
```

### Error Response Format
```json
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
```

---

## Key Characteristics of FraiseQL Execution

### 1. Deterministic Performance
```
Every query has predictable performance:

- Lookup time: O(1)
- Parameter validation: O(P) where P = number of parameters
- Authorization: O(1) or O(1) per result
- SQL execution: Depends on database, but query is optimized
- Response formatting: O(N) where N = number of results

Total: Predictable and reproducible
```

### 2. No Query Interpretation
```
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
```

### 3. Automatic N+1 Prevention
```
Compile-time query analysis detects potential N+1 patterns:
❌ Bad pattern (would cause N+1):
   for user in users:
       orders = query_orders(user.id)  # N queries!

✅ Good pattern (batch loading):
   users = query_users()
   orders = query_orders_for_users([user.id for user in users])  # 1 query!

FraiseQL uses compiled templates that are already optimized.
```

### 4. Authorization Integrated
```
Permission checks happen at two points:

1. Pre-execution: Fast fail if not authorized
   Example: "user_role = 'admin'" → Check before SQL
2. Post-fetch: Filter results based on permission
   Example: "user_id = authenticated_user_id" → Filter after SQL

All compiled and optimized at build time.
```

---

## Comparison: FraiseQL vs Traditional GraphQL

### Apollo Server
```
Request → Parse (5ms) → Validate (3ms) →
Resolve User (2ms) → Execute query (20ms) →
Resolve Orders field (2ms) → Execute query (15ms) →
Format (3ms) → Total: ~50ms
```

### FraiseQL
```
Request → Lookup template (0.1ms) →
Validate params (0.5ms) → Check auth (<0.1ms) →
Execute SQL (35ms) → Format (1ms) → Total: ~37ms
```

**Result:** FraiseQL is ~26% faster because it skips interpretation overhead

---

## Real-World Example: E-Commerce Query

### Query
```graphql
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
```

### Execution
```python
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
```

---

## Performance Characteristics

### Query Latency
```
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
```

### Throughput
```
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
```

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
