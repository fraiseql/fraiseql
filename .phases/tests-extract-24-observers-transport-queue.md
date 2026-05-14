---
title: Test Extraction — fraiseql-observers transport/, job_queue/, queue/
status: planned
---

# Phase 24: `fraiseql-observers` — `transport/`, `job_queue/`, `queue/`

## Objective

Extract inline tests from the transport and queue subsystems of
`fraiseql-observers`.

## Files

### transport/ (7 files)

The transport layer connects observers to external sinks (webhooks, message
queues, etc.). All 7 files are leaf files under `transport/`.

→ `transport/tests.rs`

### job_queue/ (6 files)

Job queue implementation for deferred observer execution.

→ `job_queue/tests.rs`

### queue/ (3 files)

Lower-level queue primitives.

→ `queue/tests.rs`

## Steps

For each group:

1. Create `tests.rs` consolidating all test blocks from the group's files.
2. Add `#[cfg(test)] mod tests;` in the relevant `mod.rs`.
3. Remove inline blocks.

Import pattern example for `transport/tests.rs`:
```rust
use super::webhook::…;
use super::kafka::…;
// etc.
```

## Commit

```
refactor(observers): extract transport/, job_queue/, queue/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-observers --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-observers --lib
```
