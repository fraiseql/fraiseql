# Testing Best Practices

Comprehensive guidelines for effective testing in FraiseQL applications, covering patterns, conventions, and strategies that lead to maintainable, reliable test suites.

## Test Organization and Structure

### Directory Structure

```
tests/
├── conftest.py                  # Shared fixtures and configuration
├── unit/                        # Unit tests (fast, isolated)
│   ├── test_types.py
│   ├── test_queries.py
│   ├── test_mutations.py
│   └── test_utils.py
├── integration/                 # Integration tests (database required)
│   ├── test_repository.py
│   ├── test_transactions.py
│   └── test_database_functions.py
├── api/                        # GraphQL API tests (full stack)
│   ├── test_queries.py
│   ├── test_mutations.py
│   └── test_schema.py
├── performance/                # Performance and load tests
│   ├── test_response_times.py
│   ├── test_n_plus_one.py
│   └── locustfile.py
├── fixtures/                   # Test data and factories
│   ├── user_factory.py
│   └── sample_data.py
└── helpers/                    # Test utilities
    ├── graphql_client.py
    └── database_helpers.py
```

### Test File Naming

```python
# ✅ Good: Descriptive, follows convention
test_user_creation.py
test_graphql_queries.py
test_database_transactions.py
test_authentication_flow.py

# ❌ Bad: Vague, inconsistent
tests.py
user_tests.py
test_stuff.py
testing_file.py
```

### Test Class Organization

```python
# ✅ Good: Organized by feature with descriptive names
class TestUserCreation:
    """Tests for user creation functionality"""

    async def test_create_user_success(self):
        """Test successful user creation with valid data"""
        pass

    async def test_create_user_duplicate_email(self):
        """Test user creation fails with duplicate email"""
        pass

    async def test_create_user_invalid_email_format(self):
        """Test user creation fails with invalid email format"""
        pass

class TestUserQueries:
    """Tests for user query operations"""

    async def test_get_user_by_id_found(self):
        """Test successful user retrieval by ID"""
        pass

    async def test_get_user_by_id_not_found(self):
        """Test user retrieval returns None for non-existent ID"""
        pass

# ❌ Bad: Everything in one class
class TestUsers:
    async def test_user_stuff(self):
        pass

    async def test_another_thing(self):
        pass
```

## Test Naming Conventions

### Descriptive Test Names

```python
# ✅ Good: Clear what is being tested and expected outcome
async def test_create_user_with_valid_data_returns_user_object(self):
    """Test that creating a user with valid data returns a User object"""
    pass

async def test_get_users_with_name_filter_returns_matching_users(self):
    """Test that filtering users by name returns only matching users"""
    pass

async def test_update_user_with_invalid_id_raises_user_not_found_error(self):
    """Test that updating a non-existent user raises UserNotFoundError"""
    pass

# ❌ Bad: Vague, doesn't indicate expected behavior
async def test_create_user(self):
    pass

async def test_user_filter(self):
    pass

async def test_update_error(self):
    pass
```

### Test Name Patterns

```python
# Pattern: test_[action]_[condition]_[expected_result]

async def test_login_with_valid_credentials_returns_token(self):
    pass

async def test_login_with_invalid_password_raises_authentication_error(self):
    pass

async def test_query_posts_with_limit_returns_limited_results(self):
    pass

async def test_delete_post_with_wrong_owner_raises_permission_error(self):
    pass
```

## AAA Pattern (Arrange, Act, Assert)

### Clear Test Structure

