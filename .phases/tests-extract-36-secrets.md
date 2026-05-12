---
title: Test Extraction — fraiseql-secrets
status: planned
---

# Phase 36: `fraiseql-secrets`

## Objective

Extract inline tests from all 20 files in `fraiseql-secrets`.

## Files by subsystem

### encryption/ (15 files)

The largest subsystem in the crate. Likely includes:

- Key derivation, symmetric encryption, asymmetric encryption
- Key wrapping, envelope encryption
- Rotation, versioning
- Backends: software, HSM, KMS

→ `encryption/tests.rs`

### secrets_manager/ (2 files)

| File |
|------|
| `secrets_manager/mod.rs` |
| `secrets_manager/cache.rs` (or similar) |

→ `secrets_manager/tests.rs`

### secrets_manager/backends/ (2 files)

| File |
|------|
| `secrets_manager/backends/mod.rs` |
| `secrets_manager/backends/env.rs` (or similar) |

→ `secrets_manager/backends/tests.rs`

### secrets_manager/backends/vault/ residual

> `secrets_manager/backends/vault/tests.rs` already exists — merge residual
> blocks from vault leaf files into it.

## Steps

1. Confirm directory structure by reading `crates/fraiseql-secrets/src/`.
2. For `encryption/` (the large group): create `encryption/tests.rs`.
   `encryption/` is likely a directory with a `mod.rs`; add declaration there.
3. For remaining subdirectories: standard pattern.

## Commit

```
refactor(secrets): extract inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-secrets --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-secrets --lib
```
