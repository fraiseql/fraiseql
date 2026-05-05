---
title: Test Extraction — fraiseql-cli codegen/, config/, top-level leaf files
status: planned
---

# Phase 32: `fraiseql-cli` — `codegen/`, `config/`, top-level leaf files

## Objective

Extract inline tests from the remaining `fraiseql-cli` areas and complete the
crate. After this phase, `fraiseql-cli` has zero inline test blocks.

## Files

### codegen/ (2 files)

| File |
|------|
| `codegen/mod.rs` |
| `codegen/typescript.rs` (or similar) |

→ `codegen/tests.rs`

### config/ (3 files)

| File |
|------|
| `config/mod.rs` |
| `config/validation.rs` |
| `config/defaults.rs` (or similar) |

→ `config/tests.rs`

### config/toml_schema/ (1 file)

| File |
|------|
| `config/toml_schema/mod.rs` |

→ `config/toml_schema/tests.rs`

### output/ (1 file)

| File |
|------|
| `output/mod.rs` |

→ `output/tests.rs`

### Top-level leaf files

| File |
|------|
| `introspection.rs` |
| `output_schemas.rs` |
| `runner.rs` |

→ `src/tests.rs` (or append to existing from phase 30/31 if one was created)

## Steps

1. For each subdirectory: create `tests.rs`, add `#[cfg(test)] mod tests;`
   in the module root.
2. Top-level leaf files → `src/tests.rs`. Add declaration in `lib.rs` or
   `main.rs` as appropriate.

## Commit

```
refactor(cli): extract codegen/, config/ inline tests to tests.rs — cli complete
```

## Verification

```bash
cargo clippy -p fraiseql-cli --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-cli --lib
# Zero violations check:
grep -rn "^mod tests {" crates/fraiseql-cli/src/ --include="*.rs" | grep -v "/tests\.rs:" && echo FAIL || echo PASS
```
