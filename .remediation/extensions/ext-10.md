# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension 10

*Written 2026-03-05. Eleventh independent assessor.*
*Extends all nine preceding plans without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings confirmed against HEAD (latest commit: `140eea10c`).*
*Scope: MCP executor, file upload validation, trace ID generation, RBAC error handling,*
*and the dual error-type architecture.*

---

## Context and Methodology

This assessment read all ten existing plans fully (base + extensions 1–9), then performed
independent deep-reads on five areas previous assessors left unexplored:

1. The `fraiseql-server/src/mcp/` directory (MCP executor and tool handler)
2. The `fraiseql-server/src/files/` directory (upload validation and storage)
3. The `fraiseql-server/src/tracing_server.rs` (server-level trace ID generation — distinct
   from `fraiseql-observers/src/tracing/propagation.rs` which was fixed in the in-flight changes)
4. The `fraiseql-server/src/api/rbac_management.rs` error handling (distinct from E2 in
   Extension I, which covers the auth bypass — this covers what happens *after* auth is added)
5. The `fraiseql-error` crate and its relationship to `fraiseql-server`'s own `error.rs`

Prior coverage confirmed:
- RBAC authentication bypass (E2, Extension I) — **not repeated here**
- GET handler auth bypass (E1, Extension I) — **not repeated here**
- W3C trace context in `fraiseql-observers/src/tracing/propagation.rs` (Extension IX) — **not repeated here**
- `ChecksumValidation` unhandled provider (Extension VIII) — **not repeated here**
- Static subscription metrics test pollution (AD1, Extension IX) — **not repeated here**

| Category | Count | Severity |
|---|---|---|
| GraphQL injection via MCP tool arguments | 1 | Critical |
| Trace ID collision under concurrent load (server module) | 1 | High |
| RBAC silent error swallowing (data integrity, outage masking) | 1 | High |
| File magic-byte validation too permissive for images | 1 | Medium |
| `AsyncValidatorConfig.timeout` stored but never enforced | 1 | Medium |
| `parse_size` failure silently falls back in validation | 1 | Medium |
| Dual error-type ecosystems with no bridging contract | 1 | Low |

---

## Track R — MCP Executor Correctness (Priority: Critical)

### R1 — `graphql_value(String)` Does Not Escape Inner Double Quotes → GraphQL Injection

**File:** `crates/fraiseql-server/src/mcp/executor.rs`, lines 123–136

**Problem:**

The MCP executor converts client-supplied MCP tool call arguments to a GraphQL query
string using `graphql_value()`:

```rust
fn graphql_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{s}\""),  // ← s is NOT escaped
        serde_json::Value::Number(n) => n.to_string(),
        // ...
    }
}
```

