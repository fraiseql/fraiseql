# Test Utility Adoption Policy

**Effective from**: Campaign 3, Batch 1 completion

---

## Rule

All new integration tests in FraiseQL must use `fraiseql-test-utils` for
common setup tasks. Rolling your own helpers in individual test files is
not permitted for the patterns listed below.

---

## Mandatory imports from `fraiseql-test-utils`

| Need | Correct import | Banned alternative |
|------|---------------|-------------------|
| Resolve `DATABASE_URL` | `fraiseql_test_utils::database_url()` | `std::env::var("DATABASE_URL")` in test files |
| Compile a test schema | `fraiseql_test_utils::setup_test_schema(json)` | Inline `Compiler::default().compile(...)` |
| Assert response success | `fraiseql_test_utils::assert_graphql_success(&r)` | `assert!(r["errors"].is_null())` |
| Assert no errors | `fraiseql_test_utils::assert_no_graphql_errors(&r)` | Inline null check |
| Assert error message | `fraiseql_test_utils::assert_graphql_error_contains(&r, "msg")` | Inline string search |
| Assert error code | `fraiseql_test_utils::assert_graphql_error_code(&r, "CODE")` | Inline extension check |
| Controlled time | `fraiseql_test_utils::ManualClock::new(ts)` | `SystemTime::now()` or `Instant::now()` |
| Observer test setup | `fraiseql_test_utils::observers::ObserverTestHarness` | Local helper structs |

---

## Enforcement

The CI step `check-test-imports` (added in Campaign 3, Batch 1) runs
`tools/check-test-imports.sh` on every pull request. It will fail if any
test file under `crates/*/tests/` contains:

- `std::env::var("DATABASE_URL")`

Additional patterns may be added to the script as the policy matures.

---

## Adding new helpers to `fraiseql-test-utils`

If a common pattern is identified that is not yet in the crate:

1. Add the helper to the appropriate submodule (`db.rs`, `schema.rs`,
   `assertions.rs`, `observers.rs`, `saga.rs`)
2. Re-export it from `lib.rs`
3. Update the table in this document
4. Update the `//!` doc comment in `lib.rs`

Do not solve the problem locally in a test file and leave a `TODO: move to
test-utils` comment. Either add it to the crate in the same PR or open an
issue before merging.

---

## Exemptions

The following are not covered by this policy:

- **Unit tests** (`#[cfg(test)]` blocks inside `src/` files) — these test
  the module directly and should not import from a separate utilities crate.
- **Benchmarks** — handled by `../velocitybench`.
- **Property tests** — `proptest` strategies are test-file-local by nature.

---

## Rationale

Prior to Campaign 3, three distinct `DATABASE_URL` resolution patterns existed
with different error semantics and connection string defaults. A test that
passed in CI (where `DATABASE_URL` was set) failed silently or connected to
the wrong database in local development, depending on which file's convention
was in use.

Centralising these patterns in `fraiseql-test-utils` ensures:
1. Consistent behaviour across all test files
2. A single place to update defaults when they change
3. Improved discoverability — developers can find available helpers by
   reading the crate docs rather than grep-ing across test files