```python
# ✅ Good: Clear AAA structure
async def test_create_user_success(self, test_db):
    """Test successful user creation"""
    # Arrange
    user_data = {
        "name": "Test User",
        "email": "test@example.com",
        "password": "secure_password"
    }

    # Act
    user_id = await test_db.call_function(
        "fn_create_user",
        p_name=user_data["name"],
        p_email=user_data["email"],
        p_password_hash="hashed_password"
    )

    # Assert
    assert user_id is not None

    created_user = await test_db.find_one("v_user", where={"id": user_id})
    assert created_user["name"] == user_data["name"]
    assert created_user["email"] == user_data["email"]

# ❌ Bad: Mixed arrange/act/assert
async def test_create_user_bad(self, test_db):
    user_id = await test_db.call_function("fn_create_user", p_name="Test", p_email="test@example.com", p_password_hash="hash")
    assert user_id is not None
    user_data = {"name": "Test", "email": "test@example.com"}
    created_user = await test_db.find_one("v_user", where={"id": user_id})
    assert created_user["name"] == user_data["name"]
```

### Complex Arrange Sections

```python
async def test_user_can_only_edit_own_posts(self, test_db):
    """Test that users can only edit their own posts"""
    # Arrange
    # Create first user
    user1_id = await test_db.call_function(
        "fn_create_user",
        p_name="User One",
        p_email="user1@example.com",
        p_password_hash="hash1"
    )

    # Create second user
    user2_id = await test_db.call_function(
        "fn_create_user",
        p_name="User Two",
        p_email="user2@example.com",
        p_password_hash="hash2"
    )

    # Create post by user1
    post_id = await test_db.call_function(
        "fn_create_post",
        p_title="User1's Post",
        p_content="Content by user1",
        p_author_id=user1_id
    )

    # Act
    # Try to update user1's post as user2
    with pytest.raises(PermissionError):
        await update_post_as_user(
            post_id=post_id,
            user_id=user2_id,  # Different user
            updates={"title": "Hacked Title"}
        )

    # Assert
    # Post should remain unchanged
    post = await test_db.find_one("v_post", where={"id": post_id})
    assert post["title"] == "User1's Post"
```

## Test Data Management

### Using Factories

```python
# factories.py
import factory
from factory import fuzzy
from datetime import datetime, timedelta

class UserFactory(factory.Factory):
    """Factory for creating test users"""
    class Meta:
        model = dict  # Or your User model

    id = factory.Faker("uuid4")
    name = factory.Faker("name")
    email = factory.Faker("email")
    password_hash = "hashed_password_123"
    created_at = fuzzy.FuzzyDateTime(
        datetime.now() - timedelta(days=30),
        datetime.now()
    )

class PostFactory(factory.Factory):
    """Factory for creating test posts"""
    class Meta:
        model = dict

    id = factory.Faker("uuid4")
    title = factory.Faker("sentence", nb_words=4)
    content = factory.Faker("paragraph", nb_sentences=5)
    status = fuzzy.FuzzyChoice(["draft", "published", "archived"])
    created_at = fuzzy.FuzzyDateTime(
        datetime.now() - timedelta(days=7),
        datetime.now()
    )

# Using factories in tests
async def test_user_posts_relationship(self, test_db):
    """Test user-posts relationship"""
    # Create test data with factories
    user_data = UserFactory()
    post1_data = PostFactory(title="First Post")
    post2_data = PostFactory(title="Second Post")

    # Create in database
    user_id = await create_user_in_db(test_db, user_data)
    post1_id = await create_post_in_db(test_db, post1_data, author_id=user_id)
    post2_id = await create_post_in_db(test_db, post2_data, author_id=user_id)

    # Test the relationship
    user_posts = await test_db.find("v_post", where={"author_id": user_id})
    assert len(user_posts) == 2
```

### Fixture Strategies

