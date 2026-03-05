# FraiseQL — Master Remediation Plan 3

> Issues identified by independent post-v2.1.0-dev "rapport d'étonnement".
> Ordered by risk, then dependency. Batches can be parallelized unless
> a **Requires** note is present.

Severity: 🔴 Critical · 🟠 High · 🟡 Medium · 🔵 Low
Status:   ✅ Done · ❌ Blocked · 🔄 In progress · (blank) Pending

---

## BATCH 1 — Test utility consolidation

**Risk**: Three inconsistent `DATABASE_URL` helpers with different error
semantics and default connection strings cause tests to behave differently
in CI vs. local dev depending on which file they happen to be in.
`fraiseql-test-utils` is nearly invisible despite providing exactly the
shared infrastructure each crate reimplements independently.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| TU-1  | 🟠  | ✅ Done | Add `database_url() -> String` to `fraiseql-test-utils` — panics with actionable message if `DATABASE_URL` is unset; uses no fallback (tests that run without a DB must be `#[ignore]`) | `crates/fraiseql-test-utils/src/db.rs` (new) |
| TU-2  | 🟠  | ✅ Done | Replace the three inconsistent inline `DATABASE_URL` patterns with `fraiseql_test_utils::database_url()` — `database_integration_test.rs` (silent fallback), `database_query_test.rs` (local `require_database_url`), `observer_test_helpers.rs` (auth-credentialed fallback) | `crates/fraiseql-server/tests/` |
| TU-3  | 🟡  | ✅ Done | Move observer test setup from `observer_test_helpers.rs` into `fraiseql-test-utils::observers` submodule — exposes `ObserverTestHarness`, `cleanup_test_data()`, and `get_test_id()` to all crates | `crates/fraiseql-test-utils/src/observers.rs` (new) |
| TU-4  | 🟡  | ✅ Done | Add `fraiseql-test-utils` as `[dev-dependencies]` to `fraiseql-observers/Cargo.toml` and replace any duplicated setup code in that crate's tests | `crates/fraiseql-observers/Cargo.toml` |
| TU-5  | 🟡  | ✅ Done | Document `fraiseql-test-utils` in its crate-level `//!` doc comment with a usage table listing every public helper, its purpose, and a one-line example | `crates/fraiseql-test-utils/src/lib.rs` |
| TU-6  | 🟡  | ✅ Done | Assert adoption in CI: add a `tools/check-test-imports.sh` script that greps for bare `std::env::var("DATABASE_URL")` in `tests/` directories and fails if any remain after this batch | `tools/check-test-imports.sh` (new) |
| TU-7  | 🔵  | ✅ Done | Add `setup_test_schema(schema_json: &str) -> CompiledSchema` to `fraiseql-test-utils` — compiles a schema string for use in unit tests that need a `CompiledSchema` without a real file | `crates/fraiseql-test-utils/src/schema.rs` (new) |
| TU-8  | 🔵  | ✅ Done | Add `assert_graphql_error_code(response, code)` and `assert_field_path(response, path, value)` helpers — currently tests inline these comparisons verbosely | `crates/fraiseql-test-utils/src/assertions.rs` |
| TU-9  | 🔵  | ✅ Done | Add `fraiseql-test-utils` to the test-quality-standards infrastructure doc from Campaign 2 — all new tests must import shared helpers before rolling their own | `infrastructure/test-utility-adoption-policy.md` (new) |

See `batches/batch-1-test-utility.md` for the exact helper signatures and
migration checklist.

---

## BATCH 2 — Error path coverage

**Risk**: `compile()` has four internal error paths (parser, validator,
lowering, codegen) with zero failing-input tests. A refactor that silently
changes error types or messages will not be caught. Database adapters have
no tests for connection failure, query timeout, or response parsing errors —
the most common production failure modes.

