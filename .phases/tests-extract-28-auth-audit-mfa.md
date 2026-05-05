---
title: Test Extraction — fraiseql-auth audit/, MFA, session
status: planned
---

# Phase 28: `fraiseql-auth` — `audit/`, MFA (`totp_mfa.rs`), session files

## Objective

Extract inline tests from the security and session subsystems of `fraiseql-auth`.

## Files

### audit/ (2 files)

| File |
|------|
| `audit/chain.rs` |
| `audit/logger.rs` |

→ `audit/tests.rs`

### MFA and session leaf files

| File | Notes |
|------|-------|
| `totp_mfa.rs` | TOTP MFA implementation (~499 test lines) |
| `session.rs` | Session management |
| `session_postgres.rs` | PostgreSQL session backend |
| `state_store.rs` | OAuth state storage |
| `state_encryption.rs` | State encryption |
| `pkce.rs` | PKCE implementation |
| `security_config.rs` | Security configuration |
| `security_init.rs` | Security initialization |

These are all top-level leaf files. Consolidate into `src/tests.rs` or group
logically:
- Session-related (`session.rs`, `session_postgres.rs`, `state_store.rs`,
  `state_encryption.rs`) → one section in `tests.rs`
- Auth flow (`pkce.rs`, `totp_mfa.rs`) → another section

## Steps

1. Create `audit/tests.rs`; add declaration in `audit/mod.rs`.
2. Create `src/tests.rs` for all top-level leaf files (or append to existing
   if one was created in phase 27).
3. In `lib.rs` (or equivalent root), add `#[cfg(test)] mod tests;`.

## Commit

```
refactor(auth): extract audit/, MFA, session inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-auth --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-auth --lib
```
