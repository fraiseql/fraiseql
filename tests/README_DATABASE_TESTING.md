# Database Testing Guide

This guide explains how to use real PostgreSQL databases in tests instead of mocks.

## Quick Start

### Using Docker/Podman

1. **With Docker Compose:**
   ```bash
   docker-compose up -d
   pytest
   ```

2. **With Podman (pasta networking):**
   ```bash
   # Option 1: Use the provided script
   ./scripts/podman-postgres.sh

   # Option 2: Use podman-compose
   podman-compose -f podman-compose.yml up -d

   # Option 3: Run tests with Podman testcontainers
   TESTCONTAINERS_PODMAN=true pytest
   ```

### Test Fixtures

The new database testing setup provides these fixtures:

- `db_pool`: Session-scoped connection pool
- `db_connection`: Test-scoped connection with automatic rollback
- `db_cursor`: Cursor for simple operations
- `create_test_table`: Factory for creating test tables
- `create_test_view`: Factory for creating test views
- `db_connection_committed`: Connection with committed changes (for special cases)

## Writing Database Tests

### Basic Example

```python
import pytest
from fraiseql.db import DatabaseQuery, FraiseQLRepository
from psycopg.sql import SQL


@pytest.mark.database
class TestMyFeature:
    @pytest.fixture
    async def test_schema(self, db_connection, create_test_table):
        """Set up test tables."""
        await create_test_table(
            db_connection,
            "users",
            """
            CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                data JSONB NOT NULL DEFAULT '{}'::jsonb
            )
            """
        )

        # Insert test data
        await db_connection.execute("""
            INSERT INTO users (data) VALUES
            ('{"name": "Test User"}'::jsonb)
        """)

    @pytest.mark.asyncio
    async def test_query(self, repository, test_schema):
        """Test a database query."""
        query = DatabaseQuery(
            statement=SQL("SELECT data->>'name' as name FROM users"),
            params={},
            fetch_result=True
        )
        result = await repository.run(query)

        assert len(result) == 1
        assert result[0]["name"] == "Test User"
```

### Transaction Isolation

Each test runs in its own transaction that's automatically rolled back:

```python
@pytest.mark.asyncio
async def test_isolation(self, db_connection):
    # This insert will be rolled back after the test
    await db_connection.execute("INSERT INTO users (data) VALUES ('{}'::jsonb)")

    # Verify it was inserted within the test
    result = await db_connection.execute("SELECT COUNT(*) FROM users")
    count = await result.fetchone()
    assert count[0] == 1
    # After test completes, the insert is rolled back
```

### Using Committed Data

For tests that need committed data (e.g., testing transaction behavior):

```python
@pytest.mark.asyncio
async def test_with_commits(self, db_connection_committed):
    # This fixture creates a unique schema for isolation
    # Changes are committed but the entire schema is dropped after
    await db_connection_committed.execute("CREATE TABLE test (id INT)")
    await db_connection_committed.commit()
    # Table persists for the duration of the test
```

## Running Tests

### All Tests (including database tests)
```bash
pytest
```

### Only Database Tests
```bash
pytest -m database
```

### Skip Database Tests
```bash
pytest --no-db
```

### With Coverage
```bash
pytest --cov=src/fraiseql -m database
```

### Parallel Execution
```bash
pytest -n auto  # Uses pytest-xdist
```

## Environment Variables

- `TESTCONTAINERS_PODMAN=true`: Use Podman instead of Docker
- `TESTCONTAINERS_RYUK_DISABLED=true`: Disable Ryuk (cleanup container)
- `POSTGRES_IMAGE=postgres:15`: Use a different PostgreSQL version

## Migration from Mocks

To migrate existing mock-based tests:

1. Run the migration analyzer:
   ```bash
   python scripts/migrate_tests_to_real_db.py
   ```

2. Follow the generated suggestions for each test file

3. Key changes:
   - Replace `mock_pool` → `db_pool`
   - Replace `AsyncMock()` → real database operations
   - Add `@pytest.mark.database` to test classes
   - Create test schema using fixtures

## Best Practices

1. **Use transactions for isolation**: The default `db_connection` fixture provides this
2. **Create minimal schemas**: Only create tables/data needed for each test
3. **Use meaningful test data**: Makes tests easier to understand
4. **Verify both positive and negative cases**: Test error conditions too
5. **Keep tests independent**: Don't rely on execution order

## Troubleshooting

### Container fails to start
- Check if Docker/Podman is running
- Verify no port conflicts on 5432/5433
- Check container logs: `docker logs <container-id>`

### Tests are slow
- Reuse the session-scoped pool
- Use `pytest-xdist` for parallel execution
- Consider keeping unit tests with mocks for non-database logic

### Permission errors with Podman
- Use `TESTCONTAINERS_PODMAN=true`
- Ensure proper SELinux labels (`:Z` suffix on volumes)
- Consider `--userns=keep-id` flag

### Database not found errors
- Ensure containers are running
- Check connection string in test output
- Verify PostgreSQL is healthy: `docker exec <container> pg_isready`
