# Error Type Hierarchy

FraiseQL has 75+ public error types across its workspace crates. This document maps the error flow from internal crate errors through to the HTTP response, and clarifies naming collisions between same-named types in different crates.

## Error Flow

```
Crate-internal errors
    |
    v
FraiseQLError (fraiseql-error::core_error)
    |
    v  impl From<FraiseQLError> for ServerError
ServerError (fraiseql-server)
    |
    v  Domain handlers convert to RuntimeError
RuntimeError (fraiseql-error)
    |
    v  impl IntoResponse for RuntimeError
ErrorResponse JSON + StatusCode
    |
    v
HTTP response to client
```

## Error Types by Crate

### `fraiseql-error` (Domain Aggregation Layer)

The central error crate. All runtime crates depend on it.

| Type | File | Purpose |
|------|------|---------|
| `FraiseQLError` | `core_error.rs` | Core enum: Parse, Validation, Database, Auth, Timeout, Unsupported, etc. `#[non_exhaustive]` |
| `RuntimeError` | `lib.rs` | Domain-level aggregator: Auth, Webhook, File, Observer, Notification, Integration, Config. Implements `IntoResponse` |
| `ErrorResponse` | `http.rs` | OAuth 2.0-style JSON body with `error`, `error_description`, `error_code`, `error_uri`, `details`, `retry_after` |
| `AuthError` | `auth.rs` | **Domain-level** auth errors for HTTP response mapping |
| `ObserverError` | `observer.rs` | **Domain-level** observer errors for RuntimeError aggregation |
| `WebhookError` | `webhook.rs` | Signature validation, timestamps, idempotency |
| `FileError` | `file.rs` | Upload limits, MIME types, storage backends |
| `NotificationError` | `notification.rs` | Provider config, rate limiting, template rendering |
| `IntegrationError` | `integration.rs` | External service failures (search, cache, queues) |
| `ConfigError` | `config.rs` | File not found, parse errors, env vars |

### `fraiseql-auth` (Internal Auth Layer)

| Type | File | Purpose |
|------|------|---------|
| `AuthError` | `error.rs` | **Internal** OIDC/JWT/session errors with 19 variants. NOT the same as `fraiseql_error::AuthError` |
| `PkceError` | `pkce.rs` | PKCE state management (expired, not found) |
| `DecryptionError` | `state_encryption.rs` | AEAD decryption failures |

### `fraiseql-wire` (Wire Protocol Layer)

| Type | File | Purpose |
|------|------|---------|
| `Error` | `error.rs` | PostgreSQL wire protocol errors |
| `AuthError` | `auth/mod.rs` | **Wire-protocol** SCRAM-SHA-256 errors. NOT the same as the other two `AuthError` types |

### `fraiseql-observers` (Observer Runtime)

| Type | File | Purpose |
|------|------|---------|
| `ObserverError` | `error.rs` | **Operational** observer errors with structured codes (OB001-OB014). NOT the same as `fraiseql_error::ObserverError` |
| `ObserverErrorCode` | `error.rs` | Stable error codes for structured logging |
| `JobQueueError` | `job_queue/traits.rs` | Job queue operation failures |

### `fraiseql-server` (HTTP Server)

| Type | File | Purpose |
|------|------|---------|
| `ServerError` | `lib.rs` | Server startup/runtime: Bind, Config, IO, Database |
| `GraphQLError` | `error.rs` | GraphQL spec-compliant error response |
| `ErrorCode` | `error.rs` | Stable codes: ValidationError, ParseError, Unauthenticated, Forbidden, etc. |
| `SchemaLoadError` | `schema/loader.rs` | Schema compilation/loading |
| `ProtocolError` | `subscriptions/protocol.rs` | WebSocket subscription protocol |

### `fraiseql-core` (Execution Engine)

| Type | File | Purpose |
|------|------|---------|
| `SecurityError` | `security/errors.rs` | Query validation: depth, complexity, size, CORS, CSRF, rate limits |
| `AuditError` | `security/audit.rs` | Audit logging failures |
| `ApqError` | `apq/storage.rs` | Persisted query operations |
| `GraphQLParseError` | `graphql/parser.rs` | Query parsing |
| `FragmentError` | `graphql/fragment_resolver.rs` | Fragment resolution |
| `KmsError` | `security/kms/error.rs` | Key management operations |

## Naming Collisions

### Three `AuthError` Types

These are NOT duplicates. They serve different layers:

| Crate | Layer | When to Use |
|-------|-------|-------------|
| `fraiseql-error` | Domain/HTTP | Mapping auth failures to HTTP status codes and client-facing error messages |
| `fraiseql-auth` | Middleware | Internal JWT/OIDC processing with diagnostic detail (never exposed to clients) |
| `fraiseql-wire` | Protocol | SCRAM-SHA-256 authentication in the PostgreSQL wire protocol |

### Two `ObserverError` Types

| Crate | Layer | When to Use |
|-------|-------|-------------|
| `fraiseql-error` | Domain | Aggregating observer failures into `RuntimeError` for HTTP responses |
| `fraiseql-observers` | Runtime | Rich operational errors with OB-codes for structured logging and retry decisions |

## HTTP Response Sanitization

All `RuntimeError` -> `ErrorResponse` conversions strip internal details:

| Internal Error | Client Sees | Logged Internally |
|---------------|-------------|-------------------|
| `Auth(InvalidToken{reason: "JWT exp claim..."})` | "Authentication failed" | Full JWT parse reason |
| `Database{sql_state: "42P01", ...}` | "A database error occurred" | SQL state, connection info |
| `Config{path: "/etc/fraiseql/..."}` | "A configuration error occurred" | File paths, env vars |

## Rules for Adding New Error Types

1. **Crate-internal errors** stay in their crate. Use `From` impls to convert to `FraiseQLError` at crate boundaries.
2. **New domain errors** go in `fraiseql-error` and get a variant in `RuntimeError`.
3. **HTTP mapping** is defined in `fraiseql-error/src/http.rs` via the `IntoResponse` impl.
4. **Never expose internal paths, SQL, or credentials** in client-facing error messages.
