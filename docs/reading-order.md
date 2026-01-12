# FraiseQL v2 Reading Order Guide

**Version:** 1.0
**Date:** January 11, 2026
**Status:** Complete
**Purpose:** Role-based navigation paths through FraiseQL v2 documentation

---

## How to Use This Guide

This document provides **reading paths** tailored to different roles and use cases. Each path includes:
- **Documents to read** in recommended order
- **Estimated time** to complete each document
- **Key takeaways** from each document
- **What you'll be able to do** after completing the path

**Choose your path:**
1. [New to FraiseQL? Start Here](#new-to-frais

eql-start-here)
2. [For Architects](#for-architects)
3. [For Compiler Developers](#for-compiler-developers)
4. [For Runtime Developers](#for-runtime-developers)
5. [For Database Architects](#for-database-architects)
6. [For Operations / DevOps](#for-operations--devops)
7. [For Security Engineers](#for-security-engineers)
8. [For Frontend Developers](#for-frontend-developers)

---

## New to FraiseQL? Start Here

**Goal:** Understand what FraiseQL is and how it differs from traditional GraphQL servers.

**Total Time:** ~45 minutes

### Path

1. **`README.md`** (5 minutes)
   - High-level overview
   - Core principles
   - Feature list
   - **Key Takeaway:** FraiseQL is a compiled database execution layer, not a GraphQL server

2. **`docs/prd/PRD.md`** — Read Sections 1-2 only (20 minutes)
   - Vision and philosophy
   - System architecture
   - **Key Takeaway:** Compilation over interpretation, database as source of truth

3. **`docs/GLOSSARY.md`** — Skim, bookmark for reference (10 minutes)
   - Key terminology
   - **Key Takeaway:** AuthoringIR, CompiledSchema, database targeting

4. **`docs/architecture/database/database-targeting.md`** — Read Sections 1-3 (10 minutes)
   - Multi-database support via compile-time specialization
   - **Key Takeaway:** Same schema source, different compiled outputs per database

**After this path, you'll understand:**
- What FraiseQL is and isn't
- Core architectural principles
- How multi-database support works
- Key terminology

**Next Steps:**
- Choose a role-specific path below based on your work
- Or continue to "Complete Documentation Path" for comprehensive coverage

---

## For Architects

**Goal:** Understand FraiseQL's architecture, design decisions, and trade-offs.

**Total Time:** ~3.5 hours

### Phase 1: Foundation (1 hour)

1. **`README.md`** (5 min)
2. **`docs/prd/PRD.md`** (30 min)
   - All sections
   - **Key Takeaway:** Design principles, architectural boundaries
3. **`docs/GLOSSARY.md`** (10 min)
4. **`docs/architecture/database/database-targeting.md`** (15 min)
   - **Key Takeaway:** Compile-time schema specialization

### Phase 2: Core Architecture (1.5 hours)

5. **`docs/architecture/core/authoring-languages.md`** (20 min)
   - Language-agnostic compilation via AuthoringIR
   - **Key Takeaway:** One canonical language per org, translation paths for migration

6. **`docs/architecture/core/compilation-pipeline.md`** (30 min)
   - 7 phases from authoring → CompiledSchema
   - **Key Takeaway:** Deterministic, static analysis at compile time

7. **`docs/architecture/core/execution-model.md`** (40 min)
   - 6 runtime phases
   - **Key Takeaway:** Runtime is deterministic, executes compiled plans

8. **`docs/architecture/integration/federation.md`** (40 min)
   - Federation v2 architecture and implementation
   - Direct DB federation for multi-database scenarios
   - **Key Takeaway:** Subgraph composition via direct database connections + HTTP fallback

### Phase 3: Specifications (1 hour)

9. **`docs/specs/compiled-schema.md`** (25 min)
   - CompiledSchema JSON structure
   - **Key Takeaway:** Immutable artifact, database-target-specific

10. **`docs/specs/schema-conventions.md`** (35 min)
   - Database patterns (tb_*, v_*, fn_*)
   - **Key Takeaway:** Conventions enable automatic CQRS, efficient composition

**After this path, you'll be able to:**
- Explain FraiseQL's architecture to stakeholders
- Understand compile-time vs runtime responsibilities
- Evaluate FraiseQL for your use case
- Design schema conventions for your domain

**Recommended Next:**
- `docs/adrs/ADR-009-federation-architecture.md` — Federation design decisions
- `docs/architecture/realtime/subscriptions.md` — Real-time events and event streaming
- `docs/architecture/performance/advanced-optimization.md` — Advanced performance optimization
- `docs/architecture/decisions/design-decisions.md` — Core design rationale
- `docs/architecture/core/compilation-phases.md` — Detailed compilation phase breakdown

---

## For Compiler Developers

**Goal:** Build or extend the FraiseQL compiler.

**Total Time:** ~4 hours

### Phase 1: Authoring System (1.5 hours)

1. **`docs/specs/authoring-contract.md`** (40 min)
   - Type declarations, decorators, validation rules
   - **Key Takeaway:** What schema authors write

2. **`docs/architecture/core/authoring-languages.md`** (25 min)
   - Language-agnostic compilation
   - **Key Takeaway:** All languages → AuthoringIR

3. **`docs/GLOSSARY.md`** — Reference as needed (25 min)
   - Focus on: AuthoringIR, CompiledSchema, Binding, Projection

### Phase 2: Compilation Pipeline (1.5 hours)

4. **`docs/architecture/core/compilation-pipeline.md`** (50 min)
   - Complete 7-phase pipeline
   - **Key Takeaway:** Phase 4 (WHERE type generation) is database-specific

5. **`docs/architecture/database/database-targeting.md`** (40 min)
   - Capability manifest, operator filtering
   - **Key Takeaway:** Compile-time specialization, not runtime translation

### Phase 3: Output (1 hour)

6. **`docs/specs/compiled-schema.md`** (35 min)
   - JSON structure, validation rules
   - **Key Takeaway:** This is what compiler produces

7. **`docs/specs/schema-conventions.md`** (25 min)
   - Binding validation (types → database views)
   - **Key Takeaway:** Compiler validates bindings against database schema

**After this path, you'll be able to:**
- Implement a new authoring language (e.g., Rust, Go)
- Extend the compiler with new phases
- Add database target support (new capability manifest)
- Debug compilation errors

**Recommended Next:**
- `docs/reference/where-operators.md` — Complete operator catalog
- `docs/reference/scalars.md` — Scalar type library
- `docs/architecture/core/compilation-phases.md` — Deep dive into each compilation phase
- `docs/specs/capability-manifest.md` — Database capability specifications

---

## For Runtime Developers

**Goal:** Build or extend the FraiseQL Rust runtime.

**Total Time:** ~3 hours

### Phase 1: CompiledSchema (45 min)

1. **`docs/specs/compiled-schema.md`** (30 min)
   - JSON structure, all fields
   - **Key Takeaway:** This is runtime's input

2. **`docs/GLOSSARY.md`** (15 min)
   - Focus on: Backend Lowering, Projection, Authorization

### Phase 2: Execution Pipeline (1.5 hours)

3. **`docs/architecture/core/execution-model.md`** (60 min)
   - 6 runtime phases in detail
   - **Key Takeaway:** Deterministic execution of compiled plans

4. **`docs/architecture/database/database-targeting.md`** (30 min)
   - Backend lowering (SQL generation)
   - **Key Takeaway:** Runtime translates SDL predicates to database-specific SQL

### Phase 3: Performance & Operations (1.5 hours)

5. **`docs/specs/caching.md`** (20 min)
   - Query result caching, cache invalidation
   - **Key Takeaway:** Phase 0 of execution

6. **`docs/specs/persisted-queries.md`** (25 min)
   - APQ protocol, 3 security modes
   - **Key Takeaway:** Phase 0 APQ resolution before parsing

7. **`docs/architecture/realtime/subscriptions.md`** (50 min)
   - Database-native event streams, transport adapters
   - **Key Takeaway:** Subscriptions are compiled event projections, not GraphQL resolvers

**After this path, you'll be able to:**
- Implement a new backend lowering module (e.g., DuckDB, ClickHouse)
- Extend the runtime with new execution phases
- Add performance optimizations
- Debug runtime errors
- Build subscription transport adapters (graphql-ws, webhooks, Kafka)

**Recommended Next:**
- `docs/reference/where-operators.md` — SQL generation for each operator
- `docs/enterprise/rbac.md` — Authorization enforcement
- `docs/guides/monitoring.md` — Instrumentation
- `docs/specs/cdc-format.md` — Event format for subscriptions
- `docs/architecture/core/execution-semantics.md` — Detailed execution semantics
- `docs/architecture/reliability/error-handling-model.md` — Error handling strategies
- `docs/architecture/performance/performance-characteristics.md` — Performance analysis

---

## For Database Architects

**Goal:** Design database schemas that work optimally with FraiseQL.

**Total Time:** ~2.5 hours

### Phase 1: Conventions (1 hour)

1. **`docs/prd/PRD.md`** — Section 3.2 only (15 min)
   - Schema conventions overview
   - **Key Takeaway:** Opinionated patterns required

2. **`docs/specs/schema-conventions.md`** (45 min)
   - Complete conventions reference
   - **Key Takeaway:** tb_*, v_*, fn_* patterns; dual-key strategy; JSONB composition

### Phase 2: Read Model (1 hour)

3. **`docs/prd/PRD.md`** — Section 3.1.1 and 3.1.5 (20 min)
   - Read model, JSONB composition
   - **Key Takeaway:** Pre-aggregated views enable O(1) composition

4. **`docs/specs/cdc-format.md`** (40 min)
   - CDC event format
   - **Key Takeaway:** Debezium-compatible, audit trail

### Phase 3: Enterprise Features (30 min)

5. **`docs/enterprise/audit-logging.md`** — Sections 1-3 only (30 min)
   - Audit columns, cryptographic chain
   - **Key Takeaway:** Audit columns required for CDC

**After this path, you'll be able to:**
- Design FraiseQL-compatible database schemas
- Create efficient read views with JSONB projections
- Set up pre-aggregated views for relationships
- Implement audit logging and soft deletes

**Recommended Next:**
- `docs/reference/where-operators.md` — Understanding filterable paths
- `docs/guides/production-deployment.md` — Index strategies
- `docs/architecture/database/arrow-plane.md` — Arrow-based data plane for analytics
- `docs/specs/pagination-keyset.md` — Keyset pagination patterns

---

## For Analytics Engineers

**Goal:** Build analytical systems with FraiseQL's fact table patterns and aggregations.

**Total Time:** ~3-4 hours

### Phase 1: Core Concepts (1 hour)

1. **`docs/architecture/core/compilation-vs-runtime.md`** (20 min)
   - Understand compile-time vs runtime separation
   - **Key Takeaway:** Python/TS authoring → Rust compilation → Rust runtime

2. **`docs/architecture/core/execution-model.md`** (30 min)
   - GraphQL query execution phases
   - Phase 2.5: Aggregation Resolution
   - **Key Takeaway:** How GROUP BY and aggregates are compiled

3. **`docs/prd/PRD.md`** — Section 3.5 only (10 min)
   - Analytical Execution Semantics overview
   - **Key Takeaway:** No joins principle, fact table patterns

### Phase 2: Analytics Architecture (1.5 hours)

4. **`docs/architecture/analytics/fact-dimension-pattern.md`** ⭐ **START HERE** (40 min)
   - Fact table structure (measures, dimensions, filters)
   - No joins principle
   - Aggregate tables = fact tables with different granularity
   - **Key Takeaway:** All analytical tables use same pattern: SQL columns (measures) + JSONB (dimensions)

5. **`docs/architecture/analytics/aggregation-model.md`** (30 min)
   - GROUP BY compilation strategy
   - Aggregate function selection
   - Temporal bucketing (DATE_TRUNC, DATE_FORMAT, strftime, DATEPART)
   - HAVING clause validation
   - **Key Takeaway:** Compile-time schema analysis → optimized SQL

6. **`docs/specs/analytical-schema-conventions.md`** (20 min)
   - Naming conventions: tf_ (fact), ta_ (aggregate), td_ (dimension)
   - Column patterns: measures, dimensions, denormalized filters
   - Index recommendations
   - **Key Takeaway:** Conventions required by FraiseQL compiler

### Phase 3: Database-Specific Implementation (1 hour)

7. **`docs/specs/aggregation-operators.md`** (30 min)
   - PostgreSQL: Full support (STDDEV, VARIANCE, FILTER)
   - MySQL: Basic support (no statistical functions)
   - SQLite: Minimal support
   - SQL Server: Enterprise support (STDEV, VAR, JSON_VALUE)
   - **Key Takeaway:** Database capability manifest determines available functions

8. **`docs/architecture/database/database-targeting.md`** (15 min)
   - Compile-time database specialization
   - **Key Takeaway:** GraphQL schema matches database capabilities

9. **`docs/specs/capability-manifest.md`** — Section 3.4 only (15 min)
   - Aggregation operators in capability manifest
   - **Key Takeaway:** How compiler knows which aggregates to generate

### Phase 4: Practical Application (30 min)

10. **`docs/guides/analytics-patterns.md`** ⭐ **PRACTICAL EXAMPLES** (30 min)
    - 10 common query patterns with SQL execution
    - Simple aggregation, GROUP BY, temporal bucketing, HAVING
    - Performance optimization tips
    - Database-specific notes
    - **Key Takeaway:** Copy-paste patterns for real analytics queries

**After this path, you'll be able to:**
- Design fact tables with measures + dimensions
- Write GraphQL aggregate queries (GROUP BY, HAVING, temporal bucketing)
- Understand performance characteristics (SQL columns 10-100x faster than JSONB)
- Use pre-aggregated tables (ta_*) for common rollups
- Leverage database-specific aggregate functions

**Recommended Next:**
- `docs/architecture/database/arrow-plane.md` — Section 5.5 for BI tool integration
- `docs/architecture/analytics/window-functions.md` — Phase 5 planned features (ROW_NUMBER, LAG/LEAD)
- `docs/specs/window-operators.md` — Window function reference
- `docs/specs/schema-conventions.md` — Section 4.3.1 for analytical pre-aggregated tables

**Important Notes:**
- FraiseQL does **NOT** support joins; all dimensions must be denormalized at ETL time
- ETL is managed by DBA/data team; FraiseQL provides GraphQL query interface only
- Aggregate tables (`ta_*`) have same structure as fact tables (`tf_*`), just different granularity

---

## For Operations / DevOps

**Goal:** Deploy, monitor, and maintain FraiseQL in production.

**Total Time:** ~3 hours

### Phase 1: Deployment (1.5 hours)

1. **`docs/guides/production-deployment.md`** (60 min)
   - Kubernetes deployment, HPA, Pod Security
   - **Key Takeaway:** Complete production setup

2. **`docs/specs/security-compliance.md`** (30 min)
   - Security profiles (STANDARD, REGULATED, RESTRICTED)
   - **Key Takeaway:** Choose profile based on compliance needs

### Phase 2: Monitoring (1 hour)

3. **`docs/guides/monitoring.md`** (60 min)
   - Prometheus metrics, OpenTelemetry, health checks
   - **Key Takeaway:** Complete observability setup

### Phase 3: Performance (30 min)

4. **`docs/specs/caching.md`** (15 min)
   - Query result caching backends
   - **Key Takeaway:** Memory vs Database backends

5. **`docs/specs/persisted-queries.md`** — Sections 1-3 only (15 min)
   - APQ deployment patterns
   - **Key Takeaway:** 3 security modes for different environments

**After this path, you'll be able to:**
- Deploy FraiseQL to Kubernetes
- Configure monitoring and alerting
- Tune performance (caching, connection pooling)
- Implement security best practices

**Recommended Next:**
- `docs/enterprise/rbac.md` — Role-based access control setup
- `docs/enterprise/kms.md` — Key management integration
- `docs/guides/observability.md` — Observability best practices
- `docs/architecture/observability/observability-model.md` — Observability architecture

---

## For Security Engineers

**Goal:** Understand FraiseQL's security model and harden deployments.

**Total Time:** ~3 hours

### Phase 1: Security Model (1 hour)

1. **`docs/prd/PRD.md`** — Section 4 only (30 min)
   - Authentication (external), authorization (declarative)
   - **Key Takeaway:** No user code execution, deterministic enforcement

2. **`docs/specs/security-compliance.md`** (30 min)
   - Security profiles, SBOM, NIS2 compliance
   - **Key Takeaway:** REGULATED profile for production

### Phase 2: Authorization (1.5 hours)

3. **`docs/enterprise/rbac.md`** (60 min)
   - Hierarchical roles, field-level auth
   - **Key Takeaway:** 3 enforcement layers

4. **`docs/enterprise/audit-logging.md`** (30 min)
   - Cryptographic chain, HMAC signatures
   - **Key Takeaway:** Tamper-evident audit trails

### Phase 3: Operations (30 min)

5. **`docs/specs/introspection.md`** (15 min)
   - Introspection policies
   - **Key Takeaway:** Disable in production

6. **`docs/guides/production-deployment.md`** — Section 6 only (15 min)
   - Security hardening checklist
   - **Key Takeaway:** Pod Security Standards, network policies

**After this path, you'll be able to:**
- Configure authentication providers
- Design authorization rules
- Set up audit logging
- Harden production deployments

**Recommended Next:**
- `docs/enterprise/kms.md` — Field encryption with KMS
- `docs/architecture/security/security-model.md` — Security architecture deep dive
- `docs/architecture/security/authentication-detailed.md` — Authentication implementation details
- `docs/architecture/reliability/consistency-model.md` — Data consistency guarantees
- `docs/architecture/reliability/failure-modes-and-recovery.md` — Failure handling strategies

---

## For Frontend Developers

**Goal:** Query FraiseQL APIs effectively from client applications.

**Total Time:** ~1.5 hours

### Phase 1: GraphQL API (45 min)

1. **`README.md`** (5 min)
2. **`docs/prd/PRD.md`** — Sections 1, 3.1, 5 only (30 min)
   - What FraiseQL provides
   - Read model
   - GraphQL semantics
   - **Key Takeaway:** Deterministic, type-safe API

3. **`docs/GLOSSARY.md`** — WHERE Operators, APQ, Cache Invalidation (10 min)

### Phase 2: Performance (30 min)

4. **`docs/specs/caching.md`** — Sections 1-4 (15 min)
   - Query result caching
   - Cache invalidation with graphql-cascade
   - **Key Takeaway:** Client-side cache invalidation patterns

5. **`docs/specs/persisted-queries.md`** — Sections 1-3 (15 min)
   - APQ protocol
   - **Key Takeaway:** Register queries at build time for security + performance

### Phase 3: Filtering (15 min)

6. **`docs/reference/where-operators.md`** — Skim (15 min)
   - WHERE operator catalog
   - **Key Takeaway:** Database-specific operators available

**After this path, you'll be able to:**
- Write efficient GraphQL queries
- Implement client-side caching with cache invalidation
- Use APQ for security and performance
- Filter queries with database-specific operators

**Recommended Next:**
- `docs/specs/introspection.md` — Schema introspection
- `docs/reference/scalars.md` — Custom scalar types
- `docs/specs/pagination-keyset.md` — Advanced pagination techniques
- `docs/guides/testing-strategy.md` — Testing GraphQL APIs

---

## Complete Documentation Path

**Goal:** Comprehensive understanding of FraiseQL v2.

**Total Time:** ~20-25 hours (expanded to include all architecture deep dives)

**Recommended for:** Core maintainers, technical leads, documentation contributors

### Week 1: Foundation (4-5 hours)

**Day 1:**
1. README.md (5 min)
2. PRD.md (60 min)
3. GLOSSARY.md (30 min)

**Day 2:**
4. architecture/database/database-targeting.md (30 min)
5. architecture/core/authoring-languages.md (30 min)
6. architecture/core/compilation-pipeline.md (60 min)

**Day 3:**
7. architecture/core/execution-model.md (90 min)
8. architecture/core/compilation-phases.md (50 min)

### Week 2: Specifications (5-6 hours)

**Day 4:**
9. specs/compiled-schema.md (40 min)
10. specs/authoring-contract.md (60 min)
11. specs/schema-conventions.md (50 min)

**Day 5:**
12. specs/caching.md (30 min)
13. specs/persisted-queries.md (60 min)
14. specs/security-compliance.md (40 min)

**Day 6:**
15. specs/introspection.md (25 min)
16. specs/cdc-format.md (45 min)
17. specs/capability-manifest.md (25 min)
18. specs/pagination-keyset.md (25 min)

### Week 3: Enterprise & Operations (3-4 hours)

**Day 7:**
19. enterprise/rbac.md (60 min)
20. enterprise/audit-logging.md (60 min)

**Day 8:**
21. enterprise/kms.md (50 min)
22. guides/monitoring.md (50 min)
23. guides/observability.md (35 min)

**Day 9:**
24. guides/production-deployment.md (60 min)
25. guides/testing-strategy.md (80 min)

### Week 4: Architecture Deep Dives (4-5 hours)

**Day 10:**
26. architecture/core/execution-semantics.md (50 min)
27. architecture/core/compilation-vs-runtime.md (20 min)
28. architecture/integration/federation.md (80 min)

**Day 11:**
29. architecture/realtime/subscriptions.md (55 min)
30. architecture/database/arrow-plane.md (60 min)

**Day 12:**
31. architecture/security/security-model.md (40 min)
32. architecture/security/authentication-detailed.md (60 min)

**Day 13:**
33. architecture/reliability/consistency-model.md (25 min)
34. architecture/reliability/error-handling-model.md (35 min)
35. architecture/reliability/failure-modes-and-recovery.md (40 min)
36. architecture/reliability/versioning-strategy.md (55 min)

**Day 14:**
37. architecture/performance/performance-characteristics.md (35 min)
38. architecture/performance/advanced-optimization.md (50 min)

**Day 15:**
39. architecture/integration/multiplane-interactions.md (30 min)
40. architecture/integration/extension-points.md (30 min)
41. architecture/integration/integration-patterns.md (25 min)

**Day 16:**
42. architecture/decisions/design-decisions.md (35 min)
43. architecture/decisions/anti-patterns.md (30 min)
44. architecture/decisions/state-management.md (25 min)

**Day 17:**
45. architecture/observability/observability-model.md (50 min)
46. adrs/ADR-009-federation-architecture.md (40 min)

### Week 5: Reference (As Needed)

47. reference/scalars.md (reference)
48. reference/where-operators.md (reference)

---

## Quick Reference: Document Lengths & Time Estimates

| Document | Lines | Est. Time | Depth |
|----------|-------|-----------|-------|
| **Meta** |
| README.md | 1,000+ | 5 min | Overview |
| GLOSSARY.md | 800+ | 30 min | Reference |
| reading-order.md (this) | 731 | 15 min | Navigation |
| **PRD** |
| prd/PRD.md | 1,100+ | 60 min | High-level |
| **Architecture: Core** |
| architecture/core/compilation-pipeline.md | 1,024 | 60 min | Deep |
| architecture/core/compilation-phases.md | 1,597 | 50 min | Deep |
| architecture/core/execution-model.md | 1,235 | 90 min | Deep |
| architecture/core/execution-semantics.md | 1,488 | 50 min | Deep |
| architecture/core/authoring-languages.md | 903 | 30 min | Deep |
| architecture/core/compilation-vs-runtime.md | 425 | 20 min | Medium |
| **Architecture: Database** |
| architecture/database/database-targeting.md | 644 | 30 min | Deep |
| architecture/database/arrow-plane.md | 1,756 | 60 min | Deep |
| **Architecture: Integration** |
| architecture/integration/federation.md | 2,537 | 80 min | Deep |
| architecture/integration/multiplane-interactions.md | 793 | 30 min | Medium |
| architecture/integration/extension-points.md | 783 | 30 min | Medium |
| architecture/integration/integration-patterns.md | 724 | 25 min | Medium |
| **Architecture: Realtime** |
| architecture/realtime/subscriptions.md | 1,618 | 55 min | Deep |
| **Architecture: Security** |
| architecture/security/security-model.md | 1,131 | 40 min | Deep |
| architecture/security/authentication-detailed.md | 1,853 | 60 min | Deep |
| **Architecture: Reliability** |
| architecture/reliability/consistency-model.md | 724 | 25 min | Medium |
| architecture/reliability/error-handling-model.md | 954 | 35 min | Medium |
| architecture/reliability/failure-modes-and-recovery.md | 1,136 | 40 min | Deep |
| architecture/reliability/versioning-strategy.md | 1,557 | 55 min | Deep |
| **Architecture: Performance** |
| architecture/performance/performance-characteristics.md | 977 | 35 min | Medium |
| architecture/performance/advanced-optimization.md | 1,483 | 50 min | Deep |
| **Architecture: Decisions** |
| architecture/decisions/design-decisions.md | 973 | 35 min | Medium |
| architecture/decisions/anti-patterns.md | 819 | 30 min | Medium |
| architecture/decisions/state-management.md | 694 | 25 min | Medium |
| **Architecture: Observability** |
| architecture/observability/observability-model.md | 1,369 | 50 min | Deep |
| **Core Specifications** |
| specs/compiled-schema.md | 1,200+ | 40 min | Deep |
| specs/authoring-contract.md | 1,500+ | 60 min | Deep |
| specs/schema-conventions.md | 850+ | 50 min | Medium |
| specs/cdc-format.md | 900+ | 45 min | Medium |
| specs/capability-manifest.md | 731 | 25 min | Medium |
| specs/pagination-keyset.md | 710 | 25 min | Medium |
| **Production Features** |
| specs/caching.md | 450+ | 30 min | Medium |
| specs/persisted-queries.md | 1,143+ | 60 min | Deep |
| specs/security-compliance.md | 750+ | 40 min | Medium |
| specs/introspection.md | 400+ | 25 min | Light |
| **Enterprise** |
| enterprise/rbac.md | 1,200+ | 60 min | Deep |
| enterprise/audit-logging.md | 1,200+ | 60 min | Deep |
| enterprise/kms.md | 1,100+ | 50 min | Medium |
| **Guides** |
| guides/monitoring.md | 1,108 | 50 min | Medium |
| guides/observability.md | 966 | 35 min | Medium |
| guides/production-deployment.md | 958 | 60 min | Medium |
| guides/testing-strategy.md | 2,454 | 80 min | Deep |
| **ADRs** |
| adrs/ADR-009-federation-architecture.md | 800+ | 40 min | Deep |
| **Reference** |
| reference/scalars.md | 900+ | Reference | Reference |
| reference/where-operators.md | 1,200+ | Reference | Reference |

---

## Tips for Effective Reading

### 1. Start with Your Role Path
Don't try to read everything at once. Follow the path for your role first.

### 2. Keep GLOSSARY.md Open
Reference terminology as you read. Most confusion comes from undefined terms.

### 3. Skim First, Deep Dive Later
Read document headers and summaries first. Deep dive only when needed.

### 4. Follow Cross-References
Specs link to related specs. Follow links when concepts aren't clear.

### 5. Use Real Examples
Try building a simple schema as you read. Learning by doing reinforces concepts.

### 6. Ask Questions
If documentation is unclear, file an issue. Documentation improves through feedback.

---

## Document Dependencies

**Reading order matters** for some documents. Here's the dependency graph:

```
README.md
    ↓
prd/PRD.md
    ↓
    ├─→ architecture/database/database-targeting.md
    │       ↓
    │       └─→ architecture/database/arrow-plane.md
    │
    ├─→ architecture/core/authoring-languages.md
    │       ↓
    │       └─→ architecture/core/compilation-vs-runtime.md
    │
    └─→ architecture/core/compilation-pipeline.md
            ↓
            ├─→ architecture/core/compilation-phases.md
            └─→ specs/compiled-schema.md
                    ↓
                    ├─→ specs/capability-manifest.md
                    └─→ architecture/core/execution-model.md
                            ↓
                            ├─→ architecture/core/execution-semantics.md
                            ├─→ specs/caching.md
                            ├─→ specs/persisted-queries.md
                            ├─→ specs/security-compliance.md
                            ├─→ specs/introspection.md
                            ├─→ architecture/realtime/subscriptions.md
                            └─→ architecture/performance/performance-characteristics.md
                                    ↓
                                    └─→ architecture/performance/advanced-optimization.md

prd/PRD.md
    ↓
specs/schema-conventions.md
    ↓
    ├─→ specs/cdc-format.md
    ├─→ specs/pagination-keyset.md
    └─→ enterprise/audit-logging.md

specs/authoring-contract.md
    ↓
    ├─→ reference/scalars.md
    └─→ reference/where-operators.md

specs/security-compliance.md
    ↓
    ├─→ architecture/security/security-model.md
    │       ↓
    │       └─→ architecture/security/authentication-detailed.md
    ├─→ enterprise/rbac.md
    ├─→ enterprise/audit-logging.md
    └─→ enterprise/kms.md

architecture/core/execution-model.md
    ↓
    ├─→ architecture/reliability/consistency-model.md
    ├─→ architecture/reliability/error-handling-model.md
    │       ↓
    │       └─→ architecture/reliability/failure-modes-and-recovery.md
    ├─→ guides/monitoring.md
    │       ↓
    │       └─→ architecture/observability/observability-model.md
    ├─→ guides/observability.md
    ├─→ guides/production-deployment.md
    └─→ guides/testing-strategy.md

architecture/integration/federation.md
    ↓
    ├─→ architecture/integration/multiplane-interactions.md
    ├─→ architecture/integration/extension-points.md
    ├─→ architecture/integration/integration-patterns.md
    └─→ adrs/ADR-009-federation-architecture.md

architecture/decisions/design-decisions.md
    ↓
    ├─→ architecture/decisions/anti-patterns.md
    └─→ architecture/decisions/state-management.md

architecture/reliability/versioning-strategy.md (standalone, recommended after reliability docs)
```

**Key insight:** Start with README → PRD → GLOSSARY, then choose your path based on dependencies.

---

## Feedback

This reading order guide is a living document. If you find:
- A path doesn't make sense for your role
- A document should be read earlier/later
- Time estimates are inaccurate
- Key documents are missing from a path

Please file an issue or submit a PR.

---

*End of Reading Order Guide*
