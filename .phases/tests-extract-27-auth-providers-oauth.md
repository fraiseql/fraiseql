---
title: Test Extraction — fraiseql-auth providers/, oauth/
status: planned
---

# Phase 27: `fraiseql-auth` — `providers/`, `oauth/`

## Objective

Extract inline tests from the two largest subsystems of `fraiseql-auth`.

## Files

### providers/ (9 files)

Provider implementations for various identity backends (Google, GitHub,
Apple, Microsoft, Facebook, Custom, etc.).

→ `providers/tests.rs`

> `oauth/tests.rs` already exists — this covers `oauth/` residual inline
> blocks only.

### oauth/ residual (8 files)

Files in `oauth/` that still have inline blocks despite `tests.rs` existing.
Merge their inline blocks into the existing `oauth/tests.rs`.

Files expected in `oauth/` with residual blocks:
- `oauth/mod.rs` — check for residual blocks
- `oauth/pkce.rs`, `oauth/state.rs`, `oauth/token.rs`, etc.

## Steps

1. `providers/` leaf files → create `providers/tests.rs`.
   Add `#[cfg(test)] mod tests;` in `providers/mod.rs`.
2. For each `oauth/*.rs` file with an inline block: move block into existing
   `oauth/tests.rs` under a clearly labelled section comment.

## Commit

```
refactor(auth): extract providers/, oauth/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-auth --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-auth --lib
```
