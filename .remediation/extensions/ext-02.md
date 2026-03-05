# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension II

*Written 2026-03-05. Third assessor's findings.*
*Extends both `/tmp/fraiseql-remediation-plan.md` and `/tmp/fraiseql-remediation-plan-extension.md`.*
*Benchmarks out of scope (handled by velocitybench).*
*Findings confirmed against the 30 most recent commits; issues already resolved in HEAD are not flagged.*

---

## Executive Summary

Two previous assessors covered documentation inaccuracy, authentication gaps, and stub modules.
This pass found a different category: **silent data corruption** (validated inputs use a frozen
clock; filtered queries return all rows), **SQL injection in a documented-safe path**, and
**broken public APIs** that the type system permits but that always fail at runtime.

| Category | Count | Severity |
|---|---|---|
| SQL injection / security | 1 | Critical |
| Silent correctness bugs | 2 | High |
| Non-functional public API | 2 | High |
| Undocumented config no-ops | 2 | Medium |
| Broken Prometheus format / duplicate struct | 1 | Medium |

---

## Track I — Security & Correctness (Priority: Critical / High)

---

### I1 — SQL Injection via Window Query ORDER BY (Critical)

**Files:**
- `crates/fraiseql-core/src/compiler/window_functions/planner.rs:55–61`
- `crates/fraiseql-core/src/runtime/window.rs:103, 146`

**Problem:**

Window queries (GraphQL queries whose root field ends with `_window`) receive their execution
parameters through GraphQL variables — plain JSON, not governed by GraphQL identifier naming
rules. The executor unpacks those variables and passes them as `query_json` directly to the
window planner:

```rust
// executor/aggregate.rs:82
let query_json = variables.unwrap_or(&empty_json);
self.execute_window_query(query_json, query_name, metadata).await
```

The planner extracts the `orderBy.field` from that JSON without validation:

```rust
// compiler/window_functions/planner.rs:55-61
Some(OrderByClause {
    field: item["field"].as_str()?.to_string(),  // ← user-controlled, unvalidated
    direction,
})
```

The SQL generator then interpolates it directly:

```rust
// runtime/window.rs:103
sql.push_str(&format!("{} {}", order.field, dir));
```

**Attack surface:** Any authenticated user who can call a `*_window` query can inject arbitrary
SQL into the ORDER BY clause of the generated SQL string.

**Working proof-of-concept payload:**

```json
{
  "orderBy": [
    {
      "field": "(SELECT CASE WHEN (1=1) THEN pg_sleep(5) END)",
      "direction": "ASC"
    }
  ]
}
```

This produces: `ORDER BY (SELECT CASE WHEN (1=1) THEN pg_sleep(5) END) ASC`
— a time-based blind SQL injection that can be used to enumerate data.

The executor documentation explicitly claims "All user-supplied values are escaped via
`format_sql_value` / `escape_sql_string`" (`executor/mod.rs:44`). This is false for
ORDER BY field names in window queries. `validate()` in `WindowPlanner` accepts `_metadata` as
a parameter but does not use it to validate field names against known dimensions or measures.

**Fix:**

Validate `orderBy.field` against the fact table's known dimension/measure names before accepting
it into the plan:

```rust
// In WindowFunctionPlanner::parse_order_by (new helper) or validate():
fn is_safe_column_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && s.len() <= 64
}

// In planner:
let field = item["field"].as_str()?;
if !is_safe_column_name(field) {
    // Log the violation and return None (skip this clause)
    tracing::warn!(field = field, "Rejected unsafe ORDER BY field name in window query");
    return None;
}
Some(OrderByClause { field: field.to_string(), direction })
```

Alternatively (and more correctly), validate against the fact table's known column names:

```rust
let field = item["field"].as_str()?;
let known: Vec<&str> = metadata.measures.iter().map(|m| m.name.as_str())
    .chain(metadata.dimensions.iter().map(|d| d.name.as_str()))
    .collect();
if !known.contains(&field) {
    return None; // unknown field → skip
}
```

