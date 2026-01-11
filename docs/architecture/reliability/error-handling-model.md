# Error Handling Model

**Version:** 1.0
**Status:** Complete
**Date:** January 11, 2026
**Audience:** All developers, integrators, operations engineers, architecture reviewers

---

## 1. Overview

FraiseQL's error handling model is **deterministic, predictable, and classifiable**. Unlike traditional GraphQL servers where error handling varies by resolver implementation, FraiseQL has a unified, specification-driven error model.

**Core principle:** All errors are either **preventable** (caught at compile time) or **recoverable** (clear classification at runtime).

### 1.1 Design Philosophy

**No surprises.** Clients should never encounter unexpected error types or inconsistent error formats.

**Classifiable.** Every error falls into a well-defined category with clear semantics.

**Remediable.** Every error either tells you exactly how to fix it (compile-time) or how to recover from it (runtime).

**Auditable.** Error context includes enough information for debugging without leaking sensitive data.

---

## 2. Error Categories

### 2.1 Compile-Time Errors (Schema Definition Phase)

**When:** During schema authoring and compilation
**Who sees them:** Schema authors, build systems
**Recovery:** Fix schema, recompile
**Visibility:** Never reaches clients

#### 2.1.1 Schema Validation Errors

```
Category: SCHEMA_INVALID
Code: E_SCHEMA_<subtype>_<number>

Examples:
  E_SCHEMA_TYPE_NOT_DEFINED_001
  E_SCHEMA_FIELD_NOT_FOUND_002
  E_SCHEMA_BINDING_MISSING_003
  E_SCHEMA_OPERATOR_UNSUPPORTED_004
  E_SCHEMA_AUTHORIZATION_INVALID_005
```

**Causes:**
- Type referenced but not declared
- Field referenced but not in database view
- Query/mutation without binding
- WHERE operator not supported by target database
- Authorization rule references non-existent auth context field

**Example:**
```
Error: Schema compilation failed
  Type: Type closure violation
  Code: E_SCHEMA_TYPE_NOT_DEFINED_001
  Query 'users' returns 'list[User]'
  Type 'User' is not defined
  Suggestion: Add @fraiseql.type class User or check spelling
  File: schema.py, line 42
```

#### 2.1.2 Database Binding Errors

```
Category: BINDING_INVALID
Code: E_BINDING_<subtype>_<number>

Examples:
  E_BINDING_VIEW_NOT_FOUND_010
  E_BINDING_COLUMN_NOT_FOUND_011
  E_BINDING_TYPE_MISMATCH_012
  E_BINDING_PROCEDURE_SIGNATURE_MISMATCH_013
```

**Causes:**
- Binding references view that doesn't exist in database
- Field maps to column that doesn't exist
- Field type doesn't match database column type
- Mutation input doesn't match stored procedure parameters

**Example:**
```
Error: Database binding failed
  Type: View not found
  Code: E_BINDING_VIEW_NOT_FOUND_010
  Query 'users' bound to view 'v_user_missing'
  Database: postgresql (localhost:5432/mydb)
  Suggestion: Create view v_user or fix binding to existing view
  Available views: v_user, v_user_archived, v_user_deleted
```

#### 2.1.3 Capability Errors

```
Category: DATABASE_CAPABILITY_UNSUPPORTED
Code: E_CAPABILITY_<database>_<operator>

Examples:
  E_CAPABILITY_SQLITE_REGEX_001
  E_CAPABILITY_MYSQL_COSINE_DISTANCE_002
  E_CAPABILITY_SQLSERVER_JSONB_CONTAINS_003
```

**Causes:**
- Schema uses operator not supported by target database
- Database lacks required extension (pgvector, PostGIS)

**Example:**
```
Error: Operator not supported by database
  Type: Database capability mismatch
  Code: E_CAPABILITY_SQLITE_REGEX_001
  Operator: _regex (regular expression matching)
  Target database: sqlite
  Field: User.email
  Suggestion: Use _like operator instead, or target postgresql
```

