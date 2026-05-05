---
title: Test Extraction — fraiseql-db
status: planned
---

# Phase 34: `fraiseql-db`

## Objective

Extract inline tests from all 28 files in `fraiseql-db`.

## Files by subsystem

### filters/ (5 files)

| File |
|------|
| `filters/mod.rs` |
| `filters/bracket.rs` |
| `filters/json.rs` |
| `filters/rich.rs` |
| `filters/logical.rs` |

→ `filters/tests.rs`

### Dialect + where generation (5 files)

| File |
|------|
| `dialect/mod.rs` (or similar) |
| `where_clause.rs` |
| `where_generator/mod.rs` |
| `where_sql_generator.rs` |
| `order_by.rs` |

→ `src/tests.rs` (top-level leaf files)

### Database-specific (7 files across mysql/, sqlite/, sqlserver/, postgres/)

| File |
|------|
| `mysql/mod.rs` or leaf file |
| `mysql/adapter.rs` |
| `sqlite/mod.rs` or leaf file |
| `sqlite/adapter.rs` |
| `sqlserver/mod.rs` or leaf file |
| `sqlserver/adapter.rs` |
| `postgres/adapter.rs` (leaf) |

> `postgres/adapter/tests.rs` already exists — merge residual blocks.

Each database subdirectory gets its own `tests.rs`.

### types/ (2 files)

→ `types/tests.rs`

### Remaining top-level leaf files

| File |
|------|
| `collation.rs` |
| `collation_config.rs` |
| `fraiseql_wire_adapter.rs` |
| `identifier.rs` |
| `path_escape.rs` |
| `projection_generator.rs` |
| `traits.rs` |
| `utils.rs` |
| `wire_pool.rs` |

→ `src/tests.rs`

## Commit

```
refactor(db): extract inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-db --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-db --lib
```