**Acceptance:**
- `orderBy.field` values not matching `[a-zA-Z_][a-zA-Z0-9_]*` (or known metadata columns)
  are rejected before SQL generation
- `validate()` in `WindowPlanner` actually uses the `metadata` parameter
- Integration test: window query with `field: "pg_sleep(5)"` → validation error, not
  5-second delay
- Executor documentation (`executor/mod.rs:44`) updated to accurately describe which paths
  do and do not use parameterized queries

---

### I2 — Window Query WHERE Clause Silently Discarded (High)

**Files:**
- `crates/fraiseql-core/src/compiler/window_functions/planner.rs:42–43`
- `crates/fraiseql-core/src/runtime/window.rs:88–89`

**Problem:**

The window function planner converts any WHERE clause to an empty AND node:

```rust
// planner.rs:42-43
// Parse WHERE clause (placeholder - full implementation would parse actual conditions)
let where_clause = query.get("where").map(|_| WhereClause::And(vec![]));
```

The SQL generator then emits `WHERE 1=1` if `where_clause` is `Some`:

```rust
// window.rs:88-89
if plan.where_clause.is_some() {
    sql.push_str(" WHERE 1=1"); // Placeholder
}
```

**Impact:**

- Any window query that includes a `where` parameter silently ignores all filtering conditions
  and returns the full table
- If a WHERE clause was intended to enforce data isolation (e.g., `where: {tenant_id: $tenant}`),
  the window query leaks cross-tenant data
- There is no warning log, no error, no indication in the response that conditions were dropped

**Fix (short-term):** Return a `FraiseQLError::Validation` if `where` is present in the
window query JSON, until the feature is implemented:

```rust
if query.get("where").is_some() {
    return Err(FraiseQLError::Validation {
        message: "WHERE clauses in window queries are not yet supported. \
                  Remove the 'where' key or use a pre-filtered view.".to_string(),
        path: Some("where".to_string()),
    });
}
let where_clause = None;
```

**Fix (long-term):** Implement WHERE clause parsing against the fact table's dimension columns,
feeding into the existing `WhereClause` type and emitting properly parameterized SQL.

**Acceptance:**
- A window query with `"where": {"tenant_id": "abc"}` either errors cleanly or correctly
  applies the filter
- `"WHERE 1=1"` does not appear in any emitted SQL
- The comment "Placeholder" is removed from the WHERE parsing code

---

### I3 — `get_today()` Hardcoded to 2026-02-08 (High)

**File:** `crates/fraiseql-core/src/validation/date_validators.rs:91–95`

**Problem:**

```rust
/// Get today's date as (year, month, day).
/// For testing purposes, this can be overridden.
fn get_today() -> (u32, u32, u32) {
    // In a real implementation, this would use chrono or std::time
    // For now, we'll use a fixed date for testing consistency
    (2026, 2, 8)
}
```

This function is not in `#[cfg(test)]`. It is called by four production-exported functions:

- `validate_min_age(date_str, min_age)` — returns wrong result for anyone born between
  2026-02-09 and today
- `validate_max_age(date_str, max_age)` — same
- `validate_max_days_in_future(date_str, max_days)` — returns wrong result for dates
  after 2026-02-08 + max_days
- `validate_max_days_in_past(date_str, max_days)` — wrong for all dates past 2026-02-08

All four functions are exported from `fraiseql_core::validation` and will silently produce
incorrect results in any production schema that uses `minAge`, `maxAge`, or relative date
constraints.

The date is also already in the past (today is 2026-03-05), making `validate_max_days_in_past`
broken for any date since 2026-02-08.

Tests in the file are hardcoded to this date:
```rust
// Test: "Person born 2000-01-01 is 25 (not yet 26)"
assert!(validate_min_age("2000-01-01", 26).is_ok()); // ← only true if today is 2026-02-08
```

**Fix:**

```rust
use chrono::Datelike;

fn get_today() -> (u32, u32, u32) {
    let today = chrono::Utc::now().date_naive();
    (today.year() as u32, today.month(), today.day())
}
```

