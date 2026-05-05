---
title: Test Extraction — fraiseql-test-utils, fraiseql-storage, fraiseql-error
status: planned
---

# Phase 40: `fraiseql-test-utils`, `fraiseql-storage`, `fraiseql-error`

## Objective

Extract inline tests from three small crates in one phase.

## fraiseql-test-utils (9 files)

All 9 files are leaf modules at the top level of `src/`:

| File |
|------|
| `assertions.rs` |
| `db.rs` |
| `failing_adapter.rs` |
| `fixtures.rs` |
| `lib.rs` |
| `mock_db.rs` |
| `observers.rs` |
| `saga.rs` |
| `schema_builder.rs` |

**Note**: `fraiseql-test-utils` is a test helper crate — its own tests are
meta-tests. A top-level `src/tests.rs` with declaration in `lib.rs` is the
correct approach.

## fraiseql-storage (1 file)

| File |
|------|
| `service/mod.rs` |

> Many `fraiseql-storage` files already have `tests.rs` — only `service/mod.rs`
> has a remaining inline block.

→ `service/tests.rs`

## fraiseql-error (1 file)

| File |
|------|
| `core_error.rs` |

This is a top-level leaf file. Check whether `lib.rs` exists and already has a
`#[cfg(test)] mod tests;`. If so, add a `tests.rs`. Otherwise create
`src/tests.rs` with the declaration in `core_error.rs`'s parent module.

→ `src/tests.rs` with declaration in `lib.rs`.

## Commit

```
refactor: extract test-utils, storage, error inline tests to tests.rs
```

## Verification

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-test-utils -p fraiseql-storage -p fraiseql-error --lib
```
