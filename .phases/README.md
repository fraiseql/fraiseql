# FraiseQL — Release Stabilization Plan

## Context

Branch `dev` is in active stabilization. CI was broadly fixed in prior sprints
(S1–S3, Tracks A–F, Phases 02/06/07). The remaining work is nightly fmt drift,
SDK parity gaps, Docker feature variants, and a documentation inconsistency.

**Security sprints S1–S35 + mutation gate S3**: complete.
**Quality score (2026-03-14)**: 4.50 / 5.0 ✅ (target: ≥ 4.5)

---

## Phase Overview

| # | Phase | Depends On | Status |
|---|-------|-----------|--------|
| [01](phase-01-nightly-fmt.md) | Fix nightly rustfmt drift | — | [ ] |
| [02](phase-02-sdk-fixes.md) | SDK CI fixes + REST annotation parity | 01 | [ ] |
| [03](phase-03-docker.md) | Docker image + `full` feature variant | 01 | [ ] |
| [04](phase-04-quality.md) | Docs fix + feature flag matrix + integration CI | 01–03 | [ ] |
| [05](phase-05-finalize.md) | Archaeology, SDK releases, final verification | 01–04 | [ ] |

## Dependency Map

```
Phase 01 (fmt) ──┬──▶ Phase 02 (SDK)    ──┐
                 └──▶ Phase 03 (Docker) ──┤
                                           ▼
                              Phase 04 (Quality) ──▶ Phase 05 (Finalize)
```

Phases 02 and 03 are independent of each other and can run in parallel after 01.

---

## What Was Already Done (see `archived/`)

| Archived Phase | Description | Status |
|----------------|-------------|--------|
| `phase-01` Cycle 1 | `fraiseql-server/Cargo.toml` ordering + secrets test entry | ✅ |
| `phase-02-clippy-and-docs` | Clippy (0 errors), cargo doc (0 warnings), sql_snapshots (92 pass) | ✅ |
| `phase-06-rest-transport` | REST transport core (router, translator, integration tests) | ✅ |
| `phase-07-openapi-and-edge-cases` | OpenAPI spec (static admin + dynamic schema-derived) | ✅ |

---

## Open GitHub Issues — Disposition

| Issue | Title | Handled In |
|-------|-------|-----------|
| #85 | REST annotation parity (PHP, Elixir, F# missing) | Phase 02 |
| #84 | C#, Elixir, F# SDK releases on package registries | Phase 05 |
| #82 | Arrow Flight docs contradiction (TOML vs Cargo feature) | Phase 04 |
| #80 | grpc-transport missing from official Docker image | Phase 03 |
| #83 | Observer synchronous mode | Out of scope — v2.2.0 |
| #81 | MySQL mutation support | Out of scope — v2.2.0 |

---

## Remaining CI Issues

| Job | Root Cause | Fixed In |
|-----|-----------|----------|
| Format Check | Nightly rustfmt drift (~596 files) | Phase 01 |
| Go SDK | `go.sum` missing from repository | Phase 02 |
| Docker build | `grpc-transport` not compiled into image | Phase 03 |
| Feature Flag Matrix | Unverified after recent changes | Phase 04 |
| Integration Tests | Unverified on current `dev` | Phase 04 |

## Key File References

| What | Where |
|------|-------|
| Go SDK module | `sdks/official/fraiseql-go/go.mod` (go.sum missing) |
| Docker build | `Dockerfile` (repo root) + `.github/workflows/docker-build.yml` |
| REST router | `crates/fraiseql-server/src/routes/rest/router.rs` |
| OpenAPI (static) | `crates/fraiseql-server/src/routes/api/openapi.rs` |
| Feature flag CI | `.github/workflows/feature-flags.yml` |
| Integration test infra | `docker/docker-compose.test.yml` via `make db-up` |
| Arrow Flight docs | `docs/` — `features/analytics.mdx` vs `features/arrow-dataplane.mdx` |
