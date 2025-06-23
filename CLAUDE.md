# Claude Code Context for FraiseQL

This document provides essential context for Claude Code when working on the FraiseQL project.

## Project Overview

FraiseQL is a lightweight GraphQL-to-PostgreSQL query builder using JSONB. It provides:
- GraphQL schema generation from Python type annotations
- Automatic SQL query generation
- JSONB-based data storage
- FastAPI integration
- Production-ready features (auth, monitoring, caching)

## Testing with Containers

FraiseQL uses a unified container testing system that supports both Docker and Podman.

### Running Tests with Podman

If you have Podman installed instead of Docker, set these environment variables before running tests:

```bash
export TESTCONTAINERS_PODMAN=true
export TESTCONTAINERS_RYUK_DISABLED=true
pytest
```

Or use the provided test runner script:
```bash
./scripts/test_with_podman.sh
```

### Database Tests

All database tests use the unified container system from `tests/database_conftest.py`:
- Single PostgreSQL container for the entire test session
- Each test runs in its own transaction that is rolled back
- Supports both Docker and Podman
- Socket communication for better performance

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
3. Run tests: `pytest` (or `./scripts/test_with_podman.sh` for Podman)
4. Commit with descriptive message
5. Push to branch and create PR

## Important Files

- `src/fraiseql/fastapi/app.py` - Main FastAPI application factory
- `src/fraiseql/gql/schema_builder.py` - GraphQL schema generation
- `src/fraiseql/sql/where_generator.py` - SQL WHERE clause generation
- `tests/database_conftest.py` - Unified container testing setup
