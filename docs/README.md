# FraiseQL Documentation

FraiseQL is a PostgreSQL-native GraphQL framework for Python. Build type-safe, production-ready APIs without boilerplate.

## Getting Started

New to FraiseQL? Start here:

- **[5-Minute Quickstart](getting-started/quickstart/)** - Get running in minutes
- **[Installation](getting-started/installation/)** - Setup instructions
- **[First Hour Guide](getting-started/first-hour/)** - Learn the fundamentals
- **[Core Concepts](core/concepts-glossary/)** - Essential mental models

## Learn by Example

See FraiseQL in action:

- **[Blog API Tutorial](tutorials/blog-api/)** - Build a complete API from scratch
- **[Filtering Examples](examples/advanced-filtering/)** - Query patterns and use cases
- **[RAG Tutorial](ai-ml/rag-tutorial/)** - Build AI search with pgvector
- **[Error Handling Examples](guides/error-handling-patterns/)** - Robust error management
- **[Production Deployment](tutorials/production-deployment/)** - Deploy safely

## Core Features

FraiseQL provides everything you need for modern APIs:

### pgvector Integration

Native PostgreSQL vector search for semantic search and RAG applications.
- Type-safe GraphQL integration with vector operators
- **[Learn more →](features/pgvector/)**

### GraphQL Cascade

Automatic, intelligent cache invalidation that works with your data relationships.
- Zero manual cache management
- **[Learn more →](features/graphql-cascade/)** | **[Best Practices →](guides/cascade-best-practices/)**

### LangChain Integration

Build AI-powered applications with document ingestion and semantic search.
- Production-ready patterns for RAG applications
- **[Learn more →](guides/langchain-integration/)**

### LLM Integration

Use LLMs directly in your GraphQL resolvers.
- Type-safe LLM calling from Python
- **[Learn more →](features/ai-native/)**

## Guides

Common tasks and patterns:

- **[Filtering & Querying](guides/filtering/)** - Query syntax and patterns
- **[Mutations & Data Changes](guides/mutation-sql-requirements/)** - Writing database functions
- **[Authentication](advanced/authentication/)** - Securing your API
- **[Multi-Tenancy](advanced/multi-tenancy/)** - Tenant isolation patterns
- **[Performance & Optimization](performance/index/)** - Make it fast
- **[Troubleshooting](guides/troubleshooting/)** - Common issues and solutions

## Reference

API documentation and configuration:

- **[Database API](core/database-api/)** - Query execution and methods
- **[Types & Schema](core/types-and-schema/)** - Type system and schema definition
- **[Configuration](core/configuration/)** - All configuration options
- **[Decorators](reference/decorators/)** - Python decorators reference
- **[CLI](reference/cli/)** - Command-line tools

## Architecture

How FraiseQL works under the hood:

- **[Architecture Overview](architecture/README/)** - System design
- **[Mutation Pipeline](architecture/mutation-pipeline/)** - How mutations execute
- **[Rust Pipeline](performance/rust-pipeline-optimization/)** - Performance optimizations
- **[Key Decisions](architecture/decisions/README/)** - Design rationale

## Deploy to Production

Get your API live:

- **[Deployment Guide](production/deployment/)** - Deploying FraiseQL
- **[Monitoring](production/monitoring/)** - Track and debug
- **[Health Checks](production/health-checks/)** - Readiness and liveness
- **[Security](production/security/)** - Secure your API
- **[Performance Tips](performance/index/)** - Optimize for production

## Contributing

- **[Contributing Guide](../CONTRIBUTING.md)** - How to contribute
- **[Development Style Guide](guides/common-mistakes/)** - Code standards and best practices
