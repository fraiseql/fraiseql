# Error Type Hierarchy

FraiseQL exposes a single canonical error type — `FraiseQLError` — and lets
subsystem crates compose into it via `From` impls. This document maps the
error flow from internal crate errors through to the HTTP response, and
clarifies the remaining intentional name collisions across crates.

## Error Flow

```
Crate-internal errors
    |
    v  impl From<X> for FraiseQLError  (owned by the subsystem crate; sqlx pattern)
FraiseQLError (fraiseql-error::core_error)
    |
    v  impl From<FraiseQLError> for ServerError
ServerError::Engine (fraiseql-server)
    |
    v  impl IntoResponse for FraiseQLError  (feature `axum-compat`)
ErrorResponse JSON + StatusCode
    |
    v
HTTP response to client
```

The `RuntimeError` parallel HTTP-shaped enum and the five vestigial shadow
domain enums (`fraiseql_error::{AuthError, WebhookError, NotificationError,
IntegrationError, ObserverError}`) were removed in v2.3.0; see
`DEPRECATIONS.md` for the migration matrix.

## Error Types by Crate

### `fraiseql-error` (Canonical Root)

The central error crate. All runtime crates depend on it.

| Type | File | Purpose |
|------|------|---------|
| `FraiseQLError` | `core_error.rs` | The single root error type. Engine variants (`Parse`, `Validation`, `Database`, `RateLimited`, `NotFound`, `ServiceUnavailable`, `Internal`, …) plus four domain composition variants (`Auth`, `Webhook`, `Observer`, `File`). `#[non_exhaustive]` |
| `ErrorResponse` | `http.rs` | OAuth 2.0-style JSON body with `error`, `error_description`, `error_code`, `error_uri`, `details`, `retry_after`. Built by the `IntoResponse` impl for `FraiseQLError` (`axum-compat` feature). |
| `FileError` | `file.rs` | Upload limits, MIME types, storage backends. Used directly by storage backends; composes into `FraiseQLError::File` via `#[from]`. |
| `ConfigError` | `config.rs` | File not found, parse errors, env vars. |
| `GraphQLError` | `graphql_error.rs` | GraphQL-spec error payload for response envelopes. |

### `fraiseql-auth` (Internal Auth Layer)

| Type | File | Purpose |
|------|------|---------|
| `AuthError` | `error.rs` | Internal OIDC/JWT/session errors. Composes into `FraiseQLError::Auth(_)` via `From` (owned by this crate). |
| `PkceError` | `pkce.rs` | PKCE state management (expired, not found). |
| `DecryptionError` | `state_encryption.rs` | AEAD decryption failures. |

### `fraiseql-wire` (Wire Protocol Layer)

| Type | File | Purpose |
|------|------|---------|
| `Error` | `error.rs` | PostgreSQL wire protocol errors. |
| `AuthError` | `auth/mod.rs` | Wire-protocol SCRAM-SHA-256 errors. Orthogonal to `fraiseql_auth::AuthError`. |

### `fraiseql-observers` (Observer Runtime)

| Type | File | Purpose |
|------|------|---------|
| `ObserverError` | `error.rs` | Operational observer errors with structured codes (OB001–OB014). Composes into `FraiseQLError::Observer(_)` via `From` (owned by this crate). |
| `ObserverErrorCode` | `error.rs` | Stable error codes for structured logging. |
| `JobQueueError` | `job_queue/traits.rs` | Job queue operation failures. |

### `fraiseql-webhooks` (Webhook Subsystem)

| Type | File | Purpose |
|------|------|---------|
| `WebhookError` | `lib.rs` | Signature validation, timestamps, idempotency, handler dispatch. Composes into `FraiseQLError::Webhook(_)` via `From` (owned by this crate). |
| `SignatureError` | `signature.rs` | Provider-specific signature verification failures. |

### `fraiseql-server` (HTTP Server)

| Type | File | Purpose |
|------|------|---------|
| `ServerError` | `lib.rs` | Server startup/runtime: `BindError`, `ConfigError`, `Engine(FraiseQLError)`, `IoError`, `Database`, `Validation`, `Conflict`, `NotFound`. The `Engine` variant is the bridge from engine errors into the server layer. |
| `GraphQLError` | `error.rs` | GraphQL spec-compliant error response. |
| `ErrorCode` | `error.rs` | Stable codes: ValidationError, ParseError, Unauthenticated, Forbidden, … |
| `SchemaLoadError` | `schema/loader.rs` | Schema compilation/loading. |
| `ProtocolError` | `subscriptions/protocol.rs` | WebSocket subscription protocol. |

### `fraiseql-core` (Execution Engine)

| Type | File | Purpose |
|------|------|---------|
| `SecurityError` | `security/errors.rs` | Query validation: depth, complexity, size, CORS, CSRF, rate limits. |
| `AuditError` | `security/audit.rs` | Audit logging failures. |
| `ApqError` | `apq/storage.rs` | Persisted query operations. |
| `GraphQLParseError` | `graphql/parser.rs` | Query parsing. |
| `FragmentError` | `graphql/fragment_resolver.rs` | Fragment resolution. |
| `KmsError` | `security/kms/error.rs` | Key management operations. |

## Name Collisions (remaining intentional duals)

### Two `AuthError` Types

These are NOT duplicates. They serve different layers:

| Crate | Layer | When to Use |
|-------|-------|-------------|
| `fraiseql-auth` | Middleware | Internal JWT/OIDC processing with diagnostic detail (never exposed to clients). Composes into `FraiseQLError::Auth`. |
| `fraiseql-wire` | Protocol | SCRAM-SHA-256 authentication in the PostgreSQL wire protocol. Orthogonal to the auth subsystem. |

(The third `AuthError` — `fraiseql_error::AuthError` — was deleted in v2.3.0; its purpose was subsumed by `FraiseQLError::Auth(_)` carrying the `fraiseql_auth::AuthError` payload.)

## HTTP Response Sanitization

All `FraiseQLError` → `ErrorResponse` conversions strip internal details
(see `crates/fraiseql-error/src/http.rs`):

| Internal Error | Client Sees | Logged Internally |
|---------------|-------------|-------------------|
| `Auth(InvalidToken{reason: "JWT exp claim..."})` | "Authentication failed" | Full JWT parse reason via `source` chain |
| `Database{sql_state: "42P01", ...}` | "A database error occurred" | SQL state, connection info |
| `Configuration{message: "..."}` | "A configuration error occurred" | File paths, env vars |
| `Internal{message, source}` | "An internal error occurred" | Full message + chained source |

Safe-to-expose variants (e.g. `Validation`, `Conflict`, `Unsupported`,
`File::TooLarge`) include their structured detail in the response body so the
client can correct the request.

## Rules for Adding New Error Types

1. **Crate-internal errors** stay in their crate. Add `impl From<X> for FraiseQLError` in the same crate that owns the new error type — never reach upward into `fraiseql-error` (which must remain a leaf).
2. **New cross-cutting variants** go on `FraiseQLError` directly in `fraiseql-error`. Coordinate this through a policy decision; the canonical taxonomy should grow slowly.
3. **HTTP mapping** is defined in `fraiseql-error/src/http.rs` via the `IntoResponse` impl. Add a match arm for every new `FraiseQLError` variant.
4. **Never expose internal paths, SQL, or credentials** in client-facing error messages.