#### 2.1.4 Authorization Configuration Errors

```
Category: AUTHORIZATION_INVALID
Code: E_AUTH_<subtype>_<number>

Examples:
  E_AUTH_CONTEXT_FIELD_NOT_FOUND_020
  E_AUTH_ROLE_UNDEFINED_021
  E_AUTH_RULE_CIRCULAR_DEPENDENCY_022
```

**Causes:**
- Authorization rule references non-existent auth context field
- Authorization rule references undefined role
- Authorization rules have circular dependencies

---

### 2.2 Runtime Errors (Query/Mutation/Subscription Execution)

**When:** During runtime query execution
**Who sees them:** Client applications
**Recovery:** Application-specific (retry, notify user, log, etc.)
**Visibility:** Always returned in GraphQL error list

#### 2.2.1 Validation Errors

```
GraphQL error
Category: VALIDATION_FAILED
Code: E_VALIDATION_<subtype>

Structure:
{
  "errors": [{
    "message": "Human-readable error message",
    "extensions": {
      "code": "E_VALIDATION_QUERY_MALFORMED_100",
      "category": "VALIDATION_FAILED",
      "remediable": true,
      "retryable": false,
      "user_actionable": true,
      "timestamp": "2026-01-11T15:35:00Z",
      "trace_id": "req_550e8400"
    }
  }]
}
```

**Error Types:**

| Subtype | Code | Cause | Retryable | Example |
|---------|------|-------|-----------|---------|
| QUERY_MALFORMED | E_VALIDATION_QUERY_MALFORMED_100 | Syntax error in GraphQL query | No | `{ users { invalid_field } }` |
| VARIABLE_TYPE_MISMATCH | E_VALIDATION_VARIABLE_TYPE_MISMATCH_101 | Variable has wrong type | No | Query expects `$id: ID!`, got string |
| ARGUMENT_MISSING | E_VALIDATION_ARGUMENT_MISSING_102 | Required argument omitted | No | `users(first: 10)` missing `after` |
| ARGUMENT_TYPE_MISMATCH | E_VALIDATION_ARGUMENT_TYPE_MISMATCH_103 | Argument has wrong type | No | Query expects `first: Int!`, got string |
| ARGUMENT_INVALID_VALUE | E_VALIDATION_ARGUMENT_INVALID_VALUE_104 | Argument value out of range or invalid | No | `first: -1` (must be >= 0) |
| DEPRECATED_FIELD | E_VALIDATION_DEPRECATED_FIELD_105 | Query uses deprecated field | No | Field marked `@deprecated(reason: "...")` |
| DIRECTIVE_INVALID | E_VALIDATION_DIRECTIVE_INVALID_106 | Unknown or invalid directive | No | `@unknown_directive` |

**Example:**
```json
{
  "errors": [{
    "message": "Field 'invalid_field' not found on type 'User'",
    "locations": [{"line": 2, "column": 5}],
    "extensions": {
      "code": "E_VALIDATION_QUERY_MALFORMED_100",
      "category": "VALIDATION_FAILED",
      "remediable": true,
      "retryable": false,
      "user_actionable": true,
      "available_fields": ["id", "name", "email", "posts"],
      "suggestion": "Did you mean 'name'?"
    }
  }]
}
```

#### 2.2.2 Authorization Errors

```
GraphQL error
Category: AUTHORIZATION_DENIED
Code: E_AUTH_<subtype>

Structure: Same as validation errors above
```

**Error Types:**

