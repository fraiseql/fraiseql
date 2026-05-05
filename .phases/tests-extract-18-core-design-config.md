---
title: Test Extraction — fraiseql-core design/, config/, remaining
status: planned
---

# Phase 18: `fraiseql-core` — `design/`, `config/`, remaining leaf files

## Objective

Extract inline tests from the remaining `fraiseql-core` subsystems and
complete the crate. After this phase, `fraiseql-core` has zero inline
test blocks.

## Files

### design/ (7 files)

The `design/` module contains pattern implementations (Builder, Factory, etc.)
or architectural components (exact names determined by reading the directory).
All 7 files are leaf files → consolidate into `design/tests.rs`.

### config/ (1 file)

| File | Notes |
|------|-------|
| `config/mod.rs` or `config.rs` | Runtime configuration |

→ `config/tests.rs` or `tests.rs` sibling.

### Remaining top-level or miscellaneous

Any files at `src/*.rs` level (e.g., `lib.rs`, `error.rs`, `types.rs`) with
inline test blocks that don't belong to a named subsystem. These consolidate
into a top-level `src/tests.rs`.

## Steps

1. Run `grep -rl "^mod tests {" crates/fraiseql-core/src/ --include="*.rs" | grep -v "/tests\.rs:"` after phases 11–17 to confirm only these files remain.
2. Extract each group as described.
3. Verify zero inline blocks remain in the crate.

## Commit

```
refactor(core): extract design/, config/ inline tests to tests.rs — core complete
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
# Zero violations check:
grep -rn "^mod tests {" crates/fraiseql-core/src/ --include="*.rs" | grep -v "/tests\.rs:" && echo FAIL || echo PASS
```
