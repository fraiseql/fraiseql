# FraiseQL Architecture Documentation

This directory contains the authoritative architecture documentation for FraiseQL v2.

## What to Read and When

| I want to... | Read |
|-------------|------|
| Understand the big picture (3-layer model, design principles) | [overview.md](overview.md) |
| Understand how GraphQL compiles to SQL | [compiler.md](compiler.md) |
| Add a new database backend | [overview.md](overview.md) — "Database Abstraction" section |
| Understand auth, RLS, and the security model | [overview.md](overview.md) — "Security Model" section |
| Understand the test strategy | [../testing.md](../testing.md) (when created) or CONTRIBUTING.md |
| Understand operational concerns (schema lifecycle, idempotency) | [../operations/](../operations/) |
| See which features each database supports | [../database-compatibility.md](../database-compatibility.md) (when created) |

## Summary

**FraiseQL v2** is a compiled GraphQL execution engine that transforms schema definitions into
optimized SQL at build time. The architecture has three distinct layers:

```
Authoring               Compilation              Runtime
(Python/TS/C#/etc.)    (Rust CLI)               (Rust Server)
      ↓                      ↓                        ↓
schema.json    +    fraiseql.toml      →    schema.compiled.json    →    GraphQL Server
```

- **[overview.md](overview.md)** — The 3-layer model, database abstraction, security model,
  error handling, testing strategy, and key architectural decisions.

- **[compiler.md](compiler.md)** — The GraphQL→SQL compilation pipeline, AST representation,
  SQL template design, and query classification in detail.

## Architecture Decision Records

Individual architectural decisions are recorded in [`../adr/`](../adr/).