| Subtype | Code | Cause | Retryable | Example |
|---------|------|-------|-----------|---------|
| NOT_AUTHENTICATED | E_AUTH_NOT_AUTHENTICATED_200 | No auth token provided or invalid | No | Request lacks Authorization header |
| INVALID_TOKEN | E_AUTH_INVALID_TOKEN_201 | Auth token malformed or expired | No | Token signature invalid |
| INSUFFICIENT_PERMISSIONS | E_AUTH_INSUFFICIENT_PERMISSIONS_202 | User lacks required role | No | Role is "user", requires "admin" |
| INSUFFICIENT_CLAIMS | E_AUTH_INSUFFICIENT_CLAIMS_203 | Auth token lacks required claims | No | Token lacks "org_id" claim |
| ROW_LEVEL_SECURITY_DENIED | E_AUTH_ROW_LEVEL_SECURITY_DENIED_204 | RLS policy prevents access | No | User cannot access this organization's data |
| FIELD_MASKING_APPLIED | E_AUTH_FIELD_MASKED_205 | Field returned as null due to auth rule | No | Field redacted for security |
| TENANT_ISOLATION_VIOLATION | E_AUTH_TENANT_VIOLATION_206 | Query crosses tenant boundary | No | Cannot query another tenant's data |

**Example:**
```json
{
  "errors": [{
    "message": "Insufficient permissions to query 'adminUsers'",
    "extensions": {
      "code": "E_AUTH_INSUFFICIENT_PERMISSIONS_202",
      "category": "AUTHORIZATION_DENIED",
      "remediable": false,
      "retryable": false,
      "user_actionable": false,
      "required_role": "admin",
      "user_role": "user"
    }
  }]
}
```

#### 2.2.3 Database Execution Errors

```
GraphQL error
Category: DATABASE_ERROR
Code: E_DB_<database>_<error_class>

Structure: Same as others
```

**Error Types:**

| Subtype | Code | Cause | Retryable | Example |
|---------|------|-------|-----------|---------|
| CONNECTION_FAILED | E_DB_POSTGRES_CONNECTION_FAILED_300 | Cannot connect to database | **Yes** | Connection timeout, network down |
| CONNECTION_POOL_EXHAUSTED | E_DB_MYSQL_POOL_EXHAUSTED_301 | No available connections | **Yes** | All connections in use, retry later |
| QUERY_TIMEOUT | E_DB_SQLSERVER_QUERY_TIMEOUT_302 | Query execution exceeded timeout | **Yes** | Long-running query, retry or optimize |
| DEADLOCK | E_DB_POSTGRES_DEADLOCK_303 | Transaction deadlock detected | **Yes** | Concurrent transaction conflict |
| CONSTRAINT_VIOLATION | E_DB_MYSQL_CONSTRAINT_VIOLATION_304 | Unique/foreign key constraint violated | No | Duplicate key, referential integrity |
| SYNTAX_ERROR | E_DB_SQLITE_SYNTAX_ERROR_305 | Generated SQL is malformed | No | Compiler bug or unsupported operation |
| PERMISSION_DENIED | E_DB_POSTGRES_PERMISSION_DENIED_306 | Database user lacks permission | No | Misconfigured database credentials |
| OUT_OF_MEMORY | E_DB_SQLSERVER_OUT_OF_MEMORY_307 | Database ran out of memory | **Yes** | Query too large, reduce batch size |
| DISK_FULL | E_DB_MYSQL_DISK_FULL_308 | Database disk full | **Yes** | Free up disk space |
| UNKNOWN | E_DB_UNKNOWN_ERROR_309 | Unclassified database error | **Yes** | See error details |

**Example (Retryable):**
```json
{
  "errors": [{
    "message": "Database connection timeout after 5s",
    "extensions": {
      "code": "E_DB_POSTGRES_CONNECTION_FAILED_300",
      "category": "DATABASE_ERROR",
      "remediable": false,
      "retryable": true,
      "retry_after_ms": 1000,
      "database": "postgresql",
      "host": "db.example.com",
      "port": 5432,
      "attempt": 1,
      "max_attempts": 3
    }
  }]
}
```

