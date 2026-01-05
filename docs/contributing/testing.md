# Testing Guide

Comprehensive testing strategies for FraiseQL applications, covering unit tests, integration tests, chaos testing, and CI/CD pipelines.

## Overview

FraiseQL provides comprehensive testing utilities for GraphQL APIs, database operations, and performance validation. Effective testing ensures reliability, prevents regressions, and enables confident deployments.

**Testing Pyramid:**
```
End-to-End Tests (Slow, High Value)
├── Integration Tests (Medium, Medium Value)
├── Component Tests (Fast, High Value)
└── Unit Tests (Fastest, Foundation)
```

## Quick Start

### Basic Test Setup

```python
# test_basic.py
import pytest
from fraiseql.testing import GraphQLTestClient

@pytest.fixture
async def client():
    """Test client for GraphQL operations."""
    from myapp import app
    return GraphQLTestClient(app)

@pytest.mark.asyncio
async def test_basic_query(client):
    """Test a simple GraphQL query."""
    response = await client.execute("""
        query {
            users(limit: 10) {
                id
                name
            }
        }
    """)

    assert response.errors is None
    assert len(response.data["users"]) <= 10
```

### Database Testing

```python
# test_db.py
import pytest
from fraiseql.testing import DatabaseTestHelper

@pytest.fixture
async def db_helper():
    """Database test helper with automatic cleanup."""
    helper = DatabaseTestHelper()
    await helper.setup()
    yield helper
    await helper.teardown()

@pytest.mark.asyncio
async def test_user_creation(db_helper):
    """Test user creation in database."""
    user_id = await db_helper.create_user(name="Alice", email="alice@test.com")

    # Verify user was created
    user = await db_helper.get_user(user_id)
    assert user.name == "Alice"
    assert user.email == "alice@test.com"
```

## Unit Testing

### Schema Testing

```python
# test_schema.py
from myapp.schema import schema
from fraiseql.testing import SchemaValidator

validator = SchemaValidator(schema)

def test_schema_validity():
    """Ensure schema is valid and well-formed."""
    assert validator.is_valid()
    assert validator.has_query_type()
    assert validator.has_mutation_type()

def test_field_definitions():
    """Test specific field definitions."""
    user_type = validator.get_type("User")
    assert user_type.has_field("id")
    assert user_type.has_field("name")
    assert user_type.field("email").is_nullable() == False

def test_resolver_coverage():
    """Ensure all fields have resolvers."""
    coverage = validator.get_resolver_coverage()
    assert coverage.unresolved_fields == []
    assert coverage.percentage == 100.0
```

### Resolver Testing

```python
# test_resolvers.py
import pytest
from unittest.mock import AsyncMock
from myapp.resolvers import user_resolver

@pytest.mark.asyncio
async def test_user_resolver():
    """Test user resolver with mocked database."""
    # Mock database call
    mock_db = AsyncMock()
    mock_db.get_user.return_value = {"id": 1, "name": "Alice"}

    # Create resolver context
    context = {"db": mock_db, "user_id": 1}

    # Test resolver
    result = await user_resolver(context)

    assert result["id"] == 1
    assert result["name"] == "Alice"
    mock_db.get_user.assert_called_once_with(1)
```

### Utility Testing

```python
# test_utils.py
from myapp.utils import validate_email, hash_password
from fraiseql.testing import TestDataGenerator

def test_email_validation():
    """Test email validation utility."""
    assert validate_email("user@example.com") == True
    assert validate_email("invalid-email") == False

def test_password_hashing():
    """Test password hashing utility."""
    password = "secure_password"
    hashed = hash_password(password)

    assert hashed != password  # Should be hashed
    assert len(hashed) > len(password)  # Should be longer

def test_data_generation():
    """Test data generation utilities."""
    generator = TestDataGenerator()

    user = generator.user()
    assert user.email.endswith("@example.com")
    assert len(user.name) > 0

    users = generator.users(count=5)
    assert len(users) == 5
```

## Integration Testing

### API Integration Tests

```python
# test_api.py
import pytest
from httpx import AsyncClient
from myapp.main import app

@pytest.fixture
async def http_client():
    """HTTP client for API testing."""
    async with AsyncClient(app=app, base_url="http://testserver") as client:
        yield client

@pytest.mark.asyncio
async def test_graphql_endpoint(http_client):
    """Test GraphQL endpoint accepts queries."""
    query = """
    {
        users {
            id
            name
        }
    }
    """

    response = await http_client.post(
        "/graphql",
        json={"query": query}
    )

    assert response.status_code == 200
    data = response.json()
    assert "data" in data
    assert "users" in data["data"]
```

