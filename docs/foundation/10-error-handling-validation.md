# 2.5: Error Handling & Validation

## Overview

FraiseQL's error handling and validation strategy ensures predictable, safe query execution across compile-time and runtime boundaries. Unlike traditional GraphQL servers that discover errors at runtime, FraiseQL catches entire classes of errors during compilation, leaving only narrow categories of runtime errors (database failures, timeouts, authorization denials) to handle.

This topic explains:

- **Error hierarchy**: All 14 error types with classification (client vs server, retryable vs permanent)
- **Validation layers**: When errors are caught (schema compilation, query compilation, parameter binding, authorization, execution)
- **Error handling patterns**: How to handle errors in client applications, HTTP responses, and recovery strategies
- **Validation best practices**: Input validation, authorization enforcement, conflict detection

### Error Handling Architecture

```text
Authoring Layer          Compilation Layer        Runtime Layer
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ Python/TypeScript│    │ fraiseql-cli     │    │ fraiseql-server  │
│ schema.py        │    │ compile          │    │ GraphQL API      │
│                  │    │                  │    │                  │
│ VALIDATION PHASE │    │ VALIDATION PHASE │    │ VALIDATION PHASE │
│ - Type syntax    │    │ - Schema refs    │    │ - Auth rules     │
│ - Field names    │    │ - Relationships  │    │ - Parameter type │
│ - Decorators     │    │ - SQL generation │    │ - Resource refs  │
└────────┬─────────┘    └────────┬─────────┘    └────────┬─────────┘
         │                       │                       │
         └──────→ Errors         └──────→ Errors        └──────→ Errors
            (Failed)                (Failed)               (Failed)
```text

---

## Error Hierarchy

FraiseQL uses a unified error type that represents all possible failures. Errors are classified by:

1. **Source**: Where the error originated (GraphQL, Database, Authorization, etc.)
2. **Severity**: Client error (4xx) vs Server error (5xx)
3. **Retryability**: Whether the operation can be safely retried

### All 14 Error Types

| Error Type | Category | HTTP Status | Retryable | Cause |
|------------|----------|-------------|-----------|-------|
| **Parse** | GraphQL Client | 400 | ❌ | Invalid GraphQL syntax (malformed query/mutation) |
| **Validation** | GraphQL Client | 400 | ❌ | Query valid but semantically wrong (wrong field/type) |
| **UnknownField** | GraphQL Client | 400 | ❌ | Field doesn't exist on type |
| **UnknownType** | GraphQL Client | 400 | ❌ | Type doesn't exist in schema |
| **Database** | Server | 500 | ❌ | Database operation failed (constraint violation, etc.) |
| **ConnectionPool** | Server | 500 | ✅ | No available connections in pool |
| **Timeout** | Server | 408 | ✅ | Query exceeded execution timeout |
| **Cancelled** | Server | 408 | ✅ | Query cancelled (client disconnect or explicit) |
| **Authorization** | Client | 403 | ❌ | User lacks permission for operation/resource |
| **Authentication** | Client | 401 | ❌ | Invalid/missing/expired credentials |
| **NotFound** | Client | 404 | ❌ | Requested resource doesn't exist |
| **Conflict** | Client | 409 | ❌ | Operation conflicts with existing data |
| **Configuration** | Server | 500 | ❌ | Invalid server configuration |
| **Internal** | Server | 500 | ❌ | Unexpected internal error (rare) |

### Error Classification

**Client Errors (4xx):**

```text
Parse, Validation, UnknownField, UnknownType,
Authentication, Authorization, NotFound, Conflict
```text

User made a mistake. The same request will fail repeatedly. Example:

```graphql
# Query has unknown field 'usernam' instead of 'username'
query {
  user(id: 1) {
    usernam  # ← Parse error (4xx)
  }
}
```text

**Server Errors (5xx):**

```text
Database, ConnectionPool, Timeout, Cancelled,
Configuration, Internal
```text

System failure outside user control. May succeed if retried. Example:

```text
Database { message: "connection refused" }  # ← 5xx error
# May succeed if database recovers and request is retried
```text

