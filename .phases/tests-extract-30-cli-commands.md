---
title: Test Extraction — fraiseql-cli commands/
status: planned
---

# Phase 30: `fraiseql-cli` — `commands/`

## Objective

Extract inline tests from `fraiseql-cli`'s command implementations.

## Files (24 files)

### commands/ top-level (17 files)

Expected to include: `compile.rs`, `serve.rs`, `validate.rs`, `explain.rs`,
`analyze.rs`, `introspect.rs`, `migrate.rs`, `sbom.rs`, `cost.rs`,
`generate.rs`, `generate_views.rs`, `dependency_graph.rs`, and others.

> `commands/extract/tests.rs`, `commands/generate/tests.rs`,
> `commands/init/tests.rs` already exist — skip those subdirectories.

### commands/federation/ (2 files)

| File |
|------|
| `commands/federation/check.rs` (~700 test lines — largest in crate) |
| `commands/federation/compose.rs` |

→ `commands/federation/tests.rs`

### commands/gateway/ (5 files)

All 5 files are leaf files under `commands/gateway/`.

→ `commands/gateway/tests.rs`

### commands/schema/ (1 file)

> `commands/schema/` directory — check if `tests.rs` already exists.

## Steps

1. `commands/` top-level leaf files → `commands/tests.rs`
   Add `#[cfg(test)] mod tests;` in `commands/mod.rs`.
2. `commands/federation/` → create `commands/federation/tests.rs`.
3. `commands/gateway/` → create `commands/gateway/tests.rs`.
4. For subdirectories that already have `tests.rs`: check for and merge
   any residual inline blocks.

## Commit

```
refactor(cli): extract commands/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-cli --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-cli --lib
```
