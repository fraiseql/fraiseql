# Phase 05: Quality Evaluation & Improvement Matrix

## Objective
Systematically raise the codebase from **3.9/5** to **≥ 4.5/5** quality score.
This is the release gate. Requires Phases 01–07 complete (all features merged first).

## Baseline (2026-03-14)

| Dimension | Score | Key Metric |
|-----------|-------|-----------|
| Test Coverage | 4.5/5 | 14,714 tests; low in observers/secrets/error |
| Documentation | 2.0/5 | 22% of public items documented (616 / 2,789) |
| Error Handling | 2.5/5 | Many `.unwrap()` in production code |
| Code Complexity | 4.0/5 | Many `#[allow]` suppressions without `// Reason:` |
| Dependencies | 4.5/5 | Clean graph; no circulars; rustls-only |
| Security | 5.0/5 | Zero unsafe; comprehensive guards (S15–S58) |
| CI/CD | 4.0/5 | Fuzz/mutation exist; mutation non-blocking |
| Architecture Docs | 4.5/5 | Clear, up-to-date; ADRs present |
| SDK Completeness | 4.0/5 | Python excellent; Java/PHP stubs |

**Release-blocking tracks**: A-Tier1, B-Tier1, D-integration-fixes
**Nice-to-have tracks**: everything else (ship at whatever point reached)

---

## New surface added in Phases 03–07

The following capabilities were merged before Phase 05 begins. Each adds new
testable surface that the quality tracks below must cover.

### Phase 03 — SDK Operation Naming Parity
- **C# SDK**: `ValidOperations` now uses uppercase `CREATE/UPDATE/DELETE/CUSTOM`
  (was lowercase — now matches Python, Go, TypeScript, Java, Kotlin, Scala, Swift)
- **Go SDK**: each example moved to its own `examples/<name>/main.go` subdirectory
  to fix `main redeclared` compilation errors

**Quality implications**:
- C# tests already updated and passing (`MutationBuilderTests.cs`)
- No new quality debt introduced

### Phase 04 — Docker Hardening
- `.dockerignore` now excludes `**/target/` (not just top-level `target/`)
  preventing crate-level Rust build artifacts from leaking into images
- Multi-stage `Dockerfile` already in place from prior work

**Quality implications**:
- Track E (CI/CD): verify Docker build in CI passes with the corrected ignore rules

### Phase 06 — REST Transport Layer

New feature: compile-time REST annotation on queries/mutations mounts HTTP routes
that delegate to the GraphQL executor without a round-trip.

**New Rust types (fraiseql-core)**:
- `RestRoute { path: String, method: String }` — per-operation REST annotation
- `RestConfig { prefix, auth, openapi_enabled, openapi_path, title, api_version }` — global REST config
- `QueryDefinition.rest: Option<RestRoute>` — opt-in REST binding for queries
- `MutationDefinition.rest: Option<RestRoute>` — opt-in REST binding for mutations
- `CompiledSchema.rest_config: Option<RestConfig>` — compiled REST configuration
- `CompiledSchema.rest_openapi_spec: Option<String>` — pre-generated OpenAPI spec

**New Rust code (fraiseql-server, feature `rest-transport`)**:
- `routes/rest/translator.rs` — `build_graphql_request()` + `classify_response()`
- `routes/rest/router.rs` — `build_rest_router()` mounts axum routes + OpenAPI endpoint
- `RestOutcome` enum: `Ok`, `Partial`, `Failure`, `NotFound`

**New Python SDK**:
- `@fraiseql.query(rest_path="/users/{id}", rest_method="GET")` — REST annotation
- `@fraiseql.mutation(rest_path="/users", rest_method="POST")` — REST annotation
- Validation: path params must match declared arguments; method must be valid

**New TypeScript SDK**:
- `OperationConfig.restPath?: string` — REST path pattern
- `OperationConfig.restMethod?: "GET" | "POST" | ...` — HTTP method

**Quality implications for Track D (tests)**:
- 15 REST transport unit tests already present in `routes/rest/tests.rs`
- 18 Python SDK REST tests in `tests/test_decorators.py`
- Integration test: REST → GraphQL → DB round-trip (not yet written, see D4 below)

