# FraiseQL Documentation Content Inventory

**Generated:** 2025-12-07
**Version Assessed:** v1.8.0-beta.1
**Total Files:** 172 markdown files across 34 directories

---

## Quality Rating Legend

- **5/5** - Excellent: Current, accurate, well-organized, follows standards
- **4/5** - Good: Mostly current, minor issues, needs polish
- **3/5** - Fair: Some outdated content, naming inconsistencies, needs updates
- **2/5** - Poor: Outdated, inconsistent, misleading
- **1/5** - Critical: Actively harmful, must be rewritten
- **N/A** - Archive: Legacy content, not for current use

---

## Category 1: Core Documentation (13 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `core/queries-and-mutations.md` | 4/5 | Standard docs, good foundation | P1 |
| `core/fraiseql-philosophy.md` | 3/5 | Uses `users` instead of `tb_user` (line 139) | **P0** |
| `core/types.md` | 4/5 | Generally accurate | P1 |
| `core/schema-discovery.md` | 4/5 | Good explanations | P1 |
| `core/resolvers.md` | 4/5 | Solid reference material | P1 |

**Gap Analysis:**
- ✅ Coverage: Comprehensive core concepts
- ❌ **CRITICAL:** Philosophy doc uses old naming, sets wrong example
- ⚠️ Missing: Core concept for trinity pattern (tb_/v_/tv_)

---

## Category 2: Features (13 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `features/graphql-cascade.md` | 4/5 | v1.8.0 feature, well documented | P1 |
| `features/mutation-result-reference.md` | 4/5 | Good mutation docs | P1 |
| `features/sql-function-return-format.md` | 4/5 | Clear specification | P1 |
| `features/vector-search.md` | 3/5 | Exists but scattered, needs consolidation | P0 |
| `features/audit-trails.md` | 3/5 | Partial coverage, KMS integration underexplained | P0 |

**Gap Analysis:**
- ✅ Coverage: Major features documented
- ❌ **CRITICAL:** Vector search capabilities scattered (not prominent)
- ❌ **CRITICAL:** SLSA provenance not in features (buried in CI/CD)
- ⚠️ Missing: Dedicated AI/ML integration guide
- ⚠️ Missing: Security profiles feature page (STANDARD/REGULATED/RESTRICTED)

---

## Category 3: Advanced Patterns (13 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `advanced/database-patterns.md` | 2/5 | Uses `orders`, `categories` (lines 1226, 1464, 1533) | **P0** |
| `advanced/multi-tenancy.md` | 2/5 | Uses `users`, `orders` (lines 153, 162) | **P0** |
| `advanced/bounded-contexts.md` | 2/5 | Uses `orders.orders` (lines 185, 194) | **P0** |
| `advanced/rust-mutation-pipeline.md` | 4/5 | Good Rust docs | P1 |
| `advanced/migration-guide.md` | 3/5 | Incomplete, missing v1.7→v1.8 notes | P0 |

**Gap Analysis:**
- ✅ Coverage: Advanced topics present
- ❌ **CRITICAL:** 3 major files use old naming conventions extensively
- ⚠️ Missing: Migration from simple tables to trinity pattern
- ⚠️ Missing: Performance tuning for vector search
- ⚠️ Missing: RAG system tutorial (end-to-end)

---

## Category 4: Database (3 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `database/table-naming-conventions.md` | 2/5 | **CONTRADICTORY:** Shows both `users` and `tb_*` without clear guidance (lines 527, 592-593) | **P0** |
| `database/database-level-caching.md` | 2/5 | Uses `users` instead of `tb_user` (lines 77, 539, 644) | **P0** |
| `database/view-strategies.md` | 3/5 | Mentions views but not consistent with tv_ pattern | P0 |

**Gap Analysis:**
- ✅ Coverage: Database concepts covered
- ❌ **CRITICAL:** Authoritative naming doc is contradictory
- ❌ **CRITICAL:** Caching examples use old naming
- ⚠️ Missing: Clear "when to use tb_/v_/tv_" decision tree

---

## Category 5: Examples (2 applications + examples/)

