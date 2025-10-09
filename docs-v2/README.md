# FraiseQL Documentation

Enterprise-grade GraphQL framework built on PostgreSQL, FastAPI, and Strawberry. Delivers sub-millisecond response times through database-first architecture and CQRS pattern implementation.

## Quick Navigation

**Getting Started**
- [5-Minute Quickstart](./quickstart.md) - Build a working API in minutes

**Core Concepts** (4 docs)
- Types and Schema - GraphQL type definitions and schema generation
- Queries and Mutations - Resolver patterns and execution
- [Database API](./core/database-api.md) - Repository patterns and query building
- Configuration - Application setup and tuning

**Performance** (1 consolidated doc)
- [Performance Optimization](./performance/index.md) - Complete optimization stack

**Advanced Patterns** (6 docs)
- Authentication - Auth patterns and security
- Multi-Tenancy - Tenant isolation strategies
- Bounded Contexts - Domain separation
- Event Sourcing - Event-driven architecture
- [Database Patterns](./advanced/database-patterns.md) - View design and N+1 prevention
- LLM Integration - AI-native architecture

**Production** (3 docs)
- Deployment - Docker, Kubernetes, cloud platforms
- Monitoring - Observability and metrics
- Security - Production hardening

**API Reference** (3 docs)
- Decorators - @type, @query, @mutation
- Configuration - FraiseQLConfig options
- Database API - Repository methods

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
| Type-Safe Schema | Python decorators generate GraphQL types | Types and Schema |
| Repository Pattern | Async database operations with structured queries | [Database API](./core/database-api.md) |
| Rust Transformation | 10-80x faster JSON processing (optional) | [Performance](./performance/index.md) |
| APQ Caching | Hash-based query persistence in PostgreSQL | [Performance](./performance/index.md) |
| JSON Passthrough | Zero-copy responses from database | [Performance](./performance/index.md) |
| Multi-Tenancy | Row-level security patterns | Multi-Tenancy |
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
