# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension 16

*Written 2026-03-05. Sixteenth independent assessor.*
*Extends all fifteen preceding plans without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings confirmed against HEAD (latest commit: `140eea10c`).*
*Scope: server routing security, field encryption wiring, migrate credential exposure,*
*OIDC validator bounds, config validation gaps, and APQ cache key correctness.*

---

## Context and Methodology

All fifteen existing plans were reviewed before this assessment. The following known findings
are **not** repeated here:
- Authentication bypass on GET/RBAC handlers (Extensions I, X)
- MCP `require_auth` flag no-op (Extension XI)
- RBAC management error handling (Extension X)
- SQL injection in Arrow Flight, window queries, tenancy `where_clause` (Extensions II, VIII, XV)
- Webhook protocol mismatches and replay attacks (Extensions IV, XII)
- Observer failover health-check threshold stored but never consulted (Extension III)
- Proc-macro incorrect async tracing (Extension IX)
- Vault `reqwest::Client` per-call creation (Extension XIII)
- `custom_scalar.rs` zero tests (Extension XIV)
- `rustls` 0.21.x CVE in Cargo.lock (Extension XIV)

The following areas were not previously examined: **server routing fail-open behavior**,
**field encryption wiring**, **migrate credential exposure**, **OIDC clock-skew bounds**,
**config numeric validation gaps**, and **env-var expansion documentation lie**.

Every finding below was verified by reading source code directly.

---

## What Works Well

- The server routing correctly **fails closed** for introspection: when
  `introspection_require_auth = true` but no OIDC is configured, the introspection endpoint
  is simply not mounted (lines 178–182 of `routing.rs`). This is the right behavior.
- `FraiseQLConfig::validate()` correctly enforces that JWT secret or auth domain is present
  when their respective auth providers are selected — the most security-critical config fields
  are guarded.
- `OidcValidatorConfig::validate()` correctly mandates that `audience` is non-`None`, which
  prevents token-confusion attacks. The error message is clear and actionable.
- The `FieldEncryptionService` type and its `from_schema` constructor are well-designed; the
  architecture of the decryption middleware in `graphql.rs` is correct. The problem is
  entirely in the wiring (see T2 below), not the design.

---

## Findings

---

### T1 — Design API endpoints mounted **unprotected** when `require_auth = true` but no OIDC [CRITICAL]

**File:** `crates/fraiseql-server/src/server/routing.rs`, lines 278–297

**Evidence:**

```rust
// routing.rs:258–297
if self.config.design_api_require_auth {
    if let Some(ref validator) = self.oidc_validator {
        // ... mount with auth middleware (correct)
    } else {
        warn!(
            "design_api_require_auth is true but no OIDC configured - design endpoints unprotected"
        );
        // ADD UNPROTECTED DESIGN ENDPOINTS  ← WRONG
        let design_router = Router::new()
            .route("/design/federation-audit", post(api::design::federation_audit_handler::<A>))
            .route("/design/cost-audit", post(api::design::cost_audit_handler::<A>))
            .route("/design/cache-audit", post(api::design::cache_audit_handler::<A>))
            .route("/design/auth-audit", post(api::design::auth_audit_handler::<A>))
            .route("/design/compilation-audit", post(api::design::compilation_audit_handler::<A>))
            .route("/design/audit", post(api::design::overall_design_audit_handler::<A>))
            .with_state(state.clone());
        app = app.nest("/api/v1", design_router); // ← mounted without any auth
    }
}
```

**`design_api_require_auth` defaults to `true`** (line 438 of `server_config.rs`):
```rust
design_api_require_auth: true, // Require auth for design endpoints
```

**The fail-open path fires whenever a developer configures `require_auth = true`
(the default) but has not yet configured OIDC.** The endpoint exposes internal
design audit APIs — federation audit, compilation audit, auth audit, cost model — to
any unauthenticated request.

**Contrast with introspection** (lines 178–182, same file):
```rust
// CORRECT: introspection fails CLOSED
} else {
    warn!("introspection_require_auth is true but no OIDC configured - introspection and schema export disabled");
    // → nothing mounted
}
```

