---
title: Test Extraction — fraiseql-core schema/
status: planned
---

# Phase 12: `fraiseql-core` — `schema/`

## Objective

Extract inline tests from the `schema/` subsystem of `fraiseql-core`.

## Files

| Directory / File | Existing tests.rs? | Notes |
|------------------|--------------------|-------|
| `schema/mod.rs` | `schema/tests.rs` ✅ | May have residual inline blocks |
| `schema/introspection/mod.rs` | no | ~887 test lines — largest block in codebase |
| `schema/introspection/field_resolver.rs` | no | |
| `schema/dependency_graph/mod.rs` | no | |
| `schema/compiled/mod.rs` | `compiled/tests.rs` ✅ | May have residual inline blocks |

> `schema/compiled/tests.rs` and `schema/tests.rs` already exist — check for
> any remaining inline blocks in those files and merge if found.

## Steps

1. `schema/introspection/` — largest block in the workspace. Create
   `schema/introspection/tests.rs`. The test block uses internal helpers;
   check for private functions that need `pub(super)` promotion.

2. `schema/dependency_graph/` — create `schema/dependency_graph/tests.rs` or
   add to `schema/tests.rs` depending on the block structure.

3. Scan `schema/mod.rs` and `schema/compiled/mod.rs` for any remaining inline
   blocks that weren't cleaned in earlier partial work.

## Commit

```
refactor(core): extract schema/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
```
