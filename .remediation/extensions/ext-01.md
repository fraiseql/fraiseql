# FraiseQL — Remediation Plan Extension

*Written 2026-03-05. Second assessor's findings.*
*Extends `/tmp/fraiseql-remediation-plan.md` without duplicating it.*
*Benchmarks out of scope (handled by velocitybench).*

---

## Executive Summary

The first assessor identified documentation inaccuracy and code correctness gaps.
This extension adds a second category of findings: **feature theater** (code that
compiles and tests pass but the functionality is not implemented), **authentication
gaps** (production routes that bypass auth), and **persistent code hygiene debts**
that the quality plan has not addressed.

Findings are grouped by severity.

---

## Track E — Security Correctness (Priority: Critical)

These are bugs in production code that affect security guarantees. They should
block any public release.

---

### E1 — GET GraphQL handler silently drops OIDC authentication context

**File:** `crates/fraiseql-server/src/routes/graphql.rs:380–428`

**Problem:**

The POST handler (`graphql_handler`) uses `OptionalSecurityContext` as an axum
extractor, which reads `AuthUser` from request extensions populated by
`oidc_auth_middleware`. The GET handler (`graphql_get_handler`) does not use
this extractor:

```rust
// POST — CORRECT: security context extracted from middleware
pub async fn graphql_handler<A: ...>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    OptionalSecurityContext(security_context): OptionalSecurityContext,  // ← extracts AuthUser
    Json(request): Json<GraphQLRequest>,
) -> ...

// GET — BUG: security context silently dropped
pub async fn graphql_get_handler<A: ...>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    Query(params): Query<GraphQLGetParams>,
) -> ... {
    // ...
    // NOTE: SecurityContext extraction will be handled via middleware in next iteration
    // For now, execute without security context
    execute_graphql_request(state, request, trace_context, None, &headers).await
    //                                                        ^^^^ AuthUser ignored
}
```

The OIDC middleware runs on both GET and POST routes and validates the JWT token.
For POST requests it then inserts `AuthUser` into extensions which the handler
extracts. For GET requests the middleware validates the token, but the handler
ignores the `AuthUser` — passing `security_context: None` to
`execute_graphql_request`.

**Impact:**
- RLS does not inject per-user WHERE clauses for GET requests: any authenticated
  user can read any data regardless of tenant or RLS policy when using GET
- `requires_scope` field-level authorization is not enforced for GET queries
- The API key fallback inside `execute_graphql_request` runs but finds no API key,
  falling through to unauthenticated execution

This is a complete auth bypass on a production-registered route.

**Fix:**

```rust
pub async fn graphql_get_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    Query(params): Query<GraphQLGetParams>,
    OptionalSecurityContext(security_context): OptionalSecurityContext,  // ← add this
) -> Result<GraphQLResponse, ErrorResponse> {
    // ...
    execute_graphql_request(state, request, trace_context, security_context, &headers).await
    //                                                      ^^^^^^^^^^^^^^^  ← was None
}
```

**Acceptance:**
- Integration test: OIDC-authenticated GET request carries security context
  (verify via RLS — user can only see their own data via GET just as via POST)
- `graphql_get_handler` uses `OptionalSecurityContext` extractor

---

### E2 — RBAC Management API is entirely unauthenticated

**File:** `crates/fraiseql-server/src/server/routing.rs:377–387`
**File:** `crates/fraiseql-server/src/api/rbac_management.rs:116–127`

**Problem:**

The RBAC management router exposes role and permission CRUD without any
authentication or authorization middleware:

```rust
// routing.rs — no auth_middleware applied to the RBAC router
let rbac_router = crate::api::rbac_management_router(rbac_state);
app = app.merge(rbac_router);  // merged directly, no route_layer for auth
```

Routes exposed unauthenticated:
- `POST /api/roles` — create any role
- `GET  /api/roles` — enumerate all roles
- `PUT  /api/roles/:id` — update any role
- `DELETE /api/roles/:id` — delete any role
- `POST /api/permissions` — create any permission
- `POST /api/user-roles` — assign any role to any user
- `DELETE /api/user-roles/:user_id/:role_id` — revoke roles
- `GET /api/audit/permissions` — read audit log

The individual handlers comment "In production: extract tenant from JWT" but
never do so.