**Retryable Errors (can be safely retried):**

```text
ConnectionPool, Timeout, Cancelled
```text

Safe to retry with exponential backoff. Examples:

- Connection pool exhausted → retry when connections available
- Query timeout → retry with possibly reduced complexity
- Client cancellation → external event, user can retry manually

---

## Validation Layers

### Layer 1: Authoring-Time Validation

Errors caught while writing Python/TypeScript schema definitions:

```python
# ✅ VALID: Proper type annotation
from fraiseql import type, field

@type
class User:
    id: int
    name: str

# ❌ INVALID: Invalid field type (caught by Python type checker)
@type
class User:
    id: int
    name: BadType  # ← Python type error (before compilation)
```text

**Tools:** Python type checking (`py`, `mypy`), TypeScript compiler

### Layer 2: Compilation-Time Validation

Errors caught by `fraiseql-cli compile schema.json`:

**Schema Reference Validation:**

```python
@type
class Post:
    id: int
    author: User  # ← Must exist and be a @type, not a Python class

# ❌ FAILS COMPILATION: Author references non-existent type
@type
class Post:
    id: int
    author: NonExistentUser

# Error: Compilation { message: "Unknown type 'NonExistentUser' referenced in Post.author" }
```text

**Relationship Validation:**

```sql
-- ✅ VALID: Foreign key exists and matches type definition
CREATE TABLE tb_post (
    pk_post_id BIGSERIAL PRIMARY KEY,
    fk_user_id BIGINT NOT NULL REFERENCES tb_user(pk_user_id)
);

-- ❌ INVALID: Foreign key points to non-existent column
CREATE TABLE tb_post (
    pk_post_id BIGSERIAL PRIMARY KEY,
    fk_user_id BIGINT NOT NULL REFERENCES tb_user(pk_nonexistent_id)
);

-- Error: Validation { message: "Foreign key fk_user_id references non-existent column tb_user.pk_nonexistent_id" }
```text

**SQL Generation Validation:**

```python
@type
class Post:
    id: int
    title: str
    # Compiler validates:
    # - Table tb_post exists in connected database
    # - Columns pk_post_id, title exist
    # - Type mapping (db int → GraphQL Int) is valid
    # If any validation fails, compilation errors
```text

### Layer 3: Request-Time Validation

Errors caught before query execution (parameter binding, authorization):

**Parameter Type Validation:**

```graphql
# Schema defines: user(id: Int!)
query {
  user(id: "abc") {  # ← String instead of Int
    name
  }
}

# Error: Validation { message: "Expected Int for parameter 'id', got String" }
```text

**Parameter Range Validation:**

```graphql
# Schema defines: users(limit: Int, offset: Int)
# Typical impl: limit [1, 10000], offset [0, ∞)
query {
  users(limit: 100000) {  # ← Exceeds maximum
    name
  }
}

# Error: Validation { message: "Parameter 'limit' must be ≤ 10000, got 100000" }
```text

**Authorization Validation:**

```rust
// Pre-execution: Check if user can execute mutation
let auth_result = check_authorization(
    user_id: "user_123",
    action: "write",           // What they want to do
    resource: "Post:456",      // What they want to access
    rules: &schema.auth_rules
);

match auth_result {
    Ok(()) => {
        // User has permission, execute query
    }
    Err(FraiseQLError::Authorization { .. }) => {
        // User lacks permission
        // Return 403 immediately without executing SQL
    }
}
```text

### Layer 4: Execution-Time Validation

Errors caught during or after SQL execution:

**Conflict Detection:**

```sql
-- ✅ Query succeeds
INSERT INTO tb_user (username) VALUES ('alice');

-- ❌ Fails at execution: User 'alice' already exists
INSERT INTO tb_user (username) VALUES ('alice');

-- Error: Database {
--   message: "duplicate key value violates unique constraint \"uc_user_username\"",
--   sql_state: Some("23505")  -- PostgreSQL unique violation code
-- }
```text

**Post-Fetch Authorization (visibility filtering):**

