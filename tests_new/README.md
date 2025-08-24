# FraiseQL Test Suite - Revamped Architecture

## 🌟 Overview

This is the **completely revamped FraiseQL test suite** designed for maximum developer productivity, maintainability, and comprehensive coverage. The new architecture provides:

- **Clear Test Classification**: Unit, Integration, and E2E tests with proper isolation
- **Centralized Fixtures**: Reusable, well-documented test utilities
- **Self-Contained Blog Demo**: Complete application showcasing FraiseQL capabilities
- **Performance & Security Testing**: Built-in benchmarking and security validation
- **Developer-Friendly**: Excellent error messages, debugging tools, and documentation

## 🏗️ Architecture Overview

```
tests_new/
├── conftest.py                 # Main test configuration
├── pytest.ini                 # Pytest settings and markers
├── requirements.txt            # Test dependencies
│
├── fixtures/                   # 🔧 Centralized test fixtures
│   ├── database.py            # Database containers & connections
│   ├── auth.py                # Authentication & authorization
│   ├── graphql.py             # GraphQL clients & schema utilities
│   └── mock_data.py           # Test data factories
│
├── utilities/                  # 🛠️ Test utility functions
│   ├── assertions/            # Custom assertions
│   │   ├── graphql.py         # GraphQL response validation
│   │   └── database.py        # Database state validation
│   ├── builders/              # Test data builders
│   │   └── graphql.py         # GraphQL query/mutation builders
│   └── database/              # Database testing utilities
│       ├── container.py       # Container management
│       └── schema.py          # Schema-qualified queries
│
├── unit/                       # ⚡ Fast, isolated unit tests
│   ├── core/                  # Core functionality
│   ├── types/                 # Type system
│   ├── mutations/             # Mutation logic
│   ├── sql/                   # SQL generation
│   └── utils/                 # Utility functions
│
├── integration/                # 🔗 Component integration tests
│   ├── database/              # Database operations
│   ├── graphql/               # GraphQL schema & resolvers
│   ├── auth/                  # Authentication flows
│   └── fastapi/               # API integration
│
├── e2e/                        # 🚀 End-to-end system tests
│   ├── scenarios/             # Complete user workflows
│   ├── performance/           # Performance benchmarks
│   ├── security/              # Security validation
│   └── blog_demo/             # 🌟 Complete blog application
│       ├── README.md          # Demo documentation
│       ├── app.py             # Complete FraiseQL app
│       ├── models.py          # Blog domain models
│       ├── schema.sql         # Database schema
│       ├── docker-compose.yml # Standalone environment
│       └── test_*.py          # Comprehensive tests
│
├── examples/                   # 📚 Example-based tests
└── regression/                # 🐛 Bug-specific regression tests
```

## 🎯 Test Categories

### Unit Tests (`unit/`)
**Fast, isolated, no external dependencies**

```bash
# Run all unit tests
pytest tests_new/unit/ -v

# Run specific unit test categories
pytest tests_new/unit/core/ -v          # Core functionality
pytest tests_new/unit/mutations/ -v     # Mutation decorators
pytest tests_new/unit/types/ -v         # Type system
```

**Characteristics:**
- Execute in < 1ms each
- Use mocks for external dependencies
- Test single functions/classes in isolation
- 100% deterministic and repeatable

### Integration Tests (`integration/`)
**Real database, component interactions**

```bash
# Run all integration tests (requires database)
pytest tests_new/integration/ -v

# Run specific integration categories
pytest tests_new/integration/database/ -v    # Database operations
pytest tests_new/integration/graphql/ -v     # GraphQL integration
pytest tests_new/integration/auth/ -v        # Authentication flows
```

**Characteristics:**
- Execute in 10-100ms each
- Use real PostgreSQL containers
- Test component interactions
- Validate data persistence and consistency

### E2E Tests (`e2e/`)
**Complete system behavior, realistic scenarios**

```bash
# Run E2E tests (use --run-e2e flag)
pytest tests_new/e2e/ --run-e2e -v

# Run blog demo tests
pytest tests_new/e2e/blog_demo/ --run-e2e -v

# Run performance benchmarks
pytest tests_new/e2e/performance/ --benchmark -v
```

