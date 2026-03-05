# Batch 1 — Test Utility Consolidation

## Problem

Three separate `DATABASE_URL` resolution patterns exist in `fraiseql-server/tests/`:

| File | Behaviour |
|------|-----------|
| `database_integration_test.rs` | Silent fallback to `postgresql:///fraiseql_test` |
| `database_query_test.rs` | Panics with "DATABASE_URL must be set" |
| `observer_test_helpers.rs` | Fallback to `postgresql://postgres:postgres@localhost/fraiseql_test` |

A test that passes in CI (where `DATABASE_URL` is set) may silently run against
the wrong database in local dev, or fail with a confusing connection error,
depending on which file's convention is in use.

`fraiseql-test-utils` exports `assert_graphql_success`, `assert_no_graphql_errors`,
`TestSagaExecutor`, and the `Clock` family, but fewer than 5% of test files
import it. Most tests re-implement assertions inline.

---

## Fix Plan

### TU-1 — Add `database_url()` to `fraiseql-test-utils`

**New file**: `crates/fraiseql-test-utils/src/db.rs`

```rust
/// Returns the test database URL.
///
/// # Panics
///
/// Panics with an actionable message if `DATABASE_URL` is not set.
/// Tests requiring a database must be marked `#[ignore]` and run with
/// `cargo nextest run --run-ignored`.
#[must_use]
pub fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "DATABASE_URL is not set. \
             Database tests must be run with a live database. \
             Set DATABASE_URL=postgresql://... or mark this test #[ignore]."
        )
    })
}
```

Add `pub mod db; pub use db::database_url;` to `lib.rs`.

### TU-2 — Migrate all three DATABASE_URL call sites

Replace every occurrence of:
```rust
std::env::var("DATABASE_URL").unwrap_or_else(|_| "...".to_string())
std::env::var("DATABASE_URL").expect("DATABASE_URL must be set...")
```
with:
```rust
use fraiseql_test_utils::database_url;
let db_url = database_url();
```

Files to update:
- `crates/fraiseql-server/tests/database_integration_test.rs`
- `crates/fraiseql-server/tests/database_query_test.rs` (remove local `require_database_url`)
- `crates/fraiseql-server/tests/observer_test_helpers.rs` (remove `get_database_url`)

### TU-3 — Move observer test helpers

Move `ObserverTestHarness`, `cleanup_test_data()`, `get_test_id()` from
`observer_test_helpers.rs` into a new `crates/fraiseql-test-utils/src/observers.rs`.
Keep `observer_test_helpers.rs` as a thin re-export shim for the single release,
then remove it in the next batch cleanup.

```rust
// crates/fraiseql-test-utils/src/observers.rs
pub struct ObserverTestHarness { ... }
pub fn cleanup_test_data(pool: &Pool, test_id: &str) { ... }
pub fn get_test_id() -> String { Uuid::new_v4().to_string() }
```

### TU-4 — Add fraiseql-test-utils to fraiseql-observers

```toml
# crates/fraiseql-observers/Cargo.toml
[dev-dependencies]
fraiseql-test-utils = { path = "../fraiseql-test-utils" }
```

Remove any duplicated test setup code from `fraiseql-observers/tests/`.

### TU-5 — Document the crate

Replace the current minimal `//!` header in `lib.rs` with:

```rust
//! # FraiseQL Test Utilities
//!
//! Shared testing infrastructure for all FraiseQL crates.
//!
//! ## Available helpers
//!
//! | Helper | Module | Purpose |
//! |--------|--------|---------|
//! | `database_url()` | `db` | Resolve `DATABASE_URL` or panic with actionable message |
//! | `setup_test_schema()` | `schema` | Compile a schema string into `CompiledSchema` |
//! | `assert_graphql_success()` | `assertions` | Assert response has no errors |
//! | `assert_no_graphql_errors()` | `assertions` | Assert `errors` field is absent |
//! | `assert_has_data()` | `assertions` | Assert `data` field is present and non-null |
//! | `assert_graphql_error_contains()` | `assertions` | Assert error message substring |
//! | `assert_graphql_error_code()` | `assertions` | Assert error extension code |
//! | `assert_field_path()` | `assertions` | Assert value at nested field path |
//! | `ManualClock` | (re-export) | Injectable clock for time-controlled tests |
//! | `ObserverTestHarness` | `observers` | Set up observer integration test environment |
//! | `TestSagaExecutor` | `saga` | Execute saga steps in tests |
//!
//! ## Quick start
//!
//! ```ignore
//! use fraiseql_test_utils::{database_url, assert_graphql_success};
//!
//! #[tokio::test]
//! #[ignore = "requires DATABASE_URL"]
//! async fn my_integration_test() {
//!     let url = database_url();
//!     // ...
//! }
//! ```
```

### TU-6 — Enforce in CI

New script `tools/check-test-imports.sh`:

```bash
#!/usr/bin/env bash
# Fails if any test file uses bare DATABASE_URL resolution instead of fraiseql-test-utils.
set -euo pipefail

PATTERN='std::env::var\("DATABASE_URL"\)'
MATCHES=$(grep -r "$PATTERN" crates/*/tests/ --include="*.rs" -l 2>/dev/null || true)

if [ -n "$MATCHES" ]; then
  echo "ERROR: Bare DATABASE_URL resolution found in test files."
  echo "Use fraiseql_test_utils::database_url() instead."
  echo ""
  echo "$MATCHES"
  exit 1
fi
echo "OK: No bare DATABASE_URL patterns in test files."
```

Add to `.github/workflows/ci.yml` as a step named `check-test-imports`.

### TU-7 — Add `setup_test_schema()`

New file `crates/fraiseql-test-utils/src/schema.rs`:

```rust
use fraiseql_core::{compiler::Compiler, schema::CompiledSchema};

/// Compile a raw schema JSON string into a `CompiledSchema` for use in tests.
///
/// # Panics
///
/// Panics with a descriptive message if the schema JSON is invalid.
#[must_use]
pub fn setup_test_schema(schema_json: &str) -> CompiledSchema {
    Compiler::default()
        .compile(schema_json)
        .expect("test schema must be valid")
}
```

### TU-8 — Add assertion helpers

Add to `crates/fraiseql-test-utils/src/assertions.rs`:

```rust
/// Assert the GraphQL response contains an error with the given extension code.
pub fn assert_graphql_error_code(response: &serde_json::Value, code: &str) { ... }

/// Assert the value at the given dot-separated field path equals `expected`.
/// Example path: `"data.user.email"`
pub fn assert_field_path(response: &serde_json::Value, path: &str, expected: &serde_json::Value) { ... }
```

### TU-9 — Update infrastructure doc

Append to `.remediation_2/infrastructure/test-quality-standards.md`:

> **Test utility adoption**
>
> New integration tests MUST import from `fraiseql-test-utils` for:
> - Database URL resolution (`database_url()`)
> - Schema compilation in tests (`setup_test_schema()`)
> - GraphQL response assertions (`assert_graphql_success()`, etc.)
>
> The `tools/check-test-imports.sh` script enforces this in CI.
> Adding a new bare `std::env::var("DATABASE_URL")` call in `tests/` will
> cause the CI `check-test-imports` step to fail.

---

## Verification

- [ ] `tools/check-test-imports.sh` exits 0 on the full codebase
- [ ] `fraiseql-test-utils` documentation renders correctly with `cargo doc -p fraiseql-test-utils --open`
- [ ] All three previously inconsistent test files use `database_url()`
- [ ] `fraiseql-observers` dev-dependencies include `fraiseql-test-utils`
- [ ] `cargo nextest run -p fraiseql-test-utils` passes