```rust
// Execution succeeds, but post-fetch rules may remove rows:

// Query: { posts { id, title, secret } }
// User permission: "read" on "Post" but NOT "Post.secret"

// SQL executes and returns: [
//   { id: 1, title: "Public", secret: "sensitive" },
//   { id: 2, title: "Draft", secret: "draft-data" }
// ]

// Post-fetch filtering removes 'secret' field from all rows:
// Result: [
//   { id: 1, title: "Public" },
//   { id: 2, title: "Draft" }
// ]
// No error - silently drops unauthorized fields
```text

**Timeout Detection:**

```rust
// Query execution exceeds configured timeout (default 30s)
let result = tokio::time::timeout(
    Duration::from_secs(30),
    execute_sql(&query)
).await;

match result {
    Ok(Ok(rows)) => Ok(rows),
    Ok(Err(db_error)) => Err(FraiseQLError::Database { .. }),
    Err(_) => Err(FraiseQLError::Timeout {
        timeout_ms: 30000,
        query: Some(truncate_query(&query, 200))
    })
}
```text

---

## GraphQL Error Response Format

FraiseQL follows the GraphQL specification for error responses. All errors are returned as JSON with error codes, locations, and path information.

### Single Error Response

```json
HTTP/1.1 400 Bad Request
Content-Type: application/json

{
  "errors": [
    {
      "message": "Unknown field 'usernam' on type 'User'",
      "code": "UNKNOWN_FIELD",
      "status": 400,
      "locations": [
        {
          "line": 3,
          "column": 5
        }
      ],
      "path": ["user", "usernam"]
    }
  ]
}
```text

### Multiple Errors Response

```json
HTTP/1.1 400 Bad Request
Content-Type: application/json

{
  "errors": [
    {
      "message": "Unknown field 'usernam' on type 'User'",
      "code": "UNKNOWN_FIELD",
      "path": ["user", "usernam"],
      "locations": [{"line": 3, "column": 5}]
    },
    {
      "message": "Expected Int for parameter 'id', got String",
      "code": "GRAPHQL_VALIDATION_FAILED",
      "path": ["user"],
      "locations": [{"line": 2, "column": 8}]
    }
  ]
}
```text

### Database Error Response

```json
HTTP/1.1 500 Internal Server Error
Content-Type: application/json

{
  "errors": [
    {
      "message": "Database error: duplicate key value violates unique constraint",
      "code": "DATABASE_ERROR",
      "status": 500,
      "extensions": {
        "sql_state": "23505",
        "retryable": false
      }
    }
  ]
}
```text

### Authorization Error Response

```json
HTTP/1.1 403 Forbidden
Content-Type: application/json

{
  "errors": [
    {
      "message": "Authorization error: insufficient permissions",
      "code": "FORBIDDEN",
      "status": 403,
      "extensions": {
        "action": "read",
        "resource": "Post:456",
        "reason": "user_role is 'viewer', requires 'editor' or above"
      }
    }
  ]
}
```text

---

## Error Handling Strategies

### Strategy 1: Fail Fast (Default)

Return first error immediately, stop processing:

```rust
// Schema: query { posts { id, title, author { name } } }
// Query execution order:
// 1. Validate GraphQL syntax       → ✅ Pass
// 2. Validate query structure      → ✅ Pass
// 3. Check authorization          → ❌ FAIL → Return 403 immediately
// 4. Execute SQL                   → (skipped)
// 5. Format response               → (skipped)

// Response:
// HTTP 403 Forbidden
// { "errors": [{"message": "...", "code": "FORBIDDEN"}] }
```text

**When to use:** Default for all queries. Safe and predictable.

### Strategy 2: Partial Execution with Field-Level Errors

Return available data with errors for failed fields:

```graphql
# Query with nested fields
query {
  posts {
    id           # ✅ Succeeds
    title        # ✅ Succeeds
    author {
      name       # ❌ Authorization denied on this field
      email      # (not fetched due to error above)
    }
  }
}
```text

**Response:**