**Example (Non-Retryable):**
```json
{
  "errors": [{
    "message": "Unique constraint violation on users.email",
    "extensions": {
      "code": "E_DB_MYSQL_CONSTRAINT_VIOLATION_304",
      "category": "DATABASE_ERROR",
      "remediable": true,
      "retryable": false,
      "user_actionable": true,
      "constraint": "unique_email",
      "table": "users",
      "field": "email",
      "value": "user@example.com",
      "suggestion": "Email already exists. Use a different email or recover account."
    }
  }]
}
```

#### 2.2.4 Execution Logic Errors

```
GraphQL error
Category: EXECUTION_ERROR
Code: E_EXEC_<subtype>

Structure: Same as others
```

**Error Types:**

| Subtype | Code | Cause | Retryable | Example |
|---------|------|-------|-----------|---------|
| FIELD_NOT_FOUND | E_EXEC_FIELD_NOT_FOUND_400 | Field doesn't exist in result | No | Compiler bug |
| PROJECTION_FAILED | E_EXEC_PROJECTION_FAILED_401 | Cannot project field from data | No | Type mismatch |
| AGGREGATION_FAILED | E_EXEC_AGGREGATION_FAILED_402 | Cannot aggregate result | No | Type mismatch in aggregation |
| PAGINATION_INVALID | E_EXEC_PAGINATION_INVALID_403 | Invalid pagination parameters | No | Invalid cursor or offset |
| CURSOR_INVALID | E_EXEC_CURSOR_INVALID_404 | Pagination cursor invalid | No | Cursor expired or tampered |
| LIMIT_EXCEEDED | E_EXEC_LIMIT_EXCEEDED_405 | Query result exceeds size limit | No | Reduce page size or filter |

**Example:**
```json
{
  "errors": [{
    "message": "Query result would exceed maximum size of 100MB",
    "extensions": {
      "code": "E_EXEC_LIMIT_EXCEEDED_405",
      "category": "EXECUTION_ERROR",
      "remediable": true,
      "retryable": false,
      "user_actionable": true,
      "limit_bytes": 104857600,
      "estimated_size_bytes": 250000000,
      "suggestion": "Use pagination with smaller batch size or add more filters"
    }
  }]
}
```

#### 2.2.5 Federation Errors

```
GraphQL error
Category: FEDERATION_ERROR
Code: E_FED_<subtype>

Structure: Same as others
```

**Error Types:**

| Subtype | Code | Cause | Retryable | Example |
|---------|------|-------|-----------|---------|
| ENTITY_RESOLUTION_FAILED | E_FED_ENTITY_RESOLUTION_FAILED_500 | Cannot resolve federated entity | **Yes** | Subgraph unavailable |
| ENTITY_NOT_FOUND | E_FED_ENTITY_NOT_FOUND_501 | Federated entity doesn't exist | No | Entity ID invalid or deleted |
| SUBGRAPH_UNAVAILABLE | E_FED_SUBGRAPH_UNAVAILABLE_502 | Federation subgraph unreachable | **Yes** | Network/DNS issue |
| SUBGRAPH_TIMEOUT | E_FED_SUBGRAPH_TIMEOUT_503 | Subgraph response too slow | **Yes** | Slow subgraph, retry |
| ENTITY_TYPE_MISMATCH | E_FED_TYPE_MISMATCH_504 | Entity has unexpected type | No | Schema mismatch |

**Example:**
```json
{
  "errors": [{
    "message": "Federation subgraph 'orders' unreachable",
    "extensions": {
      "code": "E_FED_SUBGRAPH_UNAVAILABLE_502",
      "category": "FEDERATION_ERROR",
      "remediable": false,
      "retryable": true,
      "retry_after_ms": 2000,
      "subgraph": "orders",
      "subgraph_url": "https://orders.internal/graphql",
      "attempt": 1,
      "max_attempts": 3
    }
  }]
}
```

