# fraiseql-test-utils

Internal testing utilities for the FraiseQL workspace. **Not intended for direct use** — this crate is consumed only by the test suites of other FraiseQL crates and is not published to crates.io.

Provides shared helpers for resolving the test database URL, asserting on GraphQL response shapes (success/error/error-code/field-path), and constructing minimal compiled schemas for integration tests. The goal is to keep individual crate test modules small and consistent without each one re-implementing the same set of GraphQL assertion macros.

## Helpers

| Helper | Module | Purpose |
|--------|--------|---------|
| `database_url()` | `db` | Resolve `DATABASE_URL` or panic with an actionable message |
| `assert_graphql_success()` | `assertions` | Assert a GraphQL response has no `errors` field |
| `assert_no_graphql_errors()` | `assertions` | Assert the `errors` field is absent |
| `assert_has_data()` | `assertions` | Assert `data` is present and non-null |
| `assert_graphql_error_contains()` | `assertions` | Assert an error message contains a substring |
| `assert_graphql_error_code()` | `assertions` | Assert an error extension `code` value |
| `assert_field_path()` | `assertions` | Assert a value at a nested JSON field path |

## Usage

Add as a `dev-dependency` from within the workspace only:

```toml
[dev-dependencies]
fraiseql-test-utils = { path = "../fraiseql-test-utils" }
```

## Documentation

- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
