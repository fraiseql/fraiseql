<!-- Skip to main content -->
---
title: FraiseQL v2 Guides
description: Practical how-to guides for operators, developers, and DevOps teams.
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# FraiseQL v2 Guides

Practical how-to guides for operators, developers, and DevOps teams.

---

## 🚀 Getting Started

- **[Language Generators](language-generators.md)** — Schema authoring in Python, TypeScript, Go, Java, PHP
- **[Patterns](patterns.md)** — Common schema design patterns and best practices

## 🎯 Evaluation & Decision Making

**Before you start building:**

- **[Choosing FraiseQL](choosing-fraiseql.md)** — Is FraiseQL right for your project? Use case analysis and decision matrix
- **[Consistency Model](consistency-model.md)** — Understand FraiseQL's CAP theorem choice (CP: Consistency + Partition Tolerance)

## 🛠️ Development Guides

### Testing & Profiling

- **[Testing Strategy](testing-strategy.md)** — Unit, integration, E2E, and performance testing
- **[E2E Testing](development/e2e-testing.md)** — End-to-end testing with real services
- **[Profiling Guide](development/profiling-guide.md)** — Profile and optimize code
- **[Benchmarking](development/benchmarking.md)** — Performance benchmarking with Criterion

### Code Quality

- **[Linting](development/linting.md)** — Code quality and linting standards
- **[Test Coverage](development/test-coverage.md)** — Measure and improve test coverage
- **[Developer Guide](development/developer-guide.md)** — Development environment setup

## 📊 Operations & Monitoring

- **[Deployment Guide](../deployment/)** — Deploy FraiseQL (local, Docker, Kubernetes)
- **[Production Deployment](production-deployment.md)** — Enterprise-scale Kubernetes deployments
- **[Monitoring](monitoring.md)** — Prometheus metrics and OpenTelemetry tracing
- **[Observability](observability.md)** — Logging, tracing, and metrics best practices

## 🔔 Event-Driven Architecture

- **[Observers & Webhooks](observers.md)** — Event-driven actions on database changes
- **[DDL Generation Guide](ddl-generation-guide.md)** — Generate schema from existing databases

## 🔗 Integrations

See [Integrations Guide](../integrations/) for:

- **Federation** — Multi-database composition with SAGA patterns
- **Authentication** — Auth0, Google, Keycloak, SCRAM setup
- **Arrow Flight** — High-performance analytics integration
- **Monitoring** — Grafana dashboards and alerting

## 📚 Analytics & View Selection

- **[Analytics Patterns](analytics-patterns.md)** — Common analytical query patterns
- **[Arrow Flight Integration](../integrations/arrow-flight/)** — High-performance analytics and BI tool integration
- **[View Selection Quick Reference](view-selection-quick-reference.md)** — Quick guide to view patterns
- **[View Selection Performance Testing](view-selection-performance-testing.md)** — Benchmark view selection strategies
- **[View Selection Migration Checklist](view-selection-migration-checklist.md)** — Migrate existing views to FraiseQL patterns

---

## 🎯 By Use Case

**I want to...**

- **Evaluate if FraiseQL is right for me** → [Choosing FraiseQL](choosing-fraiseql.md)
- **Understand consistency guarantees** → [Consistency Model](consistency-model.md)
- **Get started quickly** → [Language Generators](language-generators.md)
- **Design a schema** → [Patterns](patterns.md)
- **Deploy to production** → [Production Deployment](production-deployment.md)
- **Set up monitoring** → [Monitoring](monitoring.md)
- **Test my code** → [Testing Strategy](testing-strategy.md)
- **Integrate with Auth0** → [Auth0 Setup](../integrations/authentication/setup-auth0.md)
- **Set up federation** → [Federation Guide](../integrations/federation/guide.md)

---

## 📚 Related Documentation

- **[Architecture](../architecture/)** — Deep dive into FraiseQL design
- **[Specifications](../specs/)** — Complete API and feature specifications
- **[Operations](../operations/)** — Day-to-day operations and troubleshooting
- **[Configuration](../configuration/)** — Security and operational configuration
- **[Enterprise](../enterprise/)** — RBAC, audit logging, KMS

---

## 📋 Document Metadata Guide

All guides in this directory follow a consistent metadata format for discoverability and context:

```markdown
<!-- Code example in MARKDOWN -->
| Metadata | Values | Example |
|----------|--------|---------|
| **Status** | ✅ Production Ready, ⚠️ Beta, 📝 Draft | ✅ Production Ready |
| **Audience** | Developers, DevOps, DBAs, Architects, SREs | Developers, Architects |
| **Reading Time** | Estimated minutes | 10-15 minutes |
| **Last Updated** | YYYY-MM-DD | 2026-02-05 |
```text
<!-- Code example in TEXT -->

**What these mean:**

- **Status**: Indicates feature stability and support level
  - ✅ **Production Ready** - Fully tested, supported, recommended for production use
  - ⚠️ **Beta** - Functional but may have breaking changes, use in staging first
  - 📝 **Draft** - Under development, may be incomplete or unstable

- **Audience**: Who should read this guide
  - **Developers** - Application engineers building with FraiseQL
  - **DevOps** - Infrastructure and deployment specialists
  - **DBAs** - Database administrators
  - **Architects** - System architects and technical leads
  - **SREs** - Site reliability engineers

- **Reading Time**: Estimated time to read the full guide
  - Quick references (3-5 minutes)
  - Quick starts (5-10 minutes)
  - Detailed guides (15-30 minutes)
  - Deep dives (30+ minutes)

---

**Back to:** [Documentation Home](../README.md)
