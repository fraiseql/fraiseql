# Testing with FraiseQL

A comprehensive guide to testing GraphQL applications built with FraiseQL.

## Testing Philosophy

FraiseQL promotes a **database-first testing approach** where tests interact with real PostgreSQL databases in isolated environments. This ensures your tests accurately reflect production behavior while maintaining fast execution times through proper isolation strategies.

### Key Principles

- **Test Real Database Interactions**: Use actual PostgreSQL containers, not mocks
- **Async-First Testing**: All FraiseQL operations are async, tests should be too
- **Isolation by Design**: Each test gets its own database transaction or container
- **Performance Awareness**: Monitor query patterns and response times

## Testing Stack

```python
# Core testing dependencies (included in fraiseql[dev])
pytest>=8.3.5              # Modern test framework with async support
pytest-asyncio>=0.21.0     # Async test execution
pytest-cov>=4.0.0          # Coverage reporting
testcontainers[postgres]>=4.0.0  # Isolated PostgreSQL containers
pytest-mock>=3.11.0        # Advanced mocking capabilities
pytest-xdist>=3.5.0        # Parallel test execution

# Additional testing utilities
factory-boy>=3.3.0         # Test data factories
httpx>=0.25.0              # HTTP client for GraphQL requests
```

## Test Categories

### 1. Unit Tests
Test individual components in isolation:

- **Type Validation**: FraiseQL types and their field validations
- **Query Resolvers**: Logic without database interactions (mocked)
- **Mutation Handlers**: Business logic with mocked dependencies
- **Utility Functions**: Pure functions and helpers

### 2. Integration Tests
Test component interactions with real databases:

- **Repository Operations**: CRUD operations with PostgreSQL
- **Transaction Handling**: Rollback behavior and isolation
- **Database Function Calls**: PostgreSQL stored procedures
- **View Queries**: Complex SELECT operations

### 3. GraphQL API Tests
End-to-end testing of the GraphQL API:

- **Query Execution**: Full GraphQL query processing
- **Mutation Operations**: Complete mutation workflows
- **Error Handling**: GraphQL error responses and codes
- **Authentication**: Protected queries and mutations
- **Schema Validation**: Type definitions and introspection

### 4. Performance Tests
Ensure your API meets performance requirements:

- **Response Time Testing**: Latency measurements
- **Load Testing**: Concurrent user simulation
- **Query Optimization**: N+1 detection and prevention
- **Resource Usage**: Memory and connection monitoring

## Quick Start

### Installation

```bash
# Install FraiseQL with testing dependencies
pip install "fraiseql[dev]"

# Or if using requirements.txt
echo "fraiseql[dev]" >> requirements.txt
pip install -r requirements.txt
```

### Basic Test Setup

```python
# conftest.py
import pytest
import asyncio
from testcontainers.postgres import PostgresContainer
from fraiseql.repository import FraiseQLRepository

@pytest.fixture(scope="session")
def event_loop():
    """Create event loop for async tests"""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()

@pytest.fixture(scope="session")
async def postgres_container():
    """PostgreSQL container for tests"""
    with PostgresContainer("postgres:15-alpine") as postgres:
        yield postgres

@pytest.fixture
async def test_db(postgres_container):
    """Isolated database for each test"""
    database_url = postgres_container.get_connection_url()

    async with FraiseQLRepository(database_url) as repo:
        # Run test schema
        await repo.execute("""
            CREATE TABLE IF NOT EXISTS tb_user (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                email TEXT UNIQUE NOT NULL,
                created_at TIMESTAMP DEFAULT NOW()
            );

            CREATE OR REPLACE VIEW v_user AS
            SELECT id, jsonb_build_object(
                'id', id,
                'name', name,
                'email', email,
                'created_at', created_at
            ) AS data FROM tb_user;
        """)
        yield repo
```

### Running Tests

```bash
# Run all tests
pytest

# Run with coverage reporting
pytest --cov=src --cov-report=html --cov-report=term-missing

# Run only integration tests
pytest tests/integration/ -v

# Run tests in parallel
pytest -n auto

# Run specific test patterns
pytest -k "test_user" -v

# Run with detailed output
pytest -vvv --tb=short
```

## Test Organization

```
tests/
├── conftest.py              # Shared fixtures and configuration
├── unit/                    # Unit tests (no database)
│   ├── test_types.py
│   ├── test_queries.py
│   └── test_mutations.py
├── integration/             # Integration tests (with database)
│   ├── test_repository.py
│   ├── test_transactions.py
│   └── test_database_functions.py
├── api/                     # GraphQL API tests
│   ├── test_queries.py
│   ├── test_mutations.py
│   └── test_schema.py
├── performance/             # Performance and load tests
│   ├── test_response_times.py
│   └── locustfile.py
└── factories/               # Test data factories
    └── user_factory.py
```

## Environment Configuration

```bash
# .env.test
TEST_DATABASE_URL=postgresql://test:test@localhost:5432/test_db
FRAISEQL_LOG_LEVEL=DEBUG
PYTEST_PARALLEL_WORKERS=4
TESTCONTAINERS_RYUK_DISABLED=true  # Disable cleanup container
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: test
          POSTGRES_DB: fraiseql_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:

          - 5432:5432

    steps:

    - uses: actions/checkout@v4

    - name: Set up Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.11'

    - name: Install dependencies
      run: |
        pip install -e ".[dev]"

    - name: Run tests
      env:
        TEST_DATABASE_URL: postgresql://postgres:test@localhost/fraiseql_test
      run: |
        pytest tests/ \
          --cov=src/fraiseql \
          --cov-report=xml \
          --cov-report=term-missing \
          -n auto

    - name: Upload coverage
      uses: codecov/codecov-action@v3
      with:
        file: ./coverage.xml
```

## Coverage Goals

- **Overall Coverage**: 85%+ for production code
- **Critical Paths**: 95%+ for authentication, payments, data mutations
- **Error Handling**: Test all error conditions and edge cases
- **Integration Points**: 90%+ for database interactions

## Next Steps

1. **[Unit Testing](unit-testing.md)** - Test individual components with mocking
2. **[Integration Testing](integration-testing.md)** - Test database interactions
3. **[GraphQL Testing](graphql-testing.md)** - End-to-end API testing
4. **[Performance Testing](performance-testing.md)** - Load testing and optimization
5. **[Best Practices](best-practices.md)** - Testing patterns and guidelines

## Troubleshooting

### Common Issues

**PostgreSQL Connection Errors**
```bash
# Ensure PostgreSQL is available
docker run --name test-postgres -e POSTGRES_PASSWORD=test -d -p 5432:5432 postgres:15
```

**Async Test Failures**
```python
# Always use pytest.mark.asyncio
@pytest.mark.asyncio
async def test_async_function():
    result = await async_operation()
    assert result is not None
```

**Test Isolation Problems**
```python
# Use transactions or fresh containers
@pytest.fixture
async def isolated_test(test_db):
    async with test_db.transaction() as tx:
        yield tx
        # Automatic rollback
```

For detailed examples and patterns, continue to the specific testing guides in this section.