#### 2.2.6 Subscription/Event Errors

```
GraphQL error
Category: SUBSCRIPTION_ERROR
Code: E_SUB_<subtype>

Structure: Same as others (sent to client over WebSocket)
```

**Error Types:**

| Subtype | Code | Cause | Retryable | Example |
|---------|------|-------|-----------|---------|
| SUBSCRIPTION_NOT_FOUND | E_SUB_NOT_FOUND_600 | Subscription doesn't exist | No | Typo in subscription name |
| SUBSCRIPTION_FILTERS_INVALID | E_SUB_FILTERS_INVALID_601 | WHERE filters are invalid | No | Invalid filter expression |
| SUBSCRIPTION_AUTHORIZATION_DENIED | E_SUB_AUTH_DENIED_602 | Insufficient permissions | No | Role doesn't allow subscription |
| EVENT_BUFFER_OVERFLOW | E_SUB_BUFFER_OVERFLOW_603 | Event buffer full | **Yes** | Too many pending events |
| CONNECTION_CLOSED | E_SUB_CONNECTION_CLOSED_604 | WebSocket connection lost | **Yes** | Network issue, reconnect |
| EVENT_DELIVERY_FAILED | E_SUB_DELIVERY_FAILED_605 | Cannot deliver event | **Yes** | Transport issue |

**Example:**
```json
{
  "type": "error",
  "id": "1",
  "payload": {
    "errors": [{
      "message": "WebSocket connection closed unexpectedly",
      "extensions": {
        "code": "E_SUB_CONNECTION_CLOSED_604",
        "category": "SUBSCRIPTION_ERROR",
        "remediable": false,
        "retryable": true,
        "close_code": 1006,
        "reason": "connection lost",
        "reconnect_after_ms": 1000
      }
    }]
  }
}
```

#### 2.2.7 Internal Errors

```
GraphQL error
Category: INTERNAL_ERROR
Code: E_INTERNAL_<subtype>

Structure: Same as others
```

**Error Types:**

| Subtype | Code | Cause | Retryable | Example |
|---------|------|-------|-----------|---------|
| COMPILED_SCHEMA_INVALID | E_INTERNAL_SCHEMA_INVALID_700 | Compiled schema corrupted or invalid | No | Deployment/upgrade bug |
| RUNTIME_PANIC | E_INTERNAL_PANIC_701 | Runtime encountered unexpected condition | No | Compiler/runtime bug |
| CACHE_CORRUPTED | E_INTERNAL_CACHE_CORRUPTED_702 | Cache backend returned invalid data | **Yes** | Cache corruption, retry |
| UNKNOWN_ERROR | E_INTERNAL_UNKNOWN_ERROR_703 | Unclassified internal error | **Yes** | Unknown issue, retry |

**Example:**
```json
{
  "errors": [{
    "message": "Internal server error: unexpected panic",
    "extensions": {
      "code": "E_INTERNAL_PANIC_701",
      "category": "INTERNAL_ERROR",
      "remediable": false,
      "retryable": false,
      "timestamp": "2026-01-11T15:35:00Z",
      "trace_id": "req_550e8400",
      "stack_trace": "Available only in debug mode",
      "support_link": "https://github.com/fraiseql/fraiseql/issues"
    }
  }]
}
```

---

## 3. Error Response Format

### 3.1 Standard GraphQL Error Response

All runtime errors follow the GraphQL spec with FraiseQL extensions:

```json
{
  "errors": [
    {
      "message": "Human-readable error message",
      "locations": [
        {"line": 1, "column": 1}
      ],
      "path": ["users", 0, "name"],
      "extensions": {
        "code": "E_VALIDATION_QUERY_MALFORMED_100",
        "category": "VALIDATION_FAILED",
        "remediable": true,
        "retryable": false,
        "user_actionable": true,
        "timestamp": "2026-01-11T15:35:00Z",
        "trace_id": "req_550e8400",

        // Context-specific fields (optional)
        "suggestion": "Did you mean field 'email'?",
        "available_options": ["id", "name", "email"],
        "retry_after_ms": null,
        "attempt": 1,
        "max_attempts": 3
      }
    }
  ],
  "data": null
}
```

