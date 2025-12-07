# FraiseQL Documentation Architecture Blueprint

**Version:** 2.0 (Complete Redesign)
**Target:** v1.8.0+ with 10x improvement
**Author:** Documentation Architect
**Date:** 2025-12-07

---

## Executive Summary

This blueprint redesigns FraiseQL documentation to serve **7 distinct audiences** with **clear journeys**, **consistent naming conventions** (tb_/v_/tv_ topology), and **zero contradictions**. The architecture eliminates 24 critical inconsistencies, adds 8 missing high-value guides, and establishes quality gates to prevent regression.

**Key Changes:**
1. **Standardize ALL SQL examples** → tb_{entity}, v_{entity}, tv_{entity} naming
2. **Audience-first navigation** → Each persona has clear entry point
3. **Remove duplicates and contradictions** → Single source of truth
4. **Add missing critical guides** → RAG tutorial, SLSA verification, production checklists
5. **Establish quality framework** → Prevent future inconsistencies

---

## Current State Problems

### Critical Issues Identified

1. **Naming Convention Chaos** (24 files affected):
   - Authoritative naming doc (`TABLE_NAMING_CONVENTIONS.md`) shows BOTH `users` and `tb_user` without clear guidance
   - Trinity pattern doc (`trinity_identifiers.md`) uses `products` instead of `tb_product`
   - 13 files use old naming (`users`, `posts`, `comments`)
   - Examples contradict their own SQL files

2. **Duplicate/Outdated Content** (11 files):
   - Archive directory with no explanation
   - Duplicate planning docs in archived + current locations
   - Legacy getting-started with no deprecation warning
   - References to "deprecated mutation path" without context

3. **Missing Critical Documentation** (8 gaps):
   - No RAG system tutorial (LangChain + pgvector)
   - No SLSA provenance verification guide
   - No security profiles guide (STANDARD/REGULATED/RESTRICTED)
   - No production deployment checklist
   - No trinity pattern migration guide

4. **Audience Confusion**:
   - No clear entry points for different personas
   - Advanced patterns mixed with beginner content
   - Compliance features buried in technical docs
   - No "choose your journey" navigation

---

## Proposed Information Architecture

### Folder Structure (Redesigned)