Introspection fails *closed* (endpoint not mounted). Design API fails *open* (mounted without auth).
This is inconsistent and wrong.

**Fix:**
```rust
} else {
    warn!(
        "design_api_require_auth is true but no OIDC configured - \
         design endpoints NOT mounted. Configure OIDC or set \
         design_api_require_auth = false to expose them."
    );
    // Do NOT mount — same behavior as introspection
}
```

**Acceptance:**
- `design_api_require_auth = true` + no OIDC → design endpoints return 404.
- `design_api_require_auth = false` + no OIDC → design endpoints mounted without auth (explicit opt-in).
- Behavior matches introspection handling.

---

### T2 — Field encryption: server builder never initializes `GraphQLState::field_encryption` [HIGH]

**Files:**
- `crates/fraiseql-server/src/routes/graphql.rs`, line 159
- `crates/fraiseql-server/src/server/builder.rs` and `extensions.rs` (no field_encryption call)
- `crates/fraiseql-core/src/compiler/codegen.rs`, line 252

**Evidence:**

In `GraphQLState`:
```rust
// routes/graphql.rs:117, 159
pub field_encryption: Option<Arc<crate::encryption::middleware::FieldEncryptionService>>,
// ...
field_encryption: None, // ← always None in the Default impl
```

In the server builder (both `builder.rs` and `extensions.rs`), every schema-derived
subsystem has a `*_from_schema` call:
```rust
let error_sanitizer     = Self::error_sanitizer_from_schema(&schema);
let state_encryption    = Self::state_encryption_from_schema(&schema)?;
let rate_limiter        = Self::rate_limiter_from_schema(&schema).await;
// ... 6 more *_from_schema calls ...
// ← NO field_encryption_from_schema call exists
```

The activation logic in `graphql.rs` (lines 810–822) is correct:
```rust
#[cfg(feature = "secrets")]
if let Some(ref encryption) = state.field_encryption {
    if encryption.has_encrypted_fields() {
        encryption.decrypt_response(&mut response_json).await...
    }
}
```

But `state.field_encryption` is **always `None`** because the builder never sets it.
Additionally, the codegen (`codegen.rs:252`) always sets `encryption: None` on compiled
`FieldDefinition` structs:
```rust
encryption: None, // codegen.rs — field encryption metadata dropped during compilation
```

This matters because `FieldEncryptionService::from_schema()` (in `fraiseql-secrets`) filters
fields by `f.encryption.is_some()`. With all fields having `encryption: None`,
`from_schema()` would always build an empty service even if it were called.

**Net effect:** Field-level encryption is **completely non-functional** through the standard
server construction path. A developer who reads the documentation and marks database columns
as encrypted will find their data transmitted in plaintext.

**Two independent gaps must both be fixed:**

**Gap A — Codegen must propagate encryption config from IR to compiled fields.**

The compiler IR (`schema/intermediate.rs`) needs to carry field encryption config, and
`codegen.rs::map_fields()` must preserve it instead of hardcoding `None`.

**Gap B — Server builder must create and wire `FieldEncryptionService`.**

Add a builder method analogous to the existing `*_from_schema` helpers:
```rust
// In server/builder.rs
#[cfg(feature = "secrets")]
fn field_encryption_from_schema(
    schema: &CompiledSchema,
    secrets_config: Option<&SecretsConfig>,
) -> Option<Arc<FieldEncryptionService>> {
    // Only activate when secrets feature is enabled and schema has encrypted fields
    let adapter = secrets_config.map(|cfg| DatabaseFieldAdapter::from_config(cfg))?;
    let service = FieldEncryptionService::from_schema(schema, Arc::new(adapter));
    if service.has_encrypted_fields() { Some(Arc::new(service)) } else { None }
}
```

And wire it into `GraphQLState` construction.

**Acceptance:**
- A compiled schema with one field carrying `FieldEncryptionConfig` causes
  `GraphQLState::field_encryption` to be `Some(...)` at server startup.
- A round-trip test: insert encrypted ciphertext into mock DB → query via GraphQL →
  response contains decrypted plaintext.
- `codegen.rs` passes through the `encryption` field from IR when present.

---