```json
{
  "data": {
    "posts": [
      {
        "id": 1,
        "title": "GraphQL Guide",
        "author": null  // Authorization error below
      }
    ]
  },
  "errors": [
    {
      "message": "Authorization error: cannot read author.name",
      "code": "FORBIDDEN",
      "path": ["posts", 0, "author", "name"]
    }
  ]
}
```text

**When to use:** When some fields are public and others require permission. Provides better UX.

### Strategy 3: Retry with Exponential Backoff

For retryable errors (ConnectionPool, Timeout, Cancelled):

```python
import asyncio
import random

async def execute_with_retry(query, max_attempts=3):
    for attempt in range(1, max_attempts + 1):
        try:
            result = await client.execute(query)
            return result
        except RetryableError as e:
            if attempt >= max_attempts:
                raise

            # Exponential backoff with jitter
            backoff = 2 ** (attempt - 1)  # 1s, 2s, 4s
            jitter = random.uniform(0, backoff * 0.1)
            wait_time = backoff + jitter

            print(f"Attempt {attempt} failed: {e}")
            print(f"Retrying in {wait_time:.2f}s...")
            await asyncio.sleep(wait_time)

# Usage
result = await execute_with_retry(query)
```text

**Error types to retry:**

- `ConnectionPool`: Wait for available connection
- `Timeout`: May succeed with longer timeout or simpler query
- `Cancelled`: Query was interrupted, user can retry

**Error types NOT to retry:**

- `Parse`, `Validation`: Same query will fail identically
- `Database` (constraint violation): Data hasn't changed
- `Authorization`: User permissions unchanged

### Strategy 4: Graceful Degradation

Provide fallback behavior when queries fail:

```typescript
// Original query with analytics
const analyticsQuery = `
  query {
    sales {
      date
      revenue
      costs
      margin
    }
  }
`;

// Fallback query (simpler, fewer fields, faster)
const fallbackQuery = `
  query {
    sales {
      date
      revenue
    }
  }
`;

async function getAnalytics() {
  try {
    // Try full analytics
    const result = await client.query(analyticsQuery);
    return result;
  } catch (error) {
    if (error.code === 'TIMEOUT' ||
        error.code === 'CONNECTION_POOL_ERROR') {
      // Try simpler query
      console.warn('Analytics timeout, using simplified view');
      const fallback = await client.query(fallbackQuery);
      return fallback;
    }
    // Other errors: propagate
    throw error;
  }
}
```text

---

## Input Validation Best Practices

### Practice 1: Validate at Entry Points

Validate all user input before executing SQL:

```python
from fraiseql import type, field
from typing import Annotated

# Define input type with validation rules
@type
class UserInput:
    username: Annotated[str, "length: 1-50, pattern: ^[a-zA-Z0-9_]+$"]
    email: Annotated[str, "pattern: ^[^@]+@[^@]+$"]
    age: Annotated[int, "min: 13, max: 150"]

# Validation happens in compilation and at request time
@mutation
def create_user(input: UserInput) -> User:
    # By this point:
    # - input.username is guaranteed valid
    # - input.email is guaranteed valid email format
    # - input.age is in [13, 150]
    return db.insert_user(input)
```text

**Validation rules are compiled into SQL or application logic:**

```sql
-- PostgreSQL constraint (enforced by database)
CREATE TABLE tb_user (
    pk_user_id BIGSERIAL PRIMARY KEY,
    username VARCHAR(50) NOT NULL,
    email VARCHAR(255) NOT NULL,
    age INT NOT NULL CHECK (age >= 13 AND age <= 150),
    CONSTRAINT uc_user_username UNIQUE (username),
    CONSTRAINT uc_user_email UNIQUE (email)
);

-- If insert violates constraint:

-- Error: Database { sql_state: "23514" } (CHECK constraint violation)
```text

### Practice 2: List-Size Limits

Prevent queries from returning excessive data:

```graphql
# Schema definition with limits
type Query {
  users(
    limit: Int = 20
    offset: Int = 0
  ): [User!]!
}

# Validation rules (compiled):
# limit: [1, 10000]       # Can't fetch more than 10k at once
# offset: [0, ∞)          # Can start at any position

# ✅ Valid request
query {
  users(limit: 100, offset: 200) { id name }
}

# ❌ Invalid request: limit too high
query {
  users(limit: 100000, offset: 0) { id name }
}
# Error: Validation { message: "Parameter 'limit' must be ≤ 10000" }
```text

