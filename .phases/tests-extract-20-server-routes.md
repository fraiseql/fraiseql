---
title: Test Extraction — fraiseql-server routes/ (non-REST)
status: planned
---

# Phase 20: `fraiseql-server` — `routes/` (non-REST)

## Objective

Extract inline tests from `fraiseql-server`'s route handlers outside of
`routes/rest/` (which is already complete).

## Files (21 files)

### routes/api/ (9 files)

| File |
|------|
| `routes/api/admin.rs` |
| `routes/api/design.rs` |
| `routes/api/federation.rs` |
| `routes/api/metadata.rs` |
| `routes/api/openapi.rs` |
| `routes/api/query.rs` |
| `routes/api/schema.rs` |
| `routes/api/tenant_admin.rs` |
| `routes/api/usage.rs` |

### routes/studio/ (8 files)

| File |
|------|
| `routes/studio/admin.rs` |
| `routes/studio/auth_users.rs` |
| `routes/studio/data.rs` |
| `routes/studio/function_ops.rs` |
| `routes/studio/metrics_summary.rs` |
| `routes/studio/mod.rs` |
| `routes/studio/realtime_monitor.rs` |
| `routes/studio/storage_browser.rs` |

### routes/graphql/ residual (where inline blocks remain)

| File |
|------|
| `routes/graphql/app_state.rs` |
| `routes/graphql/tenant_key.rs` |
| `routes/graphql/tenant_registry.rs` |

> `routes/graphql/tests.rs` already exists — merge residual inline blocks into it.

### routes/ top-level

| File |
|------|
| `routes/auth.rs` |
| `routes/health.rs` |
| `routes/introspection.rs` |
| `routes/metrics.rs` |
| `routes/playground.rs` |
| `routes/realtime.rs` |
| `routes/subscriptions.rs` |
| `routes/storage/mod.rs` |
| `routes/functions/mod.rs` |
| `routes/grpc/handler.rs` |

## Steps

1. `routes/api/` leaf files → `routes/api/tests.rs`
2. `routes/studio/` leaf files → `routes/studio/tests.rs`
   In `routes/studio/mod.rs`: add `#[cfg(test)] mod tests;`
3. `routes/graphql/` residual → merge into existing `routes/graphql/tests.rs`
4. `routes/` top-level leaf files → `routes/tests.rs`
   In `routes/mod.rs` (if it exists) or the appropriate parent: add declaration.
5. `routes/storage/`, `routes/functions/`, `routes/grpc/` leaf files →
   respective `tests.rs` siblings.

## Commit

```
refactor(server): extract routes/ (non-REST) inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-server --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-server --lib
```