For the `String` arm, `s` is placed verbatim between double-quote delimiters.
No escaping of `"`, `\`, or newline characters is performed.

An MCP client that passes a string argument containing a double-quote can break
out of the string literal and inject arbitrary GraphQL syntax. Example:

Tool call arguments:
```json
{ "name": "foo\" } } query { sensitiveData { secrets } } query { a" }
```

Generated query:
```graphql
query { users(name: "foo" } } query { sensitiveData { secrets } } query { a") { id name } }
```

The GraphQL parser may not execute this exact injection (parser behaviour depends
on the query structure), but it opens a path for:
- Query structure manipulation
- Exfiltrating field names from types the MCP client should not enumerate
- Causing query-level errors that leak schema information via error messages

The `build_graphql_query` function also interpolates the `tool_name` directly as
the field name (`format!("{op_type} {{ {name}{args_str}{fields_str} }}")`) without
validating it against GraphQL identifier rules. A MCP tool name containing spaces,
braces, or other metacharacters would produce malformed queries or, for reserved
character sequences, potentially valid but unintended queries.

**Impact:** Any deployment with MCP enabled (`--features mcp`) is vulnerable to
argument-level GraphQL injection from MCP clients.

**Fix:**

Replace the string arm with proper escaping:

```rust
serde_json::Value::String(s) => {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"',  "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
},
```

Also validate `tool_name` against the GraphQL `Name` production
(`[_A-Za-z][_0-9A-Za-z]*`) before interpolating it:

```rust
fn validate_graphql_name(name: &str) -> Result<(), String> {
    let first = name.chars().next().ok_or_else(|| "name is empty".to_string())?;
    if !matches!(first, '_' | 'A'..='Z' | 'a'..='z') {
        return Err(format!("invalid GraphQL name: '{name}'"));
    }
    if !name.chars().all(|c| matches!(c, '_' | 'A'..='Z' | 'a'..='z' | '0'..='9')) {
        return Err(format!("invalid GraphQL name: '{name}'"));
    }
    Ok(())
}
```

The `scalar_fields_for_type` field names should receive the same validation
(they come from `CompiledSchema`, which is compiler-generated and therefore
trusted, but a belt-and-suspenders check is cheap).

**Acceptance:**
- Test: MCP call with `name = "foo\" } bad_query { id"` produces a validation
  error, not a syntactically interesting query.
- Test: MCP call with properly escaped `name = "O'Brien"` round-trips correctly.

---

## Track S — Trace ID Generation (Priority: High)

### S1 — `tracing_server.rs` Generates Collision-Prone, Non-Random Trace and Span IDs

**File:** `crates/fraiseql-server/src/tracing_server.rs`, lines 343–361

**Problem:**

The two private functions that generate trace and span IDs for the *server-level*
tracing module (`TraceContext::new()`, `TraceContext::from_request_id()`,
`TraceContext::child_span()`, `TraceContext::from_upstream()`) use system time
XOR-ed with the process ID:

```rust
fn generate_trace_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:032x}", nanos ^ u128::from(std::process::id()))
}

fn generate_span_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos()).unwrap_or(0);
    let process_id = u128::from(std::process::id());
    format!("{:016x}", (nanos ^ process_id) as u64)
}
```

**Three distinct defects:**

**a) Collision under concurrent load.** On modern multi-core hardware, two threads
calling `SystemTime::now()` within the same nanosecond produce identical `nanos`
values. Since `process_id` is constant, both XOR expressions are equal, producing
the same trace ID for two different requests. A Tokio worker pool with 8 threads
handling burst traffic will regularly produce duplicate trace IDs.

**b) Predictability / enumerability.** The trace ID is a deterministic function of
`(timestamp_ns, process_id)`. An external observer who knows the approximate time of
a request can enumerate candidate trace IDs and use them to correlate entries in any
trace aggregation system, bypassing tenant isolation of trace data.

**c) Mismatch with the fixed observer module.** The in-flight change to
`fraiseql-observers/src/tracing/propagation.rs` (confirmed in the unstaged diff)
already replaced sequential IDs with `uuid::Uuid::new_v4()`. The server-level module
was not updated alongside it, creating two different ID-generation strategies in the
same binary.

**Note:** Extension IX mentioned a W3C spec violation in the observer tracing module.
That fix (UUID v4) is correct. This finding is a *separate* module (`tracing_server.rs`)
that was not patched.

**Fix:**

```rust
fn generate_trace_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()  // 32-char lowercase hex, no dashes
}