```python
# conftest.py

# ✅ Good: Specific, focused fixtures
@pytest.fixture
async def user_with_published_posts(test_db):
    """Create a user with multiple published posts"""
    user_id = await test_db.call_function(
        "fn_create_user",
        p_name="Author User",
        p_email="author@example.com",
        p_password_hash="hash"
    )

    post_ids = []
    for i in range(3):
        post_id = await test_db.call_function(
            "fn_create_post",
            p_title=f"Published Post {i+1}",
            p_content=f"Content for post {i+1}",
            p_author_id=user_id
        )

        # Set status to published
        await test_db.execute(
            "UPDATE tb_post SET status = 'published' WHERE id = $1",
            post_id
        )
        post_ids.append(post_id)

    user = await test_db.find_one("v_user", where={"id": user_id})
    return {
        "user": user,
        "post_ids": post_ids
    }

@pytest.fixture
async def admin_user(test_db):
    """Create an admin user for testing admin operations"""
    user_id = await test_db.call_function(
        "fn_create_user",
        p_name="Admin User",
        p_email="admin@example.com",
        p_password_hash="admin_hash"
    )

    # Set admin role
    await test_db.execute(
        "UPDATE tb_user SET role = 'admin' WHERE id = $1",
        user_id
    )

    return await test_db.find_one("v_user", where={"id": user_id})

# ❌ Bad: One massive fixture that does everything
@pytest.fixture
async def everything_fixture(test_db):
    """Creates users, posts, comments, admin user, etc."""
    # This becomes hard to maintain and understand
    pass
```

## Async Testing Best Practices

### Proper Async Test Setup

```python
# ✅ Good: Proper async test configuration
@pytest.mark.asyncio
async def test_async_database_operation(self, test_db):
    """Test async database operation"""
    result = await test_db.find("v_user")
    assert isinstance(result, list)

@pytest.mark.asyncio
async def test_concurrent_operations(self, test_db):
    """Test concurrent database operations"""
    # Create multiple operations concurrently
    tasks = []
    for i in range(10):
        task = test_db.call_function(
            "fn_create_user",
            p_name=f"Concurrent User {i}",
            p_email=f"concurrent{i}@test.com",
            p_password_hash="hash"
        )
        tasks.append(task)

    user_ids = await asyncio.gather(*tasks)
    assert len(user_ids) == 10
    assert all(uid is not None for uid in user_ids)

# ❌ Bad: Missing asyncio marker or improper async usage
def test_bad_async(self, test_db):
    # This won't work - test function must be async
    result = test_db.find("v_user")  # Missing await
    assert result is not None
```

### Event Loop Management

```python
# conftest.py - ✅ Good: Proper event loop setup
@pytest.fixture(scope="session")
def event_loop():
    """Create event loop for async tests"""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()

# ✅ Good: Using asyncio.gather for concurrent operations
async def test_concurrent_user_creation(self, test_db):
    """Test creating multiple users concurrently"""
    async def create_user(index):
        return await test_db.call_function(
            "fn_create_user",
            p_name=f"User {index}",
            p_email=f"user{index}@test.com",
            p_password_hash="hash"
        )

    # Create users concurrently
    tasks = [create_user(i) for i in range(20)]
    user_ids = await asyncio.gather(*tasks)

    assert len(user_ids) == 20
    assert len(set(user_ids)) == 20  # All unique

# ❌ Bad: Sequential async operations when concurrency is intended
async def test_bad_sequential(self, test_db):
    """This runs sequentially, not concurrently"""
    user_ids = []
    for i in range(20):
        user_id = await test_db.call_function(...)  # Sequential
        user_ids.append(user_id)
```

## Mocking Best Practices

### Mock External Dependencies

```python
# ✅ Good: Mock external services, test your own code
@pytest.mark.asyncio
async def test_send_notification_email(self):
    """Test notification email sending"""
    with patch('app.services.email_service.send_email') as mock_send:
        mock_send.return_value = True

        result = await send_user_notification(
            user_id="test-id",
            message="Welcome!"
        )

        assert result is True
        mock_send.assert_called_once_with(
            to="test@example.com",
            subject="Notification",
            body="Welcome!"
        )

# ✅ Good: Mock network calls
@pytest.mark.asyncio
async def test_external_api_integration(self):
    """Test integration with external API"""
    with patch('httpx.AsyncClient.post') as mock_post:
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"success": True}
        mock_post.return_value = mock_response

        result = await call_external_api("test_data")

        assert result["success"] is True
        mock_post.assert_called_once()

# ❌ Bad: Mocking your own code (testing the mock, not the code)
async def test_bad_mocking(self):
    with patch('app.queries.get_users') as mock_get_users:
        mock_get_users.return_value = [{"id": "1", "name": "Test"}]

        users = await get_users()  # Testing the mock, not the real function
        assert len(users) == 1
```

