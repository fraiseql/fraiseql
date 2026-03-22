# FraiseQL Error Reference

This document lists all `FraiseQLError` variants, their error codes, HTTP status equivalents, and common causes with fixes.

---

## Quick Lookup by Error Code

| Error Code | HTTP | Variant | Category |
|---|---|---|---|
| `GRAPHQL_PARSE_FAILED` | 400 | [Parse](#parse) | GraphQL |
| `GRAPHQL_VALIDATION_FAILED` | 400 | [Validation](#validation) | GraphQL |
| `UNKNOWN_FIELD` | 400 | [UnknownField](#unknownfield) | GraphQL |
| `UNKNOWN_TYPE` | 400 | [UnknownType](#unknowntype) | GraphQL |
| `DATABASE_ERROR` | 500 | [Database](#database) | Database |
| `CONNECTION_POOL_ERROR` | 500 | [ConnectionPool](#connectionpool) | Database |
| `TIMEOUT` | 408 | [Timeout](#timeout) | Database |
| `CANCELLED` | 408 | [Cancelled](#cancelled) | Database |
| `FORBIDDEN` | 403 | [Authorization](#authorization) | Security |
| `UNAUTHENTICATED` | 401 | [Authentication](#authentication) | Security |
| `RATE_LIMITED` | 429 | [RateLimited](#ratelimited) | Security |
| `NOT_FOUND` | 404 | [NotFound](#notfound) | Resource |
| `CONFLICT` | 409 | [Conflict](#conflict) | Resource |
| `CONFIGURATION_ERROR` | 500 | [Configuration](#configuration) | Config |
| `UNSUPPORTED_OPERATION` | 501 | [Unsupported](#unsupported) | Config |
| `INTERNAL_SERVER_ERROR` | 500 | [Internal](#internal) | Internal |

---

## GraphQL Errors

### Parse

**Code:** `GRAPHQL_PARSE_FAILED` | **HTTP:** 400

**Message format:** `Parse error at {location}: {message}`

**Cause:** The GraphQL query has invalid syntax.

**Common triggers:**
- Missing closing braces or parentheses
- Invalid field names (e.g., starting with a number)
- Malformed variable definitions

**Fix:** Validate the query with a GraphQL linter or IDE plugin before sending.

```json
{
  "errors": [{
    "message": "Parse error at line 3, col 5: Expected '{', found '}'",
    "extensions": { "code": "GRAPHQL_PARSE_FAILED" }
  }]
}
```

---

### Validation

**Code:** `GRAPHQL_VALIDATION_FAILED` | **HTTP:** 400

**Message format:** `Validation error: {message}`

**Cause:** The query is syntactically valid but violates schema rules.

**Common triggers:**
- Required argument missing (e.g., `user` query without `id` argument)
- Wrong argument type (e.g., passing string where integer expected)
- Query depth exceeds `max_query_depth` configuration
- Query complexity exceeds `max_query_complexity` configuration
- Variable count exceeds `MAX_VARIABLES_COUNT` (1,000)
- `SecurityContext` missing when RLS is configured

**Fix:** Check the schema for required arguments and types. Reduce query depth/complexity if limits are hit.

---

### UnknownField

**Code:** `UNKNOWN_FIELD` | **HTTP:** 400

**Message format:** `Unknown field '{field}' on type '{type_name}'`

**Cause:** The query references a field that doesn't exist on the requested type.

**Common triggers:**
- Typo in field name
- Field was renamed or removed in a schema update
- Field exists but is excluded by field-level authorization

**Fix:** Check available fields on the type. FraiseQL includes "did you mean?" suggestions when the typo is close (Levenshtein distance ≤ 2).

```json
{
  "errors": [{
    "message": "Unknown field 'emial (did you mean 'email'?)' on type 'User'",
    "extensions": { "code": "UNKNOWN_FIELD" }
  }]
}
```

---

### UnknownType

**Code:** `UNKNOWN_TYPE` | **HTTP:** 400

**Message format:** `Unknown type '{type_name}'`

**Cause:** The query references a type that doesn't exist in the compiled schema.

**Fix:** Verify the type name against `schema.compiled.json` or run `fraiseql-cli validate-documents`.

---

## Database Errors

### Database

**Code:** `DATABASE_ERROR` | **HTTP:** 500

**Message format:** `Database error: {message}`

**Cause:** The underlying database returned an error during query execution.

**Common SQL state codes:**

| SQL State | Meaning | Fix |
|---|---|---|
| `23505` | Unique constraint violation | The record already exists; use an UPDATE or check for duplicates |
| `23503` | Foreign key violation | The referenced record doesn't exist |
| `42P01` | Undefined table | Run `fraiseql-cli compile` and ensure database migrations are applied |
| `42703` | Undefined column | Schema is out of sync; recompile and migrate |
| `57014` | Query cancelled by statement timeout | Increase `statement_timeout` or optimize the query |

**Note:** In production with error sanitization enabled, the detailed database message is replaced with a generic error. Check server logs for the full details.

---

### ConnectionPool

**Code:** `CONNECTION_POOL_ERROR` | **HTTP:** 500

**Message format:** `Connection pool error: {message}`

**Cause:** Unable to acquire a database connection from the pool.

**Common triggers:**
- Database server is down or unreachable
- Connection string is incorrect
- Pool is exhausted (all connections in use)
- DNS resolution failure

**Fix:**
1. Verify database connectivity: `psql $DATABASE_URL -c 'SELECT 1'`
2. Check pool configuration in `fraiseql.toml` (increase `max_connections`)
3. Look for connection leaks (long-running transactions)

**Retryable:** Yes — this error is safe to retry after a short delay.

---

### Timeout

**Code:** `TIMEOUT` | **HTTP:** 408

**Message format:** `Query timeout after {timeout_ms}ms`

**Cause:** A query exceeded the configured timeout.

**Fix:**
1. Optimize the query (add indexes, reduce result set)
2. Increase the timeout in configuration
3. Check for lock contention on the database

**Retryable:** Yes.

---

### Cancelled

**Code:** `CANCELLED` | **HTTP:** 408

**Message format:** `Query cancelled: {reason}`

**Cause:** A query was explicitly cancelled (e.g., client disconnected).

**Retryable:** Yes.

---

## Security Errors

### Authorization

**Code:** `FORBIDDEN` | **HTTP:** 403

**Message format:** `Authorization error: {message}`

**Cause:** The authenticated user lacks permission for the requested operation.

**Fields:**
- `action` — The denied action (e.g., "read", "write")
- `resource` — The resource being accessed (e.g., "User.email")

**Common triggers:**
- JWT lacks required scope for a `requires_scope` field
- RLS policy excludes the user from accessing the resource
- `on_deny` field configuration blocks access

**Fix:** Check JWT claims and field-level authorization rules in `schema.json`.

---

### Authentication

**Code:** `UNAUTHENTICATED` | **HTTP:** 401

**Message format:** `Authentication error: {message}`

**Cause:** No valid authentication credentials were provided.

**Common triggers:**
- Missing `Authorization: Bearer <token>` header
- JWT has expired
- JWT signature is invalid (wrong signing key)
- JWKS endpoint unreachable

**Fix:** Ensure a valid, non-expired JWT is included in the request.

---

### RateLimited

**Code:** `RATE_LIMITED` | **HTTP:** 429

**Message format:** `Rate limit exceeded: {message}`

**Cause:** Too many requests from the same client.

**Fields:**
- `retry_after_secs` — Seconds to wait before retrying

**Configuration (in `fraiseql.toml`):**
- `auth_start_max_requests` — Max `/auth/start` per IP per window (default: 100)
- `failed_login_max_requests` — Max failed attempts before lockout (default: 5)

**Fix:** Wait for `retry_after_secs` before retrying. In development, increase limits in `fraiseql.toml`.

---

## Resource Errors

### NotFound

**Code:** `NOT_FOUND` | **HTTP:** 404

**Message format:** `{resource_type} not found: {identifier}`

**Cause:** The requested resource doesn't exist.

**Fix:** Verify the identifier. For mutations, this means the target entity doesn't exist (e.g., UPDATE/DELETE on a nonexistent row).

---

### Conflict

**Code:** `CONFLICT` | **HTTP:** 409

**Message format:** `Conflict: {message}`

**Cause:** The operation would violate a business constraint (e.g., duplicate key, concurrent modification).

**Fix:** Check for existing resources before creating, or use idempotency keys.

---

## Configuration Errors

### Configuration

**Code:** `CONFIGURATION_ERROR` | **HTTP:** 500

**Message format:** `Configuration error: {message}`

**Cause:** FraiseQL configuration is invalid or incomplete.

**Common triggers:**
- Missing required environment variable (e.g., `STATE_ENCRYPTION_KEY`)
- Invalid `fraiseql.toml` values
- Schema compilation produced invalid output
- Pool size set to 0 or above maximum (200)

**Fix:** Run `fraiseql-cli validate-documents` to check configuration. Verify all required environment variables are set.

---

### Unsupported

**Code:** `UNSUPPORTED_OPERATION` | **HTTP:** 501

**Message format:** `Unsupported operation: {message}`

**Cause:** The requested operation is not supported by the current database backend.

**Common triggers:**
- Calling `execute_function_call` on SQLite (no stored procedures)
- Using a feature that requires a specific database backend
- Custom mutations on SQLite

**Fix:** Use a different database backend or restructure the operation.

---

## Internal Errors

### Internal

**Code:** `INTERNAL_SERVER_ERROR` | **HTTP:** 500

**Message format:** `Internal error: {message}`

**Cause:** An unexpected error occurred that doesn't fit other categories.

**Fix:** Check server logs for the full error chain. This typically indicates a bug — please report it.

---

## Error Classification

### Client vs. Server Errors

| Classification | Variants | Action |
|---|---|---|
| **Client errors** (4xx) | Parse, Validation, UnknownField, UnknownType, Authorization, Authentication, NotFound, Conflict, RateLimited | Fix the request |
| **Server errors** (5xx) | Database, ConnectionPool, Timeout, Cancelled, Configuration, Unsupported, Internal | Check server health |

### Retryable Errors

Only these errors are safe to retry automatically:

- `ConnectionPool` — Database may recover
- `Timeout` — Query may succeed with lower load
- `Cancelled` — Operation was interrupted

All other errors require fixing the request or server configuration before retrying.

---

## Error Sanitization

In production, FraiseQL sanitizes error messages to prevent information leakage:

| Setting | Effect |
|---|---|
| `generic_messages = true` (default) | Replaces detailed errors with generic messages |
| `leak_sensitive_details = false` (default) | Hides SQL state codes and internal details |
| `internal_logging = true` (default) | Full error details logged server-side |

Configure in `fraiseql.toml` under `[fraiseql.security.error_sanitization]`.

In development, set `user_facing_format = "detailed"` to see full error details.
