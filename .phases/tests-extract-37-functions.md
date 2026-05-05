---
title: Test Extraction — fraiseql-functions
status: planned
---

# Phase 37: `fraiseql-functions`

## Objective

Extract inline tests from the 16 remaining files in `fraiseql-functions`.

## Files

### runtime/wasm/ (4 files)

| File |
|------|
| `runtime/wasm/mod.rs` |
| `runtime/wasm/host.rs` |
| `runtime/wasm/instance.rs` |
| `runtime/wasm/memory.rs` (or similar) |

→ `runtime/wasm/tests.rs`

### triggers/ (3 files)

> `triggers/tests.rs` already exists — merge any residual inline blocks.

### host/ (2 files)

| File |
|------|
| `host/mod.rs` |
| `host/bridge.rs` (or similar) |

> `host/live/tests.rs` already exists. Check `host/mod.rs` for residual inline
> blocks and merge into an appropriate `tests.rs`.

### host/live/ residual

> `host/live/tests.rs` already exists — merge residual blocks only.

### runtime/deno/ residual

> `runtime/deno/tests.rs` already exists — merge residual blocks only.

### runtime/ top-level (1 file)

| File |
|------|
| `runtime/mod.rs` |

→ `runtime/tests.rs`

### store/ (1 file)

| File |
|------|
| `store/mod.rs` |

→ `store/tests.rs`

### Top-level leaf files

| File |
|------|
| `lib.rs` |
| `secrets.rs` |

→ `src/tests.rs` with declaration in `lib.rs`.

## Commit

```
refactor(functions): extract inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-functions --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-functions --lib
```
