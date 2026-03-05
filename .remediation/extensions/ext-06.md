# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension VI

*Written 2026-03-05. Seventh assessor's findings.*
*Extends all five preceding plans without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings confirmed against HEAD (latest commit: `140eea10c`).*

---

## Context and Methodology

This assessment read all five existing plans fully, then performed an independent deep dive on
four areas that previous assessors left unexamined: the CLI code-generation commands, the
Arrow Flight authentication subsystem, the fraiseql-test-utils helper library, and the
cache dependency graph. Every finding was verified directly against the source before reporting.

| Category | Count | Severity |
|---|---|---|
| CLI code generation bugs | 1 | Critical |
| Panic risks in production handlers | 2 | High |
| Cache graph correctness | 1 | High |
| Security config silent fallback (new files) | 1 | High |
| SQL DDL correctness | 1 | Medium |
| Test utility correctness | 2 | Medium |
| Observability gap | 1 | Low |

---

## Track P — CLI Code Generation Defects (Priority: Critical)

---

### P1 — `generate-views` Always Emits Literal `public.schema_placeholder` in Generated SQL

**File:** `crates/fraiseql-cli/src/commands/generate_views.rs`
**Lines:** 253, 280, 291, 306

**Problem:**

The `generate-views` command accepts an `--entity` flag whose value is validated against the
compiled schema (`validate_entity()`, line 181). The entity name is then passed through the
call chain down to four SQL-building functions. All four of them receive the entity name
but use it only in SQL *comments* — never in the `FROM` clause:

```rust
// generate_vector_arrow_view (line 273)
fn generate_vector_arrow_view(sql: &mut String, entity: &str, view_name: &str) {
    sql.push_str(&format!("CREATE VIEW {view_name} AS\n"));
    sql.push_str("SELECT\n");
    sql.push_str("    id,\n");
    sql.push_str(&format!("    -- {entity} entity fields\n"));  // ← entity used here…
    sql.push_str("    created_at,\n");
    sql.push_str("    updated_at\n");
    sql.push_str("FROM public.schema_placeholder\n");           // ← …not here
    sql.push_str("WHERE archived_at IS NULL;\n");
}
```

The same pattern applies to `generate_table_vector_view` (line 291),
`generate_table_arrow_view` (line 306), and the catch-all fallback (line 253).

Every SQL file produced by `fraiseql generate-views` is syntactically valid SQL but
**always references a non-existent relation `public.schema_placeholder`**. Running the output
against a real PostgreSQL database will immediately fail with:

```
ERROR:  relation "public.schema_placeholder" does not exist
```

The feature is effectively non-functional for any user who applies the generated SQL.

**Root cause:** The `entity` parameter was plumbed into the sub-functions but the FROM
clause was never wired up, presumably left as a template placeholder that was never replaced.

**Fix:**

Each sub-function should derive the underlying table name from the entity's `sql_source`
field (available on the `CompiledType`), or fall back to a snake-case conversion of the
entity name. The simplest correct fix:

```rust
fn generate_vector_arrow_view(sql: &mut String, entity: &str, view_name: &str) {
    // Derive table name: entity "UserProfile" → "user_profile"
    let table = to_snake_case(entity);
    sql.push_str(&format!("CREATE VIEW {view_name} AS\n"));
    sql.push_str("SELECT\n");
    sql.push_str("    id,\n");
    sql.push_str("    created_at,\n");
    sql.push_str("    updated_at\n");
    sql.push_str(&format!("FROM public.{table}\n"));
    sql.push_str("WHERE archived_at IS NULL;\n");
}
```

Alternatively, thread the `sql_source` string from the schema type down into these functions
so the FROM clause uses the authoritative source name rather than a derived name.

**Acceptance:**
```bash
grep "schema_placeholder" $(fraiseql generate-views --entity User --view-name va_user)
# → empty
```

---

## Track Q — Panic Risks in Production Handlers (Priority: High)

---

### Q1 — Arrow Flight Auth Reads `FLIGHT_SESSION_SECRET` Per-Call with `.expect()`

**File:** `crates/fraiseql-arrow/src/flight_server/auth.rs`
**Lines:** 49–50, 81–82

**Problem:**

Both `create_session_token()` and `validate_session_token()` read the environment variable
on every call and `.expect()` if it is absent:

```rust
// line 49
pub fn create_session_token(user_id: &str) -> Result<String, Status> {
    let secret = std::env::var("FLIGHT_SESSION_SECRET")
        .expect("FLIGHT_SESSION_SECRET environment variable must be set for Flight \
                 authentication (use 'openssl rand -hex 32' to generate)");
    // …
}

// line 81
pub fn validate_session_token(token: &str) -> Result<FlightClaims, Status> {
    let secret = std::env::var("FLIGHT_SESSION_SECRET")
        .expect("FLIGHT_SESSION_SECRET environment variable must be set for Flight \
                 authentication");
    // …
}
```

**Impact:**

If `FLIGHT_SESSION_SECRET` is unset, the panic occurs *inside* a gRPC request handler, not
at server startup. The consequence is:

- The Arrow Flight server appears to start normally (no startup panic).
- Any authenticated request panics the handler thread, causing an abort that propagates
  through the Tokio runtime.
- Operators have no early signal that the secret is missing — only the first client
  authentication attempt reveals the problem, in production.

**Fix:**

Read the secret once at server initialization and store it in the `FlightAuthInterceptor`
or equivalent config struct. Return a proper `Status::internal` error from the functions
if initialization was skipped (not a `.expect()`).

```rust
pub struct FlightSessionConfig {
    secret: Zeroizing<String>,  // read once at startup
}

impl FlightSessionConfig {
    pub fn from_env() -> Result<Self, Status> {
        let secret = std::env::var("FLIGHT_SESSION_SECRET")
            .map_err(|_| Status::internal(
                "FLIGHT_SESSION_SECRET not set — configure before starting server"
            ))?;
        if secret.is_empty() {
            return Err(Status::internal("FLIGHT_SESSION_SECRET must not be empty"));
        }
        Ok(Self { secret: Zeroizing::new(secret) })
    }
}
```

**Acceptance:** `FLIGHT_SESSION_SECRET=` (empty) at startup returns a startup error, not a
runtime panic on first authenticated request.

---

### Q2 — `http.rs` `.expect()` on `HeaderValue` Parse in Production Response Handler

**File:** `crates/fraiseql-error/src/http.rs`
**Line:** 147

**Problem:**

```rust
resp.headers_mut().insert(
    "Retry-After",
    retry_after.parse().expect(
        "retry_after is a numeric string and always parses as HeaderValue"
    ),
);
```

The comment accurately describes the current code path (`retry_after_header()` converts a
`u64` to `String`, which is always a valid ASCII numeric string and always produces a valid
`HeaderValue`). However:

1. `.expect()` in a production response handler violates the codebase's no-panic discipline.
   Any future refactor that changes `retry_after_header()` to return a non-numeric string
   (e.g. a quoted date per RFC 7231 §7.1.3) will cause a silent panic regression.
2. Clippy pedantic (`clippy::unwrap_used` / `clippy::expect_used`) will flag this.

**Fix:**

```rust
if let Ok(header_value) = retry_after.parse::<HeaderValue>() {
    resp.headers_mut().insert("Retry-After", header_value);
}
// If parsing somehow fails, the response is still returned without the header —
// which is safe (missing Retry-After degrades gracefully).
```

**Acceptance:** `grep -n "\.expect(" crates/fraiseql-error/src/http.rs` → empty.

---

## Track R — Cache Dependency Graph Correctness (Priority: High)

---

### R1 — `CascadeInvalidator::add_dependency` Does Not Detect Indirect Cycles

**File:** `crates/fraiseql-core/src/cache/cascade_invalidator.rs`
**Lines:** 115–136, 275–298

**Problem:**

`add_dependency` guards only against the trivial self-reference case:

```rust
pub fn add_dependency(&mut self, dependent_view: &str, dependency_view: &str) -> Result<()> {
    if dependent_view == dependency_view {
        return Err(/* self-dependency error */);
    }
    // No check for indirect cycles
    self.view_dependencies
        .entry(dependent_view.to_string())
        .or_insert_with(HashSet::new)
        .insert(dependency_view.to_string());
    // …
}
```

The public method `has_dependency_path(dependent, dependency)` exists on the same struct
and traverses the graph with a visited set — it is exactly the tool needed for cycle
detection — but it is never called from `add_dependency`.

