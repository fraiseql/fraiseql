# FraiseQL Codebase Organization

**Last Updated**: January 8, 2026
**Version**: v2.0 Preparation
**Scope**: Complete architectural overview and navigation guide

## Quick Navigation

- **[Directory Structure](#directory-structure)** - High-level layout
- **[Python Framework](#python-framework-srcfraiseql)** - Source code organization
- **[Rust Extension](#rust-extension-fraiseql_rs)** - Performance pipeline
- **[Test Suite](#test-suite)** - Testing strategy and organization
- **[Naming Conventions](#naming-conventions)** - Code style standards
- **[Module Guidelines](#module-guidelines)** - Adding new code
- **[Architecture](#architecture)** - High-level design patterns

---

## Directory Structure

### Root Level

```
fraiseql/
├── src/fraiseql/              ← Python framework (primary)
├── fraiseql_rs/               ← Rust extension (performance)
├── tests/                     ← Comprehensive test suite
├── docs/                      ← Documentation
├── deploy/                    ← Deployment & Docker configs
├── benches/                   ← Rust benchmarks
├── .archive/                  ← Archived/deprecated code
├── .claude/                   ← AI assistant instructions
├── .github/                   ← GitHub workflows & CI/CD
├── pyproject.toml             ← Python project metadata
├── Cargo.toml                 ← Rust workspace
├── Makefile                   ← Development commands
└── CHANGELOG.md               ← Release history
```

### Directory Purposes

| Directory | Purpose | When to Visit |
|-----------|---------|---------------|
| `src/fraiseql/` | Main Python framework | Most development |
| `fraiseql_rs/` | Rust performance pipeline | Performance optimization |
| `tests/` | Test suite | Adding tests, debugging |
| `docs/` | Documentation | Understanding features |
| `deploy/` | Production deployment | Deployments, Docker |
| `benches/` | Performance benchmarks | Performance work |
| `.archive/` | Legacy/deprecated code | Understanding history |

---

## Python Framework (`src/fraiseql/`)

### Tier 1: Core Execution Engine

The foundation that everything builds on.

```
core/
├── graphql_pipeline.py         # Main query execution
├── graphql_type.py             # Type system (45KB, largest core)
├── rust_pipeline.py            # Rust integration layer
├── registry.py                 # Type registry management
├── selection_tree.py           # Query field selection
├── ast_parser.py               # GraphQL AST parsing
├── fragment_resolver.py        # Fragment @include/@skip
└── nested_field_resolver.py    # Nested field resolution
```

**Key Concepts**:
- **graphql_pipeline.py**: Coordinates query execution (parse → validate → resolve → execute)
- **graphql_type.py**: Defines type system (fields, decorators, resolution)
- **rust_pipeline.py**: Bridges Python and Rust extensions for performance

**When to modify**: Changing core query execution, type resolution

---

### Tier 2: Type System (40+ Custom Scalars)

Type definitions and validation.

```
types/
├── fraise_type.py              # @fraise_type decorator
├── fraise_input.py             # @fraise_input decorator
├── fraise_interface.py         # Interface definitions
├── enum.py                     # Enum handling
├── context.py                  # GraphQL context object
├── definitions.py              # UNSET sentinel, markers
├── common.py                   # MutationResultBase, error types
├── errors.py                   # Error types
└── scalars/
    ├── standard/               # Date, DateTime, UUID, JSON
    ├── network/                # IPAddress, Port, CIDR
    ├── financial/              # Money, CurrencyCode
    ├── contact/                # Email, PhoneNumber
    ├── geographic/             # Coordinates, GeoJSON
    └── [domain-specific]       # Project-specific types
```

**Key Concepts**:
- **Decorators**: `@fraise_type`, `@fraise_input`, `@fraise_interface` define schema types
- **Scalars**: Custom type serialization/deserialization
- **Context**: Request-scoped data passed through resolvers

**When to modify**: Adding new scalar types, changing type system behavior

---

### Tier 3: SQL Query Generation

Builds efficient PostgreSQL queries from GraphQL.

```
sql/
├── where_generator.py          # WHERE clause logic
├── graphql_where_generator.py  # GraphQL input → WHERE
├── order_by_generator.py       # ORDER BY generation
├── sql_generator.py            # Complete SQL builder
├── operators/                  # Operator implementations
│   ├── core/                   # Basic operators (=, !=, <, >)
│   ├── postgresql/             # PG-specific (ltree, jsonb)
│   ├── array/                  # Array operations
│   ├── advanced/               # Network, fulltext, geospatial
│   └── fallback/               # Fallback implementations
└── where/                      # WHERE clause utilities
    ├── core/                   # Core logic
    └── operators/              # Operator handling
```

**Key Concepts**:
- **Operator Strategy Pattern**: Each operator type has dedicated handler
- **PostgreSQL-Specific**: Leverages JSONB, LTree, network types
- **WHERE Normalization**: Translates GraphQL input to SQL

**When to modify**: Changing filtering behavior, adding new operators

---

### Tier 4: Multi-Framework HTTP Architecture (v2.0+)

**Flexible, modular HTTP servers supporting both Rust (performance) and Python (compatibility).**

#### Core HTTP Module (Rust - Framework Agnostic)

```
fraiseql_rs/src/http/           # ✅ CORE (framework-agnostic)
├── router.rs                   # HTTP routing logic
├── handler.rs                  # Handler trait definitions
├── middleware.rs               # Middleware pipeline
├── response.rs                 # Response building
├── error.rs                    # Error handling
└── adapters/
    ├── rust/                   # Rust server adapters
    │   ├── axum.rs            # Axum integration
    │   ├── actix.rs           # Actix-web integration
    │   └── hyper.rs           # Hyper integration
    └── python/                 # Python server adapters
        ├── fastapi.rs         # FastAPI integration
        ├── starlette.rs       # Starlette integration
        └── custom.rs          # Custom adapter template
```

#### Modular Middleware (Shared Across All Servers)

```
src/fraiseql/http/             # ✅ MIDDLEWARE (composable, framework-agnostic)
├── middleware/
│   ├── auth.rs                # Authentication
│   ├── rbac.rs                # Role-based access
│   ├── caching.rs             # Result caching
│   ├── rate_limiting.rs       # Rate limiting
│   ├── cors.rs                # CORS support
│   ├── csrf.rs                # CSRF protection
│   ├── logging.rs             # Request logging
│   └── tracing.rs             # Distributed tracing
├── config.rs                  # Middleware configuration
└── traits.rs                  # Middleware trait definitions
```

#### Framework Options (Choose Based on Needs)

**Rust Servers (Performance)**:

```
Option 1: Axum (Recommended for v2.0)
├── Performance: 7-10x faster than Python
├── Ecosystem: Modern async Rust, growing community
├── Best for: New applications, performance-critical
└── Files: fraiseql_rs/src/http/adapters/rust/axum.rs

Option 2: Actix-web (Proven, Good for Migrations)
├── Performance: Excellent, battle-tested
├── Ecosystem: Mature, many integrations
├── Best for: Migrating from v1.8.x, proven track record
└── Files: fraiseql_rs/src/http/adapters/rust/actix.rs

Option 3: Hyper (Low-Level Control)
├── Performance: Excellent, fine-grained control
├── Ecosystem: Minimal, for custom use
├── Best for: Custom protocols, embedded use cases
└── Files: fraiseql_rs/src/http/adapters/rust/hyper.rs
```

**Python Servers (Compatibility)**:

```
Option 4: FastAPI (Python Traditional)
├── Performance: 100 req/sec per core (compatible with v1.8.x)
├── Ecosystem: Popular, large community
├── Best for: Existing Python applications, team Python expertise
├── Status: Full support v2.0, maintenance v2.1+
└── Files: src/fraiseql/fastapi/ (same as v1.8.x)

Option 5: Starlette (Lightweight Python)
├── Performance: Similar to FastAPI, lightweight
├── Ecosystem: Minimal ASGI, flexible
├── Best for: Minimal Python deployments, custom ASGI
├── Status: Full support v2.0, maintenance v2.1+
└── Files: src/fraiseql/starlette/ (same as v1.8.x)
```

**Architecture**:
```
┌────────────────────────────────┐
│  HTTP Request                  │
└─────────────┬──────────────────┘
              ↓
┌────────────────────────────────┐
│  Choose Your Server            │
├──────────────┬─────────────────┤
│ Rust (Fast)  │ Python (Compat) │
├──────────────┼─────────────────┤
│ • Axum       │ • FastAPI       │
│ • Actix      │ • Starlette     │
│ • Hyper      │                 │
└──────────────┴────────┬────────┘
                        ↓
         ┌──────────────────────────┐
         │  Modular HTTP Core       │
         │  (Shared, Framework-     │
         │   Agnostic Rust)         │
         ├──────────────────────────┤
         │  • Router                │
         │  • Handler Dispatch      │
         │  • Response Building     │
         └────────────┬─────────────┘
                      ↓
         ┌──────────────────────────┐
         │  Middleware Pipeline     │
         │  (Same for all servers)  │
         ├──────────────────────────┤
         │  • Auth (optional)       │
         │  • Caching (optional)    │
         │  • Rate Limiting (opt.)  │
         │  • Custom (user-defined) │
         └────────────┬─────────────┘
                      ↓
         ┌──────────────────────────┐
         │  GraphQL Execution       │
         │  (Shared Rust Pipeline)  │
         └────────────┬─────────────┘
                      ↓
         ┌──────────────────────────┐
         │  HTTP Response           │
         └──────────────────────────┘
```

**Benefits**:
- ✅ **Framework flexibility**: 5 options (3 Rust + 2 Python)
- ✅ **Performance options**: 7-10x faster with Rust, compatible with Python
- ✅ **Backward compatibility**: FastAPI/Starlette work exactly like v1.8.x
- ✅ **Modular middleware**: Same across all servers
- ✅ **Gradual migration**: Start with Python, move to Rust later
- ✅ **Unified core**: Single GraphQL execution path

**Status**:
- ✅ **Rust Servers**: Primary focus, production-ready (Axum recommended)
- ✅ **Python Servers**: Full support v2.0, maintenance v2.1+
- ✅ **Migration Path**: Python → Rust when ready

**When to modify**: HTTP routing, middleware behavior, server implementation
**See**: `docs/DEPRECATION_POLICY.md` for detailed server comparison

---

### Tier 5: Enterprise Features

Production-grade capabilities for large deployments.

```
enterprise/
├── auth/                       # Authentication
│   ├── auth0.py               # Auth0 integration
│   ├── jwt.py                 # JWT validation
│   └── native.py              # Built-in auth
├── rbac/                       # Role-Based Access Control
│   ├── resolver.py            # RBAC enforcement
│   ├── hierarchy.py           # Role hierarchy
│   ├── cache.py               # Cache strategy
│   └── rust_resolver.py       # Rust integration
├── audit/                      # Audit logging
│   ├── event_logger.py        # Log events
│   ├── mutations.py           # Mutation tracking
│   └── types.py               # Event types
└── security/                   # Field-level security
    ├── field_auth.py          # Field access control
    ├── constraints.py         # Validation rules
    └── validators.py          # Custom validators
```

**Key Concepts**:
- **Auth**: Multiple authentication strategies (Auth0, JWT, custom)
- **RBAC**: Fine-grained role-based permissions
- **Audit**: Comprehensive change tracking
- **Security**: Field-level access control

**When to modify**: Authentication rules, authorization logic, audit requirements

---

### Tier 6: Advanced Features

Optional capabilities for sophisticated use cases.

```
federation/                     # Apollo Federation
├── entities.py                # Entity resolution
├── decorators.py              # Federation decorators
└── batch_executor.py          # Batch execution

subscriptions/                  # Real-Time Updates
├── manager.py                 # Subscription management
├── protocol.py                # WebSocket protocol
└── lifecycle.py               # Subscription lifecycle

caching/                        # Query Result Caching
├── result_cache.py            # Cache strategy
├── postgres_cache.py          # PostgreSQL backend
├── cache_key.py               # Key generation
└── repository_integration.py   # CQRS integration

optimization/                   # Performance
├── dataloader.py              # Batch data loading
├── n_plus_one_detector.py     # N+1 detection
└── loaders.py                 # Loader utilities
```

**When to modify**: Adding federation support, subscription features, caching strategy

---

### Tier 7: Database & CQRS

Data access patterns and persistence.

```
db.py                          # Main database module (entry point)
cqrs/
├── executor.py                # Query execution
├── repository.py              # CQRS pattern
├── pagination.py              # Pagination logic
└── repository.pyi             # Type stubs

middleware/
├── apq.py                     # Automatic Persisted Queries
├── apq_caching.py             # APQ caching
├── rate_limiter.py            # Rate limiting
├── body_size_limiter.py       # Request size limits
└── graphql_info_injector.py   # Context injection
```

**Key Concepts**:
- **CQRS**: Command Query Responsibility Segregation pattern
- **APQ**: Automatic Persisted Queries for production
- **Pagination**: Cursor-based and offset-based

**When to modify**: Database connection behavior, CQRS patterns

---

### Tier 8: CLI & Tools

Command-line utilities and development tools.

```
cli/
├── commands/                  # CLI commands
│   ├── init.py               # Project initialization
│   ├── schema.py             # Schema inspection
│   └── migrate.py            # Database migrations
├── monitoring/               # Monitoring commands
└── cli.py                    # Main CLI entry point

introspection/                # Schema introspection
├── introspector.py           # Introspection engine
├── schema_builder.py         # Schema graph building
└── types.py                  # Introspection types

migration/                    # Database migration tools
├── alembic/                  # Alembic integration
└── migrations/               # Migration scripts
```

**When to modify**: Adding CLI commands, introspection features

---

### Tier 9: Utilities

Helper functions and common utilities.

```
utils/
├── casing.py                 # camelCase/snake_case conversion
├── field_helpers.py          # Field utilities
├── introspection.py          # Introspection helpers
└── decorators.py             # Decorator utilities

errors/                       # Error handling
├── base.py                   # Base error classes
├── validation.py             # Validation errors
└── graphql_errors.py         # GraphQL-specific errors

middleware/                   # Middleware components
└── [middleware implementations]
```

**When to modify**: Adding utility functions, error types

---

## Rust Extension (`fraiseql_rs/src/`)

The high-performance pipeline for JSON transformation and query execution.

```
fraiseql_rs/src/
├── http/                      # HTTP server (Axum-based)
├── graphql/                   # GraphQL parser, complexity analysis
├── query/                     # Query field analysis
├── mutation/                  # Mutation execution
├── cache/                     # Cache coherency
├── db/                        # Connection pool, transactions
├── auth/                      # JWT validation, RBAC
├── security/                  # CORS, CSRF, rate limiting
├── subscriptions/             # WebSocket management
├── pipeline/                  # Unified execution engine
├── response/                  # JSON transformation
├── federation/                # Apollo Federation
└── rbac/                      # Authorization logic
```

**Key Characteristics**:
- **Performance**: 7-10x faster than pure Python
- **JSON Transformation**: Handles complex nested structures
- **Async/Await**: Full async/await support
- **Type Safety**: Rust's type system ensures safety

**When to modify**: Performance optimization, JSON processing, async behavior

---

## Test Suite

Comprehensive testing with 5,991+ tests across 730 files.

### Organization Structure

```
tests/
├── unit/                      # 150+ files - Fast, isolated
│   ├── core/                 # Framework internals
│   ├── types/                # 100+ scalar type tests
│   ├── decorators/           # Decorator behavior
│   ├── caching/              # Cache logic
│   ├── validation/           # Input validation
│   ├── utils/                # Helper functions
│   ├── sql/                  # SQL generation (no DB)
│   └── mutations/            # Mutation patterns
│
├── integration/              # 200+ files - Component interaction
│   ├── database/
│   │   ├── repository/       # CQRS tests (25+ files)
│   │   └── sql/              # SQL with DB (25+ files)
│   ├── graphql/
│   │   ├── queries/          # Query execution
│   │   ├── mutations/        # Mutation patterns
│   │   ├── subscriptions/    # WebSocket (4 files)
│   │   └── schema/           # Schema building
│   ├── auth/                 # Authentication (15 files)
│   ├── caching/              # Cache behavior
│   ├── enterprise/
│   │   ├── rbac/             # Role-based access
│   │   └── audit/            # Audit logging
│   └── performance/          # N+1 detection, dataloaders
│
├── system/                   # 50+ files - End-to-end
│   ├── fastapi_system/       # App startup, endpoints
│   ├── cli/                  # Command-line interface
│   └── deployment/           # Monitoring, tracing
│
├── regression/               # 40+ files - Bug-specific
│   ├── v0_1_0/              # Version-specific tests
│   ├── issue_124/           # WHERE clause filtering
│   ├── issue_145/           # Issue-specific fixes
│   └── golden/              # Golden test cases
│
├── chaos/                    # 40+ files - Failure scenarios
│   ├── auth/                # Auth chaos
│   ├── cache/               # Cache failure
│   ├── concurrency/         # Race conditions
│   ├── database/            # Pool exhaustion
│   ├── network/             # Latency/packet loss
│   └── resources/           # Resource exhaustion
│
├── fixtures/                # Shared utilities
│   ├── database/            # DB setup/teardown
│   ├── auth/                # Auth helpers
│   ├── graphql/             # Query client
│   └── common/              # Shared utilities
│
└── conftest.py              # Pytest configuration & fixtures
```

### Test Categories (Pytest Markers)

```
# Test type
@pytest.mark.unit           # Fast, isolated
@pytest.mark.integration    # Component interaction
@pytest.mark.e2e            # End-to-end
@pytest.mark.performance    # Performance tests

# Feature area
@pytest.mark.database       # Database operations
@pytest.mark.auth           # Authentication
@pytest.mark.enterprise     # Enterprise features
@pytest.mark.graphql        # GraphQL execution

# Special categories
@pytest.mark.regression     # Bug-specific tests
@pytest.mark.chaos          # Failure scenarios
@pytest.mark.skip_ci        # Skip in CI
@pytest.mark.profile        # Profiling enabled
```

### Running Tests

```bash
# All tests
make test

# By category
pytest -m unit              # Fast tests only
pytest -m integration       # Integration tests
pytest -m "unit or integration"  # Multiple categories

# By feature
pytest -m auth              # Auth-related
pytest -m database          # Database tests

# By file
pytest tests/unit/core/     # Core unit tests
pytest tests/regression/issue_124/  # Specific issue
```

---

## Naming Conventions

### Python Files & Modules

**Pattern**: `snake_case.py`

```
src/fraiseql/graphql_type.py       ✅ Correct
src/fraiseql/GraphQLType.py        ❌ Incorrect

tests/unit/test_graphql_type.py    ✅ Correct
tests/unit/TestGraphQLType.py      ❌ Incorrect
```

### Python Classes

**Pattern**: `PascalCase`

```
class GraphQLType:                 ✅ Correct
class graphql_type:                ❌ Incorrect

class WhereGenerator:              ✅ Correct
class where_generator:             ❌ Incorrect
```

### Python Functions

**Pattern**: `snake_case`

```
def execute_query():               ✅ Correct
def executeQuery():                ❌ Incorrect

def build_schema():                ✅ Correct
def buildSchema():                 ❌ Incorrect
```

### Test Classes

**Pattern**: `Test[ComponentName]`

```
class TestGraphQLType:             ✅ Correct
class TestForGraphQLType:          ❌ Incorrect

class TestWhereGenerator:          ✅ Correct
class WhereGeneratorTest:          ❌ Incorrect
```

### Test Methods

**Pattern**: `test_[specific_behavior]`

```
def test_parses_simple_type():     ✅ Correct
def test_simple_type_parsing():    ✅ Correct (alternative)
def test_should_parse_type():      ❌ Incorrect
def testParseSimpleType():         ❌ Incorrect
```

### Test Files

**Pattern**: `test_[module_name].py`

```
tests/unit/test_graphql_type.py    ✅ Correct
tests/unit/graphql_type_test.py    ❌ Incorrect
tests/unit/TestGraphQLType.py      ❌ Incorrect
```

### Rust Files

**Pattern**: `snake_case.rs`

```
fraiseql_rs/src/graphql_parser.rs  ✅ Correct
fraiseql_rs/src/GraphQLParser.rs   ❌ Incorrect
```

### Rust Modules

**Pattern**: `mod.rs` with snake_case subdirs

```
fraiseql_rs/src/graphql/mod.rs     ✅ Correct
fraiseql_rs/src/parser.rs          ✅ Correct (single file)
```

---

## Module Guidelines

### Creating New Modules

**Step 1**: Decide placement based on purpose

| Purpose | Location |
|---------|----------|
| Core behavior | `src/fraiseql/core/` |
| Type definition | `src/fraiseql/types/` |
| SQL generation | `src/fraiseql/sql/` |
| HTTP server | `src/fraiseql/fastapi/` (primary) |
| Enterprise | `src/fraiseql/enterprise/` |
| Performance | `src/fraiseql/optimization/` |
| Tools | `src/fraiseql/cli/` or `src/fraiseql/introspection/` |

**Step 2**: Create module structure

```python
# my_module/__init__.py
"""Module description.

This module handles [responsibility].

Example:
    >>> from fraiseql.my_module import my_function
    >>> result = my_function()

Exports:
    - MyClass: Main implementation
    - my_function(): Primary function
"""

from .implementation import MyClass, my_function

__all__ = ["MyClass", "my_function"]
```

**Step 3**: Add type hints

```python
from typing import Optional, List, Dict

def my_function(param: str) -> Optional[int]:
    """Process parameter.

    Args:
        param: Input string

    Returns:
        Processed integer or None
    """
    ...
```

**Step 4**: Add tests in parallel

```
tests/
├── unit/my_feature/
│   └── test_implementation.py
└── integration/my_feature/
    └── test_with_database.py
```

### File Size Guidelines

Keep code maintainable by limiting file sizes:

| File Type | Max Size | Guidance |
|-----------|----------|----------|
| Source files | 1,500 lines | Break into subpackages if larger |
| Test files | 500 lines | Each test file focused on one component |
| Module __init__.py | 100 lines | Mostly exports and docstring |
| Configuration | 200 lines | Split complex configs |

---

## Architecture

### Layering Pattern

FraiseQL follows a classic **layered architecture**:

```
┌─────────────────────────────────────┐
│   HTTP Layer (FastAPI, Axum, etc)   │  (public API)
├─────────────────────────────────────┤
│   Middleware (auth, caching, etc)   │  (request handling)
├─────────────────────────────────────┤
│   GraphQL Pipeline (execution)      │  (query processing)
├─────────────────────────────────────┤
│   Type System & Decorators          │  (schema definition)
├─────────────────────────────────────┤
│   SQL Generation & Query Builder    │  (database queries)
├─────────────────────────────────────┤
│   Database Connection & CQRS        │  (data access)
├─────────────────────────────────────┤
│   Rust Extension (performance)      │  (optimization)
└─────────────────────────────────────┘
```

### Request Flow

```
1. HTTP Request (FastAPI)
   ↓
2. Middleware (auth, rate limiting)
   ↓
3. GraphQL Pipeline (parse, validate)
   ↓
4. Type Resolution (decorator handlers)
   ↓
5. SQL Generation (WHERE, ORDER BY)
   ↓
6. Database Query (CQRS repository)
   ↓
7. Rust Transformation (JSON optimization)
   ↓
8. HTTP Response
```

### Design Patterns Used

| Pattern | Location | Purpose |
|---------|----------|---------|
| **Decorator** | `@fraise_type`, `@query`, `@mutation` | Schema definition |
| **Factory** | `core/graphql_pipeline.py` | Pipeline creation |
| **Strategy** | `sql/operators/` | Operator implementations |
| **Repository** | `cqrs/repository.py` | Data access |
| **Middleware** | `middleware/` | Request handling |
| **Singleton** | `core/registry.py` | Type registry |

---

## Documentation Structure

Related documentation files:

| File | Purpose |
|------|---------|
| `README.md` | Project overview |
| `ORGANIZATION.md` (this file) | Code structure |
| `DEPRECATION_POLICY.md` | Deprecation process |
| `docs/STRUCTURE_*.md` | Module-specific docs |
| `docs/architecture/` | Architecture decisions |
| `docs/migration/` | Migration guides |
| `docs/getting-started/` | Tutorials |

---

## Quick Links

- **[Add a new feature](docs/guides/adding-features.md)** - Step-by-step guide
- **[Write tests](docs/guides/testing.md)** - Testing best practices
- **[Deprecate a feature](docs/DEPRECATION_POLICY.md)** - Deprecation process
- **[Make a release](docs/RELEASE_WORKFLOW.md)** - Release checklist

---

## Getting Help

- **Navigate structure**: See this file's [Quick Navigation](#quick-navigation)
- **Find modules**: See [Python Framework](#python-framework-srcfraiseql) tiers
- **Understand patterns**: See [Design Patterns](#design-patterns-used)
- **See examples**: Check test files in `tests/unit/` and `tests/integration/`

---

**Last Updated**: January 8, 2026
**Maintainer**: FraiseQL Core Team
**Issues**: Create GitHub issue if structure becomes unclear