`chrono` is already a dependency in `fraiseql-core` (used elsewhere).

Update the tests to use relative dates or mock `get_today` via a dependency injection
approach (e.g., a `DateProvider` trait or `#[cfg(test)] thread_local!`).

**Acceptance:**

- `get_today()` returns the actual system date
- All age and relative-date validation functions produce correct results regardless of
  when they are called
- Tests use dates computed relative to `get_today()` or mock the provider, not hardcoded
  absolute dates

---

### I4 — `InputObjectRule::Custom` Always Returns "Not Implemented" (High)

**File:** `crates/fraiseql-core/src/validation/input_object.rs:154–158`

**Problem:**

```rust
InputObjectRule::Custom { name } => Err(FraiseQLError::Validation {
    message: format!("Custom validator '{}' not implemented", name),
    path:    Some(path.to_string()),
}),
```

`InputObjectRule::Custom` is a variant in a public enum. Any schema that declares a custom
validator rule (e.g., `"rules": [{"type": "custom", "name": "my_validator"}]`) will cause
every validation call on that input object to fail with a runtime error.

There is no mechanism to register a custom validator against the `name` string. The
`EloRustValidatorRegistry` in `elo_rust_integration.rs` exists but is not consulted when
dispatching `InputObjectRule::Custom`.

The test explicitly asserts this behavior as intentional:
```rust
fn test_custom_validator_not_implemented() {
    // ...
    assert!(message.contains("not implemented"));
}
```

**Impact:** Any user reading the schema format documentation who creates a rule with
`"type": "custom"` will find that all their input validations unconditionally fail in
production. This is undocumented behavior.

**Options:**

**Option A — Wire up the registry:** Dispatch `InputObjectRule::Custom { name }` against the
`EloRustValidatorRegistry`. Add a `registry: &EloRustValidatorRegistry` parameter to the
validation call chain. Return a clear `Err` if no validator with that name is registered.

**Option B — Remove the variant:** If custom validators are not ready, remove
`InputObjectRule::Custom` from the public `InputObjectRule` enum and document it as a
planned feature. Users will then get a compile-time or schema-parse error rather than a
silent runtime "not implemented" message.

**Acceptance:**
- Either `InputObjectRule::Custom { name: "x" }` resolves against a registered validator, or
- The variant does not compile into the public API, or
- A clear feature-flag documentation exists warning that custom validators are not yet supported

---

## Track J — Undocumented Config No-ops (Priority: Medium)

---

### J1 — `cache_list_queries = false` Is a Documented No-op

**File:** `crates/fraiseql-core/src/cache/config.rs:156–162`

**Problem:**

```rust
/// Whether to cache list queries.
///
/// **Note**: Currently not implemented (all queries are cached).
/// This field is reserved for future use.
///
/// Default: `true`
pub cache_list_queries: bool,
```

The field is parsed from configuration and has a documented default, but is never read by
the cache adapter. Setting `cache_list_queries = false` in `fraiseql.toml` has no effect.

This means a user who sets this to reduce memory usage (list queries with 10,000 rows
consuming large amounts of cache memory) will silently continue caching all queries.

**Fix:**

Either evaluate `cache_list_queries` in the cache adapter (`adapter/query.rs`) before
storing results:

```rust
if !self.config.cache_list_queries && result.is_list_result() {
    return Ok(()); // Skip caching
}
```

Or remove the field and document it in CHANGELOG as deferred:

```rust
// Remove cache_list_queries from CacheConfig entirely until implemented
```

**Acceptance:** Setting `cache_list_queries = false` either noticeably changes cache behavior
(list query results not stored) or the field is absent from the public API with a documented
reason.

---

### J2 — `CascadeMetadata::from_schema()` Only Compiled in Tests

**File:** `crates/fraiseql-core/src/cache/cascade_metadata.rs:146–155`

**Problem:**

```rust
#[cfg(test)]
/// Build metadata from a compiled schema (for testing).
///
/// In production, this would be called during server initialization
/// to extract all mutations and their return types from the compiled schema.
pub fn from_schema(_schema: &CompiledSchema) -> Self {
    // For now, return empty - tests will build metadata manually
    Self::new()
}
```