**Implementation:**

```rust
// Runtime parameter validation (before SQL execution)
fn validate_parameters(params: &QueryParams) -> Result<()> {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);

    if !(1..=10000).contains(&limit) {
        return Err(FraiseQLError::validation(
            format!("limit must be 1-10000, got {}", limit)
        ));
    }

    if offset < 0 {
        return Err(FraiseQLError::validation(
            "offset must be ≥ 0"
        ));
    }

    Ok(())
}
```text

### Practice 3: String Sanitization (Implicit via Parameterization)

FraiseQL prevents SQL injection by never interpolating user input into SQL strings:

```python
# ❌ UNSAFE (don't do this):
query = f"SELECT * FROM users WHERE name = '{user_input}'"
# If user_input = "' OR '1'='1", this becomes:
# SELECT * FROM users WHERE name = '' OR '1'='1'  <- SQL injection!

# ✅ SAFE (what FraiseQL does automatically):
query = "SELECT * FROM users WHERE name = ?"
params = [user_input]
# Parameterized query - user_input is treated as pure data, never as SQL
```text

FraiseQL handles this internally:

```rust
// User provides:
query_params: { userId: "123\"; DROP TABLE users; --" }

// FraiseQL generates:
// SQL: SELECT * FROM posts WHERE pk_user_id = $1
// Params: ["123\"; DROP TABLE users; --"]
// The harmful string is treated as pure data, not executed
```text

### Practice 4: Enumeration over Free Text

Use enums to restrict input to valid values:

```python
from enum import Enum
from fraiseql import type, field

class UserRole(str, Enum):
    ADMIN = "admin"
    EDITOR = "editor"
    VIEWER = "viewer"

@type
class UpdateUserInput:
    id: int
    role: UserRole  # ← Restricted to 3 valid values

# Valid requests:
mutation {
  updateUser(input: {id: 1, role: ADMIN})  # ✅
}

# Invalid requests:
mutation {
  updateUser(input: {id: 1, role: "superuser"})  # ❌
  # Error: Validation { message: "Expected one of [admin, editor, viewer]" }
}
```text

---

## Authorization Patterns

### Pattern 1: Role-Based Access Control (RBAC)

Control access based on user role:

```python
from fraiseql import type, field, permission

@type
class Post:
    id: int
    title: str

    @permission("read", roles=["viewer", "editor", "admin"])
    secret: str

    @permission("write", roles=["editor", "admin"])
    def update(self, title: str) -> Post:
        pass

# User with role='viewer': Can read post.title, but not post.secret
# User with role='editor': Can read and write everything
# User with role='admin': Can do anything
```text

**Runtime enforcement:**

```rust
// When resolving 'secret' field
match user.role {
    Role::Viewer => Err(FraiseQLError::unauthorized("viewers cannot read post.secret")),
    Role::Editor | Role::Admin => Ok(secret_value),
}
```text

### Pattern 2: Ownership-Based Access Control

Control access based on data ownership:

```python
@type
class Post:
    id: int
    title: str

    @permission(
        "read",
        rule="owner_id == current_user_id OR is_published"
    )
    content: str

# Only the post's author or readers of published posts can read content
```text

**SQL with authorization:**

```sql
-- Query: user_123 requests unpublished post_456
-- Rule: owner_id == $1 OR is_published

-- Generated SQL:
SELECT
  pk_post_id, title,
  CASE
    WHEN owner_id = $1 OR is_published THEN content
    ELSE NULL  -- Unauthorized
  END AS content
FROM tb_post
WHERE pk_post_id = 456
-- User 456 can only see content if they own it or it's published
```text

### Pattern 3: Attribute-Based Access Control (ABAC)

Control access based on user attributes, resource attributes, and context:

```python
@type
class Document:
    id: int
    title: str

    @permission(
        "read",
        rule="""
        user.department == resource.department
        AND (resource.classification < user.clearance
             OR resource.owner_id == user.id)
        """
    )
    content: str

# User can read if:
# 1. They're in the same department, AND
# 2. Either the document is less classified than their clearance, OR they own it
```text

---

## Common Error Scenarios and Recovery

### Scenario 1: User Tries to Access Unauthorized Resource

```graphql
# User 'alice' with role 'viewer' requests:
query {
  post(id: 123) {
    title       # ✅ Allowed for viewers
    secret      # ❌ Denied for viewers
  }
}
```text

**Response:**

```json
{
  "data": {
    "post": {
      "title": "Public Post",
      "secret": null
    }
  },
  "errors": [
    {
      "message": "Authorization error: viewers cannot read post.secret",
      "code": "FORBIDDEN",
      "path": ["post", "secret"]
    }
  ]
}
```text

**Client recovery:**

```typescript
try {
  const result = await client.query(query);

  if (result.errors) {
    // Filter errors by type
    const authErrors = result.errors.filter(e => e.code === 'FORBIDDEN');
    const otherErrors = result.errors.filter(e => e.code !== 'FORBIDDEN');

    if (authErrors.length > 0 && result.data) {
      // Partially available - show what we have
      showData(result.data);
      showNotification(`Some fields unavailable (${authErrors.length} access denied)`);
    } else {
      // Complete failure
      throw new Error(otherErrors[0].message);
    }
  } else {
    showData(result.data);
  }
} catch (error) {
  showError(error.message);
}
```text

### Scenario 2: Database Connection Lost

```text
Database operation fails with ConnectionPool error
↓
Retryable: Yes (eventually, connections will become available)
↓
Client strategy: Retry with exponential backoff
```text

**Implementation:**

```rust
pub async fn execute_with_connection_retry(
    query: &str,
    max_retries: u32,
) -> Result<Vec<Row>> {
    let mut attempt = 0;

    loop {
        match execute_sql(query).await {
            Ok(rows) => return Ok(rows),
            Err(FraiseQLError::ConnectionPool { .. }) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(FraiseQLError::ConnectionPool {
                        message: format!("Failed after {} retries", max_retries),
                    });
                }

                // Exponential backoff: 100ms, 200ms, 400ms, ...
                let wait_ms = 100 * 2_u64.pow(attempt - 1);
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }
            Err(e) => return Err(e),  // Non-retryable error
        }
    }
}
```text

### Scenario 3: Query Exceeds Timeout

```text
Query execution exceeds 30s limit
↓
Error: Timeout { timeout_ms: 30000 }
↓
Retryable: Possibly (with simpler query or raised timeout)
↓
Client strategy: Retry with reduced complexity
```text

**Implementation:**

```graphql
# Original query (expensive analytics)
query FullReport {
  sales(year: 2024) {
    id
    date
    revenue
    costs
    margin
    region
    product_category
    customer_segment
  }
}

# Fallback query (reduced dimensions)
query QuickReport {
  sales(year: 2024) {
    date
    revenue
    costs
  }
}
```text

```typescript
async function getReport() {
  try {
    console.log("Fetching full report...");
    return await client.query(FullReport);
  } catch (error) {
    if (error.code === 'TIMEOUT') {
      console.warn('Full report timeout, using simplified view');
      return await client.query(QuickReport);
    }
    throw error;
  }
}
```text

### Scenario 4: Data Constraint Violation

```text
User tries to create duplicate username
↓
INSERT violates UNIQUE constraint
↓
Error: Database { sql_state: "23505" }
↓
Retryable: No (constraint violation, not transient)
↓
Client strategy: Show error, ask user for different username
```text

**Implementation:**

```typescript
async function createUser(username: string, email: string) {
  try {
    const result = await client.mutate(CreateUserMutation, {
      username,
      email
    });
    return result;
  } catch (error) {
    if (error.code === 'DATABASE_ERROR') {
      if (error.extensions.sql_state === '23505') {
        // Unique constraint violation
        throw new UserFriendlyError(
          "This username is already taken. Please try another."
        );
      }
    }
    throw error;
  }
}
```text

