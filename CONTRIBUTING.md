# Contributing to FraiseQL

Thank you for your interest in contributing to FraiseQL! This guide will help you get started with contributing to this innovative GraphQL-to-PostgreSQL framework.

## Code of Conduct

By participating in this project, you agree to abide by our Code of Conduct: be respectful, inclusive, and constructive. We foster a welcoming environment for all contributors.

## Getting Started

### Prerequisites

- Python 3.11 or higher
- PostgreSQL 14+ (or Docker/Podman for containerized testing)
- Git for version control

### Development Setup

1. **Fork and Clone**
   ```bash
   git clone https://github.com/YOUR_USERNAME/fraiseql.git
   cd fraiseql
   ```

2. **Set up Development Environment**
   ```bash
   python -m venv .venv
   source .venv/bin/activate  # On Windows: .venv\Scripts\activate
   pip install -e ".[dev]"
   ```

3. **Configure Testing with Podman** (Recommended)
   ```bash
   export TESTCONTAINERS_PODMAN=true
   export TESTCONTAINERS_RYUK_DISABLED=true
   ```
   Or for Docker users, ensure Docker is running.

4. **Install Pre-commit Hooks**
   ```bash
   pre-commit install
   ```

5. **Verify Setup**
   ```bash
   pytest tests/test_basic_functionality.py
   ruff check src/ tests/
   pyright src/
   ```

## Development Workflow

### 1. Create a Feature Branch
```bash
git checkout -b feature/descriptive-feature-name
# or
git checkout -b fix/issue-description
```

### 2. Make Your Changes

Follow these guidelines:
- **JSONB-First Architecture**: Understand that FraiseQL stores all data in JSONB columns
- **Type Safety**: Use comprehensive type hints throughout your code
- **Async/Await**: All database operations should be async
- **No Breaking Changes**: Maintain backward compatibility

### 3. Testing Requirements

**Critical: Use Real Database Testing**
```bash
# Run all tests with Podman
export TESTCONTAINERS_PODMAN=true
pytest

# Run specific test files
pytest tests/auth/test_decorators_extended.py -v

# Check test coverage
pytest --cov=src/fraiseql --cov-report=term-missing
```

**Testing Guidelines:**
- Write tests for all new functionality
- Maintain test coverage above 80%
- Use the `clear_registry` fixture for type registration tests
- Test with real PostgreSQL containers, not mocks
- Include edge cases and error conditions

### 4. Code Quality Checks

```bash
# Linting and formatting
ruff check src/ tests/ --fix
ruff format src/ tests/

# Type checking
pyright src/

# Run all quality checks
make check  # if available
```

### 5. Commit Your Changes

Use conventional commit messages:
```bash
git commit -m "feat: add GraphQL subscription caching support"
git commit -m "fix: resolve N+1 query detection in complex mutations"
git commit -m "docs: add migration guide from Strawberry GraphQL"
git commit -m "test: expand coverage for auth decorators module"
```

## Code Style and Standards

### Python Code Style
- Follow PEP 8 with Ruff configuration
- Use descriptive variable and function names
- Comprehensive type hints for all functions
- Docstrings for all public APIs using Google style
- Maximum line length: 100 characters

### Architecture Patterns
- **Repository Pattern**: Use `FraiseQLRepository` for database operations
- **CQRS**: Separate command and query responsibilities
- **Dependency Injection**: Use FastAPI's dependency system
- **Error Handling**: Use custom exception hierarchy from `fraiseql.errors`

### GraphQL Patterns
```python
@fraise_type
class User:
    """User entity with proper field definitions."""
    id: UUID = fraise_field(description="Unique user identifier")
    name: str = fraise_field(description="User's display name")
    email: str = fraise_field(description="User's email address")
```

## Testing Guidelines

### Test Structure
- Mirror source structure: `tests/` matches `src/fraiseql/`
- Use descriptive test class and method names
- Group related tests in classes
- Include docstrings for complex test scenarios

### Database Testing
```python
import pytest
from fraiseql.gql.schema_builder import SchemaRegistry

@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()

@pytest.mark.asyncio
async def test_user_creation_with_repository(db_session):
    """Test user creation through repository pattern."""
    # Use real database session from conftest.py
    repository = FraiseQLRepository(db_session)
    # ... test implementation
```

### Test Categories
- **Unit Tests**: Individual function and class testing
- **Integration Tests**: Database and API integration
- **Security Tests**: Authentication, authorization, input validation
- **Performance Tests**: N+1 detection, caching, query optimization

## Documentation Requirements

### Code Documentation
- All public functions and classes must have docstrings
- Include parameter descriptions and return value information
- Provide usage examples for complex APIs

### Feature Documentation
- Update relevant documentation in `docs/` for new features
- Add examples to demonstrate usage
- Update migration guides if breaking changes are introduced