**Secondary problem:** `RbacDbBackend::ensure_schema()` is never called.
The tables `fraiseql_roles`, `fraiseql_permissions`, etc. do not exist until
manually created, so all RBAC calls will fail with a table-not-found error
in a fresh deployment.

**Fix — authentication:**

Apply the OIDC auth middleware (or require admin scope) to the RBAC router
before merging it:

```rust
let rbac_router = if let Some(ref validator) = self.oidc_validator {
    let auth_state = OidcAuthState::new(validator.clone());
    crate::api::rbac_management_router(rbac_state)
        .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
} else {
    tracing::warn!("RBAC management API is unprotected: no OIDC validator configured");
    crate::api::rbac_management_router(rbac_state)
};
app = app.merge(rbac_router);
```

**Fix — schema initialization:**

```rust
let rbac_backend = Arc::new(
    crate::api::rbac_management::db_backend::RbacDbBackend::new(db_pool.clone()),
);
if let Err(e) = rbac_backend.ensure_schema().await {
    return Err(ServerError::Initialization(format!("RBAC schema setup failed: {e}")));
}
```

**Acceptance:**
- Unauthenticated request to `GET /api/roles` → 401 (not 200)
- Fresh deployment with `backup` feature → RBAC tables exist before first request

---

## Track F — Feature Theater (Priority: High)

These are modules that are documented or implicitly marketed as features but
contain only stub implementations. They should either be completed or clearly
labeled as unimplemented.

---

### F1 — Backup system is entirely stub code

**Files:** `crates/fraiseql-server/src/backup/`

**Problem:**

All four backup provider implementations are stubs that return success without
performing any actual backup:

```rust
// postgres_backup.rs
async fn backup(&self) -> BackupResult<BackupInfo> {
    // In production, would:
    // 1. Run: pg_dump -h localhost -U postgres fraiseql > backup.sql
    // ...
    Ok(BackupInfo { size_bytes: 0, verified: false, ... })  // ← stub
}

async fn health_check(&self) -> BackupResult<()> {
    // In production, would connect and run: SELECT 1;
    // For now, simulate success
    Ok(())  // ← always healthy, never checks
}
```

`BackupManager::start()` doesn't start a scheduler:

```rust
pub async fn start(&self) -> Result<(), String> {
    // In production, would spawn scheduler task
    // For now, just validate all providers are healthy (via eprintln)
    ...
}
```

`VALUE_PROPOSITION.md` lists:
- "Automated backup scheduling" — not implemented
- "Integration with cloud storage (S3, GCS)" — not implemented
- "Backup encryption and signing" — not implemented
- "Point-in-time recovery support" — not implemented

The `backup` feature flag is declared in `Cargo.toml` as an empty feature
(`backup = []`), gating modules that are entirely stub code.

**Options:**

**Option A (Recommended short-term):** Label the entire module as
`#[cfg(feature = "backup")]` **preview** and add a prominent doc comment and
feature flag note:

```rust
//! # Backup System (Preview — Not Production Ready)
//!
//! **Warning:** This module is a structural preview. No backup operation
//! currently writes or reads real data. Enable this feature in production
//! at your own risk — backup calls will return success without doing anything.
//!
//! Tracking issue: <link>
```

Remove all backup claims from `VALUE_PROPOSITION.md` until implemented.

**Option B (Long-term):** Implement `PostgresBackupProvider` using `tokio::process::Command`
to run `pg_dump`. This is the highest-priority provider; the others can remain labeled
"Preview".

**Acceptance for Option A:**
- `docs/VALUE_PROPOSITION.md` contains no mention of backup, DR, or S3/GCS
  integration unless those features are actually implemented
- The `backup` feature flag `Cargo.toml` entry carries a doc comment: "Preview —
  structural scaffolding only; backup operations are no-ops"
- `cargo doc -p fraiseql-server --features backup` shows the preview warning

---

### F2 — Syslog audit backend sends nothing

**File:** `crates/fraiseql-core/src/audit/syslog_backend.rs`

**Problem:**

`SyslogAuditBackend` is published, exported, and documented. `VALUE_PROPOSITION.md`
states "Audit logging to PostgreSQL, syslog, or file backends". The `send_to_syslog`
method:

