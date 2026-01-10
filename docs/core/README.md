# Core Documentation

Essential FraiseQL concepts, architecture, and core features.

## Getting Started

- **[Concepts & Glossary](concepts-glossary.md)** - Core terminology and mental models
  - CQRS pattern, JSONB views, Trinity identifiers, Database-first architecture
- **[FraiseQL Philosophy](fraiseql-philosophy.md)** - Design principles and trade-offs
- **[Project Structure](project-structure.md)** - How to organize FraiseQL projects

## Type System & Schema

- **[Types and Schema](types-and-schema.md)** - Complete guide to FraiseQL's type system
  - `@type` decorator and GraphQL type mapping
  - Input types, success/failure patterns
  - Type composition and reusability
- **[Queries and Mutations](queries-and-mutations.md)** - Define GraphQL operations
  - `@query` and `@mutation` decorators
  - Auto-generated resolvers
  - Success/failure pattern implementation

## Database Integration

- **[Database API](database-api.md)** - PostgreSQL connection and query execution
  - Connection pooling and management
  - Calling PostgreSQL functions
  - Transaction handling
- **[DDL Organization](ddl-organization.md)** - SQL schema organization patterns
  - Naming conventions: `tb_*`, `v_*`, `tv_*`, `fn_*`
  - Migration strategies
- **[PostgreSQL Extensions](postgresql-extensions.md)** - Required and recommended extensions
  - uuid-ossp, ltree, pg_trgm, PostGIS

## Advanced Concepts

- **[Rust Pipeline Integration](rust-pipeline-integration.md)** - How the Rust acceleration works
  - JSONB → Rust → HTTP response path
  - Field selection optimization
  - Performance characteristics
- **[Explicit Sync Pattern](explicit-sync.md)** - Table views (tv_*) synchronization
  - When to use table views vs regular views
  - Sync function patterns
  - Performance trade-offs

## Configuration & Dependencies

- **[Configuration](configuration.md)** - Application configuration reference
  - Database settings
  - APQ configuration
  - Caching backends
  - Security and CORS
- **[Dependencies](dependencies.md)** - Required and optional Python/system dependencies
- **[Migrations](migrations.md)** - Database schema migration strategies

## Quick Navigation

**New to FraiseQL?** Start here:
1. [Concepts & Glossary](concepts-glossary.md) - Understand the mental model
2. [Types and Schema](types-and-schema.md) - Learn the type system
3. [Database API](database-api.md) - Connect to PostgreSQL
4. [Queries and Mutations](queries-and-mutations.md) - Build your API

**Building production apps?**
- [Configuration](configuration.md) - Production settings
- [Rust Pipeline Integration](rust-pipeline-integration.md) - Performance optimization
- [Explicit Sync Pattern](explicit-sync.md) - Complex data patterns