---

## Validation in Different Scenarios

### Scenario A: Simple Read Query

```graphql
query {
  user(id: 1) {
    name
  }
}
```text

**Validation order:**

1. ✅ Parse GraphQL syntax
2. ✅ Validate 'user' query exists
3. ✅ Validate 'id' parameter is Int
4. ✅ Validate 'name' field exists on User type
5. ✅ Check authorization (user can read User.name)
6. ✅ Execute SQL: SELECT name FROM tb_user WHERE pk_user_id = $1
7. ✅ Check authorization (post-fetch, if any field-level rules)
8. ✅ Format JSON response
9. ✅ Return result

### Scenario B: Mutation with Input Validation

```graphql
mutation {
  createPost(input: {
    title: "New Post"
    content: "Content here"
    tags: ["rust", "graphql"]
  }) {
    id
    title
  }
}
```text

**Validation order:**

1. ✅ Parse GraphQL syntax
2. ✅ Validate 'createPost' mutation exists
3. ✅ Validate 'input' structure matches PostInput type:
   - title: String (length: 1-500) ✅
   - content: String (length: 1-10000) ✅
   - tags: [String] (max items: 10) ✅
4. ✅ Check authorization (user can write posts)
5. ✅ Execute SQL: INSERT INTO tb_post (title, content) VALUES ($1, $2)
6. ✅ Validate FOREIGN KEY constraints (tags table exists, etc.)
7. ✅ If constraint violation: Database error (non-retryable)
8. ✅ Format response with created post
9. ✅ Return result

### Scenario C: Complex Analytics Query

```graphql
query {
  salesByRegion(limit: 100, year: 2024) {
    region
    total
    items {
      id
      date
      revenue
    }
  }
}
```text

**Validation order:**

1. ✅ Parse GraphQL syntax
2. ✅ Validate 'salesByRegion' query exists
3. ✅ Validate parameters: limit (1-10000), year (1900-2100)
4. ✅ Check authorization (user can read analytics)
5. ✅ Check authorization (post-fetch filtering on sensitive fields)
6. ✅ Execute Arrow Flight streaming query (large result set)
7. ✅ If timeout: Return Timeout error after 30s
8. ✅ If connection pool exhausted: Return ConnectionPool error (retryable)
9. ✅ Format Arrow response (binary, 5-10x smaller than JSON)
10. ✅ Return result

---

## Validation Best Practices Checklist

- [ ] **Fail early**: Catch errors at compile time when possible (types, references)
- [ ] **Validate input types**: Ensure parameters match expected types before SQL execution
- [ ] **Enforce range limits**: Set min/max for numeric inputs, length for strings
- [ ] **Use enums**: Restrict categorical input to fixed set of valid values
- [ ] **Check authorization first**: Deny access before executing expensive queries
- [ ] **Classify errors**: Distinguish client errors (4xx) from server errors (5xx)
- [ ] **Retry responsibly**: Only retry retryable errors (ConnectionPool, Timeout, Cancelled)
- [ ] **Expose error codes**: Let clients distinguish error types by code, not message text
- [ ] **Log errors consistently**: Include error code, user ID, resource, timestamp
- [ ] **Don't leak internals**: Error messages to clients should not expose DB schema or system details
- [ ] **Track error rates**: Monitor error types and rates to detect issues early

---

## Real-World Error Handling Example

### E-Commerce: Order Creation with Full Validation

```graphql
mutation {
  createOrder(input: {
    userId: 123
    items: [
      { productId: 1, quantity: 2 }
      { productId: 2, quantity: 1 }
    ]
    shippingAddress: "123 Main St"
  }) {
    orderId
    status
    total
  }
}
```text

**Comprehensive error handling:**