```rust
async fn send_to_syslog(&self, message: &str) -> AuditResult<()> {
    // ...
    // NOTE: In production, this would use tokio::net::UdpSocket to send the message.
    // For now, we'll implement a minimal version that returns success.

    if self.host.is_empty() {
        return Err(AuditError::NetworkError("Syslog host not configured".to_string()));
    }

    Ok(())  // ← always Ok, no UDP packet sent
}
```

Both `port` and `timeout` fields are `#[allow(dead_code)]` — confirming they are
unused because the UDP implementation is missing.

The 22 tests in `syslog_backend_tests.rs` verify formatting and struct construction
but **none test that a UDP packet is actually sent**. The test for "unreachable host"
comments: "This may or may not error depending on network configuration".

**Fix:**

Implement using `tokio::net::UdpSocket`:

```rust
async fn send_to_syslog(&self, message: &str) -> AuditResult<()> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await
        .map_err(|e| AuditError::NetworkError(format!("Failed to bind UDP socket: {e}")))?;

    let addr = format!("{}:{}", self.host, self.port);
    let truncated = if message.len() > 1024 { &message[..1024] } else { message };

    tokio::time::timeout(self.timeout, socket.send_to(truncated.as_bytes(), &addr))
        .await
        .map_err(|_| AuditError::NetworkError(format!("Syslog send timed out to {addr}")))?
        .map_err(|e| AuditError::NetworkError(format!("UDP send failed: {e}")))?;

    Ok(())
}
```

Add a test that binds a local UDP socket and verifies receipt.

If implementing is not feasible immediately, label the backend as "Preview" and
update `VALUE_PROPOSITION.md` to qualify: "Audit logging to PostgreSQL and file
backends (syslog backend in preview)".

**Acceptance:**
- `test_syslog_log_single_event` actually verifies a UDP packet is received at a
  bound local socket, not just that `Ok(())` is returned
- `port` and `timeout` fields no longer require `#[allow(dead_code)]`

---

### F3 — Three backup providers compiled but never accessible

**Files:**
- `crates/fraiseql-server/src/backup/clickhouse_backup.rs`
- `crates/fraiseql-server/src/backup/redis_backup.rs`
- `crates/fraiseql-server/src/backup/elasticsearch_backup.rs`

**Problem:**

All three are compiled as part of the `backup` feature, exported from `backup/mod.rs`,
but never registered anywhere in `BackupManager`. They have `#[allow(dead_code)]`
with the comment "implemented but not yet registered in BackupManager". Their
`backup()` implementations are identical stubs.

**Fix:**

Either register them in `BackupManager` (with appropriate feature flags like
`clickhouse`, `elasticsearch`), or remove them from the public API surface until
they are real. Leaving them as dead code in an exported module misleads users
inspecting the API.

**Acceptance:** No `#[allow(dead_code)]` on any `pub struct` in the backup module.
Every exported struct is either used or removed.

---

## Track G — Code Hygiene Debt (Priority: Medium)

---

### G1 — `eprintln!` in production server code

**File:** `crates/fraiseql-server/src/backup/backup_manager.rs:60–63`

**Problem:**

```rust
Ok(_) => {
    eprintln!("✓ Backup provider '{}' healthy", name);  // ← stderr, not tracing
}
Err(e) => {
    eprintln!("✗ Backup provider '{}' failed health check: {:?}", name, e);  // ← stderr
}
```

These bypass the server's structured `tracing` subscriber entirely, cannot be
filtered or captured by log collectors, and will intermix with other stderr output.

**Fix:**

```rust
Ok(_) => tracing::info!(provider = %name, "Backup provider healthy"),
Err(e) => tracing::warn!(provider = %name, error = ?e, "Backup provider failed health check"),
```

**Acceptance:** `grep -rn "eprintln!\|println!" crates/fraiseql-server/src/ --include="*.rs"`
→ empty (CLI commands in `fraiseql-cli` are acceptable callers of `println!`).

---

### G2 — Deprecated `X-XSS-Protection` header in security middleware

**Files:**
- `crates/fraiseql-server/src/middleware/cors.rs:105`
- `crates/fraiseql-core/src/security/headers.rs:17`

**Problem:**