### T3 — `migrate` passes database URL (including credentials) as process argv [HIGH]

**File:** `crates/fraiseql-cli/src/commands/migrate.rs`, lines 142–145, 160–170, 184–186

**Evidence:**
```rust
// migrate.rs:142–145
let status = Command::new("confiture")
    .args(["up", "--source", dir, "--database-url", database_url])
    .status()
    .context("Failed to execute confiture")?;
```

The `database_url` value — which for PostgreSQL takes the form
`postgresql://user:password@host:5432/dbname` — is passed as a command-line argument to the
external `confiture` process. All three migration verbs (`up`, `down`, `status`) do this.

**Why this matters:**
1. **Process argument lists are world-readable on Linux.** Any local user can run `cat
   /proc/<pid>/cmdline` or `ps aux` and see the full connection string, including the password.
   The window is narrow (milliseconds) but real in CI environments where multiple build agents
   share a host.
2. **Shell history and audit logs** may record the argument if the CLI is invoked via shell
   script.
3. **`confiture` itself** may log its argv in verbose mode or on error, writing credentials
   to stdout/stderr which may be captured in CI logs.

**Mitigation options** (in order of preference):

1. **Environment variable**: `confiture` likely honours `DATABASE_URL` from the environment.
   Set it as an env var on the spawned process instead of an argument:
   ```rust
   let status = Command::new("confiture")
       .args(["up", "--source", dir])
       .env("DATABASE_URL", database_url)  // not visible in argv
       .status()?;
   ```

2. **Stdin pipe**: If `confiture` supports reading the URL from stdin, use `Stdio::piped()`
   and write the URL to the child's stdin immediately after spawn.

3. **Temp credentials file**: Write the URL to a `0600` temp file, pass `--database-url-file
   /tmp/xyz`, delete after child exits.

**Acceptance:**
- `ps aux | grep confiture` during a migration does not show the database password.
- `confiture` subprocess receives credentials through environment variable `DATABASE_URL`.

---

### T4 — `expand_env_vars` documents `$VAR` support but regex only handles `${VAR}` [MEDIUM]

**File:** `crates/fraiseql-core/src/config/mod.rs`, lines 809–837

**Evidence:**
```rust
/// Expand environment variables in a string.
///
/// Supports `${VAR}` and `$VAR` syntax.  // ← documented claim
fn expand_env_vars(content: &str) -> String {
    static ENV_VAR_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")  // ← only ${VAR}
            .expect("env var regex is valid")
    });
    // ...
}
```

The regex `\$\{([A-Za-z_][A-Za-z0-9_]*)\}` matches only the `${VAR}` form.
`$VAR` (no braces) is not matched and is silently left as a literal string in the config.

**Operator impact:** A developer following standard shell conventions who writes:
```toml
[database]
url = "$DATABASE_URL"
```
gets a runtime error about a malformed database URL. Only `"${DATABASE_URL}"` works.
The documentation inside the source file (visible via `cargo doc`) misleads them.

**Fix options:**
1. Add a second regex pass for `$VAR` without braces.
2. Remove the `$VAR` claim from the doc comment to match the actual behavior.

Option 2 is safer: `$VAR` without braces is ambiguous in TOML contexts (e.g.,
`url = "$HOST:5432"` would incorrectly expand `$HOST` when the intent is a literal `$`).
Removing the claim avoids a footgun.

**Acceptance:**
- The doc comment accurately describes the supported syntax.
- If `$VAR` is implemented: a test verifies `url = "$DATABASE_URL"` expands correctly.
- If removed: a test verifies `url = "$DATABASE_URL"` is left unexpanded and produces a clear error.

---

### T5 — `FraiseQLConfig::validate()` omits all numeric bounds checks [MEDIUM]

**File:** `crates/fraiseql-core/src/config/mod.rs`, lines 766–800

**Evidence:**
```rust
pub fn validate(&self) -> Result<()> {
    // Database URL required
    if self.database.url.is_empty() && self.database_url.is_empty() { ... }
    // Validate auth config (JWT secret / auth domain)
    if self.auth.enabled { ... }
    Ok(())
}
```

The following fields accept any value within their numeric type's range and are never checked:

| Field | Type | Dangerous value | Impact |
|-------|------|----------------|--------|
| `database.max_connections` | `u32` | `0` | sqlx panics at pool creation |
| `database.min_connections` | `u32` | `> max_connections` | sqlx panics at pool creation |
| `database.connection_timeout_secs` | `u64` | `0` | queries never timeout |
| `rate_limiting.requests_per_window` | `u32` | `0` | every request is rate-limited immediately |
| `server.port` | `u16` | `0` | OS assigns ephemeral port; service unreachable |

None of these produce a `FraiseQLError::Configuration` at validation time. The error
(if any) surfaces later as a panic or opaque runtime failure.

**Fix:** Add bounds checks to `validate()`:
```rust
if self.database.max_connections == 0 {
    return Err(FraiseQLError::Configuration {
        message: "database.max_connections must be > 0".to_string(),
    });
}
if self.database.min_connections > self.database.max_connections {
    return Err(FraiseQLError::Configuration {
        message: format!(
            "database.min_connections ({}) must be ≤ max_connections ({})",
            self.database.min_connections, self.database.max_connections
        ),
    });
}
// ... similar for other fields
```

**Acceptance:**
- `FraiseQLConfig::validate()` returns `Err` for each of the five dangerous values above.
- A unit test for each error case.
- No silent numeric misconfigurations survive `validate()`.

---

### T6 — OIDC `clock_skew_secs` has no upper bound; misconfiguration accepts arbitrarily old tokens [MEDIUM]

**File:** `crates/fraiseql-core/src/security/oidc.rs`, lines 118–119, 605

**Evidence:**
```rust
// OidcValidatorConfig:
#[serde(default = "default_clock_skew")]
pub clock_skew_secs: u64,  // ← u64, unbounded

// In validate_token():
validation.leeway = self.config.clock_skew_secs;  // passed directly to jsonwebtoken
```

`jsonwebtoken`'s `leeway` is applied symmetrically: a token whose `exp` is
`now - leeway` is still accepted. With `clock_skew_secs: 86400` (one day), tokens
expired 24 hours ago are accepted. With `u64::MAX`, every expired token ever issued
by the configured issuer is accepted.

The `OidcValidatorConfig::validate()` method checks that `audience` is present and
`allowed_algorithms` is non-empty, but does **not** check `clock_skew_secs`.

**OIDC Core 1.0 §3.1.3.7** recommends that the leeway for `exp` be "a few minutes" to
account for clock skew between the IdP and the resource server. Anything beyond 5 minutes
is outside specification intent.

**Fix:** Add a maximum to `validate()`:
```rust
const MAX_CLOCK_SKEW_SECS: u64 = 300; // 5 minutes — OIDC Core recommendation

if self.clock_skew_secs > MAX_CLOCK_SKEW_SECS {
    return Err(SecurityError::SecurityConfigError(format!(
        "clock_skew_secs ({}) exceeds the maximum of {} seconds. \
         Large values allow significantly expired tokens to be accepted.",
        self.clock_skew_secs, MAX_CLOCK_SKEW_SECS
    )));
}
```

**Acceptance:**
- `OidcValidatorConfig::validate()` returns `Err` for `clock_skew_secs > 300`.
- A unit test verifies that `clock_skew_secs = 301` is rejected.
- A unit test verifies that `clock_skew_secs = 60` (the default) is accepted.

---

### T7 — APQ cache key silently degrades on variable serialization failure [LOW]

**File:** `crates/fraiseql-core/src/apq/hasher.rs`, line 119

**Evidence:**
```rust
pub fn hash_query_with_variables(query: &str, variables: &JsonValue) -> String {
    // ...
    let variables_json = serde_json::to_string(variables).unwrap_or_default();
    //                                                    ^^^^^^^^^^^^^^^^
    //                                  "" on failure → combined = "hash:"
    let combined = format!("{query_hash}:{variables_json}");
    // ...
}
```

If `serde_json::to_string(variables)` fails (which requires the variables `JsonValue` to
contain a map keyed with non-string types — a constraint that the `serde_json` crate's
`Value::Object` type guarantees cannot happen), the empty string is used, making all
variable combinations hash to the same key as an empty variable set.