In a production build, there is no way to automatically populate `CascadeMetadata` from
a compiled schema. The module doc comment (`cascade_metadata.rs:17`) shows a flow diagram
with `build_from_schema()` as a key step, but that step is only compiled when
`#[cfg(test)]` is active.

Production servers that enable cache cascade invalidation must build `CascadeMetadata`
manually by calling `add_entity_mutation_mapping()` for every mutation in the schema.
If a developer relies on the documented auto-build behavior and doesn't manually configure
the metadata, cascade invalidation simply never fires.

**Fix:**

Implement the production version:

```rust
// Remove #[cfg(test)]
pub fn from_schema(schema: &CompiledSchema) -> Self {
    let mut metadata = Self::new();
    for mutation in schema.mutations() {
        if let Some(entity) = mutation.return_type.base_type() {
            metadata.add_entity_mutation_mapping(
                mutation.name.as_str(),
                entity,
            );
        }
    }
    metadata
}
```

**Acceptance:**
- `CascadeMetadata::from_schema()` is available in non-test builds
- Server builder calls it during initialization
- Module documentation matches actual behavior

---

## Track K — Code Quality (Priority: Medium)

---

### K1 — Duplicate `MetricsCollector` with Invalid Prometheus Format

**Files:**
- `crates/fraiseql-server/src/operational/metrics.rs` — broken implementation
- `crates/fraiseql-server/src/metrics_server.rs` — correct implementation (actually used)

**Problem:**

`fraiseql-server` contains two public structs both named `MetricsCollector` in different
modules. The server's `lib.rs` re-exports the one from `metrics_server`:

```rust
// lib.rs:202
pub use metrics_server::{MetricsCollector, PrometheusMetrics};
```

The `operational::MetricsCollector` is exported from `operational/mod.rs` but never used
by any server code — it's a dead export with an incorrect Prometheus format.

The broken Prometheus output (lines 72, 76, 80):

```rust
prometheus_lines.push(format!("graphql_requests_total {{{}}}", request_count));
```

`{{{N}}}` in Rust format strings produces literal braces around the value:
`graphql_requests_total {42}`.

Valid Prometheus text format requires no braces around the sample value:
`graphql_requests_total 42`.

Any user who discovers `operational::MetricsCollector` and uses it (it is a public type)
will produce metrics that are rejected by Prometheus scrapers.

**Fix:**

Option A — Remove `operational::MetricsCollector` and `metrics_summary` from the public
API. The correct implementation already exists in `metrics_server::MetricsCollector`.

Option B — Fix the format strings:

```rust
prometheus_lines.push(format!("graphql_requests_total {}", request_count));
prometheus_lines.push(format!("graphql_errors_total {}", error_count));
prometheus_lines.push(format!("graphql_duration_ms {}", avg_duration_ms));
```

And replace the `format!("...", count)` patterns with `to_string()` calls.

**Acceptance:**
- Only one public `MetricsCollector` exists in the `fraiseql-server` public API
- `cargo doc -p fraiseql-server` shows no ambiguous `MetricsCollector` type
- Prometheus output passes `grep -E '^[a-z_]+ [0-9]'` format validation

---

### K2 — `MockEmailDomainValidator` / `MockPhoneNumberValidator` Exported as Production API

**File:** `crates/fraiseql-core/src/validation/async_validators.rs`

**Problem:**

Two validators are exported from the public API with names that signal they are test
doubles, but are the only available implementations:

```rust
/// Mock email domain validator for testing.
///
/// In production, this would perform actual MX record lookups.
pub struct MockEmailDomainValidator { ... }

/// Mock phone number validator for testing.
///
/// In production, this would integrate with Twilio or similar service.
pub struct MockPhoneNumberValidator { ... }
```

`MockEmailDomainValidator` validates email domains against a hardcoded list of four domains:
`gmail.com`, `yahoo.com`, `outlook.com`, `example.com`. Any other domain returns a validation
error — including every corporate email domain.