### Example Format
```python
async def create_user(
    self,
    user_data: CreateUserInput,
    context: UserContext | None = None
) -> User:
    """Create a new user in the system.

    Args:
        user_data: User creation data including name and email
        context: Optional user context for authorization

    Returns:
        Created user instance with generated ID

    Raises:
        ValidationError: If user data is invalid
        AuthorizationError: If context lacks required permissions

    Example:
        >>> user_input = CreateUserInput(name="John", email="john@example.com")
        >>> user = await repository.create_user(user_input)
        >>> print(user.id)  # Generated UUID
    """
```

## Pull Request Process

### Before Submitting
1. **Rebase on Latest Main**
   ```bash
   git fetch origin
   git rebase origin/main
   ```

2. **Run Full Test Suite**
   ```bash
   export TESTCONTAINERS_PODMAN=true
   pytest --cov=src/fraiseql
   ```

3. **Check All Quality Gates**
   ```bash
   ruff check src/ tests/
   ruff format --check src/ tests/
   pyright src/
   ```

### PR Checklist
- [ ] Tests pass with real database containers
- [ ] Code coverage maintained or improved
- [ ] Documentation updated for new features
- [ ] Conventional commit messages used
- [ ] No breaking changes (or properly documented)
- [ ] Security considerations addressed
- [ ] Performance impact assessed

### PR Description Template
```markdown
## Description
Brief description of changes and motivation.

## Type of Change
- [ ] Bug fix (non-breaking change fixing an issue)
- [ ] New feature (non-breaking change adding functionality)
- [ ] Breaking change (fix or feature causing existing functionality to change)
- [ ] Documentation update

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests pass
- [ ] Manual testing performed

## Security
- [ ] No security vulnerabilities introduced
- [ ] Input validation implemented
- [ ] Authorization checks in place (if applicable)

## Performance
- [ ] No performance regressions
- [ ] Caching considerations addressed
- [ ] Database query optimization considered
```

## Security Guidelines

### Input Validation
- Validate all user inputs using the security validators
- Use parameterized queries (already handled by the framework)
- Implement proper authorization checks

### Authentication & Authorization
```python
from fraiseql.auth.decorators import requires_auth, requires_permission

@requires_auth
@requires_permission("users:write")
async def create_user(info, input: CreateUserInput) -> User:
    """Create user with proper authorization."""
    pass
```

### Security Testing
- Test authentication and authorization scenarios
- Validate input sanitization
- Check for SQL injection protection
- Verify CSRF protection for mutations

## Performance Guidelines

### Query Optimization
- Use DataLoader pattern for N+1 prevention
- Implement appropriate caching strategies
- Consider query complexity limits

### Database Best Practices
- Leverage PostgreSQL JSONB capabilities
- Use connection pooling appropriately
- Implement proper indexing strategies

## Release Process

### Version Bumping
- Follow semantic versioning (SemVer)
- Update version in `pyproject.toml`
- Update `CHANGELOG.md` with release notes

### Release Checklist
- [ ] All tests pass on CI
- [ ] Documentation updated
- [ ] Security scan passes
- [ ] Performance benchmarks acceptable
- [ ] Breaking changes documented

## Community and Support

### Getting Help
- **Issues**: Use GitHub Issues for bugs and feature requests
- **Discussions**: Use GitHub Discussions for questions and ideas
- **Documentation**: Check `docs/` directory for comprehensive guides

### Contributing Areas
- **Core Framework**: Database layer, GraphQL integration, performance
- **Security**: Authentication, authorization, input validation
- **Documentation**: Guides, examples, API documentation
- **Testing**: Test coverage, integration tests, performance tests
- **Examples**: Real-world application examples

### Code Review Process
- All contributions require review by maintainers
- Focus on code quality, security, and performance
- Constructive feedback and collaborative improvement
- Recognition for valuable contributions

## Troubleshooting Common Issues

### Database Connection Issues
```bash
# Ensure Podman is running
podman ps

# Check environment variables
echo $TESTCONTAINERS_PODMAN
```

### Test Failures
- Clear registry between tests using `clear_registry` fixture
- Ensure proper async/await usage in tests
- Check for resource cleanup in test teardown

### Type Checking Issues
- Ensure all imports are properly typed
- Use `TYPE_CHECKING` for circular imports
- Add type: ignore comments sparingly with explanations

## Recognition

We appreciate all contributions to FraiseQL! Contributors will be:
- Listed in CONTRIBUTORS.md
- Mentioned in release notes for significant contributions
- Invited to join the maintainers team for sustained contributions

Thank you for helping make FraiseQL better! 🚀

---

For questions about contributing, please open an issue or start a discussion on GitHub.
