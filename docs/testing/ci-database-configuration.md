# CI/CD Database Configuration

This document explains how FraiseQL's test suite handles database connections in different environments.

## Overview

FraiseQL supports multiple database configuration methods to accommodate different CI/CD environments:

1. **Testcontainers** (default for local development)
2. **External Database** (for CI/CD environments like GitHub Actions)
3. **Podman Support** (alternative to Docker)

## Configuration Methods

### 1. Testcontainers (Local Development)

By default, tests use testcontainers to spin up a PostgreSQL container:

```bash
# Standard test run (uses Docker)
pytest

# With Podman
export TESTCONTAINERS_PODMAN=true
export TESTCONTAINERS_RYUK_DISABLED=true
pytest

# Or use the helper script
./scripts/test_with_podman.sh
```

### 2. External Database (CI/CD)

In CI/CD environments, you can use an external PostgreSQL service:

```bash
# Set the database URL
export TEST_DATABASE_URL=postgresql://user:pass@localhost:5432/test_db
# or
export DATABASE_URL=postgresql://user:pass@localhost:5432/test_db

# Run tests
pytest
```

When `TEST_DATABASE_URL` or `DATABASE_URL` is set, the test suite will:
- Skip testcontainers initialization
- Use the provided database URL directly
- Still maintain test isolation via transactions

### 3. GitHub Actions Configuration

Our GitHub Actions workflows use service containers:

```yaml
services:
  postgres:
    image: postgres:16-alpine
    env:
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: fraiseql
      POSTGRES_DB: fraiseql_test
    ports:
      - 5432:5432

env:
  TEST_DATABASE_URL: postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test
```

## Test Markers

Database tests are marked with `@pytest.mark.database`:

```python
@pytest.mark.database
class TestDatabaseIntegration:
    async def test_something(self, db_pool):
        # Test code
```

To run only non-database tests:
```bash
pytest -m "not database"
```

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `TEST_DATABASE_URL` | External database URL (preferred) | `postgresql://user:pass@host:5432/db` |
| `DATABASE_URL` | Alternative database URL | `postgresql://user:pass@host:5432/db` |
| `TESTCONTAINERS_PODMAN` | Enable Podman support | `true` |
| `TESTCONTAINERS_RYUK_DISABLED` | Disable Ryuk for Podman | `true` |

## Troubleshooting

### Docker/Podman Not Available

If you see "Docker not available" errors:
1. Install Docker or Podman
2. For Podman, set the environment variables:
   ```bash
   export TESTCONTAINERS_PODMAN=true
   export TESTCONTAINERS_RYUK_DISABLED=true
   ```

### Permission Denied Errors

For Docker socket permission errors:
- Add your user to the docker group: `sudo usermod -aG docker $USER`
- Or use Podman which doesn't require root

### Disk Space Issues

If you encounter "no space left on device":
- Clean up containers: `podman system prune -a --volumes -f`
- Clear Docker cache: `docker system prune -a --volumes -f`

## Best Practices

1. **Local Development**: Use testcontainers for isolation and reproducibility
2. **CI/CD**: Use service containers with `TEST_DATABASE_URL` for speed
3. **Test Isolation**: All database tests run in transactions that are rolled back
4. **Parallel Testing**: The unified container system supports parallel test execution
