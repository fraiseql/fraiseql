# FraiseQL v1 Documentation

This directory contains comprehensive documentation for FraiseQL v1.

## Structure

### 📖 Philosophy (`philosophy/`)
- `WHY_FRAISEQL.md` - The problem we solve
- `CQRS_FIRST.md` - CQRS as the foundation
- `RUST_ACCELERATION.md` - Why Rust transformation
- `CODEGEN_VISION.md` - The endgame: CLI codegen

**Purpose**: Explain WHY before HOW. These docs answer "Why does FraiseQL exist?" and "What problems does it solve?"

### 🏗️ Architecture (`architecture/`)
- `OVERVIEW.md` - High-level architecture diagram
- `NAMING_CONVENTIONS.md` - tb_* vs tv_* vs mv_*
- `COMMAND_QUERY_SEPARATION.md` - How CQRS works in FraiseQL
- `SYNC_STRATEGIES.md` - Explicit sync (no triggers)
- `RUST_INTEGRATION.md` - How Python↔Rust works

**Purpose**: Technical deep dives into architectural decisions and patterns.

### 📚 Guides (`guides/`)
- `QUICK_START.md` - 5-minute hello world
- `DATABASE_SETUP.md` - PostgreSQL + JSONB setup
- `WRITING_QUERIES.md` - @query decorator usage
- `WRITING_MUTATIONS.md` - Explicit sync pattern
- `TYPE_SYSTEM.md` - @type, scalars, etc.
- `PERFORMANCE.md` - Benchmarks, optimization tips

**Purpose**: Practical how-to guides for common tasks.

### 🔧 API Reference (`api/`)
- `DECORATORS.md` - @query, @mutation, @type, etc.
- `REPOSITORY.md` - CommandRepository, QueryRepository
- `SYNC_FUNCTIONS.md` - sync_tv_*, batch operations
- `CLI.md` - fraiseql CLI commands

**Purpose**: Complete API documentation for all public interfaces.

### 💡 Examples (`examples/`)
- `BASIC_BLOG.md` - Simple blog with CQRS
- `ECOMMERCE_API.md` - Product catalog example
- `SAAS_MULTI_TENANT.md` - Advanced patterns

**Purpose**: Real-world examples demonstrating patterns and best practices.

## Documentation Principles

1. **Philosophy First**: Explain WHY before HOW
2. **Show Performance**: Include benchmarks in every doc
3. **Code Examples**: Every concept has working code
4. **Progressive Disclosure**: Simple → Advanced
5. **Visual**: Include architecture diagrams for visual learners

## Status

🚧 All documentation is under development as part of Phase 1 (Planning).
