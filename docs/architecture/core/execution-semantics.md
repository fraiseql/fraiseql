<!-- Skip to main content -->
---
title: FraiseQL Execution Semantics: Query, Mutation, and Subscription Runtime Behavior
description: 1. [Executive Summary](#executive-summary)
keywords: ["design", "query-execution", "scalability", "performance", "patterns", "mutation", "security"]
tags: ["documentation", "reference"]
---

# FraiseQL Execution Semantics: Query, Mutation, and Subscription Runtime Behavior

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Framework runtime engineers, SDK developers, optimization specialists

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Query Execution Semantics](#1-query-execution-semantics)
3. [Mutation Execution Semantics](#2-mutation-execution-semantics)
4. [Subscription Execution Semantics](#3-subscription-execution-semantics)
5. [Execution Guarantees & Trade-offs](#4-execution-guarantees--trade-offs)
6. [Streaming & Pagination](#5-streaming--pagination)
7. [Summary: Execution Flow Diagram](#6-summary-execution-flow-diagram)

---

## Executive Summary

FraiseQL execution semantics define the precise runtime behavior of queries, mutations, and subscriptions. The runtime receives a **compiled schema** (fully optimized, database-specific, authorization-aware execution plans) and executes deterministically without interpretation.

**Core principle**: No interpretation at runtime. Everything is compiled; execution is a straightforward state machine following pre-computed plans.

**Three execution patterns:**

1. **Query Execution** — Read-only, deterministic, cacheable
2. **Mutation Execution** — Write-only, atomic, transactional
3. **Subscription Execution** — Event-driven, ordered, durable

---

## 1. Query Execution Semantics

### 1.1 Query Execution Phases

Every query follows a deterministic five-phase execution model:

```text
<!-- Code example in TEXT -->
 Request Parsing & Validation
    ↓
 Authorization & Context Binding
    ↓
 Parameter Binding & SQL Preparation
    ↓
 Database Execution
    ↓
 Response Transformation & Streaming
```text
<!-- Code example in TEXT -->

### 1.2 Phase 1: Request Parsing & Validation

**Input**: GraphQL query string (may be pre-compiled or sent at request time)

```graphql
<!-- Code example in GraphQL -->
query GetUserPosts($userId: ID!, $limit: Int) {
  user(id: $userId) {
    id
    name
    posts(first: $limit) {
      id
      title
    }
  }
}
```text
<!-- Code example in TEXT -->

**Parsing tasks:**

1. **Query field validation:**
   - Verify `user` field exists in Query type ✅
   - Verify `posts` field exists in User type ✅
   - Verify all fields are readable (not mutations) ✅

2. **Argument validation:**
   - Verify `id` argument required, value provided ✅
   - Verify `first` argument optional, type is Int ✅
   - Verify argument values are correct type ✅

3. **Fragment validation:**
   - Resolve fragment definitions
   - Check for circular fragment spreads
   - Verify fragment type conditions

4. **Variable validation:**
   - Check variable types match declared types
   - Verify required variables provided
   - Check default values

**If validation fails:**

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Field 'unknownField' doesn't exist on type 'User'",
      "code": "E_BINDING_UNKNOWN_FIELD_202",
      "locations": [{ "line": 5, "column": 10 }],
      "extensions": {
        "category": "BINDING_ERROR",
        "retryable": false,
        "remediable": true,
        "suggestion": "Did you mean 'email'?"
      }
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

### 1.3 Phase 2: Authorization & Context Binding

**Input**: Validated query + User context (user ID, roles, permissions)

**Authorization checks:**

```python
<!-- Code example in Python -->
# Check query-level authorization:
# "Can this user execute this query at all?"

# Check field-level authorization:
# "Can this user access each field?"

# Example:
query {
  user(id: "user-123") {
    id              # Public, anyone can read
    email           # @authorize(rule="owner_or_admin") - check binding
    admin_notes     # @authorize(rule="admin_only") - check binding
  }
}
```text
<!-- Code example in TEXT -->

**Authorization evaluation (compile-time generated SQL WHERE clauses):**

```sql
<!-- Code example in SQL -->
-- SELECT ... FROM v_user
-- WHERE
--   (authorization rules)
--   AND current_user_id = 'user-456'  (user context)
--   AND current_user_role IN ('admin', 'user')  (role context)
```text
<!-- Code example in TEXT -->

**If authorization fails:**

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Access denied: You don't have permission to view this field",
      "code": "E_AUTH_PERMISSION_401",
      "extensions": {
        "category": "AUTHORIZATION_ERROR",
        "retryable": false,
        "remediable": false,
        "reason": "You must be the user owner or an admin to view email"
      }
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

**If authorization succeeds:**

- User context (user ID, roles) is bound to SQL parameters
- Authorization rules become WHERE clauses
- Field-level masking rules recorded for response transformation

### 1.4 Phase 3: Parameter Binding & SQL Preparation

**Input**: Validated query + Authorized user context + Variables

**Steps:**

1. **Load compiled SQL plan:**

   ```rust
<!-- Code example in RUST -->
   let sql_plan = compiled_schema.queries.get("GetUserPosts")?;
   // Already optimized from compilation phase
   ```text
<!-- Code example in TEXT -->

2. **Bind variables to SQL parameters:**

   ```rust
<!-- Code example in RUST -->
   let mut params = Vec::new();
   params.push(sql_param("user_id", variables["userId"]));
   params.push(sql_param("limit", variables.get("limit").unwrap_or(20)));
   params.push(sql_param("current_user_id", user_context.id));
   // Bind all authorization context
   ```text
<!-- Code example in TEXT -->

3. **Resolve dynamic values:**

   ```rust
<!-- Code example in RUST -->
   // Some fields may have runtime defaults:
   // @field(default=NOW()) → Bind current timestamp
   // @field(default=current_user_id) → Bind from context
   ```text
<!-- Code example in TEXT -->

4. **Type coercion:**

   ```rust
<!-- Code example in RUST -->
   // Convert GraphQL types to database types:
   // ID → UUID or String (depending on database)
   // DateTime → TIMESTAMP (depending on database)
   // JSON → JSONB (PostgreSQL) or JSON (MySQL)
   ```text
<!-- Code example in TEXT -->

5. **Prepared statement execution:**

   ```rust
<!-- Code example in RUST -->
   // Use prepared statement for safety and performance:
   let prepared = db.prepare(sql_plan.query)?;
   prepared.query(params)?
   ```text
<!-- Code example in TEXT -->

**If parameter binding fails:**

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Invalid parameter: userId must be a valid UUID",
      "code": "E_VALIDATION_INVALID_TYPE_103",
      "extensions": {
        "parameter": "userId",
        "expected": "UUID",
        "received": "invalid-uuid-string"
      }
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

### 1.5 Phase 4: Database Execution

**Input**: Prepared statement + Parameters + Execution plan

**Execution strategy:**

```rust
<!-- Code example in RUST -->
async fn execute_query(
    prepared: PreparedStatement,
    params: Parameters,
    timeout: Duration,
) -> Result<QueryResults> {
    // Set execution timeout
    let execution = tokio::time::timeout(timeout, async {
        // Execute with connection from pool
        db_pool.query_one(&prepared, &params).await
    });

    match execution {
        Ok(Ok(rows)) => {
            // Success: Process rows
            Ok(rows)
        }
        Ok(Err(db_err)) => {
            // Database error (constraint violation, etc.)
            Err(map_database_error(db_err))
        }
        Err(_) => {
            // Timeout
            Err(QueryTimeoutError)
        }
    }
}
```text
<!-- Code example in TEXT -->

**Query execution guarantees:**

1. **Atomicity:** Query sees consistent snapshot (READ_COMMITTED or SERIALIZABLE)
2. **Determinism:** Same inputs → Same outputs (no race conditions)
3. **Isolation:** Query doesn't see concurrent mutations (isolation level dependent)
4. **Timeout protection:** Query fails if exceeds timeout (default 30 seconds)
5. **Memory limits:** Query fails if result set too large (default 100MB)

**Database errors mapped to error codes:**

```rust
<!-- Code example in RUST -->
// PostgreSQL error → FraiseQL error code
match pg_error.code {
    "57014" => E_DB_QUERY_CANCELLED_305,      // Query cancelled
    "58030" => E_DB_DISK_FULL_310,            // Disk full
    "DEADLOCK" => E_DB_DEADLOCK_311,         // Deadlock
    "TIMEOUT" => E_DB_QUERY_TIMEOUT_302,     // Query timeout
    "CONNECTION_FAILURE" => E_DB_CONNECTION_FAILED_301,
    _ => E_DB_UNKNOWN_ERROR_399
}
```text
<!-- Code example in TEXT -->

**If database execution fails:**

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Query timeout: Execution exceeded 30 seconds",
      "code": "E_DB_QUERY_TIMEOUT_302",
      "extensions": {
        "category": "DATABASE_ERROR",
        "retryable": true,
        "remediable": false,
        "advice": "Query too complex. Consider adding filters or pagination."
      }
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

### 1.6 Phase 5: Response Transformation & Streaming

**Input**: Database results (rows, arrays, JSON)

**Transformation steps:**

1. **Deserialize database values:**

   ```rust
<!-- Code example in RUST -->
   // JSONB → Rust struct
   // TIMESTAMP → DateTime
   // UUID → String
   ```text
<!-- Code example in TEXT -->

2. **Apply field-level masking:**

   ```rust
<!-- Code example in RUST -->
   // Fields with @authorize rules:
   // If user not authorized → Set field to NULL

   for field in response_fields {
       if !field.user_authorized {
           field.value = null;
       }
   }
   ```text
<!-- Code example in TEXT -->

3. **Transform to GraphQL response format:**

   ```rust
<!-- Code example in RUST -->
   // Rust struct → JSON (GraphQL response)
   // Rename fields (database column names → GraphQL field names)
   // Nest objects (flatten JSONB into nested objects)
   ```text
<!-- Code example in TEXT -->

4. **Stream response (if large):**

   ```rust
<!-- Code example in RUST -->
   // For large result sets, stream response:
   // Chunk data into 64KB packets
   // Send chunks as soon as available
   // Allow client to start processing while query still executing
   ```text
<!-- Code example in TEXT -->

5. **Include execution metadata:**

   ```rust
<!-- Code example in RUST -->
   {
     "data": {...},
     "extensions": {
       "execution": {
         "duration_ms": 45,
         "query_plan": "cached",
         "cache_hit": false,
         "rows_returned": 20,
         "rows_scanned": 1000
       }
     }
   }
   ```text
<!-- Code example in TEXT -->

**Example response transformation:**

```rust
<!-- Code example in RUST -->
// Database result (raw):
Row {
    pk_user: 1,
    id: UUID("f47ac10b-58cc-4372-a567-0e02b2c3d479"),
    name: "Alice",
    email: None,  // Masked (user not authorized)
    posts_json: [
        {"id": UUID(...), "title": "Post 1"},
        {"id": UUID(...), "title": "Post 2"}
    ]
}

// Transformed to GraphQL:
{
    "data": {
        "user": {
            "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
            "name": "Alice",
            "email": null,  // Masked
            "posts": [
                {"id": "...", "title": "Post 1"},
                {"id": "...", "title": "Post 2"}
            ]
        }
    },
    "extensions": {
        "execution": {
            "duration_ms": 12
        }
    }
}
```text
<!-- Code example in TEXT -->

### 1.7 Query Execution Error Handling

**Execution errors can occur at any phase:**

| Phase | Error | Code | Retryable |
|-------|-------|------|-----------|
| 1 | Query syntax invalid | E_BINDING_* | No |
| 2 | User not authorized | E_AUTH_* | No |
| 3 | Parameter invalid | E_VALIDATION_* | No |
| 4 | Database timeout | E_DB_QUERY_TIMEOUT_302 | Yes |
| 4 | Database connection failed | E_DB_CONNECTION_FAILED_301 | Yes |
| 4 | Database disk full | E_DB_DISK_FULL_310 | No |
| 5 | Response too large | E_DB_RESULT_TOO_LARGE_312 | No |

**Partial success (field-level errors):**

FraiseQL allows partial results when field-level errors occur:

```graphql
<!-- Code example in GraphQL -->
query GetUsers {
  users {
    id        # ✅ Success
    name      # ✅ Success
    email     # ⚠️ Error (permission denied)
    ssn       # ⚠️ Error (field doesn't exist)
  }
}
```text
<!-- Code example in TEXT -->

**Response with field errors:**

```json
<!-- Code example in JSON -->
{
  "data": {
    "users": [
      {
        "id": "user-1",
        "name": "Alice",
        "email": null,  // Error: not authorized
        "ssn": null     // Error: field not found
      }
    ]
  },
  "errors": [
    {
      "message": "Access denied: Cannot access email",
      "code": "E_AUTH_PERMISSION_401",
      "path": ["users", 0, "email"]
    },
    {
      "message": "Field 'ssn' doesn't exist on User",
      "code": "E_BINDING_UNKNOWN_FIELD_202",
      "path": ["users", 0, "ssn"]
    }
  ]
}
```text
<!-- Code example in TEXT -->

### 1.8 Query Caching

**Caching strategy:**

```rust
<!-- Code example in RUST -->
async fn execute_query_with_cache(
    query_name: &str,
    variables: &Variables,
    user_id: &str,
) -> Result<QueryResponse> {
    // Build cache key (includes user context for RLS)
    let cache_key = format!(
        "{}:{}:{}",
        query_name,
        hash(variables),
        user_id
    );

    // Check cache
    if let Some(cached) = cache.get(&cache_key) {
        return Ok(cached);
    }

    // Execute query
    let response = execute_query(...).await?;

    // Cache result (if cacheable)
    if response.meta.cacheable {
        cache.set(&cache_key, response.clone(), ttl)?;
    }

    Ok(response)
}
```text
<!-- Code example in TEXT -->

**Cache invalidation:**

Query cache is invalidated when:

- TTL expires
- Related table is modified (INSERT/UPDATE/DELETE)
- User authorization changes
- Authorization rule changes

---

## 2. Mutation Execution Semantics

### 2.1 Mutation Execution Phases

Mutations follow a stricter five-phase model with atomic guarantees:

```text
<!-- Code example in TEXT -->
 Request Validation
    ↓
 Input Validation & Transformation
    ↓
 Pre-mutation Authorization
    ↓
 Atomic Transaction Execution
    ↓
 Post-mutation Response & Events
```text
<!-- Code example in TEXT -->

### 2.2 Phase 1: Request Validation

Similar to query validation, but with stricter rules:

```graphql
<!-- Code example in GraphQL -->
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
<!-- Code example in TEXT -->

**Validation checks:**

1. Field is a mutation (not a query) ✅
2. All required arguments provided ✅
3. Argument types correct ✅
4. No side effects allowed in nested queries (only mutations allow nested mutations) ✅

### 2.3 Phase 2: Input Validation & Transformation

**Input validation rules:**

```rust
<!-- Code example in RUST -->
// Validate input type
struct CreatePostInput {
    title: String,      // Required, max 256 chars
    content: String,    // Required, max 100KB
}

// Validation
if input.title.len() == 0 {
    return Err(E_VALIDATION_EMPTY_TITLE_104);
}
if input.title.len() > 256 {
    return Err(E_VALIDATION_TITLE_TOO_LONG_105);
}
if input.content.len() > 100_000 {
    return Err(E_VALIDATION_CONTENT_TOO_LARGE_106);
}
```text
<!-- Code example in TEXT -->

**Custom validators:**

```python
<!-- Code example in Python -->
@FraiseQL.mutation
def create_post(input: CreatePostInput) -> Post:
    """Create a new post"""

    @FraiseQL.validate
    def validate_title(title: str) -> None:
        # Custom validation logic
        if "spam" in title.lower():
            raise ValidationError("Title contains spam keywords")

    @FraiseQL.transform
    def normalize_title(title: str) -> str:
        # Transform input (e.g., strip whitespace, normalize unicode)
        return title.strip().casefold()
```text
<!-- Code example in TEXT -->

**If validation fails:**

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Validation error: title cannot be empty",
      "code": "E_VALIDATION_EMPTY_TITLE_104",
      "extensions": {
        "field": "input.title",
        "validation_rule": "required"
      }
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

### 2.4 Phase 3: Pre-mutation Authorization

**Authorization checks before modification:**

```rust
<!-- Code example in RUST -->
// "Can this user execute this mutation?"
// "Can this user modify this resource?"

// Example: CreatePost
// - Can user create posts? (role-based)
// - Can user create posts in this project? (resource-based)
// - Does user have write permission on Post type?
```text
<!-- Code example in TEXT -->

**Authorization evaluation:**

```sql
<!-- Code example in SQL -->
-- Check if user can create posts
SELECT 1
FROM tb_authorization_rules
WHERE user_id = $current_user_id
  AND action = 'create'
  AND resource_type = 'Post'
LIMIT 1
```text
<!-- Code example in TEXT -->

**If authorization fails:**

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "You don't have permission to create posts",
      "code": "E_AUTH_PERMISSION_401",
      "extensions": {
        "action": "create",
        "resource_type": "Post",
        "required_role": "author"
      }
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

### 2.5 Phase 4: Atomic Transaction Execution

**Transaction guarantees:**

```rust
<!-- Code example in RUST -->
async fn execute_mutation(
    mutation: MutationPlan,
    input: MutationInput,
    user_id: String,
) -> Result<MutationResponse> {
    // Begin transaction
    let mut txn = db.begin_transaction().await?;

    // Set isolation level
    txn.execute("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").await?;

    // Execute mutation
    match execute_mutation_steps(&mut txn, &mutation, &input, &user_id).await {
        Ok(response) => {
            // Commit transaction
            txn.commit().await?;
            Ok(response)
        }
        Err(err) => {
            // Rollback transaction
            txn.rollback().await?;
            Err(err)
        }
    }
}
```text
<!-- Code example in TEXT -->

**All-or-nothing semantics:**

```graphql
<!-- Code example in GraphQL -->
mutation CreatePostAndComment {
  createPost(title: "New Post") {
    id
  }
  createComment(postId: $postId, text: "Comment") {  # Error!
    id
  }
}
```text
<!-- Code example in TEXT -->

**Result:**

- If first operation succeeds but second fails → Entire transaction rolls back
- Both operations must succeed for changes to be persisted
- Either all operations complete or none do (atomicity)

**Example mutation execution:**

```text
<!-- Code example in TEXT -->

1. BEGIN TRANSACTION
2. Validate input ✅
3. Check authorization ✅
4. INSERT INTO tb_post (title, content, author_id, created_at)
   VALUES ('New Post', 'Content', user-123, NOW()) ✅
5. INSERT INTO tb_audit_log (action, user_id, entity_type, entity_id)
   VALUES ('create', user-123, 'Post', post-456) ✅
6. PUBLISH change event (NOTIFY post_created) ✅
7. COMMIT TRANSACTION ✅
8. Return response {id: post-456, title: "New Post"}
```text
<!-- Code example in TEXT -->

**Deadlock handling:**

```rust
<!-- Code example in RUST -->
// If deadlock detected, retry mutation
for attempt in 1..=3 {
    match execute_mutation_in_transaction(...).await {
        Ok(response) => return Ok(response),
        Err(DatabaseError::Deadlock) if attempt < 3 => {
            // Sleep and retry
            tokio::time::sleep(Duration::from_millis(10 * attempt)).await;
            continue;
        }
        Err(err) => return Err(err),
    }
}
```text
<!-- Code example in TEXT -->

**Constraint violation handling:**

```rust
<!-- Code example in RUST -->
// Constraint check during transaction
match db.execute(INSERT_STATEMENT).await {
    Ok(rows) => {
        // Success
    }
    Err(DatabaseError::UniqueViolation(field)) => {
        // Handle constraint violation
        return Err(E_VALIDATION_DUPLICATE_VALUE_107);
    }
    Err(DatabaseError::ForeignKeyViolation(field)) => {
        // Handle foreign key violation
        return Err(E_VALIDATION_INVALID_REFERENCE_108);
    }
}
```text
<!-- Code example in TEXT -->

### 2.6 Phase 5: Post-mutation Response & Events

**After transaction commits:**

1. **Event publishing:**

   ```rust
<!-- Code example in RUST -->
   // Publish change events (for subscriptions)
   event_bus.publish(Event {
       type: "PostCreated",
       entity_type: "Post",
       entity_id: "post-456",
       timestamp: now(),
       user_id: "user-123",
       changes: {
           "title": "New Post",
           "content": "Content"
       }
   });
   ```text
<!-- Code example in TEXT -->

2. **Side effects (webhooks, notifications):**

   ```rust
<!-- Code example in RUST -->
   // Post-mutation side effects (not in transaction)
   // These run after commit, so they see changes
   trigger_webhooks("post.created", post);
   send_notification(post.author_id, "Your post was created");
   invalidate_caches("posts");
   ```text
<!-- Code example in TEXT -->

3. **Response transformation:**

   ```rust
<!-- Code example in RUST -->
   // Return new entity state
   {
       "data": {
           "createPost": {
               "id": "post-456",
               "title": "New Post"
           }
       },
       "extensions": {
           "mutation": {
               "duration_ms": 23,
               "rows_affected": 1,
               "cache_invalidated": ["posts", "user-123-posts"]
           }
       }
   }
   ```text
<!-- Code example in TEXT -->

### 2.7 Mutation Error Handling

**All-or-nothing error semantics:**

If mutation errors mid-transaction:

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Post with that title already exists",
      "code": "E_VALIDATION_DUPLICATE_VALUE_107",
      "phase": "validation"
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

**Result: Nothing is persisted. Entire transaction rolled back.**

| Error Type | Retryable | Phase | Rollback |
|-----------|-----------|-------|----------|
| Validation error | No | Before transaction | N/A |
| Authorization denied | No | Before transaction | N/A |
| Deadlock | Yes | In transaction | ✅ Auto-rollback |
| Constraint violation | No | In transaction | ✅ Auto-rollback |
| Connection failed | Maybe | In transaction | ✅ Auto-rollback |
| Timeout | Yes | In transaction | ✅ Auto-rollback |

### 2.8 Mutation Idempotency

**Non-idempotent mutations (default):**

```graphql
<!-- Code example in GraphQL -->
mutation CreatePost($title: String!) {
  createPost(title: $title) {
    id
  }
}

# Call twice with same input → Create two posts
# Two different IDs returned
```text
<!-- Code example in TEXT -->

**Idempotent mutations (with idempotency key):**

```graphql
<!-- Code example in GraphQL -->
mutation CreatePostIdempotent(
  $title: String!,
  $idempotency_key: String!
) {
  createPost(
    title: $title,
    idempotency_key: $idempotency_key
  ) {
    id
  }
}

# Call twice with same input and idempotency_key
# Returns same ID both times (request deduplication)
# Database checks: If idempotency_key exists, return existing result
```text
<!-- Code example in TEXT -->

**Idempotency implementation:**

```rust
<!-- Code example in RUST -->
async fn execute_idempotent_mutation(
    idempotency_key: &str,
    mutation: &Mutation,
) -> Result<MutationResponse> {
    // Check if we've seen this idempotency key
    if let Some(cached_response) = cache.get(idempotency_key) {
        return Ok(cached_response);
    }

    // Execute mutation
    let response = execute_mutation(mutation).await?;

    // Cache response with idempotency key
    cache.set(idempotency_key, response.clone(), ttl)?;

    Ok(response)
}
```text
<!-- Code example in TEXT -->

---

## 3. Subscription Execution Semantics

### 3.1 Subscription Execution Model

Subscriptions are **event-driven, long-lived connections** that push updates to clients:

```text
<!-- Code example in TEXT -->
Client establishes connection
    ↓
Client sends subscription request
    ↓
Server creates event listener
    ↓
Server streams events to client (loop)
    ↓
When event occurs:
    - Fetch updated entity
    - Apply authorization
    - Send to client
    ↓
Client closes connection
    ↓
Server cleans up listener
```text
<!-- Code example in TEXT -->

### 3.2 Phase 1: Subscription Establishment

**Client sends subscription:**

```graphql
<!-- Code example in GraphQL -->
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
<!-- Code example in TEXT -->

**Server validation (same as query validation):**

1. Subscription field exists ✅
2. All requested fields valid ✅
3. User authorized for subscription ✅

**If validation fails:**

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Subscription 'postCreated' doesn't exist",
      "code": "E_BINDING_UNKNOWN_FIELD_202"
    }
  ]
}
```text
<!-- Code example in TEXT -->

### 3.3 Phase 2: Event Listener Registration

**Server creates event listener:**

```rust
<!-- Code example in RUST -->
struct SubscriptionListener {
    subscription_id: String,
    user_id: String,
    event_type: String,          // "post_created"
    filter: Option<Filter>,       // Optional event filters
    auth_rules: Vec<AuthRule>,    // Subscription-level auth
    created_at: DateTime,
}

// Register listener
let listener = SubscriptionListener {
    subscription_id: "sub-123",
    user_id: "user-456",
    event_type: "post_created",
    filter: Some(Filter { author_id: "user-456" }),
    auth_rules: vec![AuthRule::OwnerOrAdmin],
    created_at: now(),
};

event_manager.register(listener).await?;
```text
<!-- Code example in TEXT -->

**Connection setup:**

```rust
<!-- Code example in RUST -->
// WebSocket connection established
// Send confirmation to client
send_to_client({
    "type": "connection_ack",
    "payload": {
        "connection_id": "conn-789",
        "protocol": "graphql-ws"
    }
});

// Send subscription acknowledgement
send_to_client({
    "type": "complete",
    "id": "sub-123",
    "payload": {
        "subscriptionId": "sub-123",
        "listening": true
    }
});
```text
<!-- Code example in TEXT -->

### 3.4 Phase 3: Event Capture & Filtering

**When database event occurs:**

```sql
<!-- Code example in SQL -->
-- PostgreSQL NOTIFY triggered:
NOTIFY post_created, '{
  "id": "post-789",
  "author_id": "user-123",
  "title": "New Post"
}'
```text
<!-- Code example in TEXT -->

**Event filtering (early):**

```rust
<!-- Code example in RUST -->
// Apply event-level filters BEFORE authorization
// Filters determine which events to process at all

let event = parse_notification(payload);

// Check subscription filters
if let Some(filter) = listener.filter {
    // Filter by author: only notify if event.author_id == user_id
    if !filter.matches(&event) {
        return;  // Skip this event
    }
}

// Event passed filter, continue to authorization
```text
<!-- Code example in TEXT -->

**Per-entity event ordering guarantee:**

```rust
<!-- Code example in RUST -->
// FraiseQL guarantees event ordering per entity:
// All events for Post #123 are delivered in order
// But events for different entities may interleave:

// Timeline:
// Event 1: Post #123 created (from user A)
// Event 2: Post #456 created (from user B)
// Event 3: Post #123 updated (from user A)
//
// Delivery to subscribers:
// - Post #123 subscriber: Event 1 → Event 3 (ordered)
// - Post #456 subscriber: Event 2 (isolated)
// - All posts subscriber: Event 1 → Event 2 → Event 3 (global order maintained)
```text
<!-- Code example in TEXT -->

### 3.5 Phase 4: Authorization & Masking

**Apply subscription-level authorization:**

```rust
<!-- Code example in RUST -->
// Check: Can this user receive this event?
// Different from mutation authorization (event producer != event consumer)

if !listener.auth_rules.iter().all(|rule| rule.allows_event(&event, &user_context)) {
    return;  // User not authorized for this event, skip
}

// Mark field-level masking
for field in response_fields {
    if !user_authorized_for_field(&user_context, field) {
        field.masked = true;
    }
}
```text
<!-- Code example in TEXT -->

**Example authorization:**

```graphql
<!-- Code example in GraphQL -->
subscription OnPostCreated {
  postCreated {
    id            # Public
    title         # Public
    content       # @authorize(rule="owner_or_admin") - May be masked
    author {
      name        # Public
      email       # @authorize(rule="admin_only") - May be masked
    }
  }
}
```text
<!-- Code example in TEXT -->

**Authorization result:**

```json
<!-- Code example in JSON -->
{
  "type": "next",
  "id": "sub-123",
  "payload": {
    "data": {
      "postCreated": {
        "id": "post-789",
        "title": "New Post",
        "content": null,  // Masked (user not owner/admin)
        "author": {
          "name": "Alice",
          "email": null   // Masked (user not admin)
        }
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

### 3.6 Phase 5: Entity Resolution & Response

**Resolve full entity from event:**

```rust
<!-- Code example in RUST -->
// Event only contains ID, resolve full entity:
async fn resolve_event_entity(
    event: Event,
    subscription_field: FieldDef,
) -> Result<Entity> {
    // Execute query to fetch full entity:
    // SELECT * FROM v_post WHERE id = $event_id

    let entity = db.query_one(
        "SELECT ... FROM v_post WHERE id = $1",
        vec![event.entity_id]
    ).await?;

    Ok(entity)
}
```text
<!-- Code example in TEXT -->

**Why full resolution?**

- Events contain only changes (small payload)
- Clients want full entity state (not just delta)
- Authorization may have changed since event triggered
- Fields may include nested relationships

**Response transformation:**

```rust
<!-- Code example in RUST -->
// Transform database entity to subscription response

let response = SubscriptionMessage {
    message_type: "next",
    subscription_id: "sub-123",
    payload: TransformToGraphQL(entity),
};

send_to_client(response).await?;
```text
<!-- Code example in TEXT -->

**Example subscription response:**

```json
<!-- Code example in JSON -->
{
  "type": "next",
  "id": "sub-123",
  "payload": {
    "data": {
      "postCreated": {
        "id": "post-789",
        "title": "New Post",
        "author": {
          "name": "Alice"
        }
      }
    },
    "extensions": {
      "subscription": {
        "event_id": "evt-999",
        "event_timestamp": "2026-01-15T10:30:45Z",
        "event_delay_ms": 23,
        "server_timestamp": "2026-01-15T10:30:45.023Z"
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

### 3.7 Connection Lifecycle

**Keepalive messages (prevent connection timeout):**

```rust
<!-- Code example in RUST -->
// Send keepalive every 30 seconds if no data
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        if !events_sent_recently {
            send_to_client(KeepAliveMessage);
        }
    }
});
```text
<!-- Code example in TEXT -->

**Client heartbeat:**

```graphql
<!-- Code example in GraphQL -->
# Client can send ping to verify connection alive
{"type": "ping"}

# Server responds
{"type": "pong"}
```text
<!-- Code example in TEXT -->

**Connection cleanup:**

```rust
<!-- Code example in RUST -->
// When client closes subscription:
event_manager.unregister(&listener).await?;

// Send completion
send_to_client({
    "type": "complete",
    "id": "sub-123"
});

// Close WebSocket connection (if no other subscriptions)
if ws_connection.subscriptions.is_empty() {
    ws_connection.close().await?;
}
```text
<!-- Code example in TEXT -->

### 3.8 Subscription Error Handling

**Errors during subscription:**

| Scenario | Behavior | Message |
|----------|----------|---------|
| Field authorization denied | Send error | E_AUTH_PERMISSION_401 |
| Entity deleted before sending | Send null | Entity no longer exists |
| Database connection lost | Retry subscription | Automatic reconnection |
| Client connection lost | Clean up listener | Release resources |
| Event buffer overflow | Drop subscription | Buffer exceeded, subscription terminated |

**Event buffer overflow:**

```rust
<!-- Code example in RUST -->
// FraiseQL maintains per-subscription event buffer (1000 events default)
// If producer outpaces consumer:

if buffer.len() > max_size {
    // Terminate subscription
    send_to_client({
        "type": "error",
        "id": "sub-123",
        "payload": {
            "message": "Subscription buffer overflow",
            "code": "E_SUB_BUFFER_OVERFLOW_601"
        }
    });

    // Close subscription
    event_manager.terminate(&listener).await?;
}
```text
<!-- Code example in TEXT -->

**Network error during event delivery:**

```rust
<!-- Code example in RUST -->
// If sending event to client fails:
match send_to_client(event).await {
    Ok(_) => {},
    Err(ConnectionError) => {
        // Mark subscription for cleanup
        pending_cleanup.push(listener.subscription_id);

        // Stop sending events temporarily
        sleep(Duration::from_secs(1)).await;

        // Attempt reconnection up to 3 times
        if reconnect_attempts < 3 {
            reconnect_attempts += 1;
            continue;
        }

        // Give up
        event_manager.terminate(&listener).await?;
    }
}
```text
<!-- Code example in TEXT -->

### 3.9 Multi-subscription Handling

**Multiple subscriptions on same connection:**

```graphql
<!-- Code example in GraphQL -->
subscription OnPostCreated {
  postCreated { id title }
}

subscription OnCommentCreated {
  commentCreated { id content }
}
```text
<!-- Code example in TEXT -->

**Implementation:**

```rust
<!-- Code example in RUST -->
struct WebSocketConnection {
    conn_id: String,
    user_id: String,
    subscriptions: HashMap<String, SubscriptionListener>,  // Multiple subscriptions
    event_buffer: Vec<Event>,  // Shared buffer
}

// When event arrives, check all subscriptions
for (sub_id, listener) in connection.subscriptions {
    if listener.event_type == event.type {
        // Send to this subscription
    }
}
```text
<!-- Code example in TEXT -->

**Resource limits per connection:**

- Max subscriptions per connection: 100
- Max total events per second: 10,000
- Max buffer size per subscription: 1,000 events
- Connection timeout: 5 minutes idle

---

## 4. Execution Guarantees & Trade-offs

### 4.1 Execution Guarantees

| Aspect | Query | Mutation | Subscription |
|--------|-------|----------|--------------|
| **Atomicity** | Read snapshot | All-or-nothing | Per-event |
| **Consistency** | ACID ✅ | ACID ✅ | Eventual |
| **Isolation** | SERIALIZABLE* | SERIALIZABLE | Per-event |
| **Ordering** | N/A | Sequential | Per-entity ordering |
| **Durability** | Yes | Yes | Temporary buffer |
| **Deduplication** | Idempotent | Idempotency key | Message deduplication |

*Isolation level configurable per database

### 4.2 Performance Trade-offs

**Query vs Caching:**

- Uncached query: ~50-100ms
- Cached query: ~1-5ms (100x faster)
- Cache TTL: Application-dependent (default 5 minutes)

**Mutation vs Atomicity:**

- Atomic transaction: ~20-50ms (slower, but safe)
- Non-atomic: ~5-10ms (faster, but risky)
- Default: Atomic (safety over speed)

**Subscription vs Latency:**

- Direct event: <5ms (fast, high volume)
- With entity resolution: 10-50ms (slower, complete data)
- With authorization: 10-50ms (safety overhead)

### 4.3 Timeout Defaults

| Operation | Default Timeout | Configurable |
|-----------|-----------------|--------------|
| Query execution | 30 seconds | ✅ Per-query |
| Mutation execution | 30 seconds | ✅ Per-mutation |
| Subscription connection | 5 minutes idle | ✅ Global |
| Database connection | 5 seconds | ✅ Pool-level |
| Authorization check | 5 seconds | ✅ Per-rule |

---

## 5. Streaming & Pagination

### 5.1 Query Streaming

For large result sets, FraiseQL streams results:

```rust
<!-- Code example in RUST -->
// Stream response in chunks
async fn stream_large_query(
    query: Query,
    chunk_size: usize,
) -> impl Stream<Item = QueryChunk> {
    // Start streaming after first chunk (50ms into query)
    // Send 64KB chunks every 100ms
    // Allow client to start processing before query completes
}
```text
<!-- Code example in TEXT -->

**Streaming response format:**

```text
<!-- Code example in TEXT -->
Frame 1: {"data": {"users": [user1, user2, ...]}}
Frame 2: [user3, user4, ...]
Frame 3: [user5, user6, ...]
```text
<!-- Code example in TEXT -->

### 5.2 Pagination Strategies

**Offset/limit pagination:**

```graphql
<!-- Code example in GraphQL -->
query GetPosts($offset: Int!, $limit: Int!) {
  posts(offset: $offset, limit: $limit) {
    id
    title
  }
}
```text
<!-- Code example in TEXT -->

**Keyset pagination (recommended for scale):**

```graphql
<!-- Code example in GraphQL -->
query GetPosts($after: String, $first: Int!) {
  posts(after: $after, first: $first) {
    id
    title
    cursor  # For next page
  }
}
```text
<!-- Code example in TEXT -->

**Keyset cursor implementation:**

```rust
<!-- Code example in RUST -->
// Cursor encodes sort key + result ID
// Format: base64(sort_key:result_id)

let cursor = base64_encode(format!("{}:{}", created_at, post_id));
// Cursor: "MjAyNi0wMS0xNVQxMDozMDo0NVo6cG9zdC03ODk="

// Decode for next query
let (sort_key, result_id) = base64_decode(cursor)?;
// Query: SELECT * FROM posts WHERE created_at < $sort_key ORDER BY created_at DESC LIMIT 20
```text
<!-- Code example in TEXT -->

---

## 6. Summary: Execution Flow Diagram

```text
<!-- Code example in TEXT -->
┌─────────────────────────────────────────────────────────────┐
│ Client Request (Query/Mutation/Subscription)                │
└────────────────┬────────────────────────────────────────────┘
                 │
        ┌────────▼────────┐
        │  Validate       │
        │  - Syntax       │
        │  - Types        │
        │  - Fields       │
        └────────┬────────┘
                 │
        ┌────────▼────────┐
        │ Authorize       │
        │  - User context │
        │  - Permissions  │
        └────────┬────────┘
                 │
        ┌────────▼────────┐
        │ Bind            │
        │  - Parameters   │
        │  - Variables    │
        └────────┬────────┘
                 │
        ┌────────▼────────────────────┐
        │ Execute (3 paths)           │
        ├─────────────────────────────┤
        │ Query Path:                 │
        │  1. Execute SQL (READ)      │
        │  2. Transform response      │
        │  3. Cache if applicable     │
        │                             │
        │ Mutation Path:              │
        │  1. Begin transaction       │
        │  2. Execute SQL (WRITE)     │
        │  3. Commit/rollback         │
        │  4. Publish events          │
        │                             │
        │ Subscription Path:          │
        │  1. Register listener       │
        │  2. Stream events (long-lived) │
        │  3. Transform + filter each │
        └────────┬────────────────────┘
                 │
        ┌────────▼────────┐
        │ Send Response   │
        │  - Data         │
        │  - Errors       │
        │  - Extensions   │
        └────────┬────────┘
                 │
        ┌────────▼────────────────┐
        │ Client Processes Result │
        └─────────────────────────┘
```text
<!-- Code example in TEXT -->

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL's execution semantics ensure deterministic, atomic, and transparent runtime behavior. The compiled schema drives execution; the runtime follows the plan.
