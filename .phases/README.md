# FraiseQL — Phase Roadmap

**Current stable**: v2.2.1  
**In development**: v2.3.0 (target: Q3 2027, min 6 weeks on `dev`)  
**Long-term vision**: "Supabase for AI" — AI agents as the primary producers of
backends. FraiseQL is the runtime that SpecQL deploys.

---

## Release history

| Version | Theme | Status |
|---------|-------|--------|
| v2.0.0 | Stability & correctness | ✅ Released 2026-03-02 |
| v2.1.0 | Performance & observability | ✅ Released 2026-03-10 |
| v2.2.0 | `sql_source_dispatch`, REST, changelog, DLQ, claims enrichment | ✅ Released 2026-04 |
| v2.2.1 | CLI publish fixes, testcontainers upgrade | ✅ Released 2026-04 |
| **v2.3.0** | Multi-tenancy, SDK gen, Federation 2, cache fix | 🔨 In progress |

> Note: The `roadmap.md` v2.2.0 section describes "Federation Maturity" which
> was NOT what shipped. The roadmap needs updating in Phase 15.

---

## Phase Overview

| Phase | Title | Crates | Version | Status |
|-------|-------|--------|---------|--------|
| 1–9 | Foundation → Platform integration | all | v2.1.x / v2.2.x | ✅ Complete |
| **10** | Security hardening (S30–S58 remainder) | secrets, wire, auth, server, federation, webhooks | v2.2.x patch | [ ] |
| **11** | Multi-tenancy | core, server | v2.3.0 | [ ] |
| **12** | `fraiseql-sdk-gen` crate | new crate, cli, server | v2.3.0 | [ ] |
| **13** | Federation 2 maturity | federation, server | v2.3.0 | [ ] |
| **14** | Pool & cache optimization | db, core | v2.3.0 | [ ] |
| **15** | Finalize v2.3.0 | all | v2.3.0 | [ ] |

---

## SpecQL Coordination

SpecQL (companion project at `~/code/specql`) depends on FraiseQL in two ways:

### Runtime dependencies (blocking SpecQL phases)

| FraiseQL phase | SpecQL phase unblocked |
|---------------|------------------------|
| Phase 11 (multi-tenancy) | SpecQL `remove-axum` P03 — provisioning loop calls `PUT /admin/tenants/{id}` |
| Phase 12 (`fraiseql-sdk-gen`) | SpecQL `platform-gaps` P15 — SDK generation API upgrades from 501 stubs |
| Phase 13 (federation metrics) | SpecQL `platform-gaps` P12 — observability API needs `GET /admin/metrics` with federation counters |

### SpecQL open phase campaigns (independent of FraiseQL)

| SpecQL campaign | Phases remaining | Can start |
|----------------|-----------------|-----------|
| `20260427-specql-platform-gaps/` | P06–P16 (11 phases, ~44 TDD cycles) | P06–P10 now; P11 needs FraiseQL P11; P15 needs FraiseQL P12 |
| `20260428-remove-axum/` | P02–P07 (6 phases) | P02 now; P03 needs FraiseQL P11 |
| `20260414-fraiseql-216-alignment/` | P06–P08 (type system extensions, naming_convention, finalize) | Now — pure SpecQL work |

---

## Sequencing rationale

1. **Phase 10 first** — ships as v2.2.x patch releases. No feature-branch
   complexity; security fixes are always highest priority.
2. **Phase 11 (multi-tenancy) before federation** — multi-tenancy changes the
   core executor registry. Federation entity resolution must be built on top
   of the stable tenant-aware executor, not retrofitted.
3. **Phase 12 (`fraiseql-sdk-gen`) parallel with 11** — independent new crate,
   no shared infrastructure. Can be developed simultaneously.
4. **Phase 13 (federation) after 11** — Apollo Federation 2 entity resolution
   needs `TenantExecutorRegistry` to be stable.
5. **Phase 14 (pool/cache) parallel with 13** — performance optimization of
   independent subsystems; no ordering dependency.
6. **Phase 15 (finalize) always last** — archaeology, roadmap update, release cut.

---

## Active branches

| Branch | Purpose |
|--------|---------|
| `feat/phase9-finalization` | Phase 9 complete — open PR #252 → dev |
| `dev` | Integration target (currently at v2.2.1) |
| `main` | Stable releases |
