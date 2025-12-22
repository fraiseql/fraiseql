# FraiseQL Documentation

FraiseQL is a PostgreSQL-native GraphQL framework for Python. Build type-safe, production-ready APIs without boilerplate.

## Getting Started

New to FraiseQL? Start here:

- **[5-Minute Quickstart](getting-started/quickstart.md)** - Get running in minutes
- **[Installation](getting-started/installation.md)** - Setup instructions
- **[First Hour Guide](getting-started/first-hour.md)** - Learn the fundamentals
- **[Core Concepts](core/concepts-glossary.md)** - Essential mental models

## Learn by Example

See FraiseQL in action:

- **[Blog API Tutorial](tutorials/blog-api.md)** - Build a complete API from scratch
- **[Filtering Examples](examples/advanced-filtering.md)** - Query patterns and use cases
- **[RAG Tutorial](ai-ml/rag-tutorial.md)** - Build AI search with pgvector
- **[Error Handling Examples](guides/error-handling-patterns.md)** - Robust error management
- **[Production Deployment](tutorials/production-deployment.md)** - Deploy safely

## Core Features

FraiseQL provides everything you need for modern APIs:

### pgvector Integration

Native PostgreSQL vector search for semantic search and RAG applications.
- Type-safe GraphQL integration with vector operators
- **[Learn more →](features/pgvector.md)**

### GraphQL Cascade

Automatic, intelligent cache invalidation that works with your data relationships.
- Zero manual cache management
- **[Learn more →](features/graphql-cascade.md)** | **[Best Practices →](guides/cascade-best-practices.md)**

### LangChain Integration

Build AI-powered applications with document ingestion and semantic search.
- Production-ready patterns for RAG applications
- **[Learn more →](features/langchain-integration.md)**

### LLM Integration

Use LLMs directly in your GraphQL resolvers.
- Type-safe LLM calling from Python
- **[Learn more →](features/llm-integration.md)**

## Guides

Common tasks and patterns:

- **[Filtering & Querying](guides/filtering.md)** - Query syntax and patterns
- **[Mutations & Data Changes](guides/mutation-sql-requirements.md)** - Writing database functions
- **[Authentication](advanced/authentication.md)** - Securing your API
- **[Multi-Tenancy](advanced/multi-tenancy.md)** - Tenant isolation patterns
- **[Performance & Optimization](performance/index.md)** - Make it fast
- **[Troubleshooting](guides/troubleshooting.md)** - Common issues and solutions

## Reference

API documentation and configuration:

- **[Database API](core/database-api.md)** - Query execution and methods
- **[Types & Schema](core/types-and-schema.md)** - Type system and schema definition
- **[Configuration](core/configuration.md)** - All configuration options
- **[Decorators](reference/decorators.md)** - Python decorators reference
- **[CLI](reference/cli.md)** - Command-line tools

## Architecture

How FraiseQL works under the hood:

- **[Architecture Overview](architecture/README.md)** - System design
- **[Mutation Pipeline](architecture/mutation-pipeline.md)** - How mutations execute
- **[Rust Pipeline](performance/rust-pipeline-optimization.md)** - Performance optimizations
- **[Key Decisions](architecture/decisions/README.md)** - Design rationale

## Deploy to Production

Get your API live:

- **[Deployment Guide](production/deployment.md)** - Deploying FraiseQL
- **[Monitoring](production/monitoring.md)** - Track and debug
- **[Health Checks](production/health-checks.md)** - Readiness and liveness
- **[Security](production/security.md)** - Secure your API
- **[Performance Tips](performance/index.md)** - Optimize for production

## Contributing

- **[Contributing Guide](../CONTRIBUTING.md)** - How to contribute
- **[Development Style Guide](development/style-guide.md)** - Code standards