### Repository Mocking for Unit Tests

```python
# ✅ Good: Mock repository for unit tests
@pytest.mark.asyncio
async def test_user_creation_business_logic(self):
    """Test user creation business logic without database"""
    # Arrange
    mock_repo = AsyncMock()
    mock_repo.call_function.return_value = "new-user-id"
    mock_repo.find_one.return_value = {
        "id": "new-user-id",
        "name": "Test User",
        "email": "test@example.com"
    }

    info = MagicMock()
    info.context = {"repo": mock_repo}

    # Act
    user = await create_user_resolver(
        info,
        name="Test User",
        email="test@example.com",
        password="password123"
    )

    # Assert
    assert user.name == "Test User"
    mock_repo.call_function.assert_called_once_with(
        "fn_create_user",
        p_name="Test User",
        p_email="test@example.com",
        p_password_hash=mock.ANY  # Password hash varies
    )
```

## Database Testing Strategies

### Transaction Isolation

```python
# ✅ Good: Use transactions for test isolation
@pytest.fixture
async def isolated_test_db(test_db):
    """Provide database with automatic rollback"""
    async with test_db.transaction() as tx:
        yield tx
        # Transaction automatically rolls back

async def test_with_isolation(self, isolated_test_db):
    """Test with automatic cleanup"""
    # Create test data
    user_id = await isolated_test_db.call_function(
        "fn_create_user",
        p_name="Isolated Test User",
        p_email="isolated@test.com",
        p_password_hash="hash"
    )

    assert user_id is not None
    # No cleanup needed - transaction rolls back automatically

# ✅ Good: Manual cleanup when transactions aren't sufficient
async def test_with_manual_cleanup(self, test_db):
    """Test with manual resource cleanup"""
    created_ids = []

    try:
        # Create test data
        for i in range(5):
            user_id = await test_db.call_function(
                "fn_create_user",
                p_name=f"Cleanup User {i}",
                p_email=f"cleanup{i}@test.com",
                p_password_hash="hash"
            )
            created_ids.append(user_id)

        # Run test logic
        users = await test_db.find("v_user", where={"name": {"like": "%Cleanup%"}})
        assert len(users) == 5

    finally:
        # Manual cleanup
        for user_id in created_ids:
            await test_db.execute("DELETE FROM tb_user WHERE id = $1", user_id)
```

### Database State Verification

```python
# ✅ Good: Verify database state changes
async def test_user_deletion_cascades_to_posts(self, test_db, user_with_posts):
    """Test that deleting user also deletes their posts"""
    user_id = user_with_posts["user"]["id"]
    post_ids = user_with_posts["post_ids"]

    # Verify initial state
    initial_posts = await test_db.find("v_post", where={"author_id": user_id})
    assert len(initial_posts) == len(post_ids)

    # Act
    deleted = await test_db.call_function("fn_delete_user", p_user_id=user_id)
    assert deleted is True

    # Verify final state
    remaining_user = await test_db.find_one("v_user", where={"id": user_id})
    remaining_posts = await test_db.find("v_post", where={"author_id": user_id})

    assert remaining_user is None
    assert len(remaining_posts) == 0  # Cascade deletion worked

# ✅ Good: Test data consistency
async def test_post_creation_updates_user_stats(self, test_db, sample_user):
    """Test that creating posts updates user statistics"""
    user_id = sample_user["id"]

    # Check initial post count
    initial_count = await test_db.fetchval(
        "SELECT post_count FROM v_user_stats WHERE user_id = $1",
        user_id
    ) or 0

    # Create a post
    post_id = await test_db.call_function(
        "fn_create_post",
        p_title="Stats Test Post",
        p_content="Testing statistics update",
        p_author_id=user_id
    )

    # Verify stats updated
    final_count = await test_db.fetchval(
        "SELECT post_count FROM v_user_stats WHERE user_id = $1",
        user_id
    )

    assert final_count == initial_count + 1
```

