# FraiseQL v2.3.0 Extension Phases

## Overview

Continuation of v2.3.0 development. The first sprint (`v2.3.0-phases/`) addressed
security hardening, performance verification, integration test coverage, and code
quality. This sprint tackles the deferred items from that sprint plus new feature
work before the v2.3.0 release.

## Phase Structure

| # | Phase | Theme | Status |
|---|-------|-------|--------|
| 06 | Deferred Test Infrastructure | WebSocket + auth test harnesses | [ ] |
| 07 | fraiseql-storage Repair | Fix pre-existing compile errors | [ ] |
| 08 | Studio Metrics Backend | Wire real collectors into metrics_summary | [ ] |
| 09 | Hot-Reload Cache Rebind | Fix TODO(#184) — re-wrap adapter on schema reload | [ ] |
| 10 | Finalize v2.3.0 | Release prep, changelog, version bump | [ ] |

## Deferred Items Carried Forward from v2.3.0 Sprint

| Cycle | What | Why Deferred |
|-------|------|-------------|
| P03-C1 | Subscription forwarder integration test | Requires mock WebSocket subgraph server |
| P03-C6 | `GET /auth/me` integration test | Requires JWT token issuance in test context |

## Known Issues Being Fixed

| Location | Issue | Introduced |
|----------|-------|-----------|
| `fraiseql-storage` (azure.rs, gcs.rs, backend/mod.rs) | `FraiseQLError: From<FileError>` not satisfied | Pre-v2.2.0 |
| `platform_e2e_test.rs` | `fraiseql_server::subsystems` + `FunctionsConfig` not in public API | Pre-v2.2.0 |

## Success Criteria

- [ ] All deferred integration tests passing (P03-C1 and P03-C6)
- [ ] `cargo check -p fraiseql-storage --all-features` clean
- [ ] `platform_e2e_test.rs` compiles and passes
- [ ] Studio metrics endpoint returns real data (not zero-value placeholder)
- [ ] Hot-reload correctly re-wraps adapter (`TODO(#184)` resolved)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] All tests pass; version bumped to `2.3.0`