`X-XSS-Protection: 1; mode=block` is a deprecated header:
- Removed from Chrome 78 (2019) and modern Edge
- Removed from OWASP's Secure Headers Project recommendations
- Can introduce vulnerabilities in some older browser implementations
  (the "block" mode can be exploited to exfiltrate data via selective blocking)

For a security library advertising enterprise-grade security headers, shipping
a deprecated and potentially harmful header is a credibility issue.

**Fix:**

Remove the header entirely. The CSP header already handles XSS protection for
modern browsers. Add a comment explaining why it is absent:

```rust
// X-XSS-Protection deliberately omitted:
// This header is deprecated, removed from modern browsers, and can introduce
// vulnerabilities in older implementations. CSP provides XSS protection instead.
// See: https://owasp.org/www-project-secure-headers/#x-xss-protection
```

Update the tests in `headers.rs` that `assert!(headers.has("X-XSS-Protection"))`.

**Acceptance:**
- `grep -rn "X-XSS-Protection" crates/ --include="*.rs"` → empty
- `cargo test -p fraiseql-core` still passes

---

### G3 — `'unsafe-inline'` in production Content-Security-Policy

**File:** `crates/fraiseql-server/src/middleware/cors.rs:112–118`

**Problem:**

The production CSP in `security_headers_middleware` includes:

```
style-src 'self' 'unsafe-inline'
```

`'unsafe-inline'` for styles allows injected `<style>` tags or `style=` attributes,
weakening CSS injection protection. For a framework serving GraphQL APIs (not static
HTML), there is likely no legitimate need for inline styles in the HTTP response.

**Fix:**

Remove `'unsafe-inline'` from the production CSP:

```rust
"default-src 'self'; script-src 'self'; style-src 'self'"
```

If inline styles are genuinely needed (e.g., for the GraphQL playground), either:
- Use a nonce: `style-src 'self' 'nonce-{random}'`
- Or scope the permissive CSP to playground routes only, not all routes

**Note:** `SecurityHeaders::development()` in `headers.rs` includes both
`'unsafe-inline'` and `'unsafe-eval'` — that's acceptable for dev. The concern
is the production middleware.

**Acceptance:**
- `grep "unsafe-inline" crates/fraiseql-server/src/middleware/cors.rs` → only in
  `cors_layer()` (dev-only CORS function) or test code

---

### G4 — Silent serialization failure in RBAC response handlers

**File:** `crates/fraiseql-server/src/api/rbac_management.rs:153, 178, 198, 237, 263, 299`

**Problem:**

```rust
Ok(role) => (
    StatusCode::CREATED,
    Json(serde_json::to_value(role).unwrap_or_default()),  // ← null on failure
).into_response(),
```

If `serde_json::to_value(role)` fails (type doesn't implement `Serialize` correctly,
or contains non-serializable fields), the client receives:
- HTTP 201 Created
- Body: `null`

The client has no indication that the role was created but cannot be represented.
This is a silent data loss pattern: success is reported but the response is empty.

**Fix:**

```rust
let role_json = serde_json::to_value(role)
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to serialize role response");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "serialization_failed"})))
            .into_response()
    })?;
(StatusCode::CREATED, Json(role_json)).into_response()
```

Or preferably, define typed response DTOs that implement `Serialize` and use
`Json(dto)` directly (axum's `Json` extractor calls `serde_json::to_vec` internally
and returns 500 on failure).

**Acceptance:**
- No `.unwrap_or_default()` calls on `serde_json::to_value(...)` in non-test code
  in `rbac_management.rs`

---

### G5 — Observer retry config deserialization failure is silently swallowed

**File:** `crates/fraiseql-server/src/observers/runtime.rs:213–214`

**Problem:**

```rust
let retry_config: ObserverRetryConfig =
    serde_json::from_value(observer.retry_config.clone()).unwrap_or_default();
```

If the stored `retry_config` JSON is malformed (e.g., an old schema that changed
field names), the observer silently uses `ObserverRetryConfig::default()`. The
operator has no indication that the configured retry behavior is being ignored.

This is especially problematic in a multi-tenant environment where observers are
configured per-tenant: a misconfigured retry policy silently becomes "use defaults".

**Fix:**