## Error Testing Patterns

### Exception Testing

```python
# ✅ Good: Specific exception testing
async def test_create_user_with_duplicate_email_raises_specific_error(self, test_db):
    """Test that duplicate email raises appropriate error"""
    # Create first user
    await test_db.call_function(
        "fn_create_user",
        p_name="First User",
        p_email="duplicate@test.com",
        p_password_hash="hash1"
    )

    # Try to create second user with same email
    with pytest.raises(DuplicateEmailError) as exc_info:
        await test_db.call_function(
            "fn_create_user",
            p_name="Second User",
            p_email="duplicate@test.com",
            p_password_hash="hash2"
        )

    # Verify error details
    assert "duplicate@test.com" in str(exc_info.value)
    assert exc_info.value.error_code == "DUPLICATE_EMAIL"

# ✅ Good: Multiple error conditions
@pytest.mark.parametrize("email,expected_error", [
    ("", "Email cannot be empty"),
    ("not-an-email", "Invalid email format"),
    ("@domain.com", "Invalid email format"),
    ("user@", "Invalid email format"),
])
async def test_user_creation_email_validation(self, test_db, email, expected_error):
    """Test various email validation errors"""
    with pytest.raises(ValidationError) as exc_info:
        await test_db.call_function(
            "fn_create_user",
            p_name="Test User",
            p_email=email,
            p_password_hash="hash"
        )

    assert expected_error in str(exc_info.value)

# ❌ Bad: Generic exception catching
async def test_bad_error_handling(self, test_db):
    """Bad example of error testing"""
    with pytest.raises(Exception):  # Too generic
        await test_db.call_function(
            "fn_create_user",
            p_name="Test",
            p_email="duplicate@test.com",
            p_password_hash="hash"
        )
```

### Error Recovery Testing

```python
# ✅ Good: Test error recovery and cleanup
async def test_transaction_rollback_on_error(self, test_db):
    """Test that failed transactions properly roll back"""
    initial_user_count = await test_db.fetchval("SELECT COUNT(*) FROM tb_user")

    try:
        async with test_db.transaction() as tx:
            # Create a user
            user_id = await tx.call_function(
                "fn_create_user",
                p_name="Transaction User",
                p_email="transaction@test.com",
                p_password_hash="hash"
            )

            # Verify user was created in transaction
            user = await tx.find_one("v_user", where={"id": user_id})
            assert user is not None

            # Force an error
            raise Exception("Simulated error")

    except Exception:
        pass  # Expected error

    # Verify rollback occurred
    final_user_count = await test_db.fetchval("SELECT COUNT(*) FROM tb_user")
    assert final_user_count == initial_user_count  # No change

    # Verify user was not created
    user = await test_db.find_one("v_user", where={"email": "transaction@test.com"})
    assert user is None
```

## Performance Testing Patterns

### Response Time Assertions