```
docs/
├── README.md                          # Hub with persona-based navigation
│
├── quickstart/                        # 15-minute "Hello World"
│   ├── installation.md
│   ├── first-api.md                   # Uses tb_user, tb_post (trinity pattern)
│   └── deployment.md
│
├── journeys/                          # NEW: Persona-based learning paths
│   ├── backend-engineer.md            # 5+ years, evaluating FraiseQL
│   ├── junior-developer.md            # 1-2 years, learning framework
│   ├── ai-ml-engineer.md              # Building RAG systems
│   ├── devops-engineer.md             # Production deployment
│   ├── security-officer.md            # Compliance evaluation
│   ├── architect-cto.md               # Strategic decision-making
│   └── procurement-officer.md         # Federal/defense procurement
│
├── core/                              # Fundamental concepts (CORRECTED)
│   ├── philosophy.md                  # FIX: Use tb_user throughout
│   ├── queries-and-mutations.md
│   ├── types.md
│   ├── schema-discovery.md
│   ├── resolvers.md
│   └── trinity-pattern.md             # NEW: Dedicated trinity pattern intro
│
├── database/                          # Database patterns (CORRECTED)
│   ├── naming-conventions.md          # FIX: Clear tb_/v_/tv_ recommendation
│   ├── trinity-identifiers.md         # MOVE from patterns/, FIX naming
│   ├── views-and-aggregates.md        # FIX: Consistent tv_ pattern
│   ├── caching.md                     # FIX: Use tb_user
│   ├── migrations.md                  # NEW: Simple tables → trinity pattern
│   └── performance-tuning.md
│
├── features/                          # Feature documentation
│   ├── graphql-cascade.md
│   ├── mutations.md
│   ├── vector-search.md               # CONSOLIDATE scattered docs
│   ├── audit-trails.md
│   ├── multi-tenancy.md               # MOVE from advanced/
│   ├── security-profiles.md           # NEW: STANDARD/REGULATED/RESTRICTED
│   └── observability.md
│
├── ai-ml/                             # NEW: AI/ML integration hub
│   ├── README.md                      # Overview of AI/ML capabilities
│   ├── vector-search-guide.md         # 6 operators, best practices
│   ├── rag-tutorial.md                # NEW: End-to-end RAG with LangChain
│   ├── langchain-integration.md
│   ├── llamaindex-integration.md
│   └── embedding-strategies.md        # NEW: Choosing embeddings
│
├── advanced/                          # Advanced patterns (CORRECTED)
│   ├── database-patterns.md           # FIX: Use tb_/v_/tv_ naming
│   ├── bounded-contexts.md            # FIX: Use tb_ naming
│   ├── rust-pipeline.md
│   ├── custom-scalars.md
│   └── performance-optimization.md
│
├── security-compliance/               # NEW: Security & compliance hub
│   ├── README.md                      # Executive summary for non-technical
│   ├── slsa-provenance.md             # NEW: What is SLSA, verification guide
│   ├── audit-trails-deep-dive.md
│   ├── kms-integration.md             # Multi-provider guide
│   ├── security-profiles.md           # Compliance framework mapping
│   ├── rbac-row-level-security.md     # MOVE from advanced/
│   └── compliance-matrix.md           # NEW: NIST, FedRAMP, NIS2, DoD checklist
│
├── production/                        # Production deployment
│   ├── deployment-checklist.md        # NEW: Pre-launch validation
│   ├── kubernetes.md                  # NEW: K8s manifests + guide
│   ├── aws-ecs.md                     # NEW: ECS deployment
│   ├── docker-best-practices.md
│   ├── monitoring-setup.md            # Prometheus + Grafana + Loki
│   ├── incident-runbook.md            # NEW: Common issues + solutions
│   └── performance-troubleshooting.md # NEW: Debugging slow queries
│
├── examples/                          # Working examples (CORRECTED)
│   ├── blog-simple/
│   │   ├── README.md                  # FIX: Explain trinity pattern, use tb_/v_/tv_
│   │   └── db/setup.sql               # ALREADY CORRECT
│   ├── blog-enterprise/
│   ├── rag-system/                    # NEW: Full RAG example
│   │   ├── README.md
│   │   ├── schema.sql                 # tb_document, v_document, tv_document_embedding
│   │   └── app.py
│   ├── multi-tenant-saas/             # NEW: RLS + tenant isolation
│   └── compliance-demo/               # NEW: SLSA + audit trails
│
├── reference/                         # API reference
│   ├── config.md
│   ├── decorators.md
│   ├── mutations-api.md
│   ├── vector-operators.md            # NEW: 6 pgvector operators reference
│   └── security-config.md             # NEW: Security profile settings
│
├── development/                       # Contributing (CORRECTED)
│   ├── contributing.md
│   ├── framework-submission-guide.md  # FIX: Use tb_/v_/tv_ naming
│   ├── testing-strategy.md
│   ├── pre-push-hooks.md
│   └── docs-contributing.md           # NEW: How to contribute to docs
│
├── autofraiseql/                      # AutoFraiseQL (CORRECTED)
│   ├── README.md                      # FIX: Use tb_user
│   └── postgresql-comments.md         # FIX: Use tb_user
│
├── architecture/                      # Architecture decisions
│   ├── decisions/                     # ADRs
│   ├── mutation-pipeline.md           # UPDATE: Remove "v2 format" references
│   ├── security-architecture.md
│   ├── slsa-architecture.md           # NEW: SLSA implementation diagram
│   └── rust-pipeline-architecture.md  # NEW: Rust performance deep-dive
│
├── runbooks/                          # Operational runbooks
│   ├── ci-troubleshooting.md          # FIX: Use tb_user in examples
│   ├── production-incidents.md        # NEW: P0/P1/P2 incident playbook
│   └── performance-debugging.md       # NEW: Query profiling, optimization
│
├── migration/                         # NEW: Migration guides
│   ├── from-strawberry.md
│   ├── from-graphene.md
│   ├── from-postgraphile.md
│   ├── simple-to-trinity.md           # NEW: Migrating from users → tb_user
│   └── v1.7-to-v1.8.md                # Version upgrade guide
│
└── archive/                           # Legacy (CLEANED UP)
    ├── README.md                      # NEW: "These are historical, do not use"
    └── [historical docs]              # Remove duplicates, keep only unique

```

### Navigation Hierarchy

#### Top-Level Categories (docs/README.md)