### Database Integration Tests

```python
# test_db_integration.py
import pytest
from sqlalchemy import text
from myapp.database import get_db_session

@pytest.mark.asyncio
async def test_database_connection():
    """Test database connectivity and basic operations."""
    async with get_db_session() as session:
        # Test connection
        result = await session.execute(text("SELECT 1"))
        assert result.scalar() == 1

        # Test schema
        result = await session.execute(text("""
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = 'public'
        """))
        tables = result.scalars().all()
        assert "users" in tables

@pytest.mark.asyncio
async def test_foreign_key_constraints():
    """Test database constraints are enforced."""
    async with get_db_session() as session:
        # Try to create post with non-existent user
        with pytest.raises(Exception):  # Foreign key violation
            await session.execute(text("""
                INSERT INTO posts (title, user_id)
                VALUES ('Test Post', 99999)
            """))
            await session.commit()
```

### External Service Integration

```python
# test_external_services.py
import pytest
from unittest.mock import patch
from myapp.services import EmailService, PaymentService

@pytest.mark.asyncio
async def test_email_service():
    """Test email service integration."""
    with patch("smtplib.SMTP") as mock_smtp:
        service = EmailService()

        await service.send_welcome_email("user@example.com", "Alice")

        # Verify SMTP was called correctly
        mock_smtp.assert_called_once()
        smtp_instance = mock_smtp.return_value
        smtp_instance.sendmail.assert_called_once()

@pytest.mark.asyncio
async def test_payment_service():
    """Test payment service integration."""
    with patch("stripe.Charge.create") as mock_charge:
        mock_charge.return_value = {"id": "ch_123", "status": "succeeded"}

        service = PaymentService()
        result = await service.process_payment(1000, "usd", "tok_visa")

        assert result["status"] == "succeeded"
        mock_charge.assert_called_once_with(
            amount=1000,
            currency="usd",
            source="tok_visa"
        )
```

## End-to-End Testing

### Full Application Tests

```python
# test_e2e.py
import pytest
from playwright.async_api import async_playwright
from myapp.main import app
from fraiseql.testing import E2ETestHelper

@pytest.fixture
async def e2e_helper():
    """End-to-end test helper."""
    helper = E2ETestHelper(app)
    await helper.start_server()
    yield helper
    await helper.stop_server()

@pytest.mark.asyncio
async def test_user_registration_flow(e2e_helper):
    """Test complete user registration flow."""
    async with async_playwright() as p:
        browser = await p.chromium.launch()
        page = await browser.new_page()

        try:
            # Navigate to registration page
            await page.goto(e2e_helper.url + "/register")

            # Fill registration form
            await page.fill("#email", "alice@example.com")
            await page.fill("#password", "secure_password")
            await page.fill("#confirm_password", "secure_password")

            # Submit form
            await page.click("#register-button")

            # Verify success
            await page.wait_for_selector("#welcome-message")
            welcome_text = await page.inner_text("#welcome-message")
            assert "Welcome, Alice!" in welcome_text

            # Verify user was created in database
            user = await e2e_helper.db.get_user_by_email("alice@example.com")
            assert user is not None
            assert user.email == "alice@example.com"

        finally:
            await browser.close()
```

### GraphQL E2E Tests