```python
# ✅ Good: Reasonable performance assertions
async def test_user_query_performance(self, test_client):
    """Test that user queries meet performance requirements"""
    query = """
        query GetUsers {
            users(limit: 20) {
                id
                name
                email
            }
        }
    """

    # Warm up
    await test_client.post("/graphql", json={"query": query})

    # Measure performance
    start = time.perf_counter()
    response = await test_client.post("/graphql", json={"query": query})
    elapsed = time.perf_counter() - start

    assert response.status_code == 200
    assert elapsed < 0.5, f"Query too slow: {elapsed:.3f}s"  # Reasonable threshold

# ✅ Good: Performance regression testing
async def test_complex_query_performance_regression(self, test_client, large_dataset):
    """Test that complex queries don't regress in performance"""
    query = """
        query ComplexQuery {
            users(limit: 50) {
                posts {
                    comments {
                        author { name }
                    }
                }
            }
        }
    """

    times = []
    for _ in range(10):
        start = time.perf_counter()
        response = await test_client.post("/graphql", json={"query": query})
        elapsed = time.perf_counter() - start
        times.append(elapsed)

        assert response.status_code == 200

    avg_time = statistics.mean(times)
    p95_time = statistics.quantiles(times, n=20)[18]

    # Performance requirements
    assert avg_time < 2.0, f"Average time too slow: {avg_time:.3f}s"
    assert p95_time < 3.0, f"P95 time too slow: {p95_time:.3f}s"
```

## Test Documentation

### Docstring Best Practices

```python
# ✅ Good: Clear, informative docstrings
async def test_user_authorization_for_post_editing(self, authenticated_user, other_user_post):
    """
    Test that users can only edit their own posts.

    This test verifies that the authorization system correctly prevents
    users from editing posts that belong to other users. It should:
    1. Allow users to edit their own posts
    2. Prevent users from editing others' posts
    3. Return appropriate error codes for unauthorized attempts

    Args:
        authenticated_user: Fixture providing an authenticated user
        other_user_post: Fixture providing a post by a different user

    Expected behavior:
        - Attempting to edit another user's post should raise PermissionError
        - Error should include the specific post ID and user ID
        - Original post should remain unchanged
    """
    pass

# ✅ Good: Document test setup and expectations
async def test_database_connection_pool_under_load(self, database_url):
    """
    Test that database connection pool handles concurrent load properly.

    This test creates 50 concurrent database operations to verify:
    1. Connection pool doesn't leak connections
    2. All operations complete successfully
    3. Performance remains acceptable under load
    4. Pool properly queues requests when at capacity

    Performance expectations:
        - All 50 operations should complete within 10 seconds
        - No connection leaks (verified by checking pg_stat_activity)
        - Error rate should be 0%

    Note: This test requires a clean database with proper connection limits.
    """
    pass
```

## Coverage and Quality Metrics

### Coverage Goals

```python
# pytest.ini configuration
[tool:pytest]
addopts =
    --cov=src/fraiseql
    --cov-report=html
    --cov-report=term-missing
    --cov-fail-under=80  # Fail if coverage drops below 80%

# Coverage configuration
[tool.coverage.run]
source = ["src/fraiseql"]
omit = [
    "*/tests/*",
    "*/migrations/*",
    "*/scripts/*"
]

[tool.coverage.report]
exclude_lines = [
    "pragma: no cover",
    "def __repr__",
    "raise AssertionError",
    "raise NotImplementedError"
]
```

### Quality Checks

```bash
# Run tests with quality checks
pytest tests/ \
    --cov=src/fraiseql \
    --cov-report=html \
    --cov-report=term-missing \
    --cov-fail-under=85 \
    --maxfail=5 \
    --tb=short

# Check test performance
pytest tests/ --durations=10  # Show 10 slowest tests

# Run only fast tests during development
pytest tests/unit/ -m "not slow"

# Run with linting
pytest tests/ --flake8 --mypy
```

## CI/CD Integration

### GitHub Actions Example

