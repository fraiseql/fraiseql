# FraiseQL v1 - Project Structure

Complete overview of the project structure and organization.

## Directory Tree

```
fraiseql-v1/
├── 📄 README.md                        # Project overview
├── 📄 STRUCTURE.md                     # This file
├── 📄 pyproject.toml                   # Python project config
├── 📄 .gitignore                       # Git ignore patterns
│
├── 📁 docs/                            # Documentation
│   ├── 📄 README.md                   # Documentation index
│   ├── 📁 philosophy/                 # Why FraiseQL exists
│   │   ├── WHY_FRAISEQL.md
│   │   ├── CQRS_FIRST.md
│   │   ├── RUST_ACCELERATION.md
│   │   └── CODEGEN_VISION.md
│   ├── 📁 architecture/               # Technical deep dives
│   │   ├── OVERVIEW.md
│   │   ├── NAMING_CONVENTIONS.md
│   │   ├── COMMAND_QUERY_SEPARATION.md
│   │   ├── SYNC_STRATEGIES.md
│   │   └── RUST_INTEGRATION.md
│   ├── 📁 guides/                     # How-to guides
│   │   ├── QUICK_START.md
│   │   ├── DATABASE_SETUP.md
│   │   ├── WRITING_QUERIES.md
│   │   ├── WRITING_MUTATIONS.md
│   │   ├── TYPE_SYSTEM.md
│   │   └── PERFORMANCE.md
│   ├── 📁 api/                        # API reference
│   │   ├── DECORATORS.md
│   │   ├── REPOSITORY.md
│   │   ├── SYNC_FUNCTIONS.md
│   │   └── CLI.md
│   └── 📁 examples/                   # Example patterns
│       ├── BASIC_BLOG.md
│       ├── ECOMMERCE_API.md
│       └── SAAS_MULTI_TENANT.md
│
├── 📁 examples/                        # Working examples
│   ├── 📄 README.md
│   ├── 📁 quickstart/                 # 5-minute hello world
│   ├── 📁 blog/                       # Full blog with CQRS
│   └── 📁 ecommerce/                  # Product catalog
│
├── 📁 src/fraiseql/                   # Core library
│   ├── 📄 __init__.py                 # Public API
│   ├── 📄 py.typed                    # Type hints marker
│   │
│   ├── 📁 types/                      # Type system
│   │   ├── __init__.py
│   │   ├── fraise_type.py            # @type decorator
│   │   ├── fraise_input.py           # @input decorator
│   │   ├── field_resolver.py         # @field decorator
│   │   ├── registry.py               # Type registration
│   │   └── 📁 scalars/                # Custom scalars
│   │       ├── __init__.py
│   │       ├── uuid.py
│   │       ├── datetime.py
│   │       ├── json.py
│   │       ├── cidr.py
│   │       └── ltree.py
│   │
│   ├── 📁 decorators/                 # GraphQL decorators
│   │   ├── __init__.py
│   │   ├── query.py                  # @query decorator
│   │   ├── mutation.py               # @mutation decorator
│   │   └── subscription.py           # @subscription decorator
│   │
│   ├── 📁 repositories/               # CQRS pattern
│   │   ├── __init__.py
│   │   ├── command.py                # CommandRepository
│   │   ├── query.py                  # QueryRepository
│   │   └── sync.py                   # Sync functions
│   │
│   ├── 📁 sql/                        # SQL utilities
│   │   ├── __init__.py
│   │   ├── where_builder.py          # WHERE clause builder
│   │   └── operators.py              # SQL operators
│   │
│   ├── 📁 core/                       # Core functionality
│   │   ├── __init__.py
│   │   ├── rust_transformer.py       # Rust bridge
│   │   └── config.py                 # Configuration
│   │
│   ├── 📁 gql/                        # GraphQL schema
│   │   ├── __init__.py
│   │   ├── registry.py               # Schema registry
│   │   ├── schema_builder.py         # Schema generation
│   │   └── type_mapper.py            # Python→GraphQL types
│   │
│   ├── 📁 fastapi/                    # FastAPI integration
│   │   ├── __init__.py
│   │   ├── app.py                    # FastAPI app factory
│   │   └── middleware.py             # GraphQL middleware
│   │
│   └── 📁 cli/                        # CLI tool (future)
│       ├── __init__.py
│       ├── app.py                    # Typer CLI app
│       ├── init.py                   # fraiseql init
│       └── codegen.py                # fraiseql codegen
│
├── 📁 fraiseql_rs/                    # Rust crate
│   ├── 📄 Cargo.toml                  # Rust project config
│   └── 📁 src/
│       ├── lib.rs                    # Main library
│       ├── transform.rs              # JSON transformation
│       └── case_conversion.rs        # snake_case ↔ camelCase
│
└── 📁 tests/                          # Test suite
    ├── 📄 README.md
    ├── 📄 conftest.py                 # Pytest fixtures
    ├── 📁 fixtures/                   # Test data
    │   ├── schema.sql                # Test database schema
    │   └── sample_data.sql           # Sample data
    ├── 📁 unit/                       # Unit tests
    │   ├── test_types.py
    │   ├── test_decorators.py
    │   ├── test_repositories.py
    │   ├── test_where_builder.py
    │   ├── test_rust_transformer.py
    │   └── test_schema_builder.py
    └── 📁 integration/                # Integration tests
        ├── test_cqrs_flow.py
        ├── test_graphql_queries.py
        ├── test_mutations.py
        └── test_performance.py
```

