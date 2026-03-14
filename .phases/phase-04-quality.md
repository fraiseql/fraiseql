# Phase 04: Quality & Documentation

## Status
[ ] Not Started

## Objective
Reach quality score ≥ 4.5 / 5.0 on `dev`. Fix documentation inconsistencies
surfaced by GitHub issues and verify integration test CI is green.

## Dependencies
- Phase 01 (nightly fmt) — Format Check must pass for CI to be meaningful
- Phases 02–03 — SDK/Docker CI fixes complete

## Already Done (do not redo)
- Clippy: `cargo clippy --workspace --all-targets -- -D warnings` → 0 errors ✅
- `cargo doc --workspace --no-deps` → 0 warnings ✅
- `cargo test --test sql_snapshots -p fraiseql-core` → 92 passed ✅
- REST transport integration tests → committed in `94a52b6dd` / `ba8046eb1` ✅
- Mutation testing gate → 9.1% survival rate (≤30% threshold) ✅

---

## Cycle 1 — Fix Arrow Flight docs contradiction (issue #82)

### Problem
Two documentation pages contradict each other on how to enable Arrow Flight:
- `features/analytics.mdx`: says add `[analytics]\nenabled = true` to `fraiseql.toml`
- `features/arrow-dataplane.mdx`: says Arrow Flight is compile-time only (Cargo feature)

`reference/toml-config.mdx` does not list `[analytics]` as a valid section, confirming
the TOML path is wrong. The correct activation is the Cargo feature.

### Fix
1. Remove the `[analytics]` TOML instruction from `features/analytics.mdx`
2. Add a clear note: "Arrow Flight requires a custom build with `--features arrow`.
   The official Docker image `:full` tag includes it."
3. Add a cross-reference link between the two pages.

### Verification
```bash
grep -r '\[analytics\]' docs/  # should return nothing
```

---

## Cycle 2 — Verify feature flag matrix

### Problem
CI job `feature-flags.yml` checks all feature combinations. Verify it passes
after Phase 01 (fmt) and Phase 02 (SDK) fixes.

### Combinations to check locally
```bash
CARGO="export PATH=... && cargo"
# Default
cargo check -p fraiseql-server
# No defaults
cargo check -p fraiseql-server --no-default-features
# REST only
cargo check -p fraiseql-server --no-default-features --features rest-transport
# grpc + rest
cargo check -p fraiseql-server --features grpc-transport,rest-transport
# arrow
cargo check -p fraiseql-core --features arrow
# auth + secrets
cargo check -p fraiseql-server --features auth,secrets
```

---

## Cycle 3 — Integration test CI pass

### Problem
Integration test CI jobs (postgres, mysql, redis, nats, vault, tls) run against
real services via docker-compose. Verify these are passing on `dev`.

### Verification
Check CI run for the latest `dev` commit. If any job is red:
1. Identify the failure (network timeout vs. real test failure)
2. Fix the root cause (not the symptom)

Local check:
```bash
make db-up   # starts all 6 services
cargo nextest run -p fraiseql-core --features postgres
cargo nextest run -p fraiseql-server --features postgres,auth
```

---

## Success Criteria
- [ ] `features/analytics.mdx` corrected — no `[analytics]` TOML instruction
- [ ] `features/arrow-dataplane.mdx` links to `:full` Docker image for non-source users
- [ ] All feature flag matrix combinations (`cargo check`) pass cleanly
- [ ] Integration test CI jobs green on `dev` (verify in CI, not just locally)
- [ ] Quality score ≥ 4.5 / 5.0 (run quality evaluation script if exists)

## Branch Strategy
Work on a feature branch (e.g. `fix/quality-docs`), merge to `dev` via PR.

## Closes
- Issue #82 (Arrow Flight docs contradiction)