```markdown
# FraiseQL Documentation

## Choose Your Journey

- **[New to FraiseQL?](quickstart/first-api.md)** → 15-minute tutorial
- **[Backend Engineer](journeys/backend-engineer.md)** → Evaluating frameworks
- **[AI/ML Engineer](journeys/ai-ml-engineer.md)** → Building RAG systems
- **[DevOps](journeys/devops-engineer.md)** → Production deployment
- **[Security/Compliance](journeys/security-officer.md)** → Evaluating compliance
- **[CTO/Architect](journeys/architect-cto.md)** → Strategic decision
- **[Procurement](journeys/procurement-officer.md)** → Federal/defense verification

## Core Documentation

- [Core Concepts](core/) - Queries, mutations, types, trinity pattern
- [Database Patterns](database/) - Naming conventions, views, caching
- [Features](features/) - Cascade, vector search, audit trails, multi-tenancy
- [AI/ML Integration](ai-ml/) - RAG, LangChain, LlamaIndex, embeddings

## Production & Security

- [Production Deployment](production/) - K8s, ECS, monitoring, incidents
- [Security & Compliance](security-compliance/) - SLSA, RBAC, KMS, compliance matrix
- [Advanced Patterns](advanced/) - Rust pipeline, bounded contexts, optimization

## Reference & Examples

- [API Reference](reference/) - Config, decorators, operators
- [Working Examples](examples/) - Blog, RAG, multi-tenant, compliance
- [Migration Guides](migration/) - From other frameworks, version upgrades
```

---

## Audience Journey Maps

### Journey 1: Junior Developer (Learning Framework)

**Goal:** Build first GraphQL API with database in <1 hour

**Path:**
1. `quickstart/installation.md` (5 min) → Install FraiseQL
2. `quickstart/first-api.md` (15 min) → Hello World with tb_user table
3. `core/trinity-pattern.md` (10 min) → Understand tb_/v_/tv_ naming
4. `examples/blog-simple/` (20 min) → Build blog API
5. `core/queries-and-mutations.md` (10 min) → Deep dive into GraphQL

**Success Criteria:**
- Has working API with database
- Understands trinity pattern
- Can add new entities independently

---

### Journey 2: Senior Backend Engineer (Evaluation Phase)

**Goal:** Make informed "build vs. buy" decision

**Path:**
1. `journeys/backend-engineer.md` (5 min) → Overview of evaluation path
2. `core/philosophy.md` (10 min) → Understand design principles
3. `advanced/rust-pipeline.md` (15 min) → Performance architecture
4. `architecture/rust-pipeline-architecture.md` (20 min) → Deep dive benchmarks
5. `migration/from-strawberry.md` (15 min) → Migration effort estimate
6. `examples/blog-enterprise/` (30 min) → Evaluate complex scenario

**Success Criteria:**
- Understands performance trade-offs
- Can estimate migration effort
- Has benchmark evidence

---

### Journey 3: AI/ML Engineer (Building RAG System)

**Goal:** Implement semantic search with FraiseQL + LangChain in <2 hours

**Path:**
1. `journeys/ai-ml-engineer.md` (5 min) → AI/ML overview
2. `ai-ml/rag-tutorial.md` (60 min) → **Copy-paste RAG implementation**
3. `ai-ml/vector-search-guide.md` (20 min) → 6 operators, when to use which
4. `ai-ml/embedding-strategies.md` (15 min) → Choosing embeddings
5. `examples/rag-system/` (20 min) → Production-ready example

**Success Criteria:**
- Has working RAG pipeline
- Understands vector operators
- Can optimize search relevance

---

### Journey 4: DevOps Engineer (Production Deployment)

**Goal:** Deploy FraiseQL to production with <5 min MTTR

**Path:**
1. `journeys/devops-engineer.md` (5 min) → Deployment overview
2. `production/deployment-checklist.md` (30 min) → Pre-launch validation
3. `production/kubernetes.md` (45 min) → K8s manifests + deployment
4. `production/monitoring-setup.md` (30 min) → Prometheus + Grafana + Loki
5. `production/incident-runbook.md` (20 min) → Common issues + solutions
6. `runbooks/production-incidents.md` (15 min) → P0/P1/P2 playbook

**Success Criteria:**
- Production deployment running
- Monitoring + alerting configured
- Can resolve common incidents in <5 min

---

### Journey 5: Security Compliance Officer (Government/Enterprise)

**Goal:** Complete compliance checklist in <30 minutes