| Example | Quality | Issues | Priority |
|---------|---------|--------|----------|
| `examples/blog_simple/README.md` | 2/5 | Uses `users`, `posts`, `comments` in explanation (lines 80-129) | **P0** |
| `examples/blog_simple/db/setup.sql` | 5/5 | ✅ **CORRECT:** Uses `tb_user`, `tb_post`, trinity pattern | P1 |
| `examples/mutations_demo/README.md` | 2/5 | Uses `users` table (line 72) | **P0** |
| `examples/blog_enterprise/` | 4/5 | More complex but needs review for naming | P1 |

**Gap Analysis:**
- ✅ Coverage: Basic examples exist
- ❌ **CRITICAL:** Example READMEs contradict their own SQL files
- ⚠️ Missing: RAG system example (LangChain + pgvector)
- ⚠️ Missing: Multi-tenancy example with RLS
- ⚠️ Missing: Compliance-ready example (SLSA + audit trails)

---

## Category 6: Architecture (8 files + ADRs)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `architecture/decisions/002-ultra-direct-mutation-path.md` | 3/5 | References deprecated mutation path | P1 |
| `architecture/mutation-pipeline.md` | 3/5 | Mentions "v2 format should be updated" | P1 |
| `architecture/security-architecture.md` | 4/5 | Good security overview | P1 |

**Gap Analysis:**
- ✅ Coverage: ADRs are solid
- ⚠️ Cleanup needed: Remove/update deprecated mutation references
- ⚠️ Missing: SLSA architecture diagram
- ⚠️ Missing: Rust pipeline architecture explanation

---

## Category 7: Guides (11 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `guides/cascade-best-practices.md` | 4/5 | Good v1.8.0 guide | P1 |
| `guides/migrating-to-cascade.md` | 4/5 | Solid migration guide | P1 |
| `guides/performance-guide.md` | 3/5 | Good but missing vector search perf tuning | P0 |
| `guides/deployment-checklist.md` | 3/5 | Partial, needs production checklist expansion | P0 |

**Gap Analysis:**
- ✅ Coverage: Common tasks documented
- ⚠️ Missing: "From simple tables to trinity pattern" migration guide
- ⚠️ Missing: "Setting up SLSA provenance verification" guide
- ⚠️ Missing: "Production monitoring setup" (Prometheus + Grafana)

---

## Category 8: Getting Started (5 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `getting-started/quickstart.md` | 4/5 | Good onboarding | P1 |
| `getting-started/getting-started-legacy.md` | N/A | **ARCHIVE:** Explicitly marked legacy | **Archive** |
| `getting-started/installation.md` | 4/5 | Clear install instructions | P1 |

**Gap Analysis:**
- ✅ Coverage: Basic onboarding works
- ⚠️ Cleanup needed: Remove or clearly mark legacy file
- ⚠️ Missing: "First GraphQL API with trinity pattern" tutorial

---

## Category 9: Reference (9 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `reference/config.md` | 4/5 | Good config reference | P1 |
| `reference/decorators.md` | 4/5 | Solid API docs | P1 |
| `reference/mutations-api.md` | 4/5 | Good mutation reference | P1 |

**Gap Analysis:**
- ✅ Coverage: API reference is comprehensive
- ⚠️ Missing: Vector search operators reference (6 operators documented?)
- ⚠️ Missing: Security profile config reference

---

## Category 10: Development (8 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `development/framework-submission-guide.md` | 2/5 | Uses `users`, `posts`, `comments` (lines 283, 292, 301) | **P0** |
| `development/pre-push-hooks.md` | 4/5 | Good development workflow | P1 |
| `development/testing-strategy.md` | 4/5 | Solid testing docs | P1 |

**Gap Analysis:**
- ✅ Coverage: Development workflows documented
- ❌ **CRITICAL:** Framework submission guide uses old patterns (will mislead contributors)
- ⚠️ Missing: "Contributing to docs" guide

---

## Category 11: AutoFraiseQL (2 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `autofraiseql/README.md` | 2/5 | Uses `users` (line 53) | **P0** |
| `autofraiseql/postgresql-comments.md` | 2/5 | Uses `users` (line 91) | **P0** |

