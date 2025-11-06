# FraiseQL Documentation

## Getting Started

- **[5-Minute Quickstart](getting-started/quickstart.md)** - Fastest way to get running
- **[First Hour Guide](getting-started/first-hour.md)** - Progressive tutorial
- **[Understanding FraiseQL](guides/understanding-fraiseql.md)** - Conceptual overview
- **[Installation](getting-started/installation.md)** - Detailed setup instructions

## 🎯 Feature Discovery

- **[Feature Matrix](features/index.md)** - Complete overview of all FraiseQL capabilities
  - Core features, database features, advanced queries
  - Security, enterprise, real-time, monitoring
  - See at a glance what FraiseQL can do

## 📖 Core Concepts

**New to FraiseQL?** Start with these essential concepts:

- **[Concepts & Glossary](core/concepts-glossary.md)** ⭐ **START HERE** - Core terminology and mental models
  - CQRS Pattern - Separate reads (views) from writes (functions)
  - Trinity Identifiers - Three-tier ID system for performance and UX
  - JSONB Views vs Table Views - When to use `v_*` vs `tv_*`
  - Database-First Architecture - PostgreSQL composes, GraphQL exposes
  - Explicit Sync Pattern - Denormalized tables for complex queries

- **[Types and Schema](core/types-and-schema.md)** - Complete guide to FraiseQL's type system
- **[Database API](core/database-api.md)** - PostgreSQL integration and query execution
- **[Configuration](core/configuration.md)** - Application configuration reference

## 🔍 Querying & Filtering

FraiseQL provides two flexible syntaxes for filtering data:

- **[Where Input Types](advanced/where_input_types.md)** ⭐ **Complete filtering guide**
  - **WhereType syntax** (preferred) - Type-safe with full IDE autocomplete
  - **Dict-based syntax** - Maximum flexibility for dynamic queries
  - Nested object filtering - Filter on related object properties
  - All operators: eq, neq, gt, lt, contains, overlaps, etc.
  - Logical operators: AND, OR, NOT
  - **New in v1.2.0:** Dict-based nested filtering fully supported!

- **[Syntax Comparison Cheat Sheet](reference/where-clause-syntax-comparison.md)** 📋 **Quick reference**
  - Side-by-side examples of WhereType vs Dict
  - When to use each syntax
  - Common patterns and best practices

- **[Filter Operators Reference](advanced/filter-operators.md)** - Complete operator documentation
  - String, numeric, boolean, array operators
  - Full-text search (tsvector)
  - JSONB operators
  - Regular expressions

- **[Advanced Filtering Examples](examples/advanced-filtering.md)** - Real-world use cases
  - E-commerce product search
  - Content management systems
  - User permissions & RBAC
  - Log analysis

- **[Dict-Based Nested Filtering](examples/dict-based-nested-filtering.md)** - Dict syntax deep-dive
  - Multiple nested fields
  - CamelCase support
  - Performance optimization
  - Migration guide

## Advanced Features

- [Advanced Patterns](advanced/advanced-patterns.md)
- [Authentication](advanced/authentication.md)
- [Multi-Tenancy](advanced/multi-tenancy.md)
- [Database Patterns](advanced/database-patterns.md)

## Performance

- [Performance Guide](performance/index.md)
- [APQ Optimization](performance/apq-optimization-guide.md)
- [Rust Pipeline](performance/rust-pipeline-optimization.md)

## Reference

- [Quick Reference](reference/quick-reference.md)
- [Type Operator Architecture](architecture/type-operator-architecture.md)
- [Configuration Reference](reference/config.md)

## Guides

- [Nested Array Filtering](guides/nested-array-filtering.md)
- [Troubleshooting](guides/troubleshooting.md)
- [Troubleshooting Decision Tree](guides/troubleshooting-decision-tree.md)

## Reference

- [Testing Checklist](reference/testing-checklist.md)
- [Quick Reference](reference/quick-reference.md)
- [Type Operator Architecture](architecture/type-operator-architecture.md)
- [Configuration Reference](reference/config.md)

## Development

- [Contributing](../CONTRIBUTING.md)
- [Style Guide](development/style-guide.md)
- [Architecture Decisions](architecture/README.md)