**Result:** An indirect cycle (`v_a → v_b → v_a`) can be silently registered. The
`cascade_invalidate` BFS will terminate correctly (its own visited set prevents looping),
but the dependency graph is in a semantically invalid state. PostgreSQL views cannot
cyclically depend on each other, so a cycle in this graph represents a data modelling error
that should be surfaced at configuration time, not silently tolerated.

The test at line 519 acknowledges the gap with the comment:
```rust
// Note: Can't actually add cycle due to self-dependency check
```
but the comment is incorrect: only *direct* self-dependency is blocked.

**Fix:**

Before inserting the new edge, check whether the proposed dependency view already
transitively reaches the dependent view (which would create a cycle):

```rust
pub fn add_dependency(&mut self, dependent_view: &str, dependency_view: &str) -> Result<()> {
    if dependent_view == dependency_view {
        return Err(/* self-dependency */);
    }
    // Cycle detection: would dependency_view reach dependent_view?
    if self.has_dependency_path(dependency_view, dependent_view) {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Adding dependency '{}' → '{}' would create a cycle",
                dependent_view, dependency_view
            ),
            path: Some("cascade_invalidator::add_dependency".to_string()),
        });
    }
    // … insert edge …
}
```

**Acceptance:** A test should pass:
```rust
let mut inv = CascadeInvalidator::new();
inv.add_dependency("v_a", "v_b").unwrap();
inv.add_dependency("v_b", "v_c").unwrap();
let result = inv.add_dependency("v_c", "v_a");  // would create cycle
assert!(result.is_err());
```

---

## Track S — Security Config Silent Fallback (New Files)

---

### S1 — `validate_aud = false` Fallback in `oidc.rs` and `auth_middleware.rs`

**Files:**
- `crates/fraiseql-core/src/security/oidc.rs:601`
- `crates/fraiseql-core/src/security/auth_middleware.rs:552`

**Context:** Extension V (I1) identified `validate_aud = false` in `jwt.rs:86`. That fix
covered only the low-level `JwtValidator` library type. The same insecure-by-default pattern
appears in two additional, higher-level request-processing paths.

**Problem:**

Both files contain an identical pattern:

```rust
// oidc.rs:601  (inside the token-validation hot path)
if let Some(ref aud) = self.config.audience {
    validation.set_audience(&[aud.clone()]);
} else {
    validation.validate_aud = false;  // ← silent fallback
}

// auth_middleware.rs:552
if let Some(ref audience) = self.config.audience {
    validation.set_audience(&[audience]);
} else {
    validation.validate_aud = false;  // ← silent fallback
}
```

If an operator does not configure the `audience` field in the OIDC/JWT middleware config,
token audience validation is completely disabled. **Any JWT from any audience will be
accepted**, including tokens issued for unrelated services that share the same signing key.
This is not documented as a default in any configuration guide.

Critically, these are the middleware code paths that guard incoming GraphQL requests —
not a library function that may be optionally called. A misconfigured deployment silently
degrades to accepting cross-service tokens.

**Difference from jwt.rs (Extension V I1):** The `jwt.rs` case is a library-level default
that callers can override. These two cases are the actual request-processing middleware
where the library is used — if `audience` is absent here, no override is applied anywhere
in the request lifecycle.

**Fix:**

Require audience to be explicitly configured. If it is absent, log a startup warning
(or, preferably, fail validation configuration):

```rust
// Preferred: fail fast at startup
if self.config.audience.is_none() {
    tracing::warn!(
        "JWT audience validation is DISABLED (config.audience not set). \
         Tokens from any audience will be accepted."
    );
}
```

Or fail configuration construction entirely:
```rust
pub fn build(self) -> Result<JwtMiddlewareConfig> {
    if self.audience.is_none() {
        return Err(ConfigError::Required {
            field: "audience".to_string(),
            message: "JWT audience must be configured to prevent cross-service token acceptance".to_string(),
        });
    }
    // …
}
```

**Acceptance:** Constructing JWT/OIDC middleware config without setting `audience` either
logs a visible warning or returns a configuration error, not a silent fallback.

---

## Track T — SQL DDL Correctness (Priority: Medium)

---

### T1 — `generate_sql_constraint` Interpolates Column Names Without Quoting

**File:** `crates/fraiseql-core/src/validation/compile_time.rs`
**Lines:** 346, 349, 352

**Problem:**

