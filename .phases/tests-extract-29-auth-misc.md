---
title: Test Extraction — fraiseql-auth remaining leaf files
status: planned
---

# Phase 29: `fraiseql-auth` — remaining leaf files

## Objective

Extract inline tests from the remaining `fraiseql-auth` leaf files and
complete the crate. After this phase, `fraiseql-auth` has zero inline
test blocks.

## Files

Remaining top-level leaf files with inline blocks:

| File |
|------|
| `account_linking.rs` |
| `anonymous.rs` |
| `constant_time.rs` |
| `error_sanitizer.rs` |
| `handlers.rs` |
| `jwks.rs` |
| `jwt.rs` |
| `middleware.rs` |
| `monitoring.rs` |
| `multi_provider.rs` |
| `oidc_provider.rs` |
| `oidc_server_client.rs` |
| `operation_rbac.rs` |
| `otp.rs` |
| `phone_otp.rs` |
| `provider.rs` |
| `proxy.rs` |
| `rate_limiting.rs` |

> `rate_limiting.rs` alone has ~670 test lines — the largest file in the crate.

## Steps

1. Append to `src/tests.rs` created in phase 28, or split into thematic
   sections if the file becomes unwieldy (>600 lines total).
2. Group by concern when writing tests.rs:
   - Identity ops: `account_linking`, `anonymous`, `provider`, `multi_provider`
   - Token ops: `jwt`, `jwks`, `constant_time`, `oidc_provider`, `oidc_server_client`
   - Auth flows: `handlers`, `middleware`, `otp`, `phone_otp`, `proxy`
   - Security: `rate_limiting`, `operation_rbac`, `error_sanitizer`, `monitoring`

## Commit

```
refactor(auth): extract remaining leaf inline tests to tests.rs — auth complete
```

## Verification

```bash
cargo clippy -p fraiseql-auth --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-auth --lib
# Zero violations check:
grep -rn "^mod tests {" crates/fraiseql-auth/src/ --include="*.rs" | grep -v "/tests\.rs:" && echo FAIL || echo PASS
```