### Phase 07 — OpenAPI Spec Generation & REST Edge Cases

**New Rust code (fraiseql-core)**:
- `schema/compiled/openapi_gen.rs` — `generate_openapi_spec(schema, config) -> String`
  - Produces OpenAPI 3.1.0 JSON from compiled REST routes
  - Maps GraphQL types to `components/schemas` with BFS over nested types
  - Path/query/body parameters correctly classified by method and path template
  - `BearerAuth` security scheme when `auth = "required"` or `"optional"`
  - 10 unit tests covering all generation paths

**New server behaviour**:
- `GET {openapi_path}` served when `rest_config.openapi_enabled = true`
- Uses pre-generated `rest_openapi_spec` if embedded, else generates dynamically

**REST response edge cases (27 REST tests total)**:
- Partial response (data + errors) → HTTP 200 with `_partial: true`
- Null data + `UNAUTHENTICATED` → 401
- Null data + `FORBIDDEN` → 403
- Null data + `VALIDATION_ERROR` → 400
- Null data + `RATE_LIMITED` → 429
- Null data + unknown code → 500
- Single-item null → 404 `{"error":"Not found","operation":"…"}`
- Empty list → 200 `[]`
- Unparseable executor response → 500

**Quality implications**:
- OpenAPI tests already written and passing
- Track D4 (REST integration test) must cover the OpenAPI endpoint too

---

## Track A — Documentation (2.0 → ≥ 3.5)

### Priority order

**Tier 1 — User-facing API (must be 100% documented, release-blocking)**

1. **`fraiseql-error`** (24 public items, 12.5% → 100%)
   - Every error variant: one-line description, when returned, what user should do
   - This is the first thing downstream developers see in compiler output

2. **`fraiseql-core/src/schema/compiled/`** — `CompiledSchema` and all fields
   - Every public struct, enum, field that schema authors interact with
   - Includes new `RestRoute`, `RestConfig`, `rest_openapi_spec` (already documented)

3. **`fraiseql-server/src/server_config.rs`** — `ServerConfig` and sub-configs
   - Every configuration field with valid values, default, and example

4. **`fraiseql-cli/src/`** — all CLI command structs and their arguments

5. **`fraiseql-core/src/schema/compiled/openapi_gen.rs`** (new in Phase 07)
   - `generate_openapi_spec()` is already documented
   - Ensure helper functions have doc comments if pub

**Tier 2 — Internal API (target ≥ 60%, best-effort)**

6. `fraiseql-core/src/runtime/` — `Executor`, `ExecutorAdapter` trait
7. `fraiseql-db/src/` — `DatabaseAdapter` trait, all implementors
8. `fraiseql-auth/src/` — all public auth types
9. `fraiseql-server/src/routes/rest/` (new in Phase 06/07)
   - `TranslatedRequest`, `RestOutcome`, `build_graphql_request()`, `classify_response()`

**Tier 3 — Implementation details (best-effort, ≥ 30%)**

10. `fraiseql-observers`, `fraiseql-arrow`, `fraiseql-secrets`

### Documentation standard

```rust
/// One-line summary in active voice (no trailing period)
///
/// # Errors
///
/// Returns [`ErrorVariant`] when <condition>.
///
/// # Panics
///
/// Panics if <condition>. (only if any code path can panic)
pub fn example() -> Result<()> { ... }
```

### Verification
```bash
# Count remaining undocumented items
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps 2>&1 \
  | grep "missing documentation" | wc -l
# Tier 1 target: 0 undocumented items in fraiseql-error, compiled/, server_config.rs
```

---

## Track B — Error Handling (2.5 → ≥ 3.5)

### Step 1: Get accurate baseline

The raw `grep` count (2,646 unwraps) includes test code. Get the real number:
```bash
# Accurate production unwrap count via clippy
cargo clippy --workspace -- -W clippy::unwrap_used 2>&1 | grep "warning:" | wc -l
```

### Tier 1 — Production panic risk (release-blocking)

Fix all unwraps on operations that CAN fail at runtime:

- `fraiseql-webhooks/src/signature/`: HMAC key handling
  - `Hmac::new_from_slice(...).unwrap()` → return `Err(WebhookError::InvalidKey)`
- Any `.unwrap()` on `std::env::var()` → propagate error
- Any `.unwrap()` on file I/O in non-test code
- Any `.unwrap()` on `Mutex::lock()` → use `.expect("lock poisoned")`

Note: REST transport code uses `.unwrap_or_else(|_| ...)` for serialization —
these are intentionally infallible (serde_json serializing a Value cannot fail
for well-formed data; the fallbacks exist for defence-in-depth).

### Tier 2 — Logically infallible (add safety comments)

Every remaining `.unwrap()` in production code must either:
1. Be converted to `?` / `map_err`, OR
2. Carry a `// SAFETY:` comment explaining why it cannot fail

```rust
// SAFETY: value comes from our own serialization which always produces valid u64
let n = s.parse::<u64>().expect("internal serialization invariant");
```

### Tier 3 — `todo!()` / `unimplemented!()` (11 total)

Replace with `return Err(FraiseQLError::Unsupported { ... })` so the server
returns a proper error instead of crashing.

### Verification
```bash
# Zero unexcused unwraps in production code
grep -rn "\.unwrap()" crates/ --include="*.rs" \
  | grep -v "#\[cfg(test\|/tests/\|// SAFETY:\|// Invariant:" \
  | wc -l
# Zero todo!/unimplemented! in non-test code
grep -rn "todo!\|unimplemented!" crates/ --include="*.rs" \
  | grep -v "/tests/\|#\[cfg(test" | wc -l
```

---

## Track C — Allow Suppressions Audit

### Strategy

Work crate by crate. For each `#[allow]` without `// Reason:`:
1. **Try removing it** — if clippy no longer fires, delete the suppression
2. **If still needed** — add `// Reason: <why>`
3. **If masking a real issue** — fix the code

### Priority crates (highest suppression counts first)
1. `fraiseql-core`
2. `fraiseql-server`
3. `fraiseql-arrow`
4. `fraiseql-observers`

### Note on Phase 07 suppressions
One `#[allow(clippy::implicit_hasher)]` was added in Phase 07 to
`routes/rest/translator.rs::build_graphql_request()` — already carries a
`// Reason:` comment (callers always use std HashMap; generics add complexity
without benefit). No audit needed there.

### Note on baseline
The raw count (14,160) may include crate-level `#![allow]` that affect many
items. Count unique suppression sites, not items affected. The goal is that
every `#[allow]` site has a `// Reason:` — not a specific reduction percentage.

### Verification
```bash
grep -rn "#\[allow(" crates/ --include="*.rs" | grep -v "// Reason:" | wc -l
# Target: 0
```

---

## Track D — Test & Integration Fixes

### D1 — Pre-existing integration test failures (release-blocking)

These were broken BEFORE our changes. Fix them now:

```bash
make db-up  # start Docker services

# Run each integration suite
cargo nextest run -p fraiseql-core --features postgres -- integration
cargo nextest run -p fraiseql-core --features mysql -- integration
cargo nextest run -p fraiseql-core --features sqlite -- integration
cargo nextest run -p fraiseql-server --features auth,secrets -- integration
```

For each failure: read the error, identify root cause (schema drift, SQL change,
config issue), fix test or implementation as appropriate.

### D2 — Crates with insufficient test coverage

| Crate | Gap |
|-------|-----|
| `fraiseql-error` | Error propagation, `From` impls, display formatting |
| `fraiseql-test-utils` | Test helper correctness |
| `fraiseql-secrets` | Integration paths with Vault/AWS/env backends |
| `fraiseql-observers` | Observer runtime state transitions |

**`fraiseql-error`** (target: 100% coverage):
- `Display` formatting for every variant
- `From` impl conversions
- `ErrorContext` chain
- `ValidationFieldError` JSON round-trip

### D3 — Test Suite pre-existing failures

After Phase 01–02 fix the build, run:
```bash
cargo nextest run --workspace --all-features
```
Identify and fix remaining failures.

### D4 — REST transport integration tests (new — Phase 06/07)

Unit tests (27) already cover translator and response classification. Add
integration tests for the full REST → GraphQL → response path:

```rust
// tests/rest_integration.rs (fraiseql-server, feature = rest-transport)

#[tokio::test]
async fn test_get_user_rest_endpoint_returns_200() {
    // Set up schema with REST-annotated get_user query
    // Start test server with rest-transport feature
    // GET /rest/users/1 → expect 200 with user JSON
}

#[tokio::test]
async fn test_get_user_not_found_returns_404() {
    // GET /rest/users/99999 → expect 404 {"error":"Not found","operation":"get_user"}
}

#[tokio::test]
async fn test_post_mutation_creates_resource() {
    // POST /rest/users with body → expect 200 with created resource
}

#[tokio::test]
async fn test_openapi_endpoint_returns_valid_spec() {
    // GET /rest/openapi.json → expect 200, Content-Type: application/json
    // Parse and assert spec["openapi"] == "3.1.0"
    // Assert paths contains the REST-annotated routes
}

#[tokio::test]
async fn test_openapi_disabled_returns_404() {
    // When rest_config.openapi_enabled = false
    // GET /rest/openapi.json → expect 404
}

#[tokio::test]
async fn test_partial_response_includes_partial_flag() {
    // Configure mock executor to return data + errors
    // GET /rest/users → expect 200 {"data":…,"errors":…,"_partial":true}
}
```

Use the existing `ServerHarness` from `tests/common/server_harness.rs` as the
test infrastructure base. These tests require the `rest-transport` feature and
a live executor (can use `MockDatabaseAdapter`).

### D5 — Python SDK REST tests (new — Phase 06)

82 tests already pass in `tests/test_decorators.py`. Verify they remain clean:
```bash
cd sdks/official/fraiseql-python && PYTHONPATH=src uv run pytest -q
# Expected: 82 passed
```

No new tests needed here — coverage is complete.

### D6 — C# SDK parity tests (new — Phase 03)

Verify C# mutation operation naming tests pass after the uppercase fix:
```bash
cd sdks/official/fraiseql-csharp && dotnet test
# MutationBuilderTests must pass for CREATE/UPDATE/DELETE/CUSTOM
```

### Verification
```bash
cargo nextest run --workspace --all-features
# All integration tests (requires make db-up):
cargo nextest run --workspace --features "postgres,mysql,sqlite,auth,secrets"
# REST transport unit tests:
cargo test -p fraiseql-server --features rest-transport --lib -- rest
# OpenAPI generator tests:
cargo test -p fraiseql-core --lib -- openapi
```

---

## Track E — CI/CD Improvements

### E1 — Code coverage reporting

Add `llvm-cov` to CI:
```yaml
coverage:
  name: Code Coverage
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: llvm-tools-preview
    - uses: taiki-e/install-action@cargo-llvm-cov
    - run: cargo llvm-cov --workspace --lcov --output-path lcov.info
    - uses: codecov/codecov-action@v4
      with:
        files: lcov.info
```

Coverage gates (start permissive):
- **Minimum**: 60% line coverage workspace-wide
- **Per-crate**: 40% for any crate with > 1,000 lines

### E2 — REST transport CI job (new — Phase 06/07)

Add a dedicated CI job that verifies REST transport and OpenAPI:
```yaml
rest-transport:
  name: REST Transport
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo test -p fraiseql-core --lib -- openapi
    - run: cargo test -p fraiseql-server --features rest-transport --lib -- rest
    - run: cargo clippy -p fraiseql-server --features rest-transport -- -D warnings
```

### E3 — Python SDK CI (new — Phase 06)

Add Python REST tests to the Python SDK CI workflow:
```yaml
python-sdk:
  steps:
    - run: cd sdks/official/fraiseql-python && PYTHONPATH=src uv run pytest
    # 82 tests including REST annotation validation
```

### E4 — Make mutation testing blocking

Change `mutation.yml` from `continue-on-error: true` to `false`.
Only do this AFTER Track D stabilizes the test suite.

### Verification
Push a branch and verify all CI jobs pass.

---

## Track F — SDK Completeness (best-effort)

### Java SDK (Limited → Moderate)

