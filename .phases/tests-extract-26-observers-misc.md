---
title: Test Extraction — fraiseql-observers top-level leaf files
status: planned
---

# Phase 26: `fraiseql-observers` — top-level leaf files

## Objective

Extract inline tests from the remaining `fraiseql-observers` leaf files that
don't belong to a named subsystem. After this phase, `fraiseql-observers` has
zero inline test blocks.

## Files

Top-level files with inline test blocks (each is a direct child of `src/`):

| File |
|------|
| `actions.rs` |
| `actions_additional.rs` |
| `arrow_bridge.rs` |
| `cached_executor.rs` |
| `deduped_executor.rs` |
| `elasticsearch_sink.rs` |
| `error.rs` |
| `event.rs` |
| `factory.rs` |
| `matcher.rs` |
| `queued_executor.rs` |
| `storage.rs` |
| `traits.rs` |

> `config/tests.rs`, `condition/tests.rs`, `cli/tests.rs` already exist —
> check for any residual inline blocks in those `mod.rs` files and merge.

## Steps

Consolidate all top-level leaf files into `src/tests.rs` (a new file at the
crate's `src/` root). In `lib.rs`, add:
```rust
#[cfg(test)]
mod tests;
```

Alternatively, if `lib.rs` is too crowded, group into a `top_level/tests.rs`.
Prefer the former — a single flat `tests.rs` at the crate root.

## Commit

```
refactor(observers): extract top-level inline tests to tests.rs — observers complete
```

## Verification

```bash
cargo clippy -p fraiseql-observers --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-observers --lib
# Zero violations check:
grep -rn "^mod tests {" crates/fraiseql-observers/src/ --include="*.rs" | grep -v "/tests\.rs:" && echo FAIL || echo PASS
```