**Requires**: TU-7 recommended first (tests use `setup_test_schema`).

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| EP-1  | 🟠  | ✅ Done | Add compiler parse-error tests: invalid JSON, missing `types` key, `types` not an array, field with no `name` key — each must return `FraiseQLError::Parse` | `crates/fraiseql-core/src/compiler/mod.rs` (in-module tests) |
| EP-2  | 🟠  | ✅ Done | Add compiler validation-error tests: unknown field type, query with unknown return type — each must return `FraiseQLError::Validation` | `crates/fraiseql-core/src/compiler/mod.rs` (in-module tests) |
| EP-3  | 🟡  | ✅ Done | Add compiler lowering-error tests: query referencing undefined type, mutation referencing undefined type, subscription referencing undefined type — each returns `FraiseQLError::Validation` with type name in message | `crates/fraiseql-core/tests/compiler_error_paths.rs` (new, 6 tests) |
| EP-4  | 🟡  | ✅ Done | Add compiler codegen-error tests: field referencing undefined nested type — returns `FraiseQLError::Validation` with type name | `crates/fraiseql-core/tests/compiler_error_paths.rs` (same file, covered by EP-3) |
| EP-5  | 🟠  | ✅ Done | Add PostgreSQL adapter tests for connection failure: malformed URL fails at pool creation/`pool.get()` → `FraiseQLError::ConnectionPool` (2 tests) | `crates/fraiseql-core/src/db/postgres/adapter.rs` `unit_tests` |
| EP-6  | 🟡  | ✅ Done | Add MySQL adapter tests for the same failure modes as EP-5 (2 tests) | `crates/fraiseql-core/src/db/mysql/adapter.rs` `unit_tests` |
| EP-7  | 🟡  | ✅ Done | SQLite `execute_function_call` already has regression test `test_function_call_returns_unsupported_error` from Campaign 1 — no new code needed | `crates/fraiseql-core/src/db/sqlite/adapter.rs` |
| EP-8  | 🟡  | ✅ Done | Add validator error-branch tests for 7 validators: pattern, length (too short + too long), enum (wrong value + case mismatch), required, email (3 cases), phone (3 cases) — each asserts `FraiseQLError::Validation` with correct field path (13 tests) | `crates/fraiseql-core/tests/validation_error_paths.rs` (new) |

See `batches/batch-2-error-coverage.md` for MockPool design and the validator
error message contract.

---

## BATCH 3 — Property testing extension

**Risk**: `fraiseql-server` middleware (rate limiting with 11 bucket strategies,
auth header parsing, query complexity) and `fraiseql-observers` state machine
receive no generative test coverage. These are the components most likely to
encounter unexpected real-world input.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| PT-1  | 🟠  |        | Add property tests for rate-limit bucket key construction: arbitrary IP strings must never panic; key format must be stable under Unicode input, embedded colons, and empty strings | `crates/fraiseql-server/tests/property_rate_limiting.rs` (new) |
| PT-2  | 🟡  |        | Add property tests for auth header parsing: arbitrary `Authorization` header values must either parse successfully or return `FraiseQLError::Authentication` — must never panic | `crates/fraiseql-server/tests/property_auth_parsing.rs` (new) |
| PT-3  | 🟡  |        | Add property tests for query complexity calculation: arbitrary GraphQL query ASTs (generated by `proptest` strategies) must produce a non-negative complexity score; depth exceeding the limit must always return an error | `crates/fraiseql-server/tests/property_query_complexity.rs` (new) |
| PT-4  | 🟡  |        | Add property tests for observer state machine transitions: from any valid state, only the documented next states must be reachable; invalid transitions must return an error, never panic | `crates/fraiseql-observers/tests/property_state_machine.rs` (new) |
| PT-5  | 🔵  |        | Add property tests for cascade invalidation: arbitrary sets of mutation-to-view mappings must produce invalidation sets that are a superset of direct dependants and a subset of all views — no over-invalidation, no under-invalidation | `crates/fraiseql-core/tests/property_cache_invalidation.rs` (new) |

See `batches/batch-3-property-testing.md` for `proptest` strategy definitions
and the state machine transition table.

---

## BATCH 4 — Arrow Flight completeness

