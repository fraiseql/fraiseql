# Batch 2 — Error Path Coverage

## Problem

`Compiler::compile()` has four internal error paths (parser, validator, lowering,
codegen). In-module tests cover two happy-path scenarios. No test passes an
invalid input and asserts on the resulting error type, variant, or message.

Database adapter modules (`postgres`, `mysql`, `sqlite`, `sqlserver`) have no
tests for the most common production failure modes: connection pool exhaustion,
query timeout, response parsing error.

The validation module has 44+ validators. Happy-path coverage is good. Error
branches — the paths that emit `ValidationError` — are largely untested, meaning
a refactor that changes error field paths or messages produces no CI signal.

---

## Fix Plan

### EP-1 — Compiler parse errors

Add to `crates/fraiseql-core/src/compiler/mod.rs` in-module tests:

```rust
#[test]
fn test_compile_rejects_invalid_json() {
    let err = Compiler::default().compile("not json").unwrap_err();
    assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
}

#[test]
fn test_compile_rejects_missing_types_key() {
    let err = Compiler::default().compile(r#"{"queries": []}"#).unwrap_err();
    assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
}

#[test]
fn test_compile_rejects_types_not_array() {
    let err = Compiler::default().compile(r#"{"types": "wrong"}"#).unwrap_err();
    assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
}

#[test]
fn test_compile_rejects_field_without_name() {
    let schema = r#"{"types": [{"fields": [{"type": "String"}]}]}"#;
    let err = Compiler::default().compile(schema).unwrap_err();
    assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
}
```

### EP-2 — Compiler validation errors

Add to `crates/fraiseql-core/src/compiler/mod.rs` in-module tests:

```rust
#[test]
fn test_compile_rejects_circular_type_reference() {
    // A → B → A without @list
    let schema = include_str!("../tests/fixtures/circular_types.json");
    let err = Compiler::default().compile(schema).unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }), "got: {err:?}");
}

#[test]
fn test_compile_rejects_unknown_field_type() {
    let schema = r#"{"types": [{"name": "User", "fields": [
        {"name": "id", "type": "NonExistentType"}
    ]}]}"#;
    let err = Compiler::default().compile(schema).unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }), "got: {err:?}");
}
```

Add fixture file `crates/fraiseql-core/tests/fixtures/circular_types.json`.

### EP-3 & EP-4 — Compiler lowering and codegen errors

New file `crates/fraiseql-core/tests/compiler_error_paths.rs`:

```rust
//! Tests for compiler error paths: lowering and codegen.

use fraiseql_core::{compiler::Compiler, error::FraiseQLError};

#[test]
fn test_query_referencing_undefined_type_fails_lowering() {
    let schema = r#"{"types": [], "queries": [
        {"name": "getUser", "return_type": "User"}
    ]}"#;
    let err = Compiler::default().compile(schema).unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }), "got: {err:?}");
    // Error message must name the unknown type
    if let FraiseQLError::Validation { message, .. } = err {
        assert!(message.contains("User"), "message: {message}");
    }
}

#[test]
fn test_mutation_without_sql_source_fails_lowering() {
    let schema = r#"{"types": [{"name": "User", "fields": []}],
     "mutations": [{"name": "createUser", "return_type": "User"}]}"#;
    let err = Compiler::default().compile(schema).unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }), "got: {err:?}");
}

#[test]
fn test_unsupported_directive_combination_returns_unsupported() {
    // Use a fixture with two mutually exclusive directives
    let schema = include_str!("fixtures/unsupported_directives.json");
    let err = Compiler::default().compile(schema).unwrap_err();
    assert!(matches!(err, FraiseQLError::Unsupported { .. }), "got: {err:?}");
}
```

### EP-5 — PostgreSQL adapter connection failure

The `MockPool` approach uses the existing `DatabaseAdapter` trait.
Add to `crates/fraiseql-core/src/db/postgres/adapter.rs` in-module tests:

```rust
#[cfg(test)]
mod error_tests {
    use super::*;
    use fraiseql_core::error::FraiseQLError;

    struct AlwaysFailPool;

    impl ConnectionPool for AlwaysFailPool {
        type Connection = /* postgres connection type */;
        async fn get(&self) -> Result<Self::Connection, PoolError> {
            Err(PoolError::Backend("simulated connection failure".into()))
        }
    }

    #[tokio::test]
    async fn test_execute_query_returns_connection_pool_error_on_pool_failure() {
        let adapter = PostgresAdapter::with_pool(AlwaysFailPool);
        let result = adapter.execute_query("SELECT 1", &[]).await;
        assert!(
            matches!(result, Err(FraiseQLError::ConnectionPool { .. })),
            "got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_execute_mutation_returns_connection_pool_error_on_pool_failure() {
        let adapter = PostgresAdapter::with_pool(AlwaysFailPool);
        let result = adapter.execute_mutation("INSERT INTO t VALUES (1)", &[]).await;
        assert!(
            matches!(result, Err(FraiseQLError::ConnectionPool { .. })),
            "got: {result:?}"
        );
    }
}
```

If `PostgresAdapter` does not expose `with_pool()`, add it as a test-only
constructor gated behind `#[cfg(test)]`.

### EP-6 — MySQL adapter (same pattern)

Mirror EP-5 in `crates/fraiseql-core/src/db/mysql/adapter.rs`.

### EP-7 — SQLite adapter regression

The Campaign 1 fix made `execute_function_call` return `FraiseQLError::Unsupported`
instead of silently returning `Ok(())`. Add a regression test:

```rust
#[tokio::test]
async fn test_execute_function_call_returns_unsupported() {
    let adapter = SqliteAdapter::in_memory().await.expect("in-memory db");
    let result = adapter.execute_function_call("my_func", &[]).await;
    assert!(
        matches!(result, Err(FraiseQLError::Unsupported { .. })),
        "SQLite function calls must return Unsupported, got: {result:?}"
    );
}
```

### EP-8 — Validator error branches

New file `crates/fraiseql-core/tests/validation_error_paths.rs`:

The following validators must each have a test that:
1. Passes an invalid value
2. Asserts the result is `Err(ValidationError { field, .. })`
3. Asserts the field path in the error matches the input field name

Validators to cover: `EmailValidator`, `PhoneValidator`, `UrlValidator`,
`UuidValidator`, `DateValidator`, `IntegerRangeValidator`,
`StringLengthValidator`, `RegexPatternValidator`, `EnumMembershipValidator`,
`RequiredFieldValidator`.

```rust
#[test]
fn test_email_validator_rejects_missing_at_symbol() {
    let result = EmailValidator::default().validate("notanemail", "email");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.field, "email");
}
// ... repeat pattern for each validator
```

---

## Fixtures needed

Create `crates/fraiseql-core/tests/fixtures/`:
- `circular_types.json` — two types referencing each other without `@list`
- `unsupported_directives.json` — two mutually exclusive directives on same field

---

## Verification

- [ ] `cargo nextest run -p fraiseql-core --test compiler_error_paths` passes
- [ ] `cargo nextest run -p fraiseql-core --test validation_error_paths` passes
- [ ] In-module tests in `postgres/adapter.rs` and `mysql/adapter.rs` pass
- [ ] In-module tests in `sqlite/adapter.rs` include the regression for EP-7
- [ ] `cargo nextest run -p fraiseql-core` (full suite) still passes