```rust
pub async fn create_order(
    input: CreateOrderInput,
    user_id: i64,
) -> Result<Order> {
    // LAYER 1: Input validation (before DB access)
    validate_order_input(&input)?;  // Errors: ValidationError (4xx)

    // LAYER 2: Authorization (before DB access)
    check_user_exists(user_id).await?;  // Errors: NotFoundError (4xx)
    check_user_can_order(user_id).await?;  // Errors: AuthorizationError (4xx)

    // LAYER 3: Resource validation (before transaction)
    let mut total_price = 0.0;
    for item in &input.items {
        let product = find_product(item.product_id).await?;
        // Errors: NotFoundError (4xx), DatabaseError (5xx)

        if !product.in_stock(item.quantity) {
            return Err(FraiseQLError::Conflict {
                message: format!(
                    "Product {} only has {} units available",
                    product.id, product.stock_count
                ),
            });
        }

        total_price += product.price * item.quantity as f64;
    }

    // LAYER 4: Transaction execution (with retries for transient errors)
    loop {
        match execute_order_transaction(user_id, &input, total_price).await {
            Ok(order) => return Ok(order),

            Err(FraiseQLError::Timeout { .. }) => {
                // Transient: Retry
                warn!("Order creation timeout, retrying...");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            Err(FraiseQLError::ConnectionPool { .. }) => {
                // Transient: Retry
                warn!("Connection pool exhausted, retrying...");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }

            Err(FraiseQLError::Database { sql_state, .. }) => {
                // Non-transient: Classify and return
                match sql_state.as_deref() {
                    Some("23505") => {  // Unique constraint
                        return Err(FraiseQLError::Conflict {
                            message: "Order already exists for this user at this time"
                                .to_string(),
                        });
                    }
                    Some("23503") => {  // Foreign key
                        return Err(FraiseQLError::NotFound {
                            resource_type: "Product".to_string(),
                            identifier: format!("{}", input.items[0].product_id),
                        });
                    }
                    _ => return Err(FraiseQLError::Database {
                        message: "Failed to create order".to_string(),
                        sql_state,
                    }),
                }
            }

            Err(e) => return Err(e),  // Other errors: propagate
        }
    }
}
```text

**GraphQL response scenarios:**

Success:

```json
{
  "data": {
    "createOrder": {
      "orderId": "ord_12345",
      "status": "pending",
      "total": 99.99
    }
  }
}
```text

Validation error:

```json
{
  "errors": [{
    "message": "Validation error: quantity must be 1-1000",
    "code": "GRAPHQL_VALIDATION_FAILED",
    "path": ["createOrder"]
  }]
}
```text

Authorization error:

```json
{
  "errors": [{
    "message": "Authorization error: user is not active",
    "code": "FORBIDDEN",
    "extensions": {
      "action": "create",
      "resource": "Order"
    }
  }]
}
```text

Conflict error:

```json
{
  "errors": [{
    "message": "Conflict: Product 42 only has 5 units available",
    "code": "CONFLICT"
  }]
}
```text

Database error (transient, should retry):

```json
{
  "errors": [{
    "message": "Query timeout after 30000ms",
    "code": "TIMEOUT",
    "extensions": {
      "retryable": true
    }
  }]
}
```text

---

## Related Topics

- **2.1: Compilation Pipeline** - How errors are caught at compile time
- **2.2: Query Execution Model** - How errors occur during runtime execution
- **2.4: Type System** - Type validation and inference
- **2.6: Compiled Schema Structure** - Schema definition with error metadata
- **2.7: Performance Characteristics** - Impact of validation on performance

---

## Summary

FraiseQL's error handling strategy is **multi-layered**:

1. **Authoring time**: Python/TypeScript type checking prevents syntax errors
2. **Compilation time**: `fraiseql-cli` validates schema structure and relationships
3. **Request time**: Parameter types, ranges, and authorization checked before SQL
4. **Execution time**: Database constraints, timeouts, and post-fetch rules applied

Errors are **classified** (client vs server, retryable vs permanent), **exposed** as structured JSON with error codes, and **recoverable** through intelligent retry logic.

Validation follows best practices: **fail early** (catch at compilation if possible), **validate at boundaries** (user input only), **use strong types** (enums over strings), and **check authorization first** (before expensive operations).

With these patterns, FraiseQL applications can handle errors confidently, providing better user experience through targeted error messages and graceful degradation.
