# FraiseQL Test Suite Reorganization Plan

## Current Issues
- 247 test files scattered across 35+ directories
- Mixed granularity (unit/integration/e2e)
- Overlapping concerns and unclear boundaries
- Difficult to navigate and understand test requirements

## New Organized Structure

```
tests/
├── unit/                           # Pure unit tests (no external dependencies)
│   ├── core/                      # Core functionality
│   │   ├── types/                 # Type system tests
│   │   ├── parsing/               # AST parsing, query translation
│   │   ├── json/                  # JSON handling and validation
│   │   └── registry/              # Schema registry tests
│   ├── decorators/                # Decorator functionality
│   ├── utils/                     # Utility functions
│   └── validation/                # Input validation logic
│
├── integration/                    # Integration tests (requires DB/services)
│   ├── database/                  # Database integration
│   │   ├── repository/            # Repository pattern tests
│   │   ├── cqrs/                  # CQRS implementation
│   │   └── sql/                   # SQL generation and execution
│   ├── graphql/                   # GraphQL execution
│   │   ├── queries/               # Query execution
│   │   ├── mutations/             # Mutation execution
│   │   ├── subscriptions/         # Subscription handling
│   │   └── schema/                # Schema building and introspection
│   ├── auth/                      # Authentication/authorization
│   ├── caching/                   # Caching strategies
│   └── performance/               # Performance optimization
│
├── system/                        # End-to-end system tests
│   ├── fastapi/                   # FastAPI integration
│   ├── cli/                       # CLI functionality
│   └── deployment/                # Deployment scenarios
│
├── regression/                     # Regression tests (organized by version)
│   ├── v0_1_0/                    # Version-specific regression tests
│   ├── v0_4_0/
│   └── json_passthrough/          # Major feature regression tests
│
├── fixtures/                       # Test fixtures and utilities
│   ├── database/                  # Database setup/teardown
│   ├── auth/                      # Auth fixtures
│   └── common/                    # Common test utilities
│
├── conftest.py                    # Global test configuration
└── pytest.ini                    # Pytest configuration
```

## Migration Strategy

### Phase 1: Create New Structure
1. Create new directory hierarchy
2. Move `conftest.py` files appropriately
3. Update fixture imports

### Phase 2: Migrate Files by Category
1. **Unit Tests First**: Move pure unit tests (no DB dependencies)
2. **Integration Tests**: Move database and service-dependent tests
3. **System Tests**: Move end-to-end and API tests
4. **Regression Tests**: Consolidate version-specific and bug-fix tests

### Phase 3: Consolidate and Clean
1. Merge duplicate test classes
2. Standardize test naming conventions
3. Remove obsolete tests
4. Update import paths

### Phase 4: Verification
1. Ensure all tests pass
2. Verify test discovery works correctly
3. Update CI/CD configurations

## Benefits of New Structure

1. **Clear Separation**: Unit vs Integration vs System tests
2. **Easy Navigation**: Logical grouping by functionality
3. **Better Test Selection**: Run only relevant test suites
4. **Improved Maintainability**: Related tests grouped together
5. **Clearer Dependencies**: Obvious which tests need external services