## Component Breakdown

### 📦 Core Components (~2,800 LOC)

| Component | Location | LOC | Status | Priority |
|-----------|----------|-----|--------|----------|
| **Type System** | `src/fraiseql/types/` | 800 | 🚧 Planned | Critical |
| **Repositories** | `src/fraiseql/repositories/` | 600 | 🚧 Planned | Critical |
| **Decorators** | `src/fraiseql/decorators/` | 400 | 🚧 Planned | Critical |
| **WHERE Builder** | `src/fraiseql/sql/` | 500 | 🚧 Planned | High |
| **Rust Integration** | `src/fraiseql/core/` + `fraiseql_rs/` | 500 | 🚧 Planned | High |

### 📚 Documentation

| Section | Purpose | Status |
|---------|---------|--------|
| **Philosophy** | Why FraiseQL exists | 🚧 Planned |
| **Architecture** | Technical deep dives | 🚧 Planned |
| **Guides** | How-to tutorials | 🚧 Planned |
| **API Reference** | Complete API docs | 🚧 Planned |
| **Examples** | Real-world patterns | 🚧 Planned |

### 🔬 Testing

| Type | Location | Purpose |
|------|----------|---------|
| **Unit Tests** | `tests/unit/` | Test individual components |
| **Integration Tests** | `tests/integration/` | Test complete workflows |
| **Benchmarks** | `tests/benchmarks/` | Performance testing |

## File Naming Conventions

### Python Files
- `fraise_*.py` - Core decorators (@type, @input)
- `*_repository.py` - Repository pattern implementations
- `*_builder.py` - Builder pattern implementations
- `test_*.py` - Test files

### SQL Conventions
- `tb_*` - Tables (command side)
- `tv_*` - Table views (query side)
- `mv_*` - Materialized views (optional, for expensive aggregations)
- `fn_sync_tv_*` - Sync functions
- `pk_*` - Primary key columns
- `fk_*` - Foreign key columns

### GraphQL Conventions
- `snake_case` in database
- `camelCase` in GraphQL API
- `id` - UUID for public API
- `identifier` - Human-readable (username, slug)

## Implementation Order

### Phase 1: Core Type System (Week 1)
1. `types/fraise_type.py` - @type decorator
2. `types/fraise_input.py` - @input decorator
3. `types/field_resolver.py` - @field decorator
4. `types/scalars/` - Custom scalars
5. Tests for type system

### Phase 2: Repositories (Week 2-3)
1. `repositories/command.py` - CommandRepository
2. `repositories/query.py` - QueryRepository
3. `repositories/sync.py` - Sync functions
4. `sql/where_builder.py` - WHERE clause builder
5. Tests for repositories

### Phase 3: Schema Generation (Week 3-4)
1. `decorators/` - @query, @mutation
2. `gql/registry.py` - Schema registry
3. `gql/schema_builder.py` - Schema builder
4. Tests for schema generation

### Phase 4: Rust Integration (Week 4)
1. `fraiseql_rs/src/transform.rs` - JSON transformation
2. `core/rust_transformer.py` - Python bridge
3. Performance benchmarks

### Phase 5: Examples & Documentation (Week 5-6)
1. Quickstart example
2. Blog example
3. E-commerce example
4. Complete documentation

## Lines of Code Target

```
Core Library:       ~3,000 LOC
Documentation:      ~5,000 LOC
Examples:          ~2,000 LOC
Tests:             ~3,000 LOC
─────────────────────────────
Total:            ~13,000 LOC
```

**Philosophy**: Quality over quantity. Every line must be justifiable.

## Key Principles

1. **Simplicity**: Avoid complexity, prefer clarity
2. **Type Safety**: Leverage Python's type system
3. **Performance**: Sub-1ms queries with Rust
4. **Explicit**: No magic, clear control flow
5. **Testable**: 100% coverage on core components
6. **Documented**: Every public API has docs

---

**Status**: 🚧 Structure created, ready for implementation

**Next Step**: Begin with Type System (Phase 1)