```python
# test_graphql_e2e.py
import pytest
from fraiseql.testing import GraphQLE2ETestHelper

@pytest.fixture
async def graphql_e2e():
    """GraphQL end-to-end test helper."""
    helper = GraphQLE2ETestHelper()
    await helper.setup()
    yield helper
    await helper.teardown()

@pytest.mark.asyncio
async def test_user_management_workflow(graphql_e2e):
    """Test complete user management workflow via GraphQL."""

    # 1. Create user
    create_result = await graphql_e2e.execute("""
        mutation CreateUser($input: CreateUserInput!) {
            createUser(input: $input) {
                id
                name
                email
            }
        }
    """, variables={
        "input": {
            "name": "Alice Johnson",
            "email": "alice@example.com",
            "password": "secure_password"
        }
    })

    user_id = create_result.data["createUser"]["id"]
    assert create_result.data["createUser"]["name"] == "Alice Johnson"

    # 2. Query user
    query_result = await graphql_e2e.execute("""
        query GetUser($id: ID!) {
            user(id: $id) {
                id
                name
                email
                createdAt
            }
        }
    """, variables={"id": user_id})

    assert query_result.data["user"]["email"] == "alice@example.com"

    # 3. Update user
    update_result = await graphql_e2e.execute("""
        mutation UpdateUser($id: ID!, $input: UpdateUserInput!) {
            updateUser(id: $id, input: $input) {
                id
                name
                email
            }
        }
    """, variables={
        "id": user_id,
        "input": {"name": "Alice Smith"}
    })

    assert update_result.data["updateUser"]["name"] == "Alice Smith"

    # 4. Delete user
    delete_result = await graphql_e2e.execute("""
        mutation DeleteUser($id: ID!) {
            deleteUser(id: $id)
        }
    """, variables={"id": user_id})

    assert delete_result.data["deleteUser"] == True

    # 5. Verify user is gone
    final_query = await graphql_e2e.execute("""
        query GetUser($id: ID!) {
            user(id: $id) {
                id
            }
        }
    """, variables={"id": user_id})

    assert final_query.data["user"] is None
```

## Chaos Testing

### Network Failure Testing

```python
# test_chaos_network.py
import pytest
from fraiseql.testing.chaos import ChaosTestHelper, NetworkChaos

@pytest.mark.asyncio
async def test_network_resilience():
    """Test application resilience to network failures."""
    chaos = ChaosTestHelper()

    # Simulate network partition
    with chaos.network_partition("database", duration=5):
        # Try to execute query during network partition
        response = await chaos.execute_query("""
            query {
                users {
                    id
                    name
                }
            }
        """)

        # Should either succeed (if cached) or fail gracefully
        if response.errors:
            assert "timeout" in str(response.errors[0]).lower()
        else:
            # Cached result returned
            assert "users" in response.data
```

### Database Failure Testing

```python
# test_chaos_database.py
import pytest
from fraiseql.testing.chaos import DatabaseChaos

@pytest.mark.asyncio
async def test_database_failure_handling():
    """Test graceful handling of database failures."""
    chaos = DatabaseChaos()

    # Simulate database outage
    with chaos.database_outage(duration=10):
        response = await chaos.execute_query("""
            query {
                users {
                    id
                }
            }
        """)

        # Should return cached data or graceful error
        assert response is not None
        # Application should not crash
```

### Load Testing

```python
# test_load.py
import pytest
from fraiseql.testing.load import LoadTestHelper

@pytest.mark.asyncio
async def test_concurrent_load():
    """Test application under concurrent load."""
    load_tester = LoadTestHelper()

    # Simulate 100 concurrent users
    results = await load_tester.run_concurrent(
        query="""
            query {
                users(limit: 10) {
                    id
                    name
                }
            }
        """,
        concurrent_users=100,
        duration_seconds=60
    )

    # Analyze results
    assert results.avg_response_time < 200  # ms
    assert results.error_rate < 0.01  # 1%
    assert results.requests_per_second > 500
```

## CI/CD Testing

### GitHub Actions Configuration

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
    - uses: actions/checkout@v3
    - name: Set up Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.11'

    - name: Install dependencies
      run: |
        python -m pip install --upgrade pip
        pip install -r requirements.txt
        pip install -r requirements-dev.txt

    - name: Run tests
      run: |
        pytest --cov=myapp --cov-report=xml

    - name: Upload coverage
      uses: codecov/codecov-action@v3
      with:
        file: ./coverage.xml
```

### Test Organization

```
tests/
├── unit/                    # Fast unit tests
│   ├── test_resolvers.py
│   ├── test_utils.py
│   └── test_schema.py
├── integration/            # API and database tests
│   ├── test_api.py
│   ├── test_db.py
│   └── test_external.py
├── e2e/                    # Full application tests
│   ├── test_user_flow.py
│   └── test_admin_flow.py
├── chaos/                  # Resilience tests
│   ├── test_network.py
│   └── test_database.py
└── fixtures/               # Test data
    ├── users.json
    └── posts.json