**Gap Analysis:**
- ✅ Coverage: AutoFraiseQL explained
- ❌ **CRITICAL:** Uses old naming in examples

---

## Category 12: Production (5 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `production/deployment-scenarios.md` | 3/5 | Good scenarios but missing production checklist | P0 |
| `production/monitoring.md` | 3/5 | Basics covered, needs observability stack details | P0 |
| `production/loki-integration.md` | 4/5 | New file, good Loki docs | P1 |

**Gap Analysis:**
- ✅ Coverage: Deployment scenarios documented
- ⚠️ Missing: Kubernetes manifests example
- ⚠️ Missing: AWS ECS deployment guide
- ⚠️ Missing: "Production incident runbook"

---

## Category 13: Testing (8 files)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `testing/test-patterns.md` | 4/5 | Good testing patterns | P1 |
| `testing/class-scoped-pools.md` | 5/5 | ✅ Excellent technical deep-dive | P1 |

**Gap Analysis:**
- ✅ Coverage: Testing well documented
- ⚠️ Missing: Testing AI/ML integrations (vector search)

---

## Category 14: Archive (6 files in `/archive/`)

| File | Status | Action |
|------|--------|--------|
| `archive/FAKE_DATA_GENERATOR_DESIGN.md` | Obsolete | **REMOVE** or clearly mark "NOT IMPLEMENTED" |
| `archive/GETTING_STARTED.md` | Obsolete | **REMOVE** (replaced by getting-started/) |
| `archive/ROADMAP.md` | Outdated | **UPDATE** or remove |
| `archive/TESTING_CHECKLIST.md` | Legacy | **REMOVE** (replaced by testing/) |
| `archive/fraiseql_enterprise_gap_analysis.md` | Historical | **KEEP** but mark "HISTORICAL ONLY" |

**Action Required:** Add `README.md` to `/archive/` explaining these are historical documents not for current use.

---

## Category 15: Archived Planning (2 files in `/planning/archived-pre-v1.9/`)

| File | Status | Action |
|------|--------|--------|
| `cascade-implementation-recommendation.md` | Duplicate | **REMOVE** (exists in `/planning/` too) |
| `graphql-cascade-simplified-approach.md` | Duplicate | **REMOVE** (exists in `/planning/` too) |

**Action Required:** Delete duplicates from archived location.

---

## Category 16: Runbooks (1 file)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `runbooks/ci-troubleshooting.md` | 3/5 | Uses `users` in example (line 162) | P0 |

**Gap Analysis:**
- ✅ Coverage: CI troubleshooting exists
- ⚠️ Missing: Production incident runbook
- ⚠️ Missing: Performance troubleshooting runbook

---

## Category 17: Patterns (1 file)

| File | Quality | Issues | Priority |
|------|---------|--------|----------|
| `patterns/trinity-identifiers.md` | 2/5 | Uses `products` instead of `tb_product` (line 57) | **P0** |

**Gap Analysis:**
- ✅ Coverage: Trinity pattern documented
- ❌ **CRITICAL:** The authoritative trinity pattern doc uses old naming!

---

## Critical Findings Summary

### SQL Naming Convention Issues (P0 - CRITICAL)

**Files that MUST be updated to use tb_/v_/tv_ naming:**

1. `database/table-naming-conventions.md` - **Authoritative doc is contradictory**
2. `patterns/trinity-identifiers.md` - **Trinity pattern doc uses wrong naming**
3. `core/fraiseql-philosophy.md` - Philosophy uses old naming
4. `advanced/database-patterns.md` - Extensive old naming
5. `advanced/multi-tenancy.md` - RLS examples use old naming
6. `advanced/bounded-contexts.md` - Old naming in examples
7. `database/database-level-caching.md` - Caching examples inconsistent
8. `development/framework-submission-guide.md` - **Will mislead contributors**
9. `autofraiseql/README.md` - AutoFraiseQL uses old naming
10. `autofraiseql/postgresql-comments.md` - Old naming in examples
11. `examples/blog_simple/README.md` - **Contradicts its own SQL file**
12. `examples/mutations_demo/README.md` - Old naming
13. `runbooks/ci-troubleshooting.md` - Example uses old naming