`MockPhoneNumberValidator` does a basic `+` prefix and length check.

These are `pub` and re-exported from `fraiseql_core::validation`. A user building a product
and following the API surface will reach for `MockEmailDomainValidator` as the provided
validator, then find that `@company.com` emails fail validation.

**Fix:**

Option A — Rename and hide:

```rust
/// Test double for MX-record email validation. Not for production use.
/// Use a real implementation that calls your DNS resolver.
#[doc(hidden)]
pub struct MockEmailDomainValidator { ... }
```

And remove them from the re-exports in `validation/mod.rs`.

Option B — Implement a real `MxEmailDomainValidator` that performs actual DNS MX lookups
via `hickory-resolver` or `trust-dns-resolver`, and export that alongside the mock.

**Acceptance:**
- `MockEmailDomainValidator` is either removed from `pub` re-exports in `validation/mod.rs`
  or clearly marked `#[doc(hidden)]` with a warning that it is for testing only
- A real validator is available for production use (or `EMAIL_DOMAIN_MX` is documented as
  "not yet implemented" in feature matrix)

---

## Interaction with Existing Plans

The findings above interact with the existing remediation plan in the following ways:

| This plan | Existing plan |
|---|---|
| I1 (window ORDER BY injection) | New; unrelated to E1/E2 auth bypass |
| I2 (window WHERE discarded) | New; different from G5 (observer config silenced) |
| I3 (hardcoded date) | New; not covered by B1/B2/B3 |
| I4 (Custom validator not-implemented) | Thematically related to F1/F2 but distinct system |
| J1 (cache_list_queries no-op) | Related to B3 coverage work; add test that verifies behavior |
| J2 (CascadeMetadata cfg(test)) | Complements F1–F3 (feature theater) |
| K1 (duplicate MetricsCollector) | Complements G1 (eprintln) hygiene |
| K2 (Mock validators as production API) | Complements F2 (syslog backend sends nothing) |

---

## Execution Order

### Immediate (security, before any release)

1. **I1** — Window ORDER BY field validation (SQL injection, ~2 hours)
2. **I2** — Window WHERE clause: return error if present until implemented (~1 hour)

### Week 1 (correctness)

3. **I3** — Fix `get_today()` to use `chrono::Utc::now()` (~2 hours + test updates)
4. **I4** — Either wire up `EloRustValidatorRegistry` or remove `Custom` variant (~4 hours)

### Week 2 (hygiene)

5. **J1** — Evaluate or remove `cache_list_queries` (~2 hours)
6. **J2** — Implement `CascadeMetadata::from_schema()` for non-test builds (~3 hours)
7. **K1** — Remove or fix `operational::MetricsCollector` (~1 hour)
8. **K2** — Rename Mock validators, add `#[doc(hidden)]` or implement real validators

---

## Definition of Done (Extension II)

The remediation is complete (for this set of findings) when all of the following hold,
in addition to the original plan's and Extension I's definitions:

1. Window query with `{"orderBy": [{"field": "(SELECT pg_sleep(1))"}]}` returns a validation
   error, not a 1-second delay
2. Window query with `{"where": {"x": 1}}` returns a validation error or correct filtering
   — never `WHERE 1=1`
3. `validate_min_age("2000-01-01", 26)` returns the correct result for the actual date
   (not a frozen 2026-02-08)
4. `InputObjectRule::Custom { name: "test" }` returns a `FraiseQLError::Validation` with
   message "Custom validator 'test' not registered" — not "not implemented"
5. Setting `cache_list_queries = false` either changes observable behavior or the field is
   absent from the public API
6. `CascadeMetadata::from_schema()` compiles in non-test builds
7. `cargo doc -p fraiseql-server` shows one `MetricsCollector` and its Prometheus output
   format is valid (no `{value}` pattern)
8. `MockEmailDomainValidator` is not re-exported from `fraiseql_core::validation` (or is
   clearly `#[doc(hidden)]`)