### 3.2 Error Context Fields

**Always included:**
- `code` — Unique error identifier (E_XXX_NNN)
- `category` — Error classification (VALIDATION_FAILED, DATABASE_ERROR, etc.)
- `message` — Human-readable message
- `timestamp` — ISO 8601 timestamp
- `trace_id` — Request trace ID for logging

**Conditional fields:**
- `remediable` — Can client fix this? (schema/query fixes)
- `retryable` — Should client retry? (transient errors)
- `user_actionable` — Should client show to user? (not security details)
- `suggestion` — How to fix it
- `retry_after_ms` — Milliseconds to wait before retry

**Context-specific:**
- `database` — Which database (E_DB_* errors)
- `constraint` — Which constraint violated (database errors)
- `field` — Which field caused the error
- `available_options` — Valid choices for this field

### 3.3 Sensitive Data Redaction

**Never expose:**
- SQL queries (even if query is client-safe)
- Internal file paths
- Stack traces (unless debug mode)
- Database credentials
- Auth tokens or claims
- Unencrypted user data

**Safe to expose:**
- Field names (part of schema)
- Constraint names (helps debugging)
- Error codes (for client classification)
- Database type (postgresql, mysql, etc.)
- Operation type (query, mutation, subscription)

---

## 4. Error Classification Rules

### 4.1 Remediable vs Non-Remediable

**Remediable:** Error indicates client code/schema is wrong. Client can fix it.

Examples:
- Invalid query syntax
- Missing required field
- Constraint violation
- Deprecated field usage

**Non-Remediable:** Error indicates system state issue. Client cannot fix it.

Examples:
- Database connection failure
- Authorization denial
- Authentication failure
- Internal server error

### 4.2 Retryable vs Non-Retryable

**Retryable:** Error is transient. Retrying may succeed.

Examples:
- Database connection timeout
- Connection pool exhausted
- Query timeout (retry with better query)
- Subgraph unavailable
- Event buffer overflow

**Non-Retryable:** Error is deterministic. Retrying will fail identically.

Examples:
- Schema validation error
- Authorization denial
- Constraint violation
- Invalid query syntax

### 4.3 User-Actionable vs Hidden

**User-Actionable:** Error message safe to show end-users.

Examples:
- "Email already exists"
- "You don't have permission to access this"
- "Invalid input format"

**Hidden:** Error message for developers only, not end-users.

