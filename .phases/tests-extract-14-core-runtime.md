---
title: Test Extraction — fraiseql-core runtime/
status: planned
---

# Phase 14: `fraiseql-core` — `runtime/`

## Objective

Extract inline tests from the `runtime/` subsystem of `fraiseql-core`.

## Files (18 files)

| File | Notes |
|------|-------|
| `runtime/mod.rs` | Runtime entry point |
| `runtime/projection.rs` | Field projection (~659 test lines) |
| `runtime/executor/mod.rs` | Executor root |
| `runtime/executor/core.rs` | Core execution path |
| `runtime/executor/runners/query.rs` | Query runner (~326 test lines) |
| `runtime/executor/runners/mutation.rs` | Mutation runner |
| `runtime/executor/support/` | Various support files |
| `runtime/query_builder.rs` | Query building |
| `runtime/mutation_executor.rs` | Mutation execution |
| `runtime/variables.rs` | Variable handling |
| `runtime/selection.rs` | Selection set processing |
| `runtime/coercion.rs` | Value coercion |
| `runtime/context.rs` | Execution context |

> `runtime/executor/tests.rs`, `runtime/subscription/tests.rs`, and
> `runtime/aggregation/tests.rs` already exist — skip those subdirectories.

## Steps

Files under `runtime/executor/runners/` and `runtime/executor/support/` are
leaf files. Their tests consolidate into:

- `runtime/executor/runners/tests.rs`
- `runtime/executor/support/tests.rs`
- `runtime/tests.rs` for top-level runtime leaf files

## Commit

```
refactor(core): extract runtime/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
```