**Characteristics:**
- Execute in 100ms-5s each
- Test complete user workflows
- Validate system-wide behavior
- Include performance and security testing

## 🌟 Blog Demo Highlight

The **self-contained blog demo** (`e2e/blog_demo/`) is the crown jewel of this test suite:

### Features Demonstrated
- ✅ **Complete Blog Application**: Users, posts, comments, tags
- ✅ **Advanced GraphQL Patterns**: Queries, mutations, subscriptions
- ✅ **Database Best Practices**: PostgreSQL with JSONB, indexing
- ✅ **Authentication & Authorization**: JWT, role-based permissions
- ✅ **Performance Optimization**: N+1 prevention, caching
- ✅ **Production Ready**: Docker setup, monitoring, security

### Quick Start
```bash
cd tests_new/e2e/blog_demo

# Start the complete demo environment
docker-compose up -d

# Run comprehensive tests
pytest . -v

# Access GraphQL Playground
open http://localhost:8080/graphql
```

### Real-World Examples
```graphql
# Query with complex filtering and pagination
query GetPosts($first: Int!, $where: PostWhereInput) {
  posts(first: $first, where: $where) {
    edges {
      node {
        id
        title
        author { username }
        tags { name }
        commentCount
      }
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}

# Mutation with union result types
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    __typename
    ... on CreatePostSuccess {
      post { id title slug }
      message
    }
    ... on ValidationError {
      message
      fieldErrors { field message }
    }
  }
}
```

## 🚀 Quick Start Guide

### 1. Setup Environment
```bash
# Install dependencies
pip install -r tests_new/requirements.txt

# Or install with optional features
pip install -r tests_new/requirements.txt[dev]
```

### 2. Run Tests by Category
```bash
# Fast unit tests (no external deps)
pytest tests_new/unit/ -v

# Integration tests (requires Docker)
pytest tests_new/integration/ -v

# E2E tests (comprehensive, slower)
pytest tests_new/e2e/ --run-e2e -v

# Blog demo (showcases everything)
pytest tests_new/e2e/blog_demo/ --run-e2e -v
```

### 3. Performance & Benchmarking
```bash
# Run performance benchmarks
pytest tests_new/ --benchmark -v

# Run with performance monitoring
pytest tests_new/e2e/ --run-e2e -v --tb=short
```

### 4. Development Workflow
```bash
# Run tests with auto-reload (for TDD)
pytest-watch tests_new/unit/

# Run specific test file
pytest tests_new/unit/core/test_graphql_type.py -v

# Debug failing test
pytest tests_new/unit/mutations/test_decorators.py::TestMutationDecorator::test_basic -xvs
```

## 🔧 Configuration & Customization

### Command Line Options
```bash
--no-db              # Skip database integration tests
--no-docker          # Skip tests requiring Docker
--run-slow           # Include slow tests (skipped by default)
--run-e2e            # Include E2E tests (skipped by default)
--benchmark          # Run performance benchmarks
--parallel           # Enable parallel test execution
```

### Environment Variables
```bash
ENV=test                    # Test environment mode
TEST_DATABASE_URL=          # External database URL
DEBUG=false                 # Enable debug mode
LOG_LEVEL=WARNING           # Logging level
CLEANUP_TEST_DATA=true      # Cleanup after tests
```

### Custom Markers
```python
@pytest.mark.unit           # Fast unit tests
@pytest.mark.integration    # Integration tests
@pytest.mark.e2e            # End-to-end tests
@pytest.mark.database       # Requires database
@pytest.mark.slow           # Long-running tests
@pytest.mark.performance    # Performance benchmarks
@pytest.mark.security       # Security tests
@pytest.mark.blog_demo      # Blog demo specific
@pytest.mark.regression     # Regression tests
```

## 📊 Performance Benchmarks

The test suite includes built-in performance monitoring:

| Test Category | Target Time | Memory Limit |
|---------------|-------------|--------------|
| Unit Tests    | < 1ms       | < 10MB       |
| Integration   | < 100ms     | < 50MB       |
| E2E Tests     | < 5s        | < 200MB      |
| Blog Demo     | < 2s        | < 100MB      |