**Risk**: `execute_placeholder_query` is imported into the live service code
path in `service.rs:545`. A request that triggers the `None` branch for the
database adapter (development/testing mode) silently returns placeholder data
in production if misconfigured. Three `Status::unimplemented()` stubs in
`handlers.rs` are undocumented in any issue tracker, meaning they are invisible
to users who discover them only at runtime.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| AF-1  | 🟠  |        | Guard `execute_placeholder_query` behind `#[cfg(test)]` or a `testing` feature flag — production service must not be able to call it; add a compile-time assertion | `crates/fraiseql-arrow/src/flight_server/convert.rs:218`, `service.rs:545` |
| AF-2  | 🟠  |        | Implement `do_get` for `BulkExport` command or return a documented `Status::unimplemented` with a link to the tracking issue in the error message (currently the message is `"BulkExport not implemented yet"` with no context) | `crates/fraiseql-arrow/src/flight_server/handlers.rs:226` |
| AF-3  | 🟠  |        | Implement `do_put` for `BulkExport` command — same as AF-2 | `crates/fraiseql-arrow/src/flight_server/handlers.rs:1087` |
| AF-4  | 🟡  |        | Implement `poll_flight_info` or replace the stub with a structured `Status::unimplemented` that names the missing feature and version target | `crates/fraiseql-arrow/src/flight_server/handlers.rs:1130` |
| AF-5  | 🔵  |        | Add integration tests for the implemented Arrow Flight commands (`do_get`/`do_put` for the non-BulkExport path) — currently the flight server has zero integration tests | `crates/fraiseql-arrow/tests/flight_integration.rs` (new) |

See `batches/batch-4-arrow-flight.md` for the BulkExport protocol design and
the `#[cfg(test)]` guard pattern.

---

## BATCH 5 — Core split prerequisite work

**Risk**: CA-1 and CA-2 from Campaign 2 remain blocked by two concrete coupling
points. Without resolving them, `fraiseql-core` will reach the 150K-line split
threshold with no clear migration path. This batch resolves the coupling so the
splits from Campaign 2 can actually land.

**Requires**: All other batches recommended first (reduces surface area before
restructuring).

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| CS-1  | 🟡  |        | Relocate `compiler::aggregation::OrderByClause` and `schema::SqlProjectionHint` to a new `crates/fraiseql-core/src/types/sql_hints.rs` module — removes the import that blocks `fraiseql-db` extraction (CA-1 blocker) | `crates/fraiseql-core/src/compiler/aggregation.rs`, `schema/compiled.rs` |
| CS-2  | 🟡  |        | Introduce `ExecutorAdapter` trait in `crates/fraiseql-core/src/cache/adapter/mod.rs` that `runtime/executor/` implements — removes the direct type import that creates the circular dependency blocking CA-2 | `crates/fraiseql-core/src/cache/adapter/mod.rs`, `runtime/executor/mod.rs` |
| CS-3  | 🟡  |        | Extract `crates/fraiseql-core/src/db/` into `crates/fraiseql-db/` — unblocked after CS-1; update `fraiseql-core/Cargo.toml` to depend on `fraiseql-db` | New crate `crates/fraiseql-db/` (resolves CA-1) |
| CS-4  | 🟡  |        | Extract `crates/fraiseql-core/src/runtime/executor/` into `crates/fraiseql-executor/` — unblocked after CS-2; update `fraiseql-core/Cargo.toml` to depend on `fraiseql-executor` | New crate `crates/fraiseql-executor/` (resolves CA-2) |

See `batches/batch-5-core-split.md` for the full dependency graph analysis,
migration steps, and the `ExecutorAdapter` trait design.

---

## Infrastructure

| ID    | Priority | What | Document |
|-------|----------|------|----------|
| INF-1 | 🟠 High  | Test utility adoption policy — mandate `fraiseql-test-utils` for all new integration tests; ban bare `std::env::var("DATABASE_URL")` in test files | `infrastructure/test-utility-adoption-policy.md` |

---

## Summary by severity

| Severity   | Count | Items |
|------------|-------|-------|
| 🟠 High    | 10    | TU-1, TU-2, EP-1, EP-2, EP-5, PT-1, AF-1, AF-2, AF-3, AF-4 |
| 🟡 Medium  | 13    | TU-3..6, EP-3, EP-4, EP-6..8, PT-2..4, CS-1..4 |
| 🔵 Low      | 4    | TU-7..9, PT-5, AF-5 |
| **Total**  | **27** | |

## Dependency order

```
TU-1 → TU-2          (helpers before migration)
TU-7 → EP-1..8       (setup_test_schema before compiler tests)
TU-1..9 → CS-1..4    (reduce surface before restructuring)
CS-1  → CS-3         (relocate types before extracting crate)
CS-2  → CS-4         (trait abstraction before extracting crate)
EP-1..4 concurrently (independent compiler error paths)
PT-1..5 concurrently (independent property suites)
AF-1 before AF-2..4  (guard placeholder before implementing stubs)
```
