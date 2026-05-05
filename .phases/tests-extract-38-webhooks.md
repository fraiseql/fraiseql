---
title: Test Extraction — fraiseql-webhooks
status: planned
---

# Phase 38: `fraiseql-webhooks`

## Objective

Extract inline tests from all 14 files in `fraiseql-webhooks`.

## Files

### signature/ (12 files)

One file per webhook provider:

| File |
|------|
| `signature/mod.rs` |
| `signature/generic.rs` |
| `signature/github.rs` |
| `signature/gitlab.rs` |
| `signature/stripe.rs` |
| `signature/shopify.rs` |
| `signature/paddle.rs` |
| `signature/twilio.rs` |
| `signature/sendgrid.rs` |
| `signature/discord.rs` |
| `signature/slack.rs` |
| `signature/registry.rs` |

→ `signature/tests.rs`

All provider implementations follow the same trait pattern. Tests likely cover
HMAC verification, timestamp validation, and signature format parsing.

### Top-level leaf files (2 files)

| File |
|------|
| `config.rs` |
| `transaction.rs` |

→ `src/tests.rs` with declaration in `lib.rs`.

## Steps

1. Create `signature/tests.rs` consolidating all 12 signature files' test blocks.
   Import pattern:
   ```rust
   use super::github::…;
   use super::stripe::…;
   // etc.
   ```
2. Add `#[cfg(test)] mod tests;` in `signature/mod.rs`.
3. Create `src/tests.rs`; add declaration in `lib.rs`.

## Commit

```
refactor(webhooks): extract inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-webhooks --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-webhooks --lib
```
