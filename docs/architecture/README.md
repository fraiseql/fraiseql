# FraiseQL Architecture Documentation

This directory contains architectural documentation for FraiseQL.

## Key Documents

### Direct Path Implementation
**[direct-path-implementation.md](./direct-path-implementation.md)** - Complete documentation of the direct path pipeline that bypasses GraphQL resolvers for maximum performance.

**Status**: ✅ Implemented and working
- GraphQL → SQL → Rust → HTTP pipeline
- 3-4x performance improvement
- Full WHERE clause support
- Automatic fallback to traditional GraphQL

### Type System
**[type-operator-architecture.md](./type-operator-architecture.md)** - Documentation of FraiseQL's type system and operator strategies for WHERE clauses.

### Architectural Decisions
**[decisions/](./decisions/)** - Records of key architectural decisions and their rationale.

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

- [Advanced Patterns](../advanced/)
- [Enterprise Features](../enterprise/)
- [Examples](../../examples/)
