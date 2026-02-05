# FraiseQL v2 Documentation

**Version:** 2.0.0-alpha.1
**Status:** Alpha release - Ready for community testing
**Last Updated:** February 5, 2026

> ⚠️ **ALPHA RELEASE**: This documentation covers v2.0.0-alpha.1. Expect some features to evolve before GA (April 2026). See [ALPHA_LIMITATIONS.md](ALPHA_LIMITATIONS.md) for what's deferred. New to alpha? Start with the [Alpha Testing Guide](ALPHA_TESTING_GUIDE.md).

---

## 🚀 Quick Start

**New to FraiseQL?** Start here:

1. Read the main [README.md](../README.md) (5 minutes)
2. **[Alpha Testing Guide](ALPHA_TESTING_GUIDE.md)** ⭐ — How to test and provide feedback
3. **[Alpha Limitations](ALPHA_LIMITATIONS.md)** — What's not in this release
4. Follow the [Reading Order Guide](reading-order.md) for your role
5. Bookmark the [Glossary](GLOSSARY.md) for reference

---

## 📚 Documentation Structure

### Foundation **NEW!**

**Comprehensive foundation documentation covering core concepts and architecture** (12 topics, 10,000+ lines).

Perfect for developers new to FraiseQL or those wanting deep architectural understanding.

FraiseQL foundations documentation covers:

- What is FraiseQL? — Understanding FraiseQL's compiled GraphQL approach
- Core Concepts — Terminology and mental models
- Database-Centric Architecture — View types (v_*, tv_*, va_*, ta_*), fact tables, calendar dimensions
- Design Principles — Five principles guiding FraiseQL
- Comparisons — FraiseQL vs Apollo, Hasura, WunderGraph, REST
- Compilation Pipeline — Seven-phase compilation process
- Query Execution Model — Runtime query execution
- Data Planes Architecture — JSON (OLTP) vs Arrow (OLAP)
- Type System — Built-in scalars, relationships, type inference
- Error Handling — Error hierarchy and validation layers
- Compiled Schema Structure — schema.compiled.json format
- Performance Characteristics — Latency, throughput, scaling

---

### Arrow Flight Integration

High-performance columnar data delivery for analytics and cross-language integration.

See [integrations/arrow-flight/](integrations/arrow-flight/) for guides on:

- Overview and quick start
- System design and dual-dataplane architecture
- Step-by-step tutorial
- 4-phase adoption strategy
- Real-world performance metrics (10-50x improvements)

### Product Requirements

High-level vision, philosophy, and system requirements.

See [PRD.md](prd/PRD.md) for product requirements and design philosophy.

### Architecture

System architecture, design decisions, and technical specifications.

See [architecture/](architecture/) for comprehensive documentation including:

**Core Compilation & Execution:**
- Compilation and execution fundamentals
- Database targeting and Arrow support
- View selection guide (v_*, tv_*, va_*, ta_* patterns)
- Table pattern optimization (JSON views and columnar views)

**System Qualities:**
- Reliability — Consistency, error handling, failure modes
- Security — Security model and authentication
- Performance — Optimization and performance characteristics
- Observability — Monitoring and instrumentation model

**Advanced Topics:**
- Federation, extension points, and integration patterns
- Subscriptions and event streaming
- Architectural decisions and patterns

### [Specifications](specs/)

Detailed technical specifications for implementers.

- Compilation artifacts (CompiledSchema, AuthoringContract, Capability Manifest)
- Runtime features (Caching, Persisted Queries, Introspection, Pagination)
- Data formats (CDC, Schema Conventions)
- Security & Compliance

### [Guides](guides/)

Practical how-to guides for operators, developers, and DevOps teams.

- **Evaluation**: ⭐ **[Choosing FraiseQL](guides/choosing-fraiseql.md)** — Should you use FraiseQL? Use case analysis and decision matrix
- **Architecture**: ⭐ **[Consistency Model](guides/consistency-model.md)** — Understanding FraiseQL's CAP theorem choice (Consistency + Partition Tolerance)
- **Getting Started**: [Language Generators](guides/language-generators.md), [Patterns](guides/PATTERNS.md)
- **Deployment**: [Production Deployment](guides/production-deployment.md) — Kubernetes deployment
- **Operations**: [Monitoring](guides/monitoring.md), [Observability](guides/observability.md), [Analytics Patterns](guides/analytics-patterns.md)
- **Development**: [Testing Strategy](guides/testing-strategy.md), [Benchmarking](guides/development/benchmarking.md), [Profiling](guides/development/PROFILING_GUIDE.md)

### [Configuration](configuration/)

Configuration reference for security, networking, and operations.

- [Security Configuration](configuration/SECURITY_CONFIGURATION.md) — Security settings overview
- [TLS/SSL Configuration](configuration/TLS_CONFIGURATION.md) — HTTPS and mutual TLS
- [Rate Limiting](configuration/RATE_LIMITING.md) — Brute-force protection
- [PostgreSQL Authentication](configuration/POSTGRESQL_AUTHENTICATION.md) — Database connection

### [Deployment](deployment/)

Deployment guides for various environments.

- [Production Deployment](deployment/guide.md) — Enterprise-scale deployments
- [Database Migration](deployment/migration-projection.md) — Migrate existing schemas

### [Operations](operations/)

Day-to-day operations, monitoring, and maintenance.

- [Operations Guide](operations/guide.md) — Production operations and maintenance
- [Observability](operations/observability.md) — Monitoring and observability setup
- [Distributed Tracing](operations/distributed-tracing.md) — Trace collection
- [Health Checks](operations/reference/health-checks.md) — Health check patterns
- [Metrics Reference](operations/reference/metrics.md) — Prometheus metrics

### [Integrations](integrations/)

Integration guides for external services and databases.

- **[Federation](integrations/federation/)** — Multi-database composition with SAGA patterns
- **[Authentication](integrations/authentication/)** — Auth0, Google, Keycloak, SCRAM
- **[Arrow Flight](integrations/arrow-flight/)** — High-performance analytics

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
- **Evaluate for adoption** → [PRD](prd/PRD.md) + [Architecture Guide](architecture/)
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

- **Total Documents:** 170+ organized files
- **Total Lines:** ~60,000 lines of documentation
- **Estimated Reading Time:** 15-20 hours (complete path)
- **Organized Into:** 22 directories with clear structure
- **Last Updated:** February 1, 2026
- **Latest Restructuring:** Full documentation reorganization for clarity and navigation

---

## 🤝 Contributing

Found an issue or have suggestions?

- File an issue in the repository
- Documentation feedback is always welcome
- See unclear sections? Let us know!

---

**Next:** Choose a [reading path](reading-order.md) or explore a specific [topic](#documentation-structure).
