# Phase 07: fraiseql-storage Repair

## Objective

Fix pre-existing compile errors in `fraiseql-storage` and `platform_e2e_test.rs`
so `cargo check --workspace --all-features` is fully clean.

## Success Criteria

- [ ] `cargo check -p fraiseql-storage --all-features` produces zero errors
- [ ] `cargo test -p fraiseql-server --test platform_e2e_test --all-features` compiles and passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean

## Background

These errors existed before the v2.3.0 sprint and were explicitly excluded as
pre-existing. They need to be fixed before v2.3.0 can ship a clean workspace build.

### fraiseql-storage errors (3 files)

Affected files:

- `crates/fraiseql-storage/src/backend/azure.rs` (lines 162, 193)
- `crates/fraiseql-storage/src/backend/gcs.rs` (lines 240, 265)
- `crates/fraiseql-storage/src/backend/mod.rs` (lines 272, 292)

Root cause: `FraiseQLError: From<FileError>` trait bound not satisfied —
`FileError` was added to `fraiseql-error` but the `From` impl was never added.
Fix: either add `impl From<FileError> for FraiseQLError` in `fraiseql-error`,
or convert the error sites to use `map_err`.

### platform_e2e_test.rs errors (2 symbols)

- `fraiseql_server::subsystems` — module exists in server internals but is not
  `pub use`-ed from the crate root
- `fraiseql_server::schema::loader::FunctionsConfig` — struct was renamed or
  moved; test references old path

Fix: update the test imports to use the current public API.

## TDD Cycles

### Cycle 1: fraiseql-storage FileError Bridge

- **RED**: Run `cargo check -p fraiseql-storage --all-features`; confirm 6 errors
- **GREEN**: Add `impl From<FileError> for FraiseQLError` in `crates/fraiseql-error/src/lib.rs`;
  or convert `?`-propagation at the call sites with explicit `.map_err(FraiseQLError::from)`
- **REFACTOR**: Confirm the variant mapping is consistent with the error hierarchy
- **CLEANUP**: Zero errors, zero clippy warnings on `fraiseql-storage`

### Cycle 2: platform_e2e_test Import Repair

- **RED**: Run `cargo test -p fraiseql-server --test platform_e2e_test --all-features`;
  confirm 2 unresolved symbols
- **GREEN**: Trace the current public path of `subsystems` and `FunctionsConfig`;
  update the test imports
- **REFACTOR**: If `subsystems` should be public, add `pub use` in the server crate root;
  otherwise refactor the test to use the builder API instead of direct module access
- **CLEANUP**: Test compiles and all 15 platform E2E tests pass (10 structural + 5 gated)

## Dependencies

- Requires: Phase 06 may run in parallel
- Blocks: Phase 10 (finalize)

## Status

[ ] Not Started
