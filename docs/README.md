# FraiseQL v2 Documentation

**Version:** 1.0
**Status:** Complete
**Last Updated:** January 18, 2026

---

## 🚀 Quick Start

**New to FraiseQL?** Start here:

1. Read the main [README.md](../README.md) (5 minutes)
2. Follow the [Reading Order Guide](reading-order.md) for your role
3. Bookmark the [Glossary](GLOSSARY.md) for reference

---

## 📚 Documentation Structure

### [Product Requirements](prd/)

High-level vision, philosophy, and system requirements.

- **[PRD.md](prd/PRD.md)** — Complete product requirements and design philosophy

### [Architecture](architecture/)

System architecture, design decisions, and technical specifications.

**Core Compilation & Execution:**

- [Core Pipeline](architecture/core/) — Compilation and execution fundamentals
- [Database Integration](architecture/database/) — Database targeting and Arrow support
  - [View Selection Guide](architecture/database/view-selection-guide.md) — Choose between v_*, tv_*, va_*, ta_*
  - [tv_* Table Pattern](architecture/database/tv-table-pattern.md) — Pre-computed JSON views for GraphQL
  - [ta_* Table Pattern](architecture/database/ta-table-pattern.md) — Optimized columnar views for analytics

**System Qualities:**

- [Reliability](architecture/reliability/) — Consistency, error handling, failure modes
- [Security](architecture/security/) — Security model and authentication
- [Performance](architecture/performance/) — Optimization and performance characteristics
- [Observability](architecture/observability/) — Monitoring and instrumentation model

**Advanced Topics:**

- [Integration](architecture/integration/) — Federation, extension points, integration patterns
- [Real-time](architecture/realtime/) — Subscriptions and event streaming
- [Design Decisions](architecture/decisions/) — Architectural decisions and patterns

### [Specifications](specs/)

Detailed technical specifications for implementers.

- Compilation artifacts (CompiledSchema, AuthoringContract, Capability Manifest)
- Runtime features (Caching, Persisted Queries, Introspection, Pagination)
- Data formats (CDC, Schema Conventions)
- Security & Compliance

### [Guides](guides/)

Practical how-to guides for operators and developers.

- [Testing Strategy](guides/testing-strategy.md) — Complete testing approach
- [Production Deployment](guides/production-deployment.md) — Kubernetes deployment
- [Monitoring](guides/monitoring.md) — Prometheus, OpenTelemetry, health checks
- [Observability](guides/observability.md) — Logging, tracing, metrics

**View Selection Guides**:

- [Quick Reference](guides/view-selection-quick-reference.md) — Quick decision matrix and cheat sheet
- [Migration Checklist](guides/view-selection-migration-checklist.md) — Step-by-step migration workflow
- [Performance Testing](guides/view-selection-performance-testing.md) — Methodology for validating performance improvements
- [DDL Generation Guide](guides/ddl-generation-guide.md) — Generate SQL for table-backed views

### [Enterprise Features](enterprise/)

Enterprise-grade features for production deployments.

- [RBAC](enterprise/rbac.md) — Role-based access control
- [Audit Logging](enterprise/audit-logging.md) — Cryptographic audit trails
- [KMS Integration](enterprise/kms.md) — Key management for field encryption

### [Reference](reference/)

Complete API and operator references.

- [Scalars](reference/scalars.md) — Scalar type library
- [WHERE Operators](reference/where-operators.md) — Query filter operators

### [Architecture Decision Records](adrs/)

Historical record of architectural decisions and rationale.

- [ADR-009: Federation Architecture](adrs/ADR-009-federation-architecture.md)

---

## 📖 Reading Paths

Not sure where to start? See the **[Reading Order Guide](reading-order.md)** for curated paths:

- 🆕 **[New to FraiseQL](reading-order.md#new-to-fraiseql-start-here)** (45 min)
- 🏗️ **[For Architects](reading-order.md#for-architects)** (3.5 hours)
- ⚙️ **[For Compiler Developers](reading-order.md#for-compiler-developers)** (4 hours)
- 🦀 **[For Runtime Developers](reading-order.md#for-runtime-developers)** (3 hours)
- 🗄️ **[For Database Architects](reading-order.md#for-database-architects)** (2.5 hours)
- 🚀 **[For DevOps](reading-order.md#for-operations--devops)** (3 hours)
- 🔒 **[For Security Engineers](reading-order.md#for-security-engineers)** (3 hours)
- 💻 **[For Frontend Developers](reading-order.md#for-frontend-developers)** (1.5 hours)

---

## 🔍 Quick Reference

| Topic | Document |
|-------|----------|
| **What is FraiseQL?** | [README.md](../README.md) |
| **Key Concepts** | [GLOSSARY.md](GLOSSARY.md) |
| **Design Philosophy** | [prd/PRD.md](prd/PRD.md) |
| **How Compilation Works** | [architecture/core/compilation-pipeline.md](architecture/core/compilation-pipeline.md) |
| **How Execution Works** | [architecture/core/execution-model.md](architecture/core/execution-model.md) |
| **Database Support** | [architecture/database/database-targeting.md](architecture/database/database-targeting.md) |
| **Security Model** | [architecture/security/security-model.md](architecture/security/security-model.md) |
| **Production Deployment** | [guides/production-deployment.md](guides/production-deployment.md) |
| **Testing** | [guides/testing-strategy.md](guides/testing-strategy.md) |

---

## 🎯 Documentation by Use Case

**I want to...**

- **Understand FraiseQL** → [Reading Order: New to FraiseQL](reading-order.md#new-to-fraiseql-start-here)
- **Evaluate for adoption** → [PRD](prd/PRD.md) + [Architecture: Core](architecture/core/)
- **Write schemas** → [Specs: Authoring Contract](specs/authoring-contract.md) + [Schema Conventions](specs/schema-conventions.md)
- **Build a compiler** → [Reading Order: Compiler Developers](reading-order.md#for-compiler-developers)
- **Extend the runtime** → [Reading Order: Runtime Developers](reading-order.md#for-runtime-developers)
- **Deploy to production** → [Guides: Production Deployment](guides/production-deployment.md)
- **Implement security** → [Enterprise: RBAC](enterprise/rbac.md) + [Security Model](architecture/security/security-model.md)
- **Optimize performance** → [Performance: Advanced Optimization](architecture/performance/advanced-optimization.md)
- **Add federation** → [Architecture: Integration/Federation](architecture/integration/federation.md)
- **Query from client** → [Reading Order: Frontend Developers](reading-order.md#for-frontend-developers)

---

## 📊 Documentation Statistics

- **Total Documents:** 48 files
- **Total Lines:** ~53,000 lines
- **Estimated Reading Time:** 12-15 hours (complete path)
- **Last Updated:** January 11, 2026

---

## 🤝 Contributing

Found an issue or have suggestions?

- File an issue in the repository
- Documentation feedback is always welcome
- See unclear sections? Let us know!

---

**Next:** Choose a [reading path](reading-order.md) or explore a specific [topic](#documentation-structure).