```yaml
# .github/workflows/tests.yml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        python-version: [3.11, 3.12]

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

    - name: Set up Python ${{ matrix.python-version }}
      uses: actions/setup-python@v4
      with:
        python-version: ${{ matrix.python-version }}

    - name: Install dependencies
      run: |
        pip install -e ".[dev]"

    - name: Run unit tests
      run: |
        pytest tests/unit/ -v --cov=src/fraiseql

    - name: Run integration tests
      env:
        TEST_DATABASE_URL: postgresql://postgres:test@localhost/fraiseql_test
      run: |
        pytest tests/integration/ -v

    - name: Run API tests
      env:
        TEST_DATABASE_URL: postgresql://postgres:test@localhost/fraiseql_test
      run: |
        pytest tests/api/ -v

    - name: Check coverage
      run: |
        pytest tests/ --cov=src/fraiseql --cov-fail-under=80

    - name: Upload coverage to Codecov
      uses: codecov/codecov-action@v3
      with:
        file: ./coverage.xml
```

## Common Anti-Patterns to Avoid

### ❌ Bad Practices

```python
# Don't: Test implementation details
async def test_bad_implementation_testing(self):
    """Don't test internal implementation details"""
    user_service = UserService()
    assert hasattr(user_service, '_internal_cache')  # Implementation detail
    assert user_service._cache_size == 100  # Internal state

# Don't: Overly complex test setup
async def test_everything_at_once(self):
    """Don't create massive, complex test scenarios"""
    # 50 lines of setup code
    # Tests multiple unrelated things
    # Hard to debug when it fails

# Don't: Shared mutable state between tests
class TestWithBadState:
    users = []  # Shared state - bad!

    async def test_first(self):
        self.users.append({"id": 1})
        assert len(self.users) == 1

    async def test_second(self):
        # This test depends on the first test - bad!
        assert len(self.users) == 1

# Don't: Ignore test failures
@pytest.mark.skip("This test is flaky")  # Fix it instead!
async def test_flaky_behavior(self):
    pass

# Don't: Non-deterministic tests
async def test_random_behavior(self):
    """Don't use random values without controlling them"""
    import random
    value = random.randint(1, 100)  # Unpredictable
    assert value > 0  # This might fail randomly

# Don't: Tests that require manual setup
async def test_requires_manual_setup(self):
    """Don't write tests that need manual database setup"""
    # Assumes specific data exists in database
    user = await get_user_by_email("manually.created@example.com")
    assert user is not None  # Fails if data not manually created
```

### ✅ Good Alternatives

```python
# Do: Test behavior, not implementation
async def test_user_caching_improves_performance(self, test_client):
    """Test that user caching improves performance"""
    # First request (cache miss)
    start = time.perf_counter()
    response1 = await test_client.post("/graphql", json={"query": USER_QUERY})
    time1 = time.perf_counter() - start

    # Second request (cache hit)
    start = time.perf_counter()
    response2 = await test_client.post("/graphql", json={"query": USER_QUERY})
    time2 = time.perf_counter() - start

    # Verify caching improved performance
    assert response1.status_code == 200
    assert response2.status_code == 200
    assert time2 < time1  # Second request should be faster

# Do: Use proper test isolation
class TestWithGoodIsolation:
    async def test_first(self, test_db):
        user = await create_test_user(test_db)
        assert user["name"] == "Test User"

    async def test_second(self, test_db):
        # Independent test with own data
        user = await create_test_user(test_db, name="Different User")
        assert user["name"] == "Different User"

# Do: Control randomness in tests
async def test_deterministic_behavior(self):
    """Use controlled randomness in tests"""
    import random
    random.seed(42)  # Fixed seed for deterministic results

    values = [random.randint(1, 100) for _ in range(10)]
    assert values == [82, 15, 86, 27, 19, 57, 48, 25, 40, 51]  # Predictable

# Do: Make tests self-contained
async def test_self_contained(self, test_db):
    """Create all required test data within the test"""
    # Create test data
    user = await create_test_user(test_db, email="self.contained@example.com")
    post = await create_test_post(test_db, author_id=user["id"])

    # Test behavior
    result = await get_user_posts(user["id"])
    assert len(result) == 1
    assert result[0]["id"] == post["id"]
```

Following these best practices will lead to a maintainable, reliable test suite that provides confidence in your FraiseQL application while being pleasant to work with during development.
