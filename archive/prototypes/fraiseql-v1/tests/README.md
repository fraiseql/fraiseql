# FraiseQL Tests

Comprehensive test suite for FraiseQL v1.

## Structure

### Unit Tests (`unit/`)
Test individual components in isolation:
- `test_types.py` - Type system and decorators
- `test_repositories.py` - CommandRepository, QueryRepository
- `test_where_builder.py` - SQL WHERE clause generation
- `test_rust_transformer.py` - Rust transformation
- `test_decorators.py` - @query, @mutation registration
- `test_schema_builder.py` - GraphQL schema generation

### Integration Tests (`integration/`)
Test complete workflows:
- `test_cqrs_flow.py` - Command → Sync → Query flow
- `test_graphql_queries.py` - End-to-end GraphQL queries
- `test_mutations.py` - End-to-end mutations
- `test_performance.py` - Performance benchmarks

## Running Tests

### All tests
```bash
uv run pytest
```

### Specific test file
```bash
uv run pytest tests/unit/test_types.py -v
```

### With coverage
```bash
uv run pytest --cov=fraiseql --cov-report=html
```

### Integration tests only
```bash
uv run pytest tests/integration/ -v
```

## Test Database Setup

Integration tests require PostgreSQL:

```bash
# Create test database
createdb fraiseql_test

# Run migrations (when available)
psql fraiseql_test < tests/fixtures/schema.sql
```

## Writing Tests

Follow these patterns:

### Unit Test Example
```python
import pytest
from fraiseql.types import type

def test_type_decorator():
    @type
    class TestUser:
        id: int
        name: str

    assert TestUser.__name__ == "TestUser"
    # Verify registration, schema generation, etc.
```

### Integration Test Example
```python
import pytest
from fraiseql.repositories import CommandRepository, QueryRepository
from fraiseql.repositories.sync import sync_tv_user

@pytest.mark.asyncio
async def test_cqrs_flow(db_connection):
    # Write to command side
    cmd_repo = CommandRepository(db_connection)
    user_id = await cmd_repo.execute(
        "INSERT INTO tb_user (name) VALUES ($1) RETURNING id",
        "Alice"
    )

    # Explicit sync
    await sync_tv_user(db_connection, user_id)

    # Read from query side
    query_repo = QueryRepository(db_connection)
    user = await query_repo.find_one("tv_user", id=user_id)

    assert user["name"] == "Alice"
```

## Goals

- **100% Coverage**: All core functionality must be tested
- **Fast**: Unit tests < 1s, integration tests < 10s
- **Reliable**: No flaky tests
- **Clear**: Test names describe what they test
