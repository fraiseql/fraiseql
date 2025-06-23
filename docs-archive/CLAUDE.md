# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

FraiseQL is a strongly-typed GraphQL-to-PostgreSQL translator that bridges GraphQL queries with PostgreSQL's JSONB capabilities. It automatically generates GraphQL schemas from Python type definitions and converts GraphQL queries into optimized PostgreSQL queries.

## Development Commands

### Setup

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

### Testing

```bash
# Run all tests with Podman (REQUIRED)
TESTCONTAINERS_PODMAN=true pytest

# Run a specific test file with Podman
TESTCONTAINERS_PODMAN=true pytest tests/path/to/test_file.py

# Run a specific test with Podman
TESTCONTAINERS_PODMAN=true pytest tests/path/to/test_file.py::test_function_name

# Run tests with verbose output and Podman
TESTCONTAINERS_PODMAN=true pytest -v

# Run tests with coverage and Podman
TESTCONTAINERS_PODMAN=true pytest --cov=src/fraiseql

# Export the environment variable for the session
export TESTCONTAINERS_PODMAN=true
pytest  # Now all tests will use Podman
```

### Code Quality

```bash
# Format code with black
black src/ tests/

# Lint with ruff (configured for ALL checks)
ruff check src/

# Fix linting issues automatically
ruff check src/ --fix

# Type checking with mypy
mypy src/

# Type checking with pyright
pyright
```

## Architecture Overview

### Core Design Pattern

FraiseQL uses a JSONB-based data model where all entity data is stored in a `data` JSONB column. The system translates GraphQL queries into PostgreSQL queries that leverage JSONB operators for field extraction and filtering.

### Key Components

1. **Type System** (`src/fraiseql/types/`)
   - `@fraise_type`: Decorator for GraphQL output types
   - `@fraise_input`: Decorator for GraphQL input types
   - `fraise_field()`: Field definitions with metadata (purpose, defaults, descriptions)
   - Custom scalars in `scalars/`: UUID, DateTime, DateRange, IPAddress, JSON, etc.

2. **SQL Generation** (`src/fraiseql/sql/`)
   - Builds SELECT queries with JSONB path extraction (`data->>'field'`)
   - Dynamic WHERE clause generation with type-safe filters
   - ORDER BY and GROUP BY support

3. **GraphQL Layer** (`src/fraiseql/gql/`)
   - Central schema registry with singleton pattern
   - Automatic resolver wrapping for async functions
   - Fragment resolution support

4. **Mutations** (`src/fraiseql/mutations/`)
   - Result pattern with `@success` and `@failure` decorators for union types
   - PostgreSQL function call generation for stored procedures

### Testing Strategy

- Always use the `clear_registry` fixture when testing types to ensure clean state
- Test files mirror the source structure under `tests/`
- Registry isolation is critical - the schema builder uses a singleton pattern
- When testing SQL generation, verify both the query structure and parameter values
- **ALWAYS use Podman for database tests** - set `TESTCONTAINERS_PODMAN=true` environment variable
- **NEVER use mocks in tests** - always test with real implementations using Podman containers
- For database tests, use the fixtures from `database_conftest.py` which provide real PostgreSQL via testcontainers

### Important Patterns

1. **Field Access**: All fields are accessed via JSONB operators:

   ```sql
   SELECT data->>'name' AS name FROM users
   ```

2. **Type Registration**: Types must be registered before schema building:

   ```python
   @fraise_type
   class User:
       name: str = fraise_field(purpose="User's display name")
   ```

3. **Async Repository Pattern**: Database operations use async/await with psycopg3

4. **Camel Case Conversion**: Python snake_case automatically converts to GraphQL camelCase

### Common Development Tasks

When adding new features:

1. Define types with appropriate decorators (`@fraise_type`, `@fraise_input`)
2. Use `fraise_field()` for field metadata
3. Register custom scalars if needed
4. Write tests with registry cleanup
5. Run type checkers and linters before committing

### TestFoundry Extension

TestFoundry is an automated test generation framework for PostgreSQL databases that integrates with FraiseQL. It uses a specific PostgreSQL schema (`testfoundry`) to isolate its functions and tables.

Key points when working with TestFoundry:
- SQL functions are installed in the `testfoundry` schema
- Functions within the schema reference each other with the `testfoundry_` prefix
- The generator sets the search path to include the testfoundry schema
- Test generation creates pgTAP tests that use psql variables with `\gset`

### Commit messages

Never mention Claude / Anthropic in the commit messages.
Always try to fix the errors and never --no-verify.
