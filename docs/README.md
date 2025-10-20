# FraiseQL Documentation

Enterprise-grade GraphQL framework built on PostgreSQL, FastAPI, and Strawberry. Delivers sub-millisecond response times through database-first architecture and CQRS pattern implementation.

**📍 You are here: Complete Documentation Reference**

**New to FraiseQL?** Start with **[Getting Started](../GETTING_STARTED.md)** for personalized guidance based on your goals.

## Quick Navigation

### **🎯 Choose Your Path**

**New to GraphQL/Python/PostgreSQL?**
🟢 **[Beginner Path](./tutorials/beginner-path.md)** → Complete learning journey (2-3 hours)

**Building production APIs?**
🟡 **[Production Guide](./production/index.md)** → Enterprise deployment & performance

**Contributing to FraiseQL?**
🔴 **[Contributor Guide](../CONTRIBUTING.md)** → Development setup & architecture

**Getting Started**
- [5-Minute Quickstart](./quickstart.md) - Build a working API in minutes
- [Beginner Learning Path](./tutorials/beginner-path.md) - Complete learning journey (2-3 hours)

**Tutorials** (3 hands-on guides)
- [Beginner Learning Path](./tutorials/beginner-path.md) - Zero to production in 2-3 hours
- [Blog API Tutorial](./tutorials/blog-api.md) - Complete blog with posts, comments, users (45 min)
- [Production Deployment](./tutorials/production-deployment.md) - Docker, monitoring, security (90 min)

**Core Concepts** (5 docs)
- [Types and Schema](./core/types-and-schema.md) - GraphQL type definitions and schema generation
- [Queries and Mutations](./core/queries-and-mutations.md) - Resolver patterns and execution
- [Database API](./core/database-api.md) - Repository patterns and query building
- [Configuration](./core/configuration.md) - Application setup and tuning
- [FraiseQL Philosophy](./core/fraiseql-philosophy.md) - Design principles and architecture decisions

**Performance** (3 docs)
- [Performance Optimization](./performance/index.md) - Complete optimization stack (Rust, APQ, TurboRouter, JSON Passthrough)
- [Result Caching](./performance/caching.md) - PostgreSQL-based result caching with automatic tenant isolation
- [Caching Migration](./performance/caching-migration.md) - Add caching to existing applications

**Advanced Patterns** (6 docs)
- [Authentication](./advanced/authentication.md) - Auth patterns and security
- [Multi-Tenancy](./advanced/multi-tenancy.md) - Tenant isolation strategies
- [Bounded Contexts](./advanced/bounded-contexts.md) - Domain separation
- [Event Sourcing](./advanced/event-sourcing.md) - Event-driven architecture
- [Database Patterns](./advanced/database-patterns.md) - View design and N+1 prevention
- [LLM Integration](./advanced/llm-integration.md) - AI-native architecture

**Production** (5 docs)
- [Deployment](./production/deployment.md) - Docker, Kubernetes, cloud platforms
- [Monitoring](./production/monitoring.md) - PostgreSQL-native error tracking and caching
- [Observability](./production/observability.md) - Complete observability stack in PostgreSQL
- [Security](./production/security.md) - Production hardening
- [Health Checks](./production/health-checks.md) - Application health monitoring

**Enterprise** (1 doc)
- [Audit Logging](./enterprise/audit-logging.md) - Cryptographic chain integrity and compliance

**Reference** (4 docs)
- [CLI Reference](./reference/cli.md) - Complete command-line interface guide
- [Decorators](./reference/decorators.md) - @type, @query, @mutation
- [Configuration](./reference/config.md) - FraiseQLConfig options
- [Database API](./reference/database.md) - Repository methods

## About FraiseQL

