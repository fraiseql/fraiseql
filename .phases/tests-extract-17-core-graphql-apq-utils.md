---
title: Test Extraction — fraiseql-core graphql/, apq/, utils/
status: planned
---

# Phase 17: `fraiseql-core` — `graphql/`, `apq/`, `utils/`

## Objective

Extract inline tests from three smaller subsystems of `fraiseql-core`.

## Files

### graphql/ (6 files + 1 subdirectory)

| File | Notes |
|------|-------|
| `graphql/mod.rs` | GraphQL layer root |
| `graphql/parser.rs` | Document parser |
| `graphql/executor.rs` | Execution |
| `graphql/normalizer.rs` | Query normalizer |
| `graphql/variables.rs` | Variable extraction |
| `graphql/errors.rs` | Error mapping |
| `graphql/directive_evaluator/mod.rs` | Directive evaluation (1 file) |

### apq/ (5 files)

| File | Notes |
|------|-------|
| `apq/mod.rs` | APQ module root |
| `apq/hash.rs` | Hash computation |
| `apq/store.rs` | APQ storage |
| `apq/middleware.rs` | APQ middleware |
| `apq/metrics.rs` | APQ metrics |

### utils/ (4 files)

| File | Notes |
|------|-------|
| `utils/mod.rs` | Utility root |
| `utils/json.rs` | JSON helpers |
| `utils/string.rs` | String utilities |
| `utils/time.rs` | Time utilities |

## Steps

- `graphql/` leaf files → `graphql/tests.rs`
- `graphql/directive_evaluator/` → `graphql/directive_evaluator/tests.rs`
- `apq/` leaf files → `apq/tests.rs`
- `utils/` leaf files → `utils/tests.rs`

## Commit

```
refactor(core): extract graphql/, apq/, utils/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
```
