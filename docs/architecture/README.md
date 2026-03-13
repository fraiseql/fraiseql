# FraiseQL Architecture Documentation

This directory contains the authoritative architecture documentation for FraiseQL v2.

## What to Read and When

| I want to... | Read |
|-------------|------|
| Understand the big picture (3-layer model, design principles) | [overview.md](overview.md) |
| Understand how GraphQL compiles to SQL | [compiler.md](compiler.md) |
| **Write an SDK** (understand the schema.json contract) | [intermediate-schema.md](intermediate-schema.md) |
| Add a new database backend | [overview.md](overview.md) — "Database Abstraction" section |
| Understand auth, RLS, and the security model | [overview.md](overview.md) — "Security Model" section |
| Understand the test strategy | [../testing.md](../testing.md) |
| Understand cache sharding, TTL, and invalidation | [../modules/cache.md](../modules/cache.md) |
| Understand window function compilation | [../modules/window-functions.md](../modules/window-functions.md) |
| Understand analytics fact table design and caching | [../modules/fact-table.md](../modules/fact-table.md) |
| Understand the streaming wire protocol engine | [wire-protocol.md](wire-protocol.md) |
| Understand inbound webhooks vs. outbound observers | [webhooks.md](webhooks.md) |
| Understand operational concerns (schema lifecycle, idempotency) | [../operations/](../operations/) |
| See which features each database supports | [../database-compatibility.md](../database-compatibility.md) |

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

- **[intermediate-schema.md](intermediate-schema.md)** — The `schema.json` contract between
  SDK authoring tools (Python, TypeScript, C#, Go, Rust) and `fraiseql-cli compile`. Reference
  for SDK authors: all 32 structs, field semantics, versioning policy, and a minimal example.

- **[wire-protocol.md](wire-protocol.md)** — The `fraiseql-wire` streaming engine: purpose,
  how it differs from the database adapter, and when to use it.

## Module Orientation Guides

For the three most complex modules, read the orientation docs before opening source files:

- **[../modules/README.md](../modules/README.md)** — Navigation index
- **[../modules/cache.md](../modules/cache.md)** — Cache sharding, key security, TTL, cascade invalidation
- **[../modules/window-functions.md](../modules/window-functions.md)** — 3-stage pipeline, dialect table, adding new functions
- **[../modules/fact-table.md](../modules/fact-table.md)** — `tf_*` pattern, introspection flow, JSONB sampling

## Architecture Decision Records

Individual architectural decisions are recorded in [`../adr/`](../adr/).