Close the gap to match Go SDK level:
- Add `@FraiseQL.mutation()` with `sql_source`, `operation`, `invalidates`
- Add `FraiseQL.field()` with `requires_scope`, `deprecated`, `description`
- Add `@FraiseQL.subscription()` with `entity_type`, `topic`, `operation`
- Add `restPath` / `restMethod` to `@FraiseQL.query` and `@FraiseQL.mutation`
  (matching Python/TypeScript/Go SDK REST support added in Phase 06)
- Add 50+ unit tests covering annotation round-trips and schema JSON output

### PHP SDK (Alpha → Functional)

Minimum viable set for release:
- `@FraiseQL\Query` and `@FraiseQL\Mutation` attributes (matching Python)
- Schema JSON output including `rest` block when `rest_path` given
- PHPUnit tests
- CI workflow: PHP 8.2 + 8.3

### REST annotation parity across SDKs

Phase 06 added REST support to Python and TypeScript. Remaining SDKs:

| SDK | REST annotation | Status |
|-----|-----------------|--------|
| Python | `rest_path`, `rest_method` kwargs | ✅ Phase 06 |
| TypeScript | `restPath?`, `restMethod?` on `OperationConfig` | ✅ Phase 06 |
| Go | not yet added | ❌ Track F |
| C# | not yet added | ❌ Track F |
| Java | not yet added | ❌ Track F |
| Kotlin | not yet added | ❌ Track F |
| Scala | not yet added | ❌ Track F |
| Swift | not yet added | ❌ Track F |

For each SDK, the REST annotation should emit:
```json
{
  "rest": { "path": "/users/{id}", "method": "GET" }
}
```
in the operation's schema JSON output (matching `RestRoute` in `fraiseql-core`).

### Verification
```bash
cd sdks/official/fraiseql-java && CI=true mvn test
cd sdks/official/fraiseql-php && composer test
```

---

## Work Order

Execute in this order to maximize value and unblock downstream:

1. **Track D1** — fix integration test failures (unblocks CI gate)
2. **Track A Tier 1** — document `fraiseql-error`, `CompiledSchema`, `ServerConfig`
3. **Track B Tier 1** — fix production panic-risk unwraps
4. **Track C** — allow suppressions audit (clippy must pass)
5. **Track D3** — fix Test Suite failures
6. **Track D4** — REST transport integration tests
7. **Track E1** — add coverage CI
8. **Track E2** — REST transport CI job
9. **Track E3** — Python SDK CI
10. **Track A Tier 2** — internal API documentation (including REST transport)
11. **Track B Tier 2** — remaining unwrap audit
12. **Track D2** — add tests for under-tested crates
13. **Track D5/D6** — verify SDK tests in CI
14. **Track E4** — mutation testing blocking
15. **Track F** — SDK completeness (REST annotation parity + Java/PHP)

Each track committed independently (`docs(error):`, `fix(webhooks):`,
`chore(allows):`, etc.) for clean `git bisect`.

---

## Quality Score Projection

| Dimension | Before | After | Delta |
|-----------|--------|-------|-------|
| Test Coverage | 4.5 | 4.7 | +0.2 |
| Documentation | 2.0 | 3.5 | +1.5 |
| Error Handling | 2.5 | 3.5 | +1.0 |
| Code Complexity | 4.0 | 4.5 | +0.5 |
| CI/CD | 4.0 | 4.5 | +0.5 |
| SDK Completeness | 4.0 | 4.3 | +0.3 |
| **Total** | **3.9** | **~4.3** | **+0.4** |

Conservative projection — actual may be higher if Tier 2/3 work completes.

---

## Minimum Viable Release Threshold

The following are **release-blocking** (must complete before Phase 08 Finalize):
- [ ] Track A Tier 1: `fraiseql-error`, `CompiledSchema`, `ServerConfig` at 100%
- [ ] Track B Tier 1: zero production panic-risk unwraps
- [ ] Track D1: all integration test CI jobs pass
- [ ] Track D3: all Test Suite CI jobs pass
- [ ] Track D4: REST transport integration tests pass (new)

Everything else is nice-to-have — ship at whatever point reached.

## Estimated Effort: 5–10 days