Examples:
- "Database connection lost" (not user's problem)
- "Query timeout after 30s" (implementation detail)
- "Insufficient permissions" (doesn't explain why)

---

## 5. Error Propagation

### 5.1 Query Execution Error Propagation

When an error occurs during query execution, FraiseQL follows this strategy:

```
Query: { user { id name } posts { id title } }

Execution:
  1. Fetch user (succeeds)
  2. Fetch posts (fails with DATABASE_ERROR)

Result:
{
  "errors": [{
    "message": "Database connection lost",
    "path": ["user", "posts"],
    "extensions": { ... }
  }],
  "data": {
    "user": {
      "id": "123",
      "name": "Alice",
      "posts": null  // Field set to null due to error
    }
  }
}
```

**Rule:** Field with error is set to `null` in partial response. Parent queries continue executing. This allows clients to use partial data.

### 5.2 Mutation Error Propagation

Mutations are **atomic**: if ANY part fails, the entire mutation fails with no partial data.

```
Mutation: mutation {
  createUser(name: "Bob", email: "bob@example.com") { id }
  createPost(title: "Hello", userId: "123") { id }
}

Execution:
  1. Create user (succeeds, userId = 999)
  2. Create post (fails with CONSTRAINT_VIOLATION - userId invalid)

Result:
{
  "errors": [{
    "message": "Foreign key constraint violated: userId not found",
    "path": ["createPost"],
    "extensions": { ... }
  }],
  "data": null  // ENTIRE mutation fails, no partial data
}
```

**Rule:** Mutations provide all-or-nothing semantics. If any part fails, all changes roll back (database transaction semantics).

### 5.3 Subscription Error Propagation

Subscription errors are **per-event**. One event's error doesn't stop subscription.

```
Subscription: subscription {
  orderCreated { id amount }
}

Event 1: Success
{
  "type": "next",
  "id": "1",
  "payload": {
    "data": {
      "orderCreated": {"id": "ord_1", "amount": 100}
    }
  }
}

Event 2: Authorization error
{
  "type": "error",
  "id": "2",
  "payload": {
    "errors": [{
      "message": "Insufficient permissions for this order",
      "extensions": { "code": "E_AUTH_ROW_LEVEL_SECURITY_DENIED_204", ... }
    }]
  }
}

Event 3: Success (continues after error)
{
  "type": "next",
  "id": "3",
  "payload": {
    "data": {
      "orderCreated": {"id": "ord_3", "amount": 250}
    }
  }
}
```

**Rule:** Subscription continues after error. One error event doesn't close subscription (unless close_code indicates connection close).

---

## 6. Error Handling for Clients

### 6.1 Recommended Client Error Handling

**Step 1: Check for errors in response**
```python
response = await client.execute(query)
if response.get("errors"):
    # Handle errors
    pass
```

**Step 2: Classify errors by category**
```python
for error in response["errors"]:
    code = error["extensions"]["code"]
    category = error["extensions"]["category"]

    if category == "VALIDATION_FAILED":
        # Fix query and retry immediately
        pass
    elif category == "AUTHORIZATION_DENIED":
        # Request authentication, then retry
        pass
    elif category == "DATABASE_ERROR" and error["extensions"]["retryable"]:
        # Exponential backoff retry
        pass
    elif category == "INTERNAL_ERROR":
        # Log and notify support
        pass
```

**Step 3: Implement retry logic**
```python
async def retry_query(query, max_attempts=3, backoff_base=1000):
    for attempt in range(1, max_attempts + 1):
        response = await client.execute(query)

        if not response.get("errors"):
            return response  # Success

        retryable_errors = [
            e for e in response["errors"]
            if e["extensions"].get("retryable")
        ]

        if not retryable_errors:
            return response  # Non-retryable, give up

        if attempt < max_attempts:
            wait_ms = response["errors"][0]["extensions"].get(
                "retry_after_ms",
                backoff_base * (2 ** (attempt - 1))
            )
            await asyncio.sleep(wait_ms / 1000)
        else:
            return response  # Max attempts reached
```

### 6.2 Error Display to End-Users

**Show these errors to users:**
- Validation errors with suggestions (query/input fixes)
- Constraint violations ("Email already exists")
- Authorization denials (general message, not details)
- Timeouts (with retry option)

**Hide these errors from users:**
- Database connection details
- Internal stack traces
- SQL queries
- Auth token issues
- Internal server errors (show support contact instead)

---

## 7. Debugging & Troubleshooting

### 7.1 Using Trace IDs

Every error includes a `trace_id` for correlation:

```
Client sees:
{
  "errors": [{
    "message": "Database connection failed",
    "extensions": {
      "trace_id": "req_550e8400"
    }
  }]
}

Server logs:
[2026-01-11 15:35:00] TRACE req_550e8400: query { users { id } }
[2026-01-11 15:35:01] ERROR req_550e8400: connection timeout after 5s
```

**Use trace_id to:**
- Correlate client-side errors with server logs
- Track error through entire system (across services in federation)
- Debug transient errors that are hard to reproduce

### 7.2 Debug Mode

FraiseQL can enable debug mode to include additional error context:

```
# Enable debug mode (dev environments only!)
FRAISEQL_DEBUG=true

Response with debug mode:
{
  "errors": [{
    "message": "Query timeout",
    "extensions": {
      "code": "E_DB_POSTGRES_QUERY_TIMEOUT_302",
      "stack_trace": [
        "fraiseql::runtime::execute_query (src/runtime.rs:312)",
        "fraiseql::db::execute (src/db.rs:45)",
        "postgres::client::query (src/db/postgres.rs:128)"
      ],
      "query": "SELECT ... FROM users WHERE id = $1",
      "bindings": ["12345"],
      "duration_ms": 30001
    }
  }]
}
```

### 7.3 Error Context Logging

Enable structured logging to capture error context:

```json
{
  "timestamp": "2026-01-11T15:35:00Z",
  "level": "ERROR",
  "trace_id": "req_550e8400",
  "category": "DATABASE_ERROR",
  "code": "E_DB_POSTGRES_DEADLOCK_303",
  "operation": "mutation",
  "query_hash": "5f5a3c2b1e0d9f8c",
  "database": "postgresql",
  "user_id": "user_123",
  "tenant_id": "org_456",
  "attempt": 2,
  "duration_ms": 5234,
  "retryable": true
}
```

---

## 8. Error Codes Reference

### 8.1 Complete Error Code Catalog

| Range | Category | Count |
|-------|----------|-------|
| E_SCHEMA_* (001-009) | Schema validation | 9 |
| E_BINDING_* (010-019) | Database binding | 10 |
| E_CAPABILITY_* (020-099) | Database capability | 80 |
| E_AUTH_* (200-209) | Authorization | 10 |
| E_VALIDATION_* (100-109) | Query validation | 10 |
| E_DB_* (300-309) | Database execution | 10 |
| E_EXEC_* (400-405) | Execution logic | 6 |
| E_FED_* (500-504) | Federation | 5 |
| E_SUB_* (600-605) | Subscriptions | 6 |
| E_INTERNAL_* (700-703) | Internal errors | 4 |

**Total:** 150+ distinct error codes

---

## 9. Error Evolution & Stability

### 9.1 Error Code Stability Guarantee

**Error codes are stable:** Once assigned, error codes will never change.

**Adding errors:** New error codes may be added in minor releases (X.Y.Z → X.Y+1.Z).

**Removing errors:** Error codes are never removed, only deprecated.

**Example:** If E_DB_POSTGRES_CONNECTION_FAILED_300 is used today, it will have the same meaning in v2.1, v3.0, v10.0, etc.

### 9.2 Deprecation of Error Types

When an error becomes obsolete, it's marked deprecated:

```json
{
  "errors": [{
    "message": "...",
    "extensions": {
      "code": "E_OLD_ERROR_123",
      "deprecated": true,
      "deprecated_since": "v2.3.0",
      "use_instead": "E_NEW_ERROR_456",
      "removal_date": "2027-01-11"
    }
  }]
}
```

---

## 10. Non-Goals

**Error handling explicitly does NOT:**

- Attempt automatic recovery (clients must handle retries)
- Hide all error details (debugging requires transparency)
- Support custom error codes (standard codes only)
- Provide error translation per locale (use standard codes)
- Guarantee error message format changes (messages evolve)

---

## Summary

FraiseQL's error handling is **deterministic and classifiable**:

- ✅ All errors fall into well-defined categories
- ✅ Each error code is stable and never changes
- ✅ Errors include actionable context (retry, remediable, user-actionable)
- ✅ Partial results possible (queries), atomic results (mutations)
- ✅ Clients can implement robust error handling
- ✅ Operations can debug using trace IDs and structured logs

**Golden rule:** If the error code is the same, the error is the same. Clients can build deterministic error handling logic.

---

*End of Error Handling Model*
