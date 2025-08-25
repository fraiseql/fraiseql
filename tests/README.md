# FraiseQL Test Suite

This test suite is organized into logical layers for better maintainability and understanding.

## Test Organization

### 🔧 Unit Tests (`unit/`)
Pure unit tests with no external dependencies (no database, network, etc.)

- **`core/`**: Core FraiseQL functionality
  - **`types/`**: Type system, scalars, serialization
  - **`parsing/`**: AST parsing, query translation, fragments
  - **`json/`**: JSON handling, validation, passthrough
  - **`registry/`**: Schema registry and builder
- **`decorators/`**: Decorator functionality (@fraiseql.query, @fraiseql.mutation, etc.)
- **`utils/`**: Utility functions (casing, introspection, helpers)
- **`validation/`**: Input validation logic

### 🔗 Integration Tests (`integration/`)
Tests requiring external services (database, cache, etc.)

- **`database/`**: Database integration
  - **`repository/`**: Repository pattern, CQRS, data access
  - **`sql/`**: SQL generation, WHERE clauses, ORDER BY
- **`graphql/`**: GraphQL execution engine
  - **`queries/`**: Query execution and complexity
  - **`mutations/`**: Mutation patterns and error handling
  - **`subscriptions/`**: Real-time subscriptions
  - **`schema/`**: Schema introspection and building
- **`auth/`**: Authentication and authorization
- **`caching/`**: Caching strategies and cache invalidation
- **`performance/`**: Performance optimization (N+1 detection, field limits)

### 🌐 System Tests (`system/`)
End-to-end system tests

- **`fastapi/`**: FastAPI integration, middleware, routing
- **`cli/`**: Command-line interface functionality
- **`deployment/`**: Monitoring, tracing, production concerns

### 🐛 Regression Tests (`regression/`)
Version-specific and bug-fix regression tests

- **`v0_1_0/`**: Version 0.1.0 regression tests
- **`v0_4_0/`**: Version 0.4.0 regression tests
- **`json_passthrough/`**: JSON passthrough feature regressions

### 🛠️ Fixtures (`fixtures/`)
Test utilities and setup

- **`database/`**: Database setup, teardown, and fixtures
- **`auth/`**: Authentication fixtures and helpers
- **`common/`**: Common test utilities and patterns

## Running Tests

### Run All Tests
```bash
pytest tests_new/
```

### Run by Category
```bash
# Unit tests only (fast)
pytest tests_new/unit/

# Integration tests (requires services)
pytest tests_new/integration/

# System tests (full end-to-end)
pytest tests_new/system/

# Regression tests only
pytest tests_new/regression/
```

### Run by Functionality
```bash
# Database-related tests
pytest tests_new/integration/database/

# GraphQL-related tests
pytest tests_new/integration/graphql/

# Type system tests
pytest tests_new/unit/core/types/

# Authentication tests
pytest tests_new/integration/auth/
```

### Test Markers

Tests are marked for easy filtering:

```bash
# Run only unit tests
pytest -m unit

# Run only integration tests
pytest -m integration

# Run only database tests
pytest -m database

# Run tests that require authentication
pytest -m auth
```

## Test Naming Conventions

- **Test files**: `test_[functionality].py`
- **Test classes**: `Test[ComponentName]`
- **Test methods**: `test_[specific_behavior]`

## Dependencies by Test Layer

| Layer | External Dependencies |
|-------|----------------------|
| Unit | None (pure logic) |
| Integration | Database, Redis, External APIs |
| System | Full application stack |
| Regression | Varies by specific test |

## Migration from Old Structure

This new structure consolidates 247 test files from 35+ directories into a logical hierarchy:

- **Reduced complexity**: Clear separation of concerns
- **Better discoverability**: Logical grouping by functionality
- **Improved maintainability**: Related tests are co-located
- **Easier CI/CD**: Run only relevant test suites
- **Clearer dependencies**: Obvious which tests need external services

## Contributing

When adding new tests:

1. **Identify the layer**: Unit, Integration, or System?
2. **Find the appropriate category**: Database, GraphQL, Auth, etc.
3. **Follow naming conventions**: Clear, descriptive names
4. **Add appropriate markers**: Help with test filtering
5. **Keep tests isolated**: Each test should be independent

## Configuration Files

- **`conftest.py`**: Global test configuration and fixtures
- **`pytest.ini`**: Pytest configuration and markers
- **`fixtures/`**: Reusable test fixtures and utilities
