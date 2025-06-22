# Unified Container Testing Architecture

FraiseQL uses a **unified container approach** for database testing to maximize performance and simplicity.

## Key Features

### 🚀 Single Container Per Test Session
- One PostgreSQL container is started and reused for the entire test session
- Container is cached in `_container_cache` for test reruns
- Automatic cleanup after all tests complete

### 🔌 Socket Communication
- Podman uses Unix domain socket: `/run/user/{uid}/podman/podman.sock`
- Docker uses its standard socket
- Significantly faster than HTTP-based container communication

### 🔄 Connection Pooling
- Session-scoped connection pool (2-10 connections)
- Shared across all tests for efficiency
- Individual test isolation via transactions

## Architecture

```
┌─────────────────────────────────────────┐
│         Test Session Start              │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│    PostgreSQL Container (Session)       │
│  • Started once per session             │
│  • Cached for reruns                    │
│  • Socket communication                 │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│      Connection Pool (Session)          │
│  • Min 2, Max 10 connections            │
│  • Shared across tests                  │
└────────────────┬────────────────────────┘
                 │
        ┌────────┴────────┬────────────┐
        ▼                 ▼            ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│    Test 1     │ │    Test 2     │ │    Test N     │
│ • Transaction │ │ • Transaction │ │ • Transaction │
│ • Rollback    │ │ • Rollback    │ │ • Rollback    │
└───────────────┘ └───────────────┘ └───────────────┘
```

## Usage

### Basic Test with Database
```python
import pytest
from fraiseql.db import DatabaseQuery

@pytest.mark.database
async def test_with_database(db_connection):
    """Test using the unified container's connection."""
    result = await db_connection.execute(
        "SELECT 1 as value"
    )
    assert result.fetchone()["value"] == 1
```

### Using Connection Pool
```python
@pytest.mark.database
async def test_with_pool(db_pool):
    """Test using the connection pool directly."""
    async with db_pool.connection() as conn:
        # Your test code here
        pass
```

### Tests Requiring Committed Data
```python
@pytest.mark.database
async def test_with_commits(db_connection_committed):
    """Test that needs data to persist across queries."""
    # Uses isolated schema, cleaned up after test
    await db_connection_committed.execute(
        "CREATE TABLE test_table (id INT)"
    )
    await db_connection_committed.commit()
    # Table persists within test, cleaned up after
```

## Configuration

### Environment Variables

```bash
# Use Podman instead of Docker
export TESTCONTAINERS_PODMAN=true

# Skip container tests entirely
pytest --no-db

# Run only database tests
pytest -m database
```

### Podman Socket Setup

The system automatically configures Podman socket:
```python
# From database_conftest.py
os.environ["DOCKER_HOST"] = f"unix:///run/user/{os.getuid()}/podman/podman.sock"
os.environ["TESTCONTAINERS_RYUK_DISABLED"] = "true"
```

## Performance Benefits

1. **Container Reuse**: Single container for all tests vs one per test
2. **Socket Communication**: Unix domain socket is faster than TCP/HTTP
3. **Connection Pooling**: Reuse connections instead of creating new ones
4. **Transaction Isolation**: Rollback instead of schema creation/deletion

## Implementation Details

The unified container system is implemented in `tests/database_conftest.py`:

- `postgres_container`: Session-scoped fixture providing the container
- `db_pool`: Session-scoped connection pool
- `db_connection`: Test-scoped connection with automatic rollback
- `db_connection_committed`: For tests needing persistent changes

## Troubleshooting

### Container Not Starting
```bash
# Check Podman socket
systemctl --user status podman.socket

# Check Docker daemon
docker info
```

### Tests Skipped
```bash
# Ensure container runtime is available
podman info  # or docker info

# Check if testcontainers is installed
pip install testcontainers[postgres]
```

### Performance Issues
- Ensure you're using the session-scoped fixtures
- Check that connection pool size is appropriate
- Verify socket communication is being used (not TCP)