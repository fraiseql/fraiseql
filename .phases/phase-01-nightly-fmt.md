# Phase 01: Fix Nightly rustfmt

## Status
[~] In Progress — Cycle 1 done, Cycle 2 remaining

## What Was Already Done (Cycle 1 — archived)
- `crates/fraiseql-server/Cargo.toml`: `[package]` moved to first section ✅
- `[[test]]` entry for `secrets_manager_integration_test` added ✅
- `cargo check --workspace` passes ✅

## Objective
Fix the nightly rustfmt drift so Format Check CI passes.

**Current state**: 596 source files differ from nightly fmt output (confirmed 2026-03-14).

---

## Cycle 2 — Fix nightly rustfmt drift

### Problem
`RUSTUP_TOOLCHAIN=nightly cargo fmt --all --check` reports diffs in ~596 files.
Nightly rustfmt applies two transformations that stable does not:

**A — Trailing `// Reason:` comment relocation on `#[allow]`**
```rust
// Before (manually placed inline):
#[allow(clippy::cast_possible_truncation)] // Reason: safe because …

// After nightly fmt:
#[allow(clippy::cast_possible_truncation)]
// Reason: safe because …
```

**B — Empty doc comment line stripping**
```rust
// Before:
//!
//! This example shows …

// After nightly:
//! This example shows …
```

### Fix
```bash
RUSTUP_TOOLCHAIN=nightly cargo fmt --all
```

Then audit that no `// Reason:` comments were lost (nightly moves them to the
next line but should not drop them). Check a sample of `#[allow]` diffs:

```bash
git diff --unified=3 | grep -A2 '#\[allow'
```

### Post-fmt smoke test
```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### Verification
```bash
RUSTUP_TOOLCHAIN=nightly cargo fmt --all --check   # must exit 0
cargo check --workspace                             # must compile
```

---

## Success Criteria
- [ ] `RUSTUP_TOOLCHAIN=nightly cargo fmt --all --check` exits 0
- [ ] `cargo check --workspace` passes after fmt
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` still exits 0
- [ ] All `// Reason:` comments on `#[allow]` attributes preserved (spot-check 10)

## Blocks
Phase 03, Phase 04 (CI jobs cascade from Format Check failure)

## Estimated Effort
15–30 minutes (the fmt run itself; audit is the slow part)