**Path:**
1. `journeys/security-officer.md` (5 min) → Compliance overview (non-technical)
2. `security-compliance/README.md` (10 min) → Executive summary
3. `security-compliance/compliance-matrix.md` (10 min) → **NIST/FedRAMP/NIS2 checklist**
4. `security-compliance/slsa-provenance.md` (5 min) → SLSA verification commands
5. `security-compliance/security-profiles.md` (5 min) → STANDARD/REGULATED/RESTRICTED mapping

**Success Criteria:**
- Can fill out compliance checklist
- Has evidence for procurement
- No engineering help required

---

### Journey 6: CTO/Architect (Strategic Decision)

**Goal:** Present recommendation to board in <20 minutes prep

**Path:**
1. `journeys/architect-cto.md` (5 min) → **Executive summary with ROI**
2. `core/philosophy.md` (5 min) → Design principles
3. `architecture/security-architecture.md` (5 min) → Security overview
4. `security-compliance/compliance-matrix.md` (3 min) → Compliance evidence
5. `production/deployment-checklist.md` (2 min) → Operational maturity

**Success Criteria:**
- Has business case
- Understands TCO
- Can present to board

---

### Journey 7: Procurement Officer (Federal/Defense)

**Goal:** Validate SBOM + SLSA without engineering help

**Path:**
1. `journeys/procurement-officer.md` (5 min) → Procurement overview (non-technical)
2. `security-compliance/slsa-provenance.md` (10 min) → **Verification commands (copy-paste)**
3. `security-compliance/compliance-matrix.md` (10 min) → EO 14028 checklist
4. `reference/security-config.md` (5 min) → Security settings reference

**Success Criteria:**
- Can verify SBOM/SLSA independently
- Has procurement evidence
- No engineering help needed

---

## Cross-Reference Strategy

### Linking Principles

1. **Every page links to "Next Steps"** → No dead ends
2. **Related pages cross-linked** → Easy discovery
3. **Concepts link to examples** → See it in action
4. **Examples link to concepts** → Understand why
5. **Journey pages are hubs** → Central navigation

### Example Link Structure

**Page:** `ai-ml/rag-tutorial.md`

**Cross-references:**
- **Prerequisites:** → `quickstart/installation.md`, `core/trinity-pattern.md`
- **Concepts used:** → `features/vector-search.md`, `ai-ml/vector-search-guide.md`
- **Example code:** → `examples/rag-system/`
- **Related features:** → `ai-ml/langchain-integration.md`, `database/performance-tuning.md`
- **Next steps:** → `ai-ml/embedding-strategies.md`, `production/deployment-checklist.md`

---

## Versioning Approach

### Version-Specific Documentation

```
docs/
├── README.md                    # Always points to latest (v1.8.0)
├── versions/
│   ├── v1.7/
│   │   └── [full docs for v1.7]
│   ├── v1.8/
│   │   └── [full docs for v1.8]
│   └── latest -> v1.8           # Symlink
└── migration/
    ├── v1.6-to-v1.7.md
    ├── v1.7-to-v1.8.md
    └── breaking-changes.md
```

### Version Banners

All docs for non-latest versions show:

```
⚠️ **You are viewing docs for v1.7** → [View latest (v1.8)](../latest/)
```

### Breaking Changes Documentation

Each version upgrade guide includes:
- **What broke** → Specific API changes
- **How to fix** → Migration steps
- **Why it changed** → Link to ADR
- **Estimated effort** → Hours to migrate

---

## Quality Framework Integration

### Standards for All Documentation

1. **SQL Naming:**
   - ✅ **ALWAYS use:** `tb_user`, `v_user`, `tv_user_with_posts`
   - ❌ **NEVER use:** `users`, `users_view`, `user_view`
   - Exception: When specifically teaching migration from simple → trinity

2. **Code Examples:**
   - Must run on v1.8.0-beta.1
   - Include expected output
   - Show error handling
   - Link to example app

3. **Page Structure:**
   - Time estimate at top
   - Prerequisites clearly listed
   - "Next Steps" at bottom
   - Links to related concepts

4. **Writing Style:**
   - Active voice ("Configure X")
   - Actual commands (not "install dependencies")
   - Emoji sparingly (✅ ❌ ⚠️ only)
   - Code blocks specify language

---

## Migration Plan from Current to New Architecture

### Phase 1: Critical Fixes (Week 1)

**Priority: P0 - Fix authoritative documents**

1. Fix `database/naming-conventions.md` → Clear tb_/v_/tv_ recommendation
2. Fix `database/trinity-identifiers.md` → Use tb_product in all examples
3. Fix `core/philosophy.md` → Use tb_user
4. Fix `development/framework-submission-guide.md` → Use tb_/v_/tv_

