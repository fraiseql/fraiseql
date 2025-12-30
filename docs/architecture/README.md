# FraiseQL Architecture Documentation

This directory contains architectural documentation for FraiseQL.

## Key Documents

### Core Architecture

**[Request Flow](./request-flow.md)** - Complete end-to-end request processing pipeline with performance characteristics
- HTTP → FastAPI → GraphQL Parser → Rust Pipeline → PostgreSQL
- Query vs Mutation execution paths
- Caching layers and monitoring points
- 25-60x performance improvement over traditional frameworks

**[Trinity Pattern](./trinity-pattern.md)** - Three-identifier database design pattern
- `pk_*` (INTEGER) - Fast internal joins
- `id` (UUID) - Stable public API
- `identifier` (TEXT) - Human-readable slugs
- 7.7x faster joins with optimal storage

**[Type System](./type-system.md)** - Type mapping between Python, GraphQL, and PostgreSQL
- Automatic type conversion
- Nullability handling
- Custom scalars and enums
- Nested object patterns

**[CQRS Design](./cqrs-design.md)** - Command Query Responsibility Segregation pattern
- View-based queries (read-optimized)
- Function-based mutations (write-controlled)
- Schema separation and security
- Integration with Trinity Pattern

### Implementation Details

**[direct-path-implementation.md](./direct-path-implementation.md)** - Direct path pipeline that bypasses GraphQL resolvers

**Status**: ✅ Implemented and working
- GraphQL → SQL → Rust → HTTP pipeline
- 3-4x performance improvement
- Full WHERE clause support
- Automatic fallback to traditional GraphQL

**[type-operator-architecture.md](./type-operator-architecture.md)** - Type system and operator strategies for WHERE clauses

### Architectural Decisions
**[decisions/](./decisions/README.md)** - Records of key architectural decisions and their rationale

## Architectural Topics

- **CQRS Pattern** - Command Query Responsibility Segregation
- **View-Based Reads** - Query through database views (`v_{entity}`)
- **Trinity Pattern** - Table (`tv_*`) + View (`v_*`) + Type (Python class)
- **Hybrid Tables** - Tables with both relational columns and JSONB data
- **Direct Path** - Bypass GraphQL resolvers for performance

## Quick Reference

### Direct Path Pipeline
```
GraphQL Query → Parser → SQL + WHERE → JSONB → Rust → HTTP
              ↓
   Bypass GraphQL Resolvers (3-4x faster)
```

### Trinity Pattern
- **Table**: `tv_{entity}` - Physical storage (id + JSONB)
- **View**: `v_{entity}` - Query interface
- **Type**: `{Entity}` - GraphQL schema

## Related Documentation

- [Advanced Patterns](../advanced/advanced-patterns.md)
- [Enterprise Features](../advanced/advanced-patterns.md)
- [Examples](../../examples/)
