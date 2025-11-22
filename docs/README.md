# FraiseQL Documentation

## Getting Started

- **[5-Minute Quickstart](getting-started/quickstart.md)** - Fastest way to get running
- **[First Hour Guide](getting-started/first-hour.md)** - Progressive tutorial
- **[Understanding FraiseQL](guides/understanding-fraiseql.md)** - Conceptual overview
- **[Installation](getting-started/installation.md)** - Detailed setup instructions

## v1.5.0 Highlights

FraiseQL v1.5.0 brings AI-ready capabilities and enterprise cache management:

### pgvector Integration

Native PostgreSQL vector similarity search for RAG & semantic search applications.

- 6 distance operators (cosine, L2, inner product, L1, Hamming, Jaccard)
- HNSW and IVFFlat index support
- Full GraphQL integration with type-safe filters
- **[Get Started with pgvector →](features/pgvector.md)**

### GraphQL Cascade

Automatic cache invalidation that propagates when related data changes.

- Auto-detection from GraphQL schema
- Apollo Client and Relay integration
- Zero manual cache management
- **[Learn about Cascade →](features/graphql-cascade.md)**

### LangChain Integration

Build RAG applications with LangChain and FraiseQL.

- Document ingestion and vector storage
- Semantic search and question answering
- Production-ready patterns
- **[Build a RAG App →](guides/langchain-integration.md)**

## Feature Discovery

- **[Feature Matrix](features/index.md)** - Complete overview of all FraiseQL capabilities
  - Core features, database features, advanced queries
  - AI & Vector features (pgvector, LangChain, LLM integration)
  - Security, enterprise, real-time, monitoring

## Core Concepts

**New to FraiseQL?** Start with these essential concepts:

- **[Concepts & Glossary](core/concepts-glossary.md)** - Core terminology and mental models
  - CQRS Pattern - Separate reads (views) from writes (functions)
  - Trinity Identifiers - Three-tier ID system for performance and UX
  - JSONB Views vs Table Views - When to use `v_*` vs `tv_*`
  - Database-First Architecture - PostgreSQL composes, GraphQL exposes
  - Explicit Sync Pattern - Denormalized tables for complex queries

- **[Types and Schema](core/types-and-schema.md)** - Complete guide to FraiseQL's type system
- **[Database API](core/database-api.md)** - PostgreSQL integration and query execution
- **[Configuration](core/configuration.md)** - Application configuration reference

## Querying & Filtering

FraiseQL provides flexible filtering with two syntaxes:

- **[Filtering Guide](guides/filtering.md)** - Unified entry point for all filtering documentation
- **[Where Input Types](advanced/where_input_types.md)** - Type-safe WhereType deep dive
- **[Filter Operators Reference](advanced/filter-operators.md)** - Complete operator documentation
- **[Syntax Comparison](reference/where-clause-syntax-comparison.md)** - Side-by-side cheat sheet
- **[Advanced Examples](examples/advanced-filtering.md)** - Real-world use cases

## Advanced Features

- [Advanced Patterns](advanced/advanced-patterns.md)
- [Authentication](advanced/authentication.md)
- [Multi-Tenancy](advanced/multi-tenancy.md)
- [Database Patterns](advanced/database-patterns.md)
- [LLM Integration](advanced/llm-integration.md)

## Performance

- [Performance Guide](performance/index.md)
- [APQ Optimization](performance/apq-optimization-guide.md)
- [Rust Pipeline](performance/rust-pipeline-optimization.md)
- [CASCADE Performance](performance/cascade-invalidation.md)

## Reference

- [Quick Reference](reference/quick-reference.md)
- [Configuration Reference](reference/config.md)
- [Type Operator Architecture](architecture/type-operator-architecture.md)

## Guides

- [Troubleshooting](guides/troubleshooting.md)
- [Troubleshooting Decision Tree](guides/troubleshooting-decision-tree.md)
- [Cascade Best Practices](guides/cascade-best-practices.md)
- [Migrating to Cascade](guides/migrating-to-cascade.md)

## Development

- [Contributing](../CONTRIBUTING.md)
- [Style Guide](development/style-guide.md)
- [Architecture Decisions](architecture/README.md)
