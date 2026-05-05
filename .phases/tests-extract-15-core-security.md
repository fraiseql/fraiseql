---
title: Test Extraction — fraiseql-core security/
status: planned
---

# Phase 15: `fraiseql-core` — `security/`

## Objective

Extract inline tests from the `security/` subsystem of `fraiseql-core`.

## Files (15 files)

| File | Notes |
|------|-------|
| `security/mod.rs` | Security module root |
| `security/rate_limiter.rs` | Rate limiting |
| `security/jwt_validator.rs` | JWT validation |
| `security/error_sanitizer.rs` | Error sanitization |
| `security/audit_logger.rs` | Audit logging |
| `security/field_encryption.rs` | Field-level encryption |
| `security/constant_time.rs` | Timing-safe ops |
| `security/auth_middleware/mod.rs` | Auth middleware (1 file) |
| `security/kms/` | KMS backends (3 files) |
| `security/oidc/mod.rs` | OIDC integration |
| `security/oidc/jwks.rs` | JWKS fetching |
| `security/oidc/validator.rs` | Token validation |

> `security/oidc/tests.rs` already exists — skip that subdirectory.

## Steps

- Leaf files in `security/` → consolidate into `security/tests.rs`
- `security/auth_middleware/` → `security/auth_middleware/tests.rs`
- `security/kms/` → `security/kms/tests.rs`
- `security/oidc/mod.rs` residual inline blocks → merge into existing
  `security/oidc/tests.rs`

## Commit

```
refactor(core): extract security/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
```