### Performance Monitoring
```python
@pytest.fixture
def performance_monitor():
    """Built-in performance monitoring for all tests."""
    return PerformanceMetrics()

def test_query_performance(performance_monitor):
    with performance_monitor.measure():
        result = execute_query()

    performance_monitor.assert_performance_acceptable(max_time=0.1)
```

## 🛡️ Security Testing

Comprehensive security validation:

- **Input Validation**: XSS, SQL injection prevention
- **Authorization**: Role-based access control
- **Authentication**: JWT token validation
- **Rate Limiting**: API abuse prevention
- **Data Sanitization**: Safe content handling

```python
@pytest.mark.security
def test_sql_injection_prevention():
    malicious_input = "'; DROP TABLE users; --"
    result = create_user(name=malicious_input)
    assert_user_created_safely(result)
```

## 🧪 Testing Best Practices

### 1. Test Organization
- **Unit tests**: One test class per source class
- **Integration tests**: One test per component interaction
- **E2E tests**: One test per complete user workflow

### 2. Naming Conventions
```python
def test_[feature]_[scenario]_[expected_outcome]():
    """Test that feature does scenario and produces expected outcome."""

def test_user_creation_with_valid_data_creates_user():
    """Test that user creation with valid data creates user successfully."""

def test_graphql_query_with_invalid_field_raises_validation_error():
    """Test that GraphQL query with invalid field raises validation error."""
```

### 3. Fixture Usage
```python
# Use appropriate fixture scope
@pytest.fixture(scope="session")  # Expensive setup
def database_container(): pass

@pytest.fixture(scope="function")  # Per-test isolation
def clean_database(): pass

# Use factory patterns for flexibility
@pytest.fixture
def user_factory():
    return UserFactory()
```

### 4. Assertion Patterns
```python
# Use specific assertions
assert_no_graphql_errors(response)
assert_mutation_success(response, "createUser", "CreateUserSuccess")
assert_graphql_field_equals(response, "user.email", "test@example.com")

# Use database assertions
await assert_row_exists(db, "users", "email = %s", ("test@example.com",))
await assert_jsonb_field_equals(db, "users", "profile", "name", "John")
```

## 🔍 Debugging & Troubleshooting

### Common Issues

| Issue | Solution |
|-------|----------|
| `Docker not available` | Install Docker and ensure it's running |
| `Database connection failed` | Check PostgreSQL container is healthy |
| `Fixture 'graphql_client' not found` | Import from correct fixtures module |
| `Tests fail after SQL change` | Reset database with fresh schema |

### Debug Tools
```python
# Debug test data
def test_debug_example(debug_info):
    debug_info["queries_executed"].append(query)
    # Test automatically logs debug info on failure

# Performance profiling
def test_with_profiling(performance_monitor):
    # Automatic performance tracking and reporting

# Database inspection
async def test_database_debug(db_connection):
    data = await debug_table_contents(db_connection, "users", limit=5)
    print(f"Current users: {data}")
```

### Logging & Monitoring
```bash
# Enable debug logging
FRAISEQL_LOG_LEVEL=DEBUG pytest tests_new/ -v -s

# Monitor test performance
pytest tests_new/ --benchmark --benchmark-sort=mean

# Generate coverage report
pytest tests_new/ --cov=src/fraiseql --cov-report=html
```

## 🤝 Contributing

### Adding New Tests

1. **Choose the Right Category**
   - Unit: Fast, isolated, no external deps
   - Integration: Component interactions, database
   - E2E: Complete workflows, realistic scenarios

2. **Use Existing Fixtures**
   ```python
   def test_new_feature(graphql_client, user_factory, db_connection):
       user = user_factory.create()
       # Test implementation
   ```

3. **Add Appropriate Markers**
   ```python
   @pytest.mark.integration
   @pytest.mark.database
   def test_database_operation():
       pass
   ```

4. **Follow Naming Conventions**
   - File: `test_[feature].py`
   - Class: `Test[Feature][Aspect]`
   - Method: `test_[specific_behavior]`

### Extending the Blog Demo

The blog demo is designed to be extended with new FraiseQL features:

1. Add new models to `models.py`
2. Update schema in `schema.sql`
3. Create corresponding tests
4. Update documentation

---

**This revamped test suite represents the gold standard for GraphQL API testing, providing comprehensive coverage while maintaining excellent developer experience.**
