# Running the Complete Test Suite with Podman

## Problem Summary

Several tests are being skipped even though Podman is available because:

1. **Docker-specific tests**: Some tests use `@requires_docker` which specifically checks for Docker, not Podman
2. **Hardcoded database connections**: Some tests (like CQRS pagination) expect a database on specific ports
3. **Missing environment variables**: Tests need `TESTCONTAINERS_PODMAN=true` and `TESTCONTAINERS_RYUK_DISABLED=true`
4. **Optional dependencies**: OpenTelemetry tests skip when the package isn't installed

## Solution

### 1. Set Environment Variables

Add to your shell profile (`.bashrc`, `.zshrc`, etc.):

```bash
export TESTCONTAINERS_PODMAN=true
export TESTCONTAINERS_RYUK_DISABLED=true
```

Or create a `.env.test` file:

```bash
TESTCONTAINERS_PODMAN=true
TESTCONTAINERS_RYUK_DISABLED=true
```

### 2. Run Tests with Podman

```bash
# Run all tests with Podman support
TESTCONTAINERS_PODMAN=true TESTCONTAINERS_RYUK_DISABLED=true pytest

# Or use a test runner script
./scripts/test-with-podman.sh
```

### 3. Install Optional Dependencies

For OpenTelemetry tests:

```bash
pip install opentelemetry-api opentelemetry-sdk opentelemetry-instrumentation-fastapi
```

Or install all test dependencies:

```bash
pip install -e ".[test,tracing]"
```

## Tests That Need Updates

### 1. Docker-specific Tests

Files that use `@requires_docker`:
- `tests/deployment/test_docker.py` - Tests Docker build commands
- `tests/test_turbo_router_integration.py` - Has its own container setup

**Fix**: These should use `@requires_any_container` or be updated to work with both Docker and Podman.

### 2. Hardcoded Database Tests

- `tests/cqrs/test_pagination.py` - Expects database on port 5433
- `tests/sql/test_sql_injection_real_db.py` - May have similar issues

**Fix**: These should be updated to use the unified container system from `database_conftest.py`.

### 3. Production Mode Tests

- `tests/optimization/test_n_plus_one_detection.py` - One test requires a real database in production mode

**Fix**: This test is currently skipped, but could be updated to use a test container.

## Running All Tests

With proper setup, you should be able to run:

```bash
# Set up environment
export TESTCONTAINERS_PODMAN=true
export TESTCONTAINERS_RYUK_DISABLED=true

# Install all dependencies including optional ones
pip install -e ".[test,tracing]"

# Run all tests
pytest -xvs

# Expected result: All tests pass (except legitimately skipped ones)
```

## Container Best Practices

For tests that need additional services (like OpenTelemetry collector):

1. **Use testcontainers**: Add service containers to the unified system
2. **Session scope**: Start containers once per test session
3. **Socket communication**: Use Unix domain sockets for better performance
4. **Automatic cleanup**: Containers are cleaned up after tests

Example for OpenTelemetry:

```python
@pytest.fixture(scope="session")
def otel_collector_container():
    """Start OpenTelemetry collector for tracing tests."""
    if not HAS_DOCKER:
        pytest.skip("Container runtime not available")

    container = GenericContainer(
        image="otel/opentelemetry-collector:latest",
        ports={"4317": 4317},  # gRPC port
    )
    container.start()
    yield container
    container.stop()
```

## Conclusion

The test suite is designed to work completely on your laptop with Podman. The skipped tests are due to:
1. Environment variables not being set
2. Some tests not being updated to use the unified container system
3. Optional dependencies not being installed

With the fixes above, all tests should run successfully using Podman containers.
