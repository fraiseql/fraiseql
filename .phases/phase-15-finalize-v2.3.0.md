# Phase 15: Finalize v2.3.0

## Objective
Transform the v2.3.0 feature branch into a production-ready release:
archaeology removal, documentation accuracy, roadmap update, release cut.

## Status
[ ] Not Started

## Success Criteria
- [ ] `git grep -i "phase\|todo\|fixme\|hack"` returns nothing new
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean (zero)
- [ ] `cargo nextest run --workspace` — all tests pass
- [ ] `cargo deny check` — advisories, bans, licenses, sources all ok
- [ ] `cargo doc --workspace --no-deps` — zero warnings
- [ ] `roadmap.md` updated: v2.2.0 section marked ✅ Released, v2.3.0 section written
- [ ] `CHANGELOG.md` entry for v2.3.0 written
- [ ] `DEPRECATIONS.md` updated if any APIs deprecated in v2.3.0
- [ ] All public APIs in new crates (`fraiseql-sdk-gen`) have `# Errors` / `# Panics` doc sections
- [ ] Release workflow passes: `chore/release-v2.3.0` branch created, CI green

---

## Steps

### 1. Archaeology sweep
```bash
git grep -rn "Phase 1[0-4]\|TODO\|FIXME\|HACK\|temp\|WIP"
```
Remove all development markers added during Phases 10–14. Per the Eternal
Sunshine Principle: the repository should look like it was written in one
perfect session.

### 2. Roadmap update
- Mark `## v2.2.0 - Federation Maturity` as `✅ Released YYYY-MM-DD`
  (note: v2.2.0 shipped different features than the roadmap described —
  update the section to reflect what actually shipped: `sql_source_dispatch`,
  REST endpoints, observer DLQ, changelog/checkpoint endpoints, claims enrichment)
- Add `## v2.3.0 - Multi-Tenancy & SDK Generation` section describing:
  - Multi-tenancy (`TenantExecutorRegistry`, hot-reload, management API)
  - `fraiseql-sdk-gen` (TypeScript + Python client generation)
  - Apollo Federation 2 compliance
  - Cache targeted eviction (mutation overhead eliminated)
  - Security hardening (S30–S58 remainder)
- Update `## Future (Unprioritized)` to remove items now promoted to v2.3.0

### 3. Documentation accuracy check
- `docs/architecture/overview.md` — does it reflect multi-tenancy?
- `docs/architecture/compiler.md` — does it cover `fraiseql-sdk-gen`?
- `docs/security.md` — updated with S30–S58 fixes?
- `crates/fraiseql-sdk-gen/README.md` — new crate needs a README

### 4. CHANGELOG entry
```markdown
## [2.3.0] — YYYY-MM-DD

### Added
- Multi-tenancy: `TenantExecutorRegistry` for serving N tenants from one process
- `fraiseql-sdk-gen`: TypeScript and Python client SDK generation from compiled schema
- Apollo Federation 2: `@key`, `@external`, `@requires`, `@provides` directives
- `_entities` and `_service` queries for federation subgraph compliance
- `bb8` feature flag for active pool resizing via `PoolPressureMonitor`
- `POST /admin/sdk/generate` HTTP endpoint for runtime SDK generation

### Fixed
- Cache mutation routing overhead reduced from ~15% to <3% via targeted eviction
- Vault HTTP responses bounded at 1 MiB (S30)
- SCRAM `ScramClient.password` now Zeroized on drop (S38)
- Auth callback/refresh input length caps (S33)
- `reload_schema` path traversal guard (S33)
- Webhook SSRF protection (S52)
- Per-connection subscription cap (S52)
- `tb_tb_federation_sagas` double-prefix corrected (S44)

### Deprecated
(list anything deprecated)
```

### 5. Final gate
```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
cargo deny check
cargo doc --workspace --no-deps 2>&1 | grep "warning:"  # must be empty
```

### 6. Release branch
```bash
git checkout -b chore/release-v2.3.0
# bump version in Cargo.toml workspace
# push → CI → merge → tag v2.3.0
```

---

## SpecQL coordination checklist (before v2.3.0 tag)
- [ ] SpecQL `20260428-remove-axum/` Phase 03 tested against the multi-tenancy
      management API — provisioning daemon integration smoke test passes
- [ ] SpecQL `20260427-specql-platform-gaps/` Phase 15 SDK generation stubs
      upgraded to real generation using `fraiseql-sdk-gen`
- [ ] SpecQL platform-gaps P12 (observability API) federation metrics confirmed
      present in `GET /admin/metrics`

---

## Dependencies
- Requires: Phases 10, 11, 12, 13, 14 all complete on `dev`
- Blocks: v2.3.0 release tag