fn generate_span_id() -> String {
    let bytes = uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap();
    format!("{:016x}", u64::from_ne_bytes(bytes))
}
```

`uuid` is already a dependency of `fraiseql-server` (see `Cargo.toml`).

**Acceptance:**
- Test: 10,000 IDs generated in parallel contain no duplicates.
- Test: `generate_trace_id()` returns exactly 32 lowercase hex characters.
- Test: `generate_span_id()` returns exactly 16 lowercase hex characters.

---

## Track T — RBAC Error Handling (Priority: High)

### T1 — RBAC List Endpoints Swallow All Errors, Returning Empty Collections

**File:** `crates/fraiseql-server/src/api/rbac_management.rs`, lines 162–170, 248–253, 317–323

**Problem:**

Three RBAC list handlers treat *every* error condition — including database
connection failures, timeouts, and driver panics — identically to an empty
result set:

```rust
// list_roles (line 162–170)
async fn list_roles(State(state): State<Arc<RbacManagementState>>) -> impl IntoResponse {
    // In production: extract tenant from JWT, apply pagination
    match state.db.list_roles(None, 100, 0).await {
        Ok(roles) => Json(roles),
        Err(_) => Json(Vec::<RoleDto>::new()),   // ← ANY error → []
    }
}

// list_permissions (line 248–253) — same pattern
// list_user_roles (line 317–323) — same pattern
```

**Impact:**

1. **Outage masking.** When the database is unreachable, every list endpoint returns
   HTTP 200 with `[]`. Callers — including admin UIs and health checks — cannot
   distinguish "no roles exist" from "the database is down". An operator
   decommissioning roles via the API could interpret the empty response as confirmation
   that all roles were successfully deleted.

2. **Privilege escalation risk.** If a downstream service caches the result of
   `GET /api/roles` and uses it to make authorization decisions, a transient database
   error causes the cache to be populated with an empty role list. Subsequent
   authorization checks against the empty list may grant access to all resources
   (if the downstream logic is "allow unless a deny-role is present").

3. **Tracing gap.** The `Err(_)` arm discards the error entirely, not even logging it.
   A database outage would produce zero observable signal in any log or trace.

The pattern is also present — with different consequences — in non-list endpoints:

```rust
// update_role (line ~198)
Err(_) => (StatusCode::CONFLICT, Json(serde_json::json!({"error": "update_failed"}))).into_response(),
```

Here, any error (including network timeout or column constraint violation unrelated
to a conflict) returns 409 Conflict, which is semantically wrong for a timeout.

**Fix:**

Introduce a thin mapping from `RbacDbError` to HTTP status codes and log the error
before responding:

```rust
async fn list_roles(State(state): State<Arc<RbacManagementState>>) -> impl IntoResponse {
    match state.db.list_roles(None, 100, 0).await {
        Ok(roles) => (StatusCode::OK, Json(roles)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to list RBAC roles");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database_error", "message": "Failed to retrieve roles"})),
            ).into_response()
        }
    }
}
```

For the update endpoint, map `RbacDbError` variants to correct HTTP status codes:

```rust
Err(RbacDbError::RoleNotFound) => StatusCode::NOT_FOUND,
Err(RbacDbError::RoleDuplicate) => StatusCode::CONFLICT,
Err(RbacDbError::QueryError(e)) => {
    tracing::error!(%e, "RBAC update failed");
    StatusCode::INTERNAL_SERVER_ERROR
},
```

**Acceptance:**
- Test: `list_roles` when DB returns `QueryError` → HTTP 500, not 200.
- Test: `list_roles` when DB returns `Ok([])` → HTTP 200 with `[]`.
- Test: `update_role` timeout → HTTP 500, not 409.
- Error logged at `tracing::error!` level in all failure paths.

---

## Track U — File Upload Validation (Priority: Medium)

### U1 — `mime_types_compatible` Accepts Any `image/*` for Any Declared `image/*` — Magic Bytes Check is Bypassed for Images

**File:** `crates/fraiseql-server/src/files/validation.rs`, lines 105–125

**Problem:**

The magic-bytes validation function uses a broad same-major-type shortcut that
effectively disables content-type checking for all image uploads:

```rust
fn mime_types_compatible(detected: &str, declared: &str) -> bool {
    // ...
    // Same major type (e.g., image/*)
    let detected_major = detected.split('/').next().unwrap_or("");
    let declared_major = declared.split('/').next().unwrap_or("");

    // For images, allow any image type if major matches
    if detected_major == "image" && declared_major == "image" {
        return true;  // ← image/gif declared, image/webp detected → OK
    }
    false
}
```

This means:
- A user who declares `Content-Type: image/jpeg` can upload a GIF89a file and the
  magic-bytes validator will approve it.
- A user who declares `image/png` can upload a WebP file — which may trigger
  downstream image processing code that expects PNG structure, causing decoder panics.
- If `allowed_types = ["image/jpeg", "image/png"]`, a WebP file uploaded with
  `Content-Type: image/png` will pass the allowed-types check (declared type is
  in the list) and pass the magic-bytes check (both are `image/*`), even though
  WebP is not in the allowlist.

**Impact:** The `validate_magic_bytes` configuration option exists specifically to
protect against MIME type spoofing. The image-category shortcut defeats this protection
entirely for the most common file upload category.

**Fix:**

Remove the same-major shortcut for images. Allow exact matches and the documented
equivalents only:

```rust
fn mime_types_compatible(detected: &str, declared: &str) -> bool {
    if detected == declared {
        return true;
    }
    // Documented equivalents only — no broad image/* allowance
    matches!(
        (detected, declared),
        ("image/jpeg", "image/jpg") | ("image/jpg", "image/jpeg")
    )
}
```

If partial image-type flexibility is genuinely desired (e.g., JPEG 2000 files
that report as `image/jp2` but are declared `image/jpeg`), document it explicitly
and make it opt-in via a `FileConfig.allow_image_subtype_mismatch: bool` flag
rather than always-on.

**Acceptance:**
- `mime_types_compatible("image/webp", "image/jpeg")` → `false`
- `mime_types_compatible("image/gif", "image/png")` → `false`
- `mime_types_compatible("image/jpeg", "image/jpg")` → `true` (documented alias)
- `mime_types_compatible("image/jpeg", "image/jpeg")` → `true` (exact match)
- Existing test `test_mime_compatibility` asserts `mime_types_compatible("image/png", "image/webp")` is `true` — this test assertion is wrong and must be flipped.

---

## Track V — Validator Configuration Contract (Priority: Medium)

### V1 — `AsyncValidatorConfig.timeout` Is Stored but Enforced by No Implementation

**File:** `crates/fraiseql-core/src/validation/async_validators.rs`

**Problem:**

`AsyncValidatorConfig` has a `timeout: Duration` field that is declared, documented,
and accepted as a constructor argument:

```rust
pub struct AsyncValidatorConfig {
    pub provider:       AsyncValidatorProvider,
    pub timeout:        Duration,      // ← documented as "Timeout duration for the validation operation"
    pub cache_ttl_secs: u64,
    pub field_pattern:  String,
}

impl AsyncValidatorConfig {
    pub fn new(provider: AsyncValidatorProvider, timeout_ms: u64) -> Self {
        Self {
            provider,
            timeout: Duration::from_millis(timeout_ms),
            // ...
        }
    }
}
```

Neither `EmailFormatValidator` nor `PhoneE164Validator` ever reads
`self.config.timeout`. The `AsyncValidator` trait's `timeout()` method returns
`self.config.timeout`, but no caller ever calls `validator.timeout()` to apply it.

For the current pure-regex implementations this is harmless — there is nothing to
time out. The danger is in the future: the `AsyncValidatorProvider::Custom(String)`
variant and the docstring "validators requiring runtime operations" explicitly
anticipate network-backed implementations. When a developer adds a network-backed
validator, they will see the `timeout` field and reasonably assume it is enforced.
It is not.

Additionally, `cache_ttl_secs` has the same problem: it is stored but there is no
cache implementation anywhere in the async validator dispatch path.

**Fix:**

Two options, in order of preference:

**Option A (document and enforce the contract):**
Add a wrapper that applies the timeout using `tokio::time::timeout`:

```rust
pub async fn validate_with_timeout<V: AsyncValidator>(
    validator: &V,
    value: &str,
    field: &str,
) -> AsyncValidatorResult {
    let timeout = validator.timeout();
    if timeout.is_zero() {
        // Zero timeout = no enforcement (e.g., sync validators)
        validator.validate_async(value, field).await
    } else {
        tokio::time::timeout(timeout, validator.validate_async(value, field))
            .await
            .map_err(|_| FraiseQLError::Validation {
                message: format!("Validation of field '{field}' timed out"),
                path: Some(field.to_string()),
            })?
    }
}
```

**Option B (remove deferred fields until they are implemented):**
Remove `timeout` and `cache_ttl_secs` from `AsyncValidatorConfig`. Re-add them
when the first network-backed validator is implemented. This avoids the false-contract
problem and makes the struct smaller.

**Acceptance:**
- If Option A: test that a validator that sleeps for 2s is cancelled when `timeout = 100ms`.
- If Option B: `AsyncValidatorConfig::new()` has no `timeout_ms` parameter;
  `AsyncValidator::timeout()` is removed from the trait.

---

## Track W — Silent Configuration Fallback (Priority: Medium)

### W1 — `parse_size` Failure Falls Back Silently to 10 MiB in File Validation

**File:** `crates/fraiseql-server/src/files/validation.rs`, line 34

**Problem:**

```rust
pub fn validate_file(data: &Bytes, declared_type: &str, filename: &str, config: &FileConfig)
    -> Result<ValidatedFile, FileError>
{
    let max_size = parse_size(&config.max_size).unwrap_or(10 * 1024 * 1024);
    // ...
}
```

If an operator writes a typo in `fraiseql.toml` — for example `max_size = "10 MB"`
(space before unit) instead of `"10MB"` — `parse_size` returns `Err`. The fallback
silently uses 10 MiB regardless of intent. No warning, no log line, no startup error.

This creates a class of misconfiguration where:
- An operator sets `max_size = "1kb"` (lowercase, not matching any pattern) intending
  a 1 KB limit; files up to 10 MiB are silently accepted.
- An operator sets `max_size = "100"` intending 100 bytes; `parse_size("100")` does
  return `Ok(100)`, so this particular case works — but the operator has no signal
  that the unit-free form is intentional.

The same pattern appears in other config-driven size limits across the codebase.

**Fix:**

The configuration should be validated at server startup (or schema compilation time),
not at request time. Move the `parse_size` call to `FileConfig` validation:

```rust
impl FileConfig {
    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), ConfigError> {
        parse_size(&self.max_size).map_err(|e| ConfigError::ValidationError {
            field: "max_size".to_string(),
            message: format!("Invalid size string '{}': {}", self.max_size, e),
        })?;
        Ok(())
    }

    /// Return parsed max size in bytes.
    pub fn max_size_bytes(&self) -> usize {
        parse_size(&self.max_size).expect("validated at startup")
    }
}
```

Alternatively, store the pre-parsed value as `max_size_bytes: usize` directly in
`FileConfig` and perform the parse in the `Deserialize` implementation using
`#[serde(deserialize_with = "deserialize_size")]`.

**Acceptance:**
- Test: `FileConfig { max_size: "10 MB".into(), .. }` fails validation at startup.
- Test: `FileConfig { max_size: "10MB".into(), .. }` passes validation and returns
  10 * 1024 * 1024 bytes.
- No `.unwrap_or` on `parse_size` remains in hot paths.

---

## Track X — Dual Error Ecosystems (Priority: Low)

### X1 — `fraiseql-error::RuntimeError` and `fraiseql-server::error::ErrorResponse` Are Parallel Hierarchies With No Bridging Contract

**Files:**
- `crates/fraiseql-error/src/lib.rs` — defines `RuntimeError`, `AuthError`, `WebhookError`, etc.
- `crates/fraiseql-server/src/error.rs` — defines `GraphQLError`, `ErrorResponse`, `ErrorCode`

**Problem:**

The `fraiseql-error` crate was designed as the unified error type for all runtime
crates. It defines `RuntimeError` with an `IntoResponse` implementation that maps
errors to HTTP status codes. Yet `fraiseql-server`'s route handlers do not use
`RuntimeError`. They use `GraphQLError` / `ErrorResponse` from the server's own
`error.rs`, which has its own independent status-code mapping.

The `fraiseql-error` crate's `IntoHttpResponse` trait is defined and exported, but
as of HEAD it is called from no route handler in `fraiseql-server`. The only uses of
`fraiseql_error::RuntimeError` in `fraiseql-server` are:

1. The `RuntimeState` trait methods (startup, not per-request).
2. The `ServerStartupError::RuntimeError` enum variant (wrapping a `FraiseQLError`, not
   a `RuntimeError`).

This creates two risks:

**a) Divergent HTTP semantics.** `RuntimeError::Auth(AuthError::InsufficientPermissions)`
maps to 403 Forbidden in `fraiseql-error`. If that error type is ever surfaced through
the server's route handlers via the `RuntimeError` path, it would return 403. But the
server's `GraphQLError::forbidden()` also returns 403 — with different JSON structure.
A client receiving errors from different code paths gets inconsistent error response
shapes.

**b) Dead code attracting confusion.** The `IntoHttpResponse` trait and `ErrorResponse`
struct in `fraiseql-error` look like the authoritative HTTP error contract, but they are
effectively dead code in the primary HTTP serving path. A developer adding a new endpoint
may use `RuntimeError` (the "canonical" type) and get a response format that differs from
all other endpoints.

**Fix:**

The fix is architectural. Two options:

**Option A (make `fraiseql-error` the sole HTTP error type for server routes):**
Replace `fraiseql-server::error::ErrorResponse` references in route handlers with
`fraiseql-error::RuntimeError`. Ensure `GraphQLError` serializes inside
`RuntimeError::Internal` or a new `RuntimeError::GraphQL` variant. This is a larger
refactor but produces one canonical error format.

**Option B (acknowledge the two-layer design explicitly):**
Document that `fraiseql-error` is for startup and infrastructure errors, while
`fraiseql-server::error` is for per-request GraphQL errors. Add a
`#[doc(hidden)]` marker to `IntoHttpResponse` so it is not discoverable as
a public API, and add a module-level comment explaining the split.

Option B is significantly less invasive and appropriate if the divergence is intentional.
The current code has no such documentation.

**Acceptance (Option B):**
- `fraiseql-error/src/lib.rs` has a module-level doc comment explaining that
  `IntoHttpResponse` is for infrastructure/startup error pages, not GraphQL responses.
- `fraiseql-server::error` has a module-level doc comment pointing to `fraiseql-error`
  for non-GraphQL error surfaces.
- CI: `cargo doc --no-deps` produces no "missing documentation" warnings on public items
  in `fraiseql-error`.

---

## Summary

| ID | Finding | Severity | File | Effort |
|---|---|---|---|---|
| R1 | MCP `graphql_value()` GraphQL injection | Critical | `mcp/executor.rs` | 1–2h |
| S1 | Trace/span ID collision under concurrent load | High | `tracing_server.rs` | 30m |
| T1 | RBAC list endpoints swallow all errors silently | High | `api/rbac_management.rs` | 2h |
| U1 | `mime_types_compatible` image bypass | Medium | `files/validation.rs` | 1h |
| V1 | `AsyncValidatorConfig.timeout` unenforced | Medium | `validation/async_validators.rs` | 2–4h |
| W1 | `parse_size` silent fallback in validation | Medium | `files/validation.rs` | 1h |
| X1 | Dual error type hierarchies undocumented | Low | `fraiseql-error`, `fraiseql-server/error.rs` | 2h |

**Recommended order:** R1 (security, blocks MCP deployment) → S1 (one-line fix) →
T1 (correctness + observability) → U1 (security property restoration) → W1
(operator experience) → V1 (API contract) → X1 (documentation).