```rust
let retry_config: ObserverRetryConfig =
    serde_json::from_value(observer.retry_config.clone())
        .inspect_err(|e| {
            tracing::warn!(
                observer_name = %observer.name,
                error = %e,
                "Failed to parse observer retry_config; using defaults"
            );
        })
        .unwrap_or_default();
```

This preserves the fallback behavior while making the failure observable.

**Acceptance:**
- The deserialization failure is logged at `WARN` level with the observer name and error

---

### G6 — `#[allow(unused_imports)]` in production modules

**Files:**
- `crates/fraiseql-arrow/src/flight_server/mod.rs:46`
- `crates/fraiseql-arrow/src/flight_server/handlers.rs:14`
- `crates/fraiseql-server/src/performance.rs:6`
- `crates/fraiseql-cli/src/commands/cost.rs:7`
- `crates/fraiseql-cli/src/schema/mod.rs:19`

**Problem:**

These are not suppressed lints with reasoning — they are unresolved dead imports
with a blanket `#[allow(unused_imports)]` to silence the compiler. Each should be
investigated: either the import is needed and the dead code should be activated, or
the import should be removed.

**Fix:**

For each file: remove the `#[allow(unused_imports)]`, then fix the resulting warning
by either removing the unused import or restoring the code that uses it.

**Acceptance:** `grep -rn "#\[allow(unused_imports)" crates/ --include="*.rs"` → empty.

---

### G7 — `fraiseql-auth` lib.rs has 38 module-level `#![allow]` attributes

**File:** `crates/fraiseql-auth/src/lib.rs:6–44`

**Problem:**

The auth crate suppresses 38 pedantic lints at the module level, several of which
are counterproductive:

```rust
#![allow(clippy::redundant_clone)]  // "explicit clone at API boundaries for clarity"
```
`clippy::redundant_clone` fires when a `.clone()` is performed on a value that
is about to be moved. Suppressing it "for clarity" hides actual unnecessary
allocations in a hot authentication path.

```rust
#![allow(clippy::useless_format)]  // "for consistency with other branches"
```
`clippy::useless_format` fires when `format!("{}", x)` is used where `x.to_string()`
suffices. Suppressing it "for consistency" preserves worse code.

```rust
#![allow(clippy::missing_errors_doc)]  // "error types are self-documenting"
```
For a security crate, all public functions that can fail need documented error
conditions. Self-documenting error types are not a substitute for documenting
when each error variant is returned.

The auth crate has substantially more module-level allows than any other crate.
This is a sign that the port from `fraiseql-server` (noted in the comment
"migrated from fraiseql-server") was done mechanically without addressing lints.

**Approach:**

1. Remove `#![allow(clippy::redundant_clone)]` and fix resulting warnings
   (remove genuine redundant clones)
2. Remove `#![allow(clippy::useless_format)]` and fix resulting warnings
3. Target the same pattern as `fraiseql-server`: keep allows that have genuine
   axum-imposed justifications; eliminate cosmetic ones
4. The `missing_errors_doc` suppression should be addressed as part of B3 in the
   original remediation plan, but extended to cover `fraiseql-auth` as well

**Acceptance:** `wc -l crates/fraiseql-auth/src/lib.rs | awk '{print $1}'` < 20 lines
of allow attributes (from 38).

---

## Track H — Documentation Accuracy Extensions (Priority: Medium)

These complement Track A from the first assessor's plan.

---

### H1 — Remove backup feature claims from VALUE_PROPOSITION.md

**File:** `docs/VALUE_PROPOSITION.md`

**Problem:**

Lines referencing backup features (automated scheduling, S3/GCS integration,
point-in-time recovery, backup encryption and signing) describe an unimplemented
system. As noted in F1, all backup providers are stubs.

**Fix:**

Remove or qualify the backup section:

```diff
-**Disaster Recovery & Backup**
-- Backup and restore procedures for compiled schemas
-- Automated backup scheduling
-- Integration with cloud storage (S3, GCS)
-- Backup encryption and signing
-- 487-page production documentation with runbooks and disaster recovery
+**Disaster Recovery & Backup**
+- Compiled schema rollback (recompile from previous schema.json)
+- Operational runbooks for database failure scenarios (docs/runbooks/)
+- Backup infrastructure framework (preview — providers are structural scaffolding)
```

**Acceptance:** Claims in the docs are implemented or clearly labeled preview.

---