```

### Test Configuration

```ini
# pytest.ini
[tool:pytest]
testpaths = tests
python_files = test_*.py
python_classes = Test*
python_functions = test_*
addopts =
    --strict-markers
    --strict-config
    --disable-warnings
    --tb=short
markers =
    slow: marks tests as slow (deselect with '-m "not slow"')
    integration: marks tests as integration tests
    e2e: marks tests as end-to-end tests
    chaos: marks tests as chaos tests
```

## Performance Testing

### Benchmarking

```python
# test_performance.py
import pytest
from fraiseql.testing.performance import PerformanceTestHelper

@pytest.fixture
async def perf_helper():
    """Performance testing helper."""
    return PerformanceTestHelper()

@pytest.mark.asyncio
@pytest.mark.slow
async def test_query_performance(perf_helper):
    """Test query performance under load."""
    # Warm up
    await perf_helper.warm_up(queries=100)

    # Benchmark
    results = await perf_helper.benchmark(
        query="""
            query GetUsers($limit: Int) {
                users(limit: $limit) {
                    id
                    name
                    posts {
                        id
                        title
                    }
                }
            }
        """,
        variables={"limit": 50},
        iterations=1000,
        concurrent=10
    )

    # Assert performance requirements
    assert results.p95_latency < 200  # ms
    assert results.throughput > 1000  # requests/second
    assert results.error_rate < 0.001  # 0.1%
```

### Memory Leak Detection

```python
# test_memory.py
import pytest
import tracemalloc
from fraiseql.testing.memory import MemoryTestHelper

@pytest.mark.asyncio
async def test_memory_usage():
    """Test for memory leaks."""
    tracemalloc.start()

    memory_helper = MemoryTestHelper()

    # Run operations that might leak memory
    for i in range(1000):
        await memory_helper.execute_operation()

    # Check memory usage
    current, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()

    # Memory should not grow excessively
    assert current < 50 * 1024 * 1024  # 50MB
    assert peak < 100 * 1024 * 1024    # 100MB
```

## Best Practices

### Test Naming

```python
# ✅ Good test names
def test_user_creation_with_valid_data():
def test_user_creation_fails_with_invalid_email():
def test_user_query_returns_correct_fields():

# ❌ Bad test names
def test_user():
def test_create():
def test_query():
```

### Test Isolation

```python
# ✅ Isolated tests
@pytest.mark.asyncio
async def test_user_creation(db_session):
    """Test creates its own user."""
    user = await create_user(name="Test User")
    assert user.name == "Test User"

# ❌ Coupled tests
@pytest.mark.asyncio
async def test_user_update():
    """Depends on test_user_creation having run first."""
    # Assumes user with ID 1 exists from previous test
    user = await update_user(1, name="Updated")
    assert user.name == "Updated"
```

### Test Data Management

```python
# ✅ Factory pattern
class UserFactory:
    @staticmethod
    def create(name="Test User", email=None):
        if email is None:
            email = f"{name.lower().replace(' ', '.')}@example.com"
        return {"name": name, "email": email}

def test_user_creation():
    user_data = UserFactory.create(name="Alice")
    user = await create_user(**user_data)
    assert user.email == "alice@example.com"

# ✅ Fixtures for setup/teardown
@pytest.fixture
async def test_user():
    """Create and cleanup test user."""
    user = await create_user(name="Test User")
    yield user
    await delete_user(user.id)
```

### Mocking Strategy

```python
# ✅ Mock external dependencies
@pytest.mark.asyncio
async def test_email_notification():
    with patch("myapp.services.EmailService.send") as mock_send:
        mock_send.return_value = True

        await notify_user_registration(user_id=1)

        mock_send.assert_called_once_with(
            to="user@example.com",
            subject="Welcome!",
            template="registration"
        )

# ❌ Don't mock everything
def test_business_logic():
    # Test actual business logic, not mocks
    result = calculate_total([10, 20, 30])
    assert result == 60
```

## Next Steps

- [Performance Tuning Guide](../guides/performance-tuning.md) - Optimize application performance
- [CI/CD Setup](../contributing/release-process.md) - Set up automated testing
- [Monitoring Guide](../production/monitoring.md) - Monitor test results and application health
- [Troubleshooting](../troubleshooting/common-issues.md) - Debug test failures

---

**Effective testing gives confidence in deployments and prevents regressions. Start with unit tests, add integration tests, and include chaos testing for production readiness.**
