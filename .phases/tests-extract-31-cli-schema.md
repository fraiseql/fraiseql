---
title: Test Extraction — fraiseql-cli schema/
status: planned
---

# Phase 31: `fraiseql-cli` — `schema/`

## Objective

Extract inline tests from `fraiseql-cli`'s schema processing subsystem.

## Files (9 files)

### schema/ top-level (7 files)

| File | Notes |
|------|-------|
| `schema/mod.rs` | Schema module root |
| `schema/loader.rs` | Schema file loading |
| `schema/merger.rs` | Schema merging |
| `schema/resolver.rs` | Type resolution |
| `schema/normalizer.rs` | Schema normalization |
| `schema/diff.rs` | Schema diffing |
| `schema/fingerprint.rs` | Schema fingerprinting |

→ `schema/tests.rs`

### schema/converter/ residual (2 files)

> `schema/converter/tests.rs` already exists — merge any residual inline
> blocks from `schema/converter/mod.rs` or leaf files.

### schema/validator/ residual (2 files)

> `schema/validator/tests.rs` already exists — merge residual blocks.

### schema/intermediate/ (1 file)

| File |
|------|
| `schema/intermediate/mod.rs` |

→ `schema/intermediate/tests.rs`

## Steps

1. Create `schema/tests.rs` for top-level leaf files.
   Add `#[cfg(test)] mod tests;` in `schema/mod.rs`.
2. For `schema/converter/` and `schema/validator/`: merge residual blocks
   into their existing `tests.rs` files.
3. Create `schema/intermediate/tests.rs`; add declaration.

## Commit

```
refactor(cli): extract schema/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-cli --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-cli --lib
```