### H2 — Document RBAC Management API security model

**File:** `docs/architecture/overview.md` or a new `docs/features/rbac.md`

**Problem:**

The RBAC management API (`/api/roles`, `/api/permissions`, etc.) is only available
when `#[cfg(feature = "observers")]` is enabled and a database pool is provided.
This is not documented anywhere. Users enabling the `observers` feature may not
realize they've implicitly enabled an RBAC management API.

Until E2 is fixed, this API is also unauthenticated.

**Fix (requires E2 to be fixed first):**

Document the RBAC API:
- How to enable it (`observers` feature + database pool)
- Authentication requirements (OIDC must be configured)
- Tenant isolation behavior (currently: None, always creates global roles)
- Schema migration (`ensure_schema()` now called at startup per E2 fix)

**Acceptance:** A user can find how to use and secure the RBAC API from the docs
without reading the Rust source.

---

### H3 — Document `observers-full` deprecation in CHANGELOG and docs

**File:** `crates/fraiseql-server/Cargo.toml:138–139`

**Problem:**

```toml
# Deprecated alias — use observers-enterprise instead. Will be removed in 2.3.0.
observers-full = ["observers-enterprise"]
```

There is no CHANGELOG entry for this deprecation. Users scanning CHANGELOG.md for
breaking changes affecting 2.3.0 will not find it. There is no "2.3.0 migration
guide" or deprecation notice in the docs.

**Fix:**

Add to `CHANGELOG.md` under `[Unreleased]`:

```markdown
### Deprecated
- `observers-full` feature flag: use `observers-enterprise` instead. This alias
  will be removed in 2.3.0. Update your `Cargo.toml` dependency features accordingly.
```

Add to `docs/architecture/overview.md` under the feature flag table:

```
| `observers-full` | **Deprecated** alias for `observers-enterprise`. Remove before 2.3.0. |
```

**Acceptance:** `grep "observers-full" CHANGELOG.md` → non-empty.

---

## Execution Order

These tracks should be executed in the following order alongside the original plan:

### Immediate (before next release tag)

1. **E1** — Fix GET handler auth context drop (security bug, 1 hour fix)
2. **E2** — Fix RBAC routes unauthenticated (security bug, 2–4 hours)
3. **G1** — Replace `eprintln!` with tracing (30 minutes)

### Week 1 (alongside Track A from original plan)

4. **F1** — Label backup system as preview; remove value prop claims
5. **F2** — Implement syslog UDP sending or label as preview
6. **H1** — Remove backup claims from VALUE_PROPOSITION.md
7. **H3** — Add observers-full deprecation to CHANGELOG

### Week 2 (alongside Track B from original plan)

8. **G2** — Remove deprecated X-XSS-Protection header
9. **G3** — Remove `unsafe-inline` from production CSP
10. **G4** — Fix silent RBAC serialization failures
11. **G5** — Add warning log for observer retry config deserialization failure
12. **F3** — Remove or register dead backup providers

### Week 3–4 (alongside Track C and D)

13. **G6** — Remove `#[allow(unused_imports)]` from production modules
14. **G7** — Trim fraiseql-auth module-level allows
15. **H2** — Document RBAC API security model

---

## Definition of Done (Extension)

The extension remediation is complete when all of the following hold **in addition to**
the original plan's Definition of Done:

1. `graphql_get_handler` uses `OptionalSecurityContext` — RLS enforced on GET
2. RBAC management routes require authentication when OIDC is configured
3. `RbacDbBackend::ensure_schema()` is called at server startup
4. `VALUE_PROPOSITION.md` contains no unimplemented backup/DR feature claims
5. `SyslogAuditBackend::send_to_syslog()` either sends UDP or is labeled preview
6. No `#[allow(dead_code)]` on `pub struct` in the backup module
7. No `eprintln!` in `crates/fraiseql-server/src/` (CLI excluded)
8. `X-XSS-Protection` header removed from production middleware
9. `'unsafe-inline'` absent from production CSP in `cors.rs`
10. No `.unwrap_or_default()` on `serde_json::to_value(...)` in RBAC handlers
11. `observers-full` deprecation present in `CHANGELOG.md`
12. `wc -l crates/fraiseql-auth/src/lib.rs` allow-attributes count ≤ 20
