---
title: Test Extraction — fraiseql-server tenancy/, config/, server/, top-level
status: planned
---

# Phase 22: `fraiseql-server` — `tenancy/`, `config/`, `server/`, top-level leaf files

## Objective

Extract inline tests from the remaining `fraiseql-server` areas and complete
the crate. After this phase, `fraiseql-server` has zero inline test blocks
(outside `tests/` integration test directory).

## Files

### tenancy/ (3 files)

| File |
|------|
| `tenancy/audit.rs` |
| `tenancy/pool_factory.rs` |
| `tenancy/schema_isolation.rs` |

→ `tenancy/tests.rs`

### config/ residual (4 files)

| File | Notes |
|------|-------|
| `config/env.rs` | |
| `config/error_sanitization.rs` | |
| `config/pool_tuning.rs` | |
| `config/validation.rs` | |

> `config/tests.rs` already exists — merge residual inline blocks into it.

### server/ (1 file)

| File |
|------|
| `server/initialization.rs` |

→ `server/tests.rs`

### server_config/ residual (1 file)

| File |
|------|
| `server_config/observers.rs` |

> `server_config/tests.rs` already exists — merge residual inline blocks into it.

### schema/ (1 file)

| File |
|------|
| `schema/loader.rs` |

→ `schema/tests.rs`

### usage/ (2 files)

| File |
|------|
| `usage/aggregator.rs` |
| `usage/layer.rs` |

→ `usage/tests.rs`

### Top-level leaf files (7 files)

| File |
|------|
| `api_key.rs` |
| `arrow/database_adapter.rs` |
| `cli.rs` |
| `error.rs` |
| `extractors.rs` |
| `logging.rs` |
| `metrics_server.rs` |
| `tls.rs` |
| `token_revocation.rs` |
| `tracing_utils.rs` |
| `trusted_documents.rs` |

Leaf files that don't belong to a subdirectory consolidate into `src/tests.rs`.
Files under named subdirectories (`arrow/`, `api/rbac_management/`) use
their own `tests.rs`.

## Steps

For `api/rbac_management/db_backend.rs`:
> `api/rbac_management/tests.rs` already exists — merge into it.

## Commit

```
refactor(server): extract tenancy/, config/, remaining inline tests to tests.rs — server complete
```

## Verification

```bash
cargo clippy -p fraiseql-server --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-server --lib
# Zero violations check:
grep -rn "^mod tests {" crates/fraiseql-server/src/ --include="*.rs" | grep -v "/tests\.rs:" && echo FAIL || echo PASS
```
