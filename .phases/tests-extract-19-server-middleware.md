---
title: Test Extraction — fraiseql-server middleware/
status: planned
---

# Phase 19: `fraiseql-server` — `middleware/`

## Objective

Extract inline tests from `fraiseql-server`'s middleware layer.

## Files (16 files)

| File | Notes |
|------|-------|
| `middleware/auth.rs` | Auth middleware |
| `middleware/admin_scope.rs` | Admin scope check |
| `middleware/content_type.rs` | Content-type validation |
| `middleware/cors.rs` | CORS handling |
| `middleware/error_sanitization.rs` | Error sanitization middleware |
| `middleware/header_limits.rs` | Header size limits |
| `middleware/hs256_auth.rs` | HS256 token auth |
| `middleware/metrics.rs` | Metrics middleware |
| `middleware/oidc_auth.rs` | OIDC auth middleware |
| `middleware/tenant.rs` | Tenant isolation middleware |
| `middleware/trace.rs` | Tracing middleware |
| `middleware/rate_limit/dispatch.rs` | Rate limit dispatch |
| `middleware/rate_limit/key.rs` | Rate limit key derivation |
| `middleware/rate_limit/middleware_fn.rs` | Rate limit middleware fn |
| `middleware/rate_limit/mod.rs` | Rate limit module root |
| `middleware/rate_limit/token_bucket.rs` | Token bucket algorithm |

## Steps

1. `middleware/` leaf files → `middleware/tests.rs`
   (imports via `use super::auth::…`, `use super::cors::…`, etc.)

2. `middleware/rate_limit/` leaf files → `middleware/rate_limit/tests.rs`
   (imports via `use super::dispatch::…`, `use super::key::…`, etc.)

3. In `middleware/mod.rs`: add `#[cfg(test)] mod tests;`
4. In `middleware/rate_limit/mod.rs`: add `#[cfg(test)] mod tests;`

## Visibility watch-list

`middleware/` functions are typically `pub` or `pub(crate)`. Rate-limit
internals (`token_bucket.rs`) may have private helpers — promote to
`pub(super)` as needed.

## Commit

```
refactor(server): extract middleware/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-server --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-server --lib
```
