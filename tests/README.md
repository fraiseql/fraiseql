# FraiseQL Test Suite

A comprehensive test suite designed for reliability, maintainability, and developer productivity. Our testing architecture follows the [Testing Trophy](https://kentcdodds.com/blog/the-testing-trophy-and-testing-classifications) principle with emphasis on integration tests while maintaining strong foundations at all levels.

## 🎯 Testing Philosophy & Strategy

FraiseQL uses a multi-layer testing strategy optimized for:
- **Developer Productivity**: Fast feedback loops during development
- **Release Confidence**: Comprehensive coverage of critical paths
- **Maintainability**: Clear organization that scales with the codebase
- **Performance**: Efficient execution for CI/CD pipelines

## 📂 Test Organization & Execution Matrix

### Testing Hierarchy by Speed & Scope

| Category | Speed | Scope | Purpose | Run Frequency |
|----------|-------|--------|---------|---------------|
| **Unit** | ~1s | Single functions/classes | Component behavior | Every save |
| **Core** | ~5s | Framework internals | Core stability | Every commit |
| **Integration** | ~30s | Component interactions | API contracts | Every push |
| **System** | ~2min | Full application | End-to-end flows | Pre-merge |
| **Regression** | ~30s | Specific bug cases | Prevent regressions | CI/CD |

### 🔧 Unit Tests (`unit/`)
**Purpose**: Validate individual components in isolation
**Speed**: Sub-second execution
**Scope**: Single functions, classes, or small modules

- **`core/`**: Core FraiseQL functionality
  - **`types/`**: Type system, scalars, serialization
  - **`parsing/`**: AST parsing, query translation, fragments
  - **`json/`**: JSON handling, validation, passthrough
  - **`registry/`**: Schema registry and builder
- **`decorators/`**: Decorator functionality (@fraiseql.query, @fraiseql.mutation, etc.)
- **`utils/`**: Utility functions (casing, introspection, helpers)
- **`validation/`**: Input validation logic

**When to add unit tests**:
- New utility functions
- Complex business logic
- Data transformation logic
- Validation rules
- Error handling paths

### 🔗 Integration Tests (`integration/`)
**Purpose**: Validate component interactions and API contracts
**Speed**: Fast (1-10 seconds per test)
**Scope**: Multiple components working together

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
pytest tests/
```

### Run by Category
```bash
# Unit tests only (fast)
pytest tests/unit/

# Integration tests (requires services)
pytest tests/integration/

# System tests (full end-to-end)
pytest tests/system/

# Regression tests only
pytest tests/regression/
```

### Run by Functionality
```bash
# Database-related tests
pytest tests/integration/database/

# GraphQL-related tests
pytest tests/integration/graphql/

# Type system tests
pytest tests/unit/core/type_system/

# Authentication tests
pytest tests/integration/auth/
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

## Architecture Benefits

This structure provides:

- **Clear separation of concerns**: Each layer has distinct responsibilities
- **Logical grouping**: Related functionality is co-located
- **Selective execution**: Run only the tests you need
- **Obvious dependencies**: Easy to see which tests require external services
- **Scalable organization**: Easy to add new tests in appropriate categories

## 🚀 Workflow-Based Testing Guides

### For Feature Development
```bash
# 1. Start with unit tests for new logic
pytest tests/unit/core/ -x

# 2. Add integration tests for API changes
pytest tests/integration/graphql/ -v

# 3. Validate with system tests for full flows
pytest tests/system/fastapi_system/ -s
```

### For Bug Fixes
```bash
# 1. Write regression test first (TDD)
pytest tests/regression/ -k "new_bug_test"

# 2. Fix the bug, then validate
pytest tests/regression/ tests/integration/

# 3. Ensure no other regressions
pytest tests/ --lf  # Run last failed
```

### For Performance Optimization
```bash
# 1. Run performance benchmarks
pytest tests/integration/performance/ --benchmark

# 2. Profile specific test areas
pytest tests/integration/graphql/queries/ --profile

# 3. Validate no functionality regressions
pytest tests/integration/ tests/system/
```

### For Release Preparation
```bash
# 1. Full regression suite
pytest tests/regression/

# 2. Complete integration validation
pytest tests/integration/ --dist worksteal

# 3. System smoke tests
pytest tests/system/ --timeout=300
```

## 📊 Performance Expectations

### Target Test Performance
- **Unit tests**: < 1 second per test
- **Integration tests**: < 10 seconds per test
- **System tests**: < 60 seconds per test
- **Full suite**: < 5 minutes total

### Performance Monitoring
```bash
# Benchmark specific areas
pytest --benchmark-only tests/integration/performance/

# Profile slow tests
pytest --durations=10 tests/

# Monitor test suite growth
pytest --collect-only tests/ | grep "collected"
```

## 🧹 Test Maintenance Guidelines

### Regular Maintenance Tasks
1. **Remove obsolete tests** as functionality changes
2. **Update fixtures** when data structures evolve
3. **Optimize slow tests** exceeding performance targets
4. **Refactor duplicated logic** into shared utilities
5. **Review test coverage** for critical paths

### Version-Specific Organization
- **Regression tests** organized by version (v0_1_0/, v0_4_0/, etc.)
- **Migration tests** validate upgrade paths
- **Compatibility tests** ensure backward compatibility

## 📈 Coverage and Quality Targets

### Coverage Requirements
- **Unit tests**: 95%+ coverage of covered modules
- **Integration tests**: 85%+ coverage of API endpoints
- **Critical paths**: 100% coverage requirement

### Quality Gates
All tests must pass before:
- Merge to main branch
- Version releases
- Deployment to production

## Contributing

When adding new tests:

1. **Identify the layer**: Use the speed/scope matrix above
2. **Find the appropriate category**: Database, GraphQL, Auth, etc.
3. **Follow naming conventions**: Clear, descriptive names
4. **Add appropriate markers**: Help with test filtering
5. **Keep tests isolated**: Each test should be independent
6. **Consider performance**: Target the performance expectations above

## 🔍 Quick Reference

**Need to test a new feature?** Start with unit tests, add integration tests for APIs, consider system tests for full workflows.

**Test failing?** Check the category-specific patterns above and use `pytest -v` for detailed output.

**Performance issues?** Review performance testing patterns and use `pytest --benchmark` for measurement.

**Adding fixtures?** See `tests/fixtures/` for shared utilities and patterns.

## Configuration Files

- **`conftest.py`**: Global test configuration and fixtures
- **`pytest.ini`**: Pytest configuration and markers
- **`fixtures/`**: Reusable test fixtures and utilities

---

*This testing architecture evolves with FraiseQL. When in doubt, follow the Testing Trophy principle: more integration tests, fewer unit tests, even fewer system tests.*
