---
title: Test Extraction — fraiseql-core compiler/
status: planned
---

# Phase 11: `fraiseql-core` — `compiler/`

## Objective

Extract inline tests from the `compiler/` subsystem of `fraiseql-core`.

## Files

| File | Notes |
|------|-------|
| `compiler/mod.rs` | Compiler entry point |
| `compiler/parser.rs` | GraphQL parser (~528 test lines) |
| `compiler/validator.rs` | Schema validator (~655 test lines) |
| `compiler/linker.rs` | Type linker |
| `compiler/sql_generator.rs` | SQL template generator |
| `compiler/rest_generator.rs` | REST route generator |
| `compiler/optimizer.rs` | Query optimizer |
| `compiler/security_compiler.rs` | Security configuration compiler |

> `compiler/fact_table/` and `compiler/window_functions/` already have
> `tests.rs` files — skip them.

## Steps

For each file with an inline `mod tests { … }` block:

1. Create `compiler/<name>/tests.rs` (if the file is a directory root `mod.rs`)
   or `compiler/tests.rs` (if cleaning multiple leaf files into one).
2. Add `#[cfg(test)] mod tests;` in the source file.
3. Remove the inline block.

**Import pattern for leaf files grouped into one `tests.rs`:**
```rust
use super::parser::{…};
use super::validator::{…};
// etc.
```

## Visibility watch-list

Check each tested function's visibility. Functions only called from tests may
need promotion from `fn` → `pub(super)` after extraction. Run clippy to catch
unused-private-item warnings.

## Commit

```
refactor(core): extract compiler/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
```
