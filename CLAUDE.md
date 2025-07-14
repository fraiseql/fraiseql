# Claude Code Context for FraiseQL

This document provides essential context for Claude Code when working on the FraiseQL project.

## Project Overview

FraiseQL is a lightweight GraphQL-to-PostgreSQL query builder using JSONB. It provides:
- GraphQL schema generation from Python type annotations
- Automatic SQL query generation
- JSONB-based data storage
- FastAPI integration
- Production-ready features (auth, monitoring, caching)

## Testing with Hybrid Database Approach

FraiseQL uses a **hybrid database testing system** that automatically adapts to your environment for optimal speed:

### Quick Start (Recommended)
```bash
# 1. Install PostgreSQL locally for fastest tests
brew install postgresql  # macOS
sudo apt install postgresql  # Ubuntu

# 2. Create test database
createdb fraiseql_test

# 3. Set environment variable
export TEST_DATABASE_URL="postgresql://localhost/fraiseql_test"

# 4. Run tests (super fast!)
pytest tests -m database  # ~5 seconds vs ~60 seconds with containers
```

### Automatic Fallback
```bash
# If no local PostgreSQL, testcontainers auto-starts one
pytest tests -m database  # Slower but works everywhere
```

### Test Categories
```bash
# Unit tests (no database required)
pytest tests -m "not database"  # ~10 seconds

# Database integration tests
pytest tests -m database  # Speed depends on setup

# All tests
pytest tests  # Combines both
```

### Performance Comparison
- **Local PostgreSQL**: ⚡ ~5 seconds (10-20x faster)
- **CI/CD PostgreSQL Service**: ⚡ ~30 seconds
- **Testcontainers**: 🐌 ~60 seconds (fallback)

### Database Test Architecture

FraiseQL supports two database testing patterns:

#### Transaction Rollback Pattern (Default)
- Each test runs in its own transaction that gets rolled back
- Fast cleanup with perfect test isolation
- Works well for simple unit tests

#### Committed Data Fixture Pattern (For Integration Tests)
- Creates unique schemas for complex multi-query tests
- Data is committed and visible across connection pool
- Automatic schema cleanup after test completion
- Required for tests that need data visibility across queries

Use `db_connection` for rollback tests, `db_connection_committed` for integration tests.

**📖 See [Database Testing Guide](docs/testing/database-testing-guide.md) for complete setup instructions.**

## Code Style and Linting

Always run these commands before committing:
```bash
ruff check src/ tests/ --fix
ruff format src/ tests/
```

## Common Commands

- Run tests: `pytest`
- Run specific test: `pytest tests/path/to/test.py::TestClass::test_method`
- Run non-database tests: `pytest -m "not database"`
- Check types: `pyright`
- Build package: `python -m build`

## Architecture Notes

- GraphQL types are defined using `@fraise_type` decorator
- Database queries use JSONB columns for flexible schema
- TurboRouter provides optimized query execution in production
- All database access goes through `FraiseQLRepository`

## Development Workflow

1. Make changes
2. Run linting: `ruff check --fix && ruff format`
3. Run tests: `pytest`
4. Commit with descriptive message
5. Push to branch and create PR

## Important Files

- `src/fraiseql/fastapi/app.py` - Main FastAPI application factory
- `src/fraiseql/gql/schema_builder.py` - GraphQL schema generation
- `src/fraiseql/sql/where_generator.py` - SQL WHERE clause generation
- `tests/database_conftest.py` - Unified container testing setup

## Stack-Specific Development Patterns

### FraiseQL Type Definitions
```python
# Always use @fraise_type decorator
@fraise_type
class User:
    id: UUID
    name: str
    email: str
```

### Query Functions (NOT resolvers)
```python
# Correct: Function-based queries
@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view", limit=limit)
```

### Database Views with JSONB Pattern
```sql
-- All views must have 'data' column with JSONB
CREATE VIEW user_view AS
SELECT
    id,              -- For filtering
    tenant_id,       -- For access control
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data        -- REQUIRED: All object data here
FROM users;
```

### Frontend Integration (Vue/Nuxt)
- Use GraphQL clients like Apollo or urql
- Leverage FraiseQL's type safety with generated TypeScript types
- Implement optimistic updates for mutations

### Multi-tenant SaaS Patterns
- Always include tenant_id in views for row-level security
- Use PostgreSQL RLS (Row Level Security) when possible
- Filter by tenant_id in all queries

## Useful Utility Modules

### Common Input/Output Types
- `types/common_inputs.py` - Contains `DeletionInput` for standardized delete operations
- `types/common_outputs.py` - Contains `MutationResultRow` and `MUTATION_STATUS_MAP` for mutation results
- `types/protocols.py` - Type protocols for `FraiseQLInputType` and `FraiseQLOutputType`

### Introspection & Analysis
- `utils/introspection.py` - `describe_type()` function for analyzing FraiseQL types
- Use `describe_type(MyType)` to get field metadata, SQL bindings, and type information

### Network Utilities
- `utils/ip_utils.py` - IPv4 validation and subnet mask utilities
- `is_ipv4_address(ip)` - Validate IPv4 addresses
- `ipv4_mask_len(netmask)` - Calculate CIDR prefix length from netmask

### GraphQL Processing
- `mutations/selection_filter.py` - Filter mutation results based on GraphQL selection sets
- `gql/graphql_entrypoint.py` - Alternative GraphQL HTTP router (GraphNoteRouter)

### Database Testing Utilities
- `tests/utils/schema_utils.py` - Schema-qualified query helpers for database tests
- `get_current_schema(connection)` - Get current schema name from database connection
- `SchemaQualifiedQueryBuilder` - Fluent interface for building schema-qualified SQL
- `build_select_query()`, `build_insert_query()`, etc. - Convenience query builders
- Use for tests with `db_connection_committed` fixture to ensure proper schema isolation