While `serde_json::to_string` on a `serde_json::Value` is in practice **infallible** — the
type system guarantees map keys are always `String` — the `unwrap_or_default()` silently
hides the impossible error path in a way that makes auditors uncertain. More importantly, the
comment on lines 100–102 says "Different variable values ALWAYS produce different hashes", a
guarantee that `unwrap_or_default()` technically violates.

**Fix:** Replace with an explicit `expect()` that documents why the failure is impossible:
```rust
let variables_json = serde_json::to_string(variables)
    .expect("serde_json::Value serialization is infallible: map keys are always String");
```

Or, if fallibility is considered worth handling:
```rust
let variables_json = serde_json::to_string(variables)
    .map_err(|e| tracing::error!(error = %e, "APQ variable serialization failed"))?;
```
(changing the return type to `Option<String>` and propagating `None` to the caller to
avoid a cache hit on a degraded key).

**Acceptance:**
- `unwrap_or_default()` removed from `hash_query_with_variables`.
- The `expect()` variant includes a comment explaining the infallibility.
- The doc comment guarantee "ALWAYS produce different hashes" is not undermined by a silent
  no-op error path.

---

## Severity Summary

| ID | Component | Issue | Severity |
|----|-----------|-------|----------|
| T1 | `server/routing.rs` | Design API fail-open when `require_auth=true` but no OIDC | **Critical** |
| T2 | `routes/graphql.rs`, `server/builder.rs`, `compiler/codegen.rs` | Field encryption never initialized in server builder; codegen drops encryption metadata | **High** |
| T3 | `commands/migrate.rs` | Database credentials passed as process argv to external binary | **High** |
| T4 | `config/mod.rs` | `expand_env_vars` doc claims `$VAR` support; only `${VAR}` works | **Medium** |
| T5 | `config/mod.rs` | `validate()` omits all numeric bounds checks (pool size, port, timeouts) | **Medium** |
| T6 | `security/oidc.rs` | `clock_skew_secs` uncapped; misconfiguration accepts indefinitely old tokens | **Medium** |
| T7 | `apq/hasher.rs` | `unwrap_or_default()` on infallible serialization silently violates documented cache guarantee | **Low** |

---

## Recommended Execution Order

1. **T1** (routing.rs): one-line fix, no API surface change, highest risk — do this first.
2. **T3** (migrate credentials): switch to env-var passing — low risk, high security gain.
3. **T6** (clock skew cap): one validation check added — no behavior change for correct configs.
4. **T5** (config bounds): add validation checks — no behavior change for correct configs.
5. **T2** (field encryption wiring): requires both codegen and builder changes; coordinate
   with the IR/schema owner. Consider whether encryption metadata should flow through TOML
   config (compile time) or at server startup from a separate secrets config.
6. **T4** (env var docs): decide whether to implement `$VAR` or remove the claim.
7. **T7** (APQ): cosmetic / doc correctness — lowest urgency.

---

## Non-Issues Investigated and Cleared

The following were investigated but found to be **not bugs**:

- **SQL Server `SELECT TOP {lim}`**: `lim` is a `u32`; format injection is type-impossible.
  Not an injection risk.
- **CORS empty origins**: when empty, defaults to `localhost:3000` with a warning (routing.rs
  line 406–412). Not a wildcard allow-all; the agent's claim was incorrect.
- **Directive evaluator unknown-directive pass-through**: correct GraphQL behavior. The
  `@skip`/`@include` evaluator is for query execution, not schema-level access control.
  Security directives operate at the resolver level, not here.
- **`serde_json::to_string` failure in APQ**: technically infallible given `JsonValue`'s type
  constraints; `unwrap_or_default()` is sloppy but the cache correctness concern is theoretical.
  Flagged as T7 for cleanliness, not active correctness risk.
- **OIDC audience `None` branch (`validate_aud = false`)**: the `OidcValidatorConfig::validate()`
  method (line 291) returns an error if audience is `None`, making the `else` branch at line
  601 unreachable in practice. The defense-in-depth behavior of the `else` branch is fine.