See the [main README](../README.md#about) for the FraiseQL story, philosophy, and creator information.

## Architecture Overview

See the [main README](../README.md#architecture) for detailed architecture information, core components, and key features.

## System Requirements & Installation

See the [main README](../README.md#system-requirements) for system requirements and installation instructions.

## Hello World Example

```python
from fraiseql import FraiseQL, ID
from datetime import datetime

app = FraiseQL(database_url="postgresql://localhost/mydb")

@app.type
class Task:
    id: ID
    title: str
    completed: bool
    created_at: datetime

@app.query
async def tasks(info) -> list[Task]:
    repo = info.context["repo"]
    return await repo.find("v_task")
```

Database view:
```sql
CREATE VIEW v_task AS
SELECT jsonb_build_object(
    'id', id,
    'title', title,
    'completed', completed,
    'created_at', created_at
) AS data
FROM tb_task;
```

## Performance Stack

FraiseQL achieves sub-millisecond performance through four optimization layers:

| Layer | Technology | Speedup | Configuration |
|-------|------------|---------|---------------|
| 0 | Rust Transformation | 7-10x | `pip install fraiseql[rust]` |
| 1 | APQ Caching | 5-10x | `apq_storage_backend="postgresql"` |
| 2 | TurboRouter | 3-5x | `enable_turbo_router=True` |
| 3 | JSON Passthrough | 2-3x | Automatic with JSONB views |
| **Bonus** | **Result Caching** | **50-500x** | [PostgreSQL Cache](./performance/caching.md) |

**Combined**: 0.5-2ms response times for cached queries. See [Performance](./performance/index.md) for complete details.

## Architecture Principles

**Database-First**: PostgreSQL views define data structure and relationships. Single queries return pre-composed JSONB matching GraphQL structure.

**CQRS Pattern**: Strict separation of reads (views) and writes (functions). Read models optimized for queries, write operations enforce business rules.

**Type Safety**: Python type hints generate GraphQL schema. Repository operations are type-checked at compile time.

**Zero N+1**: Database-side composition via JSONB aggregation eliminates resolver chains and multiple queries.

## Development Workflow

1. **Design Schema**: Create PostgreSQL tables and relationships
2. **Build Views**: Compose JSONB views with `jsonb_build_object()`
3. **Define Types**: Python classes with type hints
4. **Add Queries**: Resolvers calling `repo.find()` methods
5. **Implement Mutations**: PostgreSQL functions called via `repo.call_function()`

## Documentation Structure

This documentation follows an information-dense format optimized for both human developers and AI code assistants. Each page provides:
- Structured reference material (tables, signatures, examples)
- Production-ready code samples
- Performance characteristics where measured
- Cross-references to related topics

## Learning Paths

### New to FraiseQL? Start Here

1. **[5-Minute Quickstart](./quickstart.md)** - Get a working API immediately
2. **[Beginner Learning Path](./tutorials/beginner-path.md)** - Structured 2-3 hour journey
3. **[Blog API Tutorial](./tutorials/blog-api.md)** - Build complete application
4. **[Database Patterns](./advanced/database-patterns.md)** - Production patterns

### Building Production APIs?

1. **[Performance Optimization](./performance/index.md)** - 4-layer optimization stack
2. **[Database Patterns](./advanced/database-patterns.md)** - tv_ pattern, entity change log, lazy caching
3. **[Production Deployment](./tutorials/production-deployment.md)** - Docker, monitoring, security
4. **[Multi-Tenancy](./advanced/multi-tenancy.md)** - Tenant isolation

### Quick Reference?

- **[CLI Reference](./reference/cli.md)** - All commands, options, and workflows
- **[Database API](./core/database-api.md)** - Repository methods and QueryOptions
- **[Performance](./performance/index.md)** - Rust, APQ, TurboRouter, JSON Passthrough
- **[Database Patterns](./advanced/database-patterns.md)** - Real production patterns (2,023 lines)

## Contributing

Contributions to improve documentation accuracy and completeness are welcome. Please ensure:
- Code examples are tested and copy-paste ready
- Performance claims are backed by data or marked as TBD
- Professional tone without marketing language
- Tables used for structured information

## Support

- GitHub Issues: Bug reports and feature requests
- Examples: `/examples` directory in repository
- API Reference: Complete method documentation

## License

See repository for license information.
