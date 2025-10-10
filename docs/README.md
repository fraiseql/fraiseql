# FraiseQL Documentation

Enterprise-grade GraphQL framework built on PostgreSQL, FastAPI, and Strawberry. Delivers sub-millisecond response times through database-first architecture and CQRS pattern implementation.

## Quick Navigation

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

**Performance** (1 consolidated doc)
- [Performance Optimization](./performance/index.md) - Complete optimization stack

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

**Reference** (4 docs)
- [CLI Reference](./reference/cli.md) - Complete command-line interface guide
- [Decorators](./reference/decorators.md) - @type, @query, @mutation
- [Configuration](./reference/config.md) - FraiseQLConfig options
- [Database API](./reference/database.md) - Repository methods

## About FraiseQL

FraiseQL is created by **Lionel Hamayon** ([@evoludigit](https://github.com/evoludigit)), a self-taught developer frustrated with a fundamental inefficiency in GraphQL frameworks.

**Started: April 2025**

The trigger: watching PostgreSQL return JSON, Python deserialize it to objects, then GraphQL serialize it back to JSON. This roundtrip is ridiculous.

After years with Django, Flask, FastAPI, and Strawberry GraphQL with SQLAlchemy, the answer became obvious: just let PostgreSQL return the JSON directly. Skip the ORM. Skip the object mapping. Let the database do what databases do best.

But there was a second goal: make it LLM-first. SQL and Python are massively trained in every AI model. A framework built with these as primitives means LLMs can understand the context easily and generate correct code. In the age of AI-assisted development, this matters.

FraiseQL is the result: database-first CQRS, minimal Python, maximum PostgreSQL, and architecture that's readable by both humans and AI.

**Connect:** [@evoludigit](https://github.com/evoludigit) • [Évolution digitale](https://evolution-digitale.fr)

## Architecture Overview

FraiseQL implements CQRS pattern with PostgreSQL as the single source of truth. Queries execute through JSONB views returning pre-composed data, while mutations run as PostgreSQL functions containing business logic. This architecture eliminates N+1 queries by design and achieves 0.5-2ms response times with APQ caching.

**Core Components**:
- **Views** (v_*, tv_*): Read-side projections returning JSONB data
- **Functions** (fn_*): Write-side operations with transactional guarantees
- **Repository**: Async database operations with type safety
- **Rust Transformer**: 10-80x faster JSON processing

## Key Features

| Feature | Description | Documentation |
|---------|-------------|---------------|
| Type-Safe Schema | Python decorators generate GraphQL types | [Types and Schema](./core/types-and-schema.md) |
| Repository Pattern | Async database operations with structured queries | [Database API](./core/database-api.md) |
| Rust Transformation | 10-80x faster JSON processing (optional) | [Performance](./performance/index.md) |
| APQ Caching | Hash-based query persistence in PostgreSQL | [Performance](./performance/index.md) |
| JSON Passthrough | Zero-copy responses from database | [Performance](./performance/index.md) |
| Multi-Tenancy | Row-level security patterns | [Multi-Tenancy](./advanced/multi-tenancy.md) |
| N+1 Prevention | Eliminated by design via view composition | [Database Patterns](./advanced/database-patterns.md) |

## System Requirements

**Required**:
- Python 3.11+
- PostgreSQL 14+

**Optional**:
- Rust compiler (for performance layer: 10-80x JSON speedup)

## Installation

```bash
# Standard installation
pip install fraiseql fastapi uvicorn

# With Rust performance extensions (recommended)
pip install fraiseql[rust]
```

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
| 0 | Rust Transformation | 10-80x | `pip install fraiseql[rust]` |
| 1 | APQ Caching | 5-10x | `apq_storage_backend="postgresql"` |
| 2 | TurboRouter | 3-5x | `enable_turbo_router=True` |
| 3 | JSON Passthrough | 2-3x | Automatic with JSONB views |

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