**Deliverable:** Authoritative docs are consistent

### Phase 2: Cascade Fixes (Week 1-2)

**Priority: P0 - Update all referring documents**

1. Fix advanced patterns (3 files) → tb_/v_/tv_ naming
2. Fix database docs (2 files) → tb_/v_/tv_ naming
3. Fix AutoFraiseQL (2 files) → tb_user
4. Fix examples READMEs (3 files) → Explain trinity, use tb_/v_/tv_
5. Fix runbook (1 file) → tb_user in examples

**Deliverable:** 0 files with old naming (except migration guides)

### Phase 3: New Critical Guides (Week 2)

**Priority: P0 - Create missing high-value docs**

1. Create `ai-ml/rag-tutorial.md` → Full RAG implementation
2. Create `security-compliance/slsa-provenance.md` → Verification guide
3. Create `security-compliance/security-profiles.md` → STANDARD/REGULATED/RESTRICTED
4. Create `production/deployment-checklist.md` → Pre-launch validation
5. Create `database/migrations.md` → Simple → trinity pattern
6. Create `reference/vector-operators.md` → 6 operators reference

**Deliverable:** All critical gaps filled

### Phase 4: Reorganization (Week 3)

**Priority: P1 - Implement new structure**

1. Create `journeys/` folder → 7 persona journey files
2. Reorganize folders → New structure
3. Update `docs/README.md` → Persona-based navigation
4. Move files to new locations → Redirects for old links
5. Clean up archive → Add README, remove duplicates

**Deliverable:** New architecture in place

### Phase 5: Quality Assurance (Week 4)

**Priority: P1 - Validate all changes**

1. Run persona reviews → Each journey tested
2. Check all code examples → Must run on v1.8.0
3. Validate all links → No broken links
4. Review for contradictions → Single source of truth
5. Final editorial pass → Style guide compliance

**Deliverable:** Documentation ready for release

---

## Success Metrics

### Quantitative Metrics

| Metric | Current | Target | How to Measure |
|--------|---------|--------|----------------|
| **Files with old naming** | 13 | 0 | Grep for `CREATE TABLE users` (not `tb_`) |
| **Broken links** | Unknown | 0 | Link checker script |
| **Contradictions** | >10 | 0 | Conflict detection (automated) |
| **Code examples that run** | ~70% | 100% | Test harness for all examples |
| **Personas with clear journeys** | 0 | 7 | Journey files exist + tested |
| **Missing critical guides** | 8 | 0 | Gap analysis checklist |

### Qualitative Metrics (Persona Reviews)

| Persona | Success Criteria | Current | Target |
|---------|------------------|---------|--------|
| Junior Developer | First API in <1 hour | Unknown | 95% success |
| Backend Engineer | Can evaluate in <2 hours | Unknown | 100% can decide |
| AI/ML Engineer | RAG working in <2 hours | 0% (no tutorial) | 90% success |
| DevOps | Production deploy in <4 hours | Unknown | 95% success |
| Security Officer | Compliance checklist in <30 min | 0% (no matrix) | 100% complete |
| CTO | Board presentation in <20 min | 0% (no exec summary) | 100% has materials |
| Procurement | SLSA verification in <15 min | 0% (no guide) | 100% can verify |

---

## Risk Mitigation

### Risk 1: Breaking Existing Links

**Mitigation:**
- Create redirect map (old path → new path)
- Add deprecation warnings to moved files (6 months)
- Test all external links (GitHub, blogs) still work

### Risk 2: Examples Out of Sync with Code

**Mitigation:**
- Automated testing of all code examples
- CI job fails if examples don't run
- Version pinning in examples

### Risk 3: New Inconsistencies Introduced

**Mitigation:**
- Quality gates in PR reviews
- Automated conflict detection
- Style guide enforced by linter (where possible)

### Risk 4: Team Coordination Issues

**Mitigation:**
- Clear work package dependencies
- Daily standups (5 min async)
- Shared Kanban board for visibility

---

## Next Steps

1. **Review this architecture** → User approval
2. **Create team structure** → Define roles
3. **Create work packages** → Specific tasks with acceptance criteria
4. **Create QA framework** → Review process
5. **Refine personas** → Detailed journey validation
6. **Begin execution** → Spawn team, start work

---

**End of Architecture Blueprint**