The `generate_sql_constraint` function produces PostgreSQL `CHECK` constraint expressions
for use in schema DDL. Column names are interpolated directly without quoting:

```rust
// line 346
format!("CHECK ({} {} {})", left_field, sql_op, right_field)
// line 349
format!("CHECK ({} {} {})", left_field, sql_op, right_field)
// line 352
format!("CHECK ({} {} {})", left_field, sql_op, right_field)
```

The `left_field` and `right_field` values are schema field names — user-controlled strings
validated as `FieldName` newtypes. While `FieldName` validation enforces identifier-like
characters, it does not check against the list of PostgreSQL reserved words.

**Impact:**

Schema fields named after SQL reserved words — a plausible occurrence given that
FraiseQL uses naming conventions like `order`, `select`, `end`, `offset`, `limit`,
`group`, `table`, `index`, `primary`, `default`, `check` — will produce syntactically
invalid SQL DDL. Example:

```
-- Field named "order" and "limit" on a type:
CHECK (order < limit)
-- PostgreSQL parses this as a syntax error (ORDER and LIMIT are reserved keywords)
```

The correct output requires double-quoting:
```sql
CHECK ("order" < "limit")
```

**Fix:**

Wrap both column name interpolations with a `quote_pg_identifier()` helper:

```rust
fn quote_pg_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

// In generate_sql_constraint:
format!("CHECK ({} {} {})",
    quote_pg_identifier(left_field),
    sql_op,
    quote_pg_identifier(right_field))
```

**Acceptance:** A schema with a field named `order` generates:
```sql
CHECK ("order" < "limit")
```
and the DDL applies without error in PostgreSQL.

---

## Track U — Test Utility Correctness (Priority: Medium)

---

### U1 — `assert_json_key!` Macro Produces Misleading Failures on Missing Keys

**File:** `crates/fraiseql-test-utils/src/assertions.rs`
**Lines:** 14–25

**Problem:**

```rust
macro_rules! assert_json_key {
    ($value:expr, $key:expr, $expected:expr) => {
        let parts: Vec<&str> = $key.split('.').collect();
        let mut current = $value;

        for part in parts {
            current = &current[part];  // ← serde_json Index for &str on Null/missing returns Null
        }

        assert_eq!(current, $expected);
    };
}
```

`serde_json::Value`'s `Index<&str>` implementation returns `Value::Null` when the key is
absent (it does not panic). This means:

1. `assert_json_key!(&json, "user.id", 123)` on a response that has `user` but no `id`
   field will fail with: `"left: Null != right: 123"`.
2. `assert_json_key!(&json, "user.id", serde_json::Value::Null)` will **pass** even if
   `user.id` does not exist — a false positive.
3. `assert_json_key!(&json, "nonexistent.nested.key", serde_json::Value::Null)` will
   **also pass** silently.

The macro's intended use — "assert that a specific JSON path holds a specific value" —
is broken for the null-check case and provides confusing error messages for missing-key
cases.

**Fix:**

Use `.get()` at each path segment and produce a clear error when any segment is absent:

```rust
macro_rules! assert_json_key {
    ($value:expr, $key:expr, $expected:expr) => {{
        let parts: Vec<&str> = $key.split('.').collect();
        let mut current: &serde_json::Value = $value;
        for part in &parts {
            current = current.get(part).unwrap_or_else(|| {
                panic!(
                    "assert_json_key!: key '{}' not found in path '{}'\nValue: {}",
                    part, $key, current
                )
            });
        }
        assert_eq!(current, &serde_json::json!($expected),
            "assert_json_key!: path '{}' mismatch", $key);
    }};
}
```

---

### U2 — `assert_json_key!` Macro Has No Test Coverage

**File:** `crates/fraiseql-test-utils/src/assertions.rs`
**Lines:** 114–202

**Problem:**

The test module at line 114 tests `assert_no_graphql_errors`, `assert_has_data`,
`assert_graphql_success`, and `assert_graphql_error_contains` — but has **zero** tests
for the `assert_json_key!` macro. The macro is exported (`#[macro_export]`) and is
presumably used by downstream integration tests, but its behavior on missing keys,
null values, and nested paths is unverified. This is directly related to U1: the broken
behavior described there is undetected precisely because there are no tests for it.

**Fix:**

Add tests to the existing test module:

```rust
#[test]
fn test_assert_json_key_basic() {
    let json = json!({"user": {"id": 123}});
    assert_json_key!(&json, "user.id", 123);
}

#[test]
#[should_panic(expected = "key 'id' not found")]
fn test_assert_json_key_missing_nested() {
    let json = json!({"user": {}});
    assert_json_key!(&json, "user.id", 123);
}

#[test]
#[should_panic(expected = "key 'nonexistent' not found")]
fn test_assert_json_key_missing_top_level() {
    let json = json!({"data": {}});
    assert_json_key!(&json, "nonexistent.key", "value");
}
```

---

## Track V — Observability Gap (Priority: Low)

---

### V1 — Arrow Flight `matches_filter()` Silently Drops Events on Unparseable Filter

**File:** `crates/fraiseql-arrow/src/subscription.rs`
**Lines:** 127–140 (approximately)

**Problem:**

When a subscription filter expression cannot be parsed, `matches_filter()` returns `false`,
causing the event to be silently dropped for that subscriber. No log line, metric, or
error is produced. A subscriber with a malformed filter string receives no events and no
indication of why. This degrades to a black-hole subscription that is extremely difficult
to debug.

**Fix:**

Add a tracing warn at the point of parse failure:

```rust
fn matches_filter(event: &crate::HistoricalEvent, filter: &Option<String>) -> bool {
    let Some(filter_str) = filter.as_deref() else {
        return true;
    };

    if let Some(result) = try_parse_filter(filter_str, event) {
        return result;
    }

    // Filter could not be parsed — reject event but log the problem
    tracing::warn!(
        filter = filter_str,
        event_id = ?event.id,
        "Subscription filter could not be parsed; event dropped for this subscriber"
    );
    false
}
```

**Acceptance:** A subscriber that provides a syntactically invalid filter expression
generates a `WARN` log line per dropped event, making the issue diagnosable from logs.

---

## Summary

| ID | Finding | File(s) | Severity |
|---|---|---|---|
| P1 | `generate-views` emits `public.schema_placeholder` in all SQL | `generate_views.rs:253,280,291,306` | **Critical** |
| Q1 | Arrow Flight reads `FLIGHT_SESSION_SECRET` per-call with `.expect()` | `flight_server/auth.rs:49,82` | **High** |
| Q2 | `.expect()` on `HeaderValue` parse in production response handler | `http.rs:147` | **High** |
| R1 | Indirect dependency cycles not detected in `CascadeInvalidator` | `cascade_invalidator.rs:115` | **High** |
| S1 | `validate_aud = false` silent fallback in request-middleware paths | `oidc.rs:601`, `auth_middleware.rs:552` | **High** |
| T1 | `generate_sql_constraint` interpolates column names without SQL quoting | `compile_time.rs:346,349,352` | **Medium** |
| U1 | `assert_json_key!` returns misleading null on missing key | `assertions.rs:20` | **Medium** |
| U2 | `assert_json_key!` has no test coverage | `assertions.rs:114–202` | **Medium** |
| V1 | Arrow Flight filter parse failure drops events silently | `subscription.rs` | **Low** |

---

## What This Assessment Did NOT Flag

The following were considered and explicitly excluded:

- **`unimplemented!()` in `skeletons.rs`** — These are inside Rust format strings
  (`r#"...{{}}"#`) and become the literal text `unimplemented!()` in generated source
  files shown to end-users as skeleton code. They are intentional authoring hints,
  not live Rust panics.
- **`eprintln!` in `validate_facts.rs` and `generate_views.rs`** — CLI status output to
  stderr is idiomatic; these are not the same issue as the `eprintln!` calls in
  `fraiseql-server/backup_manager.rs` (already tracked by Extension I — G1).
- **Parser accepting empty `queries` / `mutations` arrays** — `map_or(Ok(vec![]), ...)`
  is correct: a schema with only subscriptions, or only mutations, is valid.
- **`cascade_invalidate` BFS safety with cycles** — The BFS uses a visited set and will
  not infinite-loop even if cycles exist. R1 flags cycle *admission*, not *traversal*.
- **JWT `validate_aud = false` in `routes/auth.rs:310` (`revoke_token`)** — That call
  site also disables signature verification (`insecure_disable_signature_validation()`),
  making the token-parsing intentionally insecure. The comment "Decode without verification"
  is accurate. This is correct behaviour for a revocation endpoint.
