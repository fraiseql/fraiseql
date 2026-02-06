<!-- Skip to main content -->
---

title: Federation
description: Multi-database query composition with SAGA pattern consistency.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# Federation

Multi-database query composition with SAGA pattern consistency.

## Quick Start

New to federation? Start here:

1. **[Federation Guide](guide.md)** — Overview and quick start (20 minutes)
2. **[API Reference](api-reference.md)** — Complete API documentation
3. **[Deployment](deployment.md)** — Deploy federated systems
4. **[Readiness Checklist](readiness-checklist.md)** — Pre-deployment verification

## Core Concepts

Federation enables querying across multiple databases as if they were one:

```graphql
<!-- Code example in GraphQL -->
query {
  users {
    id
    name
    posts {          # May come from different database
      id
      title
      comments {    # May come from third database
        text
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

### Architecture

- **Direct Database Federation** — Connect directly to target databases (no HTTP gateway)
- **SAGA Pattern** — Distributed transactions for cross-database consistency
- **Real-time Composition** — Compose queries at query time
- **Multi-Database Support** — PostgreSQL, MySQL, SQLite, SQL Server

## Documentation

### Guides

- **[Quick Start Guide](guide.md)** — Get started with federation
- **[Deployment Guide](deployment.md)** — Deploy to production
- **[SAGA Patterns](sagas.md)** — Distributed transaction patterns

### Reference

- **[API Reference](api-reference.md)** — Complete Python, TypeScript, Rust API
- **[Readiness Checklist](readiness-checklist.md)** — Pre-deployment verification

### Operations

- **[Observability](operations/observability.md)** — Monitor federation queries
- **[Runbooks](operations/runbooks.md)** — Troubleshooting runbooks

## Architecture Overview

```text
<!-- Code example in TEXT -->
┌─────────────────┐
│  FraiseQL       │
│  Compiler       │
└────────┬────────┘
         │ (compiles)
         ↓
┌─────────────────────────────────────────┐
│ schema.compiled.json                    │
│ (contains federation targets)           │
└────────┬────────────────────────────────┘
         │
         ↓
┌──────────────────────────────────────────────────┐
│ Runtime: Federation Planner                      │
│ • Analyzes query across schema scopes            │
│ • Builds execution plan with joins               │
│ • Coordinates SAGA transactions                  │
└────────┬─────────────────────────────────────────┘
         │
         ↓
    ┌────────┴─────────┬───────────────┬────────────┐
    ↓                  ↓               ↓            ↓
┌─────────┐      ┌──────────┐    ┌─────────┐  ┌─────────┐
│PostgreSQL│     │ MySQL    │    │SQLite   │  │SQL Srv  │
│   DB #1  │     │  DB #2   │    │ DB #3   │  │ DB #4   │
└─────────┘     └──────────┘    └─────────┘  └─────────┘
```text
<!-- Code example in TEXT -->

## When to Use Federation

✅ **Use federation when**:

- Multiple databases with different data domains
- Need transactional consistency across databases
- Want transparent query composition

❌ **Don't use federation when**:

- Single database (use direct schema)
- Need extreme performance (use analytical Arrow Flight instead)

## Performance Characteristics

- **Latency**: 10-100ms for cross-database queries (network-bound)
- **Throughput**: 100-1000 qps depending on database load
- **Consistency**: Eventual with SAGA pattern

See [Operations: Observability](operations/observability.md) for monitoring.

## Support

- **Troubleshooting**: See [Runbooks](operations/runbooks.md)
- **API Help**: See [API Reference](api-reference.md)
- **Configuration**: See [Deployment Guide](deployment.md)

---

**Version**: v2.0.0
**Last Updated**: February 1, 2026
