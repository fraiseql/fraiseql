# Phase 10: Finalize v2.3.0

## Objective
Ship v2.3.0. Version bump, changelog, final verification, and release tag.

## Success Criteria
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo test --workspace` passes (excluding infra-gated tests)
- [ ] `cargo build --release` succeeds
- [ ] `cargo deny check` clean
- [ ] Version bumped to `2.3.0` in all `Cargo.toml` files
- [ ] `CHANGELOG.md` documents all v2.3.0 additions
- [ ] `roadmap.md` updated to mark v2.3.0 released
- [ ] `git grep -i "TODO\|FIXME\|HACK"` returns nothing unexpected
- [ ] Release tag `v2.3.0` created

## Steps

### 1. Final Verification
- [ ] All tests pass
- [ ] All lints pass
- [ ] Release build succeeds
- [ ] `cargo semver-checks` passes (no accidental breaking changes vs v2.2.0)

### 2. Version Bump
- [ ] `crates/fraiseql-*/Cargo.toml` — bump version to `2.3.0`
- [ ] Root `Cargo.toml` workspace version (if set)
- [ ] Inter-crate dependency version pins updated
- [ ] `cargo check` after bump to confirm consistency

### 3. Changelog
- [ ] Add `## [2.3.0] - YYYY-MM-DD` section to `CHANGELOG.md`
- [ ] Document: wasmtime upgrade (16 CVEs eliminated)
- [ ] Document: usage aggregation Redis persistence backend
- [ ] Document: `GET /auth/me` auth gating option (`metadata_require_auth`, etc.)
- [ ] Document: `subscription_require_auth` / `playground_require_auth` / `schema_export_require_auth`
- [ ] Document: 22 new integration tests
- [ ] Document: hot-reload cache rebind fix (TODO #184)
- [ ] Document: Studio metrics endpoint wired to live collectors

### 4. Roadmap Update
- [ ] Mark v2.3.0 as released in `roadmap.md`
- [ ] Update "Current Stable" and "In Development" lines
- [ ] Add v2.4.0 placeholder section

### 5. Release
Follow `releasing.md` process.

## Dependencies
- Requires: All phases 06–09 complete
- Blocks: nothing (this is the release)

## Status
[ ] Not Started
