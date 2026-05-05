---
title: Test Extraction — fraiseql-core validation/
status: planned
---

# Phase 16: `fraiseql-core` — `validation/`

## Objective

Extract inline tests from the `validation/` subsystem of `fraiseql-core`.
This is the largest single subsystem by file count (22 files).

## Files (22 files)

All files are in `validation/` or its subdirectories:

| Pattern | Count | Notes |
|---------|-------|-------|
| `validation/mod.rs` | 1 | Validation root |
| `validation/*.rs` leaf files | ~18 | Validators for individual concerns |
| `validation/custom_type_registry/` | 1+ | Already has tests.rs ✅ |

Common validators likely include:
- `query_validator.rs`, `mutation_validator.rs`
- `type_validator.rs`, `field_validator.rs`
- `argument_validator.rs`, `directive_validator.rs`
- `auth_validator.rs`, `rate_limit_validator.rs`
- `rest_validator.rs`, `subscription_validator.rs`

## Steps

1. Check `validation/custom_type_registry/tests.rs` for residual inline
   blocks — merge if any.
2. For the remaining 20+ leaf files: consolidate into `validation/tests.rs`,
   importing each tested item via `use super::<file>::…`.
3. For any subdirectory with a `mod.rs`: create its own `tests.rs`.

> Given the size (22 files), it may be cleaner to create one `tests.rs` per
> logical group (e.g., `validation/query/tests.rs`, `validation/type/tests.rs`)
> if subdirectory structure allows. Defer to what the actual file tree shows.

## Commit

```
refactor(core): extract validation/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
```
