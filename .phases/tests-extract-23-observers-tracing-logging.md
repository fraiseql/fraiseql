---
title: Test Extraction — fraiseql-observers tracing/, logging/
status: planned
---

# Phase 23: `fraiseql-observers` — `tracing/`, `logging/`

## Objective

Extract inline tests from the `tracing/` and `logging/` subsystems of
`fraiseql-observers`.

## Files

### tracing/ (8 files)

All files are in `src/tracing/`. Exact names to be confirmed by reading the
directory, but expected to include span management, context propagation,
filter configuration, sampler, exporter, and metrics bridge files.

> `tracing/tests.rs` already exists — merge residual inline blocks into it,
> or create if the existing file only covers part of the subsystem.

### logging/ (3 files)

All files are in `src/logging/`. Logging formatters, backends, and context
enrichment.

→ `logging/tests.rs`

## Steps

1. For files in `tracing/` with inline blocks: check if they're covered by the
   existing `tracing/tests.rs`. Add missing test content and remove inline blocks.
2. For `logging/` files: create `logging/tests.rs`, add declaration in
   `logging/mod.rs`, remove inline blocks.

## Commit

```
refactor(observers): extract tracing/, logging/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-observers --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-observers --lib
```
