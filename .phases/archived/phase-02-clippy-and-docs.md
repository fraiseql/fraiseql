# Phase 02: Fix Clippy Lints & Documentation CI

## Objective
Fix Clippy, `cargo doc`, `cargo audit`, Feature Flag Coverage, Code Coverage,
and Cross-DB Parity CI jobs. Requires Phase 01 (clean fmt + Cargo.toml).

## CI Jobs Fixed
- Clippy Lints
- Documentation
- Feature Flag Coverage (cascade from build)
- Security Audit (cargo audit + cargo deny)
- Code Coverage (cascade from build)
- Cross-DB Parity (cascade from build/test)

---

## Cycle 1 — Fix Clippy lints

### Problem
After Phase 01's `cargo +nightly fmt --all`, some `#[allow(clippy::X)]`
attributes lose their trailing `// Reason:` comment. The project enforces
`clippy::allow_attributes_without_reason` via `-D warnings`, so stripped
comments become lint errors.

### Fix

```bash
# Run clippy and collect all failures
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/clippy-all.txt
cargo clippy --workspace --all-targets --no-default-features -- -D warnings 2>&1 | tee /tmp/clippy-none.txt
```

Common patterns to fix:
1. Re-add stripped `// Reason:` comments to `#[allow]` attributes
2. Address any new lint warnings with code fix or justified `#[allow]`

**Rule**: Every `#[allow(clippy::X)]` must have `// Reason: <justification>` on
the same line. No exceptions.

### Verification
```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
```

---

## Cycle 2 — Fix `cargo doc`

### Problem
`cargo doc --workspace --all-features --no-deps` fails. Likely causes:
1. Broken intra-doc links from the `chore/lowercase-markdown-filenames` rename
2. Rustdoc errors in recently added code

### Fix

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps 2>&1 \
  | grep "^error" | head -30
```

For broken links: update `[text](path/to/file.md)` references to match
renamed file paths. For missing `# Errors` sections: add them to
`Result`-returning public functions.

### Verification
```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

---

## Cycle 3 — Fix `cargo audit` / `cargo deny`

### Problem
`security.yml` runs both `cargo audit` (with 5 RUSTSEC ignores) and `cargo deny check`.

### Fix

```bash
# Check current state
cargo audit \
  --ignore RUSTSEC-2023-0071 \
  --ignore RUSTSEC-2024-0384 \
  --ignore RUSTSEC-2024-0436 \
  --ignore RUSTSEC-2025-0134 \
  --ignore RUSTSEC-2026-0002

cargo deny check
```

Options (in order of preference):
1. **Update the crate**: `cargo update <vulnerable-crate>`
2. **If not exploitable**: add to the existing `--ignore` list in `security.yml`
   with a comment explaining why. All exemptions MUST have an expiry date in a
   tracking issue.

### Verification
```bash
cargo audit --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2024-0384 \
  --ignore RUSTSEC-2024-0436 --ignore RUSTSEC-2025-0134 --ignore RUSTSEC-2026-0002
cargo deny check
```

---

## Cycle 4 — Verify Feature Flag Coverage

### Problem
The `feature-flags.yml` workflow runs `cargo check -p fraiseql-server` with
7 feature combinations in a matrix. If the build is broken, all 7 fail.

### Mechanism
The workflow (`feature-flags.yml`) defines this matrix:
```yaml
features:
  - ""                                          # --no-default-features
  - "auth,secrets"
  - "observers,redis-rate-limiting"
  - "observers-nats-enterprise,redis-rate-limiting,tracing-opentelemetry"
  - "arrow,wire-backend"
  - "mcp,auth,cors"
  - "observers-enterprise,redis-apq,redis-pkce"
```

### Fix
After Phase 01 fixes the Cargo.toml, run each combination locally:
```bash
cargo check -p fraiseql-server --no-default-features
cargo check -p fraiseql-server --features "auth,secrets"
cargo check -p fraiseql-server --features "observers,redis-rate-limiting"
cargo check -p fraiseql-server --features "observers-nats-enterprise,redis-rate-limiting,tracing-opentelemetry"
cargo check -p fraiseql-server --features "arrow,wire-backend"
cargo check -p fraiseql-server --features "mcp,auth,cors"
cargo check -p fraiseql-server --features "observers-enterprise,redis-apq,redis-pkce"
```

Fix any compile errors specific to individual feature combinations (conditional
compilation issues, missing `cfg` gates, etc.).

### Verification
All 7 combinations must `cargo check` cleanly.

---

## Cascade jobs

The following jobs have no independent fix — they pass once the build compiles:
- **Code Coverage**: runs `cargo test` then `llvm-cov` — blocked by build failure
- **Cross-DB Parity**: runs snapshot tests — blocked by build failure

Once Cycles 1–4 are green, verify these pass:
```bash
cargo test --test sql_snapshots  # Cross-DB Parity
```

---

## Success Criteria
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits 0
- [ ] `cargo clippy --workspace --all-targets --no-default-features -- -D warnings` exits 0
- [ ] All `#[allow]` attributes have `// Reason:` comments
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps` exits 0
- [ ] `cargo audit` + `cargo deny` exit 0
- [ ] All 7 feature flag matrix combinations check cleanly
- [ ] `cargo test --test sql_snapshots` passes

## Estimated Effort: 2–4 hours