**Total:** 13 files with critical naming issues

### Outdated/Duplicate Content (P0 - CLEANUP)

1. Archive directory (6 files) - needs README explaining status
2. Archived planning duplicates (2 files) - delete duplicates
3. Legacy getting-started (1 file) - remove or mark clearly
4. Deprecated mutation references (2 files) - update or remove

**Total:** 11 files needing cleanup

### Missing High-Value Documentation (P0 - GAPS)

1. **RAG System Tutorial** - End-to-end LangChain + pgvector example
2. **SLSA Verification Guide** - How to verify provenance
3. **Security Profiles Guide** - STANDARD vs REGULATED vs RESTRICTED
4. **Vector Search Reference** - All 6 operators documented
5. **Trinity Pattern Migration** - From simple tables to tb_/v_/tv_
6. **Production Deployment Checklist** - Pre-launch validation
7. **Kubernetes Manifests** - Example K8s deployment
8. **Production Incident Runbook** - Common issues + solutions

**Total:** 8 critical missing guides

---

## Overall Quality Assessment

| Category | Files | Avg Quality | Critical Issues | Priority |
|----------|-------|-------------|-----------------|----------|
| **Core** | 13 | 3.8/5 | 1 (philosophy) | P0 |
| **Features** | 13 | 3.6/5 | 2 (vector search, SLSA) | P0 |
| **Advanced** | 13 | 2.7/5 | **3 major files** | **P0** |
| **Database** | 3 | 2.3/5 | **All 3 files** | **P0** |
| **Examples** | 4 | 3.3/5 | 3 (READMEs) | **P0** |
| **Architecture** | 8 | 3.5/5 | 2 (deprecated refs) | P1 |
| **Guides** | 11 | 3.5/5 | 2 (perf, deployment) | P0 |
| **Getting Started** | 5 | 4.0/5 | 1 (legacy file) | P1 |
| **Reference** | 9 | 4.0/5 | 0 | P1 |
| **Development** | 8 | 3.5/5 | **1 (submission guide)** | **P0** |
| **AutoFraiseQL** | 2 | 2.0/5 | **2 files** | **P0** |
| **Production** | 5 | 3.3/5 | 2 (missing guides) | P0 |
| **Testing** | 8 | 4.5/5 | 0 | P1 |
| **Archive** | 6 | N/A | 6 (cleanup) | P1 |
| **Runbooks** | 1 | 3.0/5 | 1 (naming) | P0 |
| **Patterns** | 1 | 2.0/5 | **1 (trinity doc!)** | **P0** |

**OVERALL SCORE: 3.2/5** - Fair quality with critical naming inconsistencies

---

## Priority Breakdown

### P0 - Critical (MUST FIX)
- **24 files** with critical issues (naming, contradictions, missing critical guides)
- Estimated effort: **80-100 hours** (with team of 5-7)

### P1 - Important (SHOULD FIX)
- **15 files** needing polish, minor updates, cleanup
- Estimated effort: **30-40 hours**

### P2 - Nice to Have
- Additional examples, advanced tutorials, edge case documentation
- Estimated effort: **20-30 hours**

---

## Recommended Next Steps

1. **Fix authoritative documents FIRST:**
   - `database/table-naming-conventions.md` - Make clear recommendation
   - `patterns/trinity-identifiers.md` - Use tb_/v_/tv_ in all examples

2. **Cascade fixes to referencing documents:**
   - All advanced patterns (3 files)
   - All database docs (3 files)
   - AutoFraiseQL (2 files)
   - Development guide (1 file)

3. **Update examples:**
   - Blog simple README
   - Mutations demo README
   - CI troubleshooting runbook

4. **Create missing critical guides:**
   - RAG tutorial
   - SLSA verification
   - Security profiles
   - Production checklist

5. **Cleanup archives:**
   - Add README to archive/
   - Delete planning duplicates
   - Mark legacy files clearly

---

**End of Inventory**
