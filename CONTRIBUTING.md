# Contributing to FraiseQL

Thank you for your interest in contributing to FraiseQL!

> **💡 Project Philosophy**: FraiseQL values clarity, correctness, and craft. See [docs/development/philosophy.md](docs/development/philosophy.md) to understand the project's design principles and collaborative approach.

## Getting Started

FraiseQL is a high-performance GraphQL framework for Python with PostgreSQL.

### Development Setup

1. **Clone the repository**
   ```bash
   git clone https://github.com/fraiseql/fraiseql.git
   cd fraiseql
   ```

2. **Install dependencies**
   ```bash
   pip install -e ".[dev,all]"
   ```

3. **Set up PostgreSQL**
   ```bash
   # Create test database
   createdb fraiseql_test
   ```

4. **Run tests**
   ```bash
   pytest
   ```

## Development Workflow

### CI/CD Pipeline

FraiseQL uses a two-tier CI pipeline for reliability and speed:

**Main CI** (runs on every PR):
- Unit tests, linting, security scans
- PostgreSQL integration tests only
- Fast feedback (~10-12 minutes)
- **Required** for merge

**Enterprise CI** (runs weekly + manual):
- Vault KMS encryption tests
- Auth0 authentication tests
- Runs when external services are available
- **Optional** (doesn't block merges)

### Running Tests

```bash
# Run all tests (may skip enterprise tests if services unavailable)
pytest

# Run only PostgreSQL tests (fast, reliable)
pytest -m 'requires_postgres'

# Run specific test file
pytest tests/path/to/test_file.py

# Run with coverage
pytest --cov=src/fraiseql

# Run enterprise tests (requires Vault/Auth0)
pytest -m 'requires_vault or requires_auth0'
```

### Local Enterprise Testing

For testing enterprise features locally:

```bash
# Vault KMS testing
docker run -d --name vault -p 8200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=fraiseql-ci-token \
  hashicorp/vault:latest

export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=fraiseql-ci-token
pytest -m 'requires_vault'

# Auth0 tests use mocks (no external service needed)
pytest -m 'requires_auth0'
```

### Code Quality

```bash
# Run linting
ruff check .

# Run type checking
mypy src/fraiseql

# Format code
ruff format .
```

### Pre-commit Hooks

The project uses pre-commit hooks to ensure code quality:

```bash
# Install hooks
pre-commit install

# Run manually
pre-commit run --all-files
```

## Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass
6. Commit your changes (`git commit -m 'Add amazing feature'`)
7. Push to your fork (`git push origin feature/amazing-feature`)
8. Open a Pull Request

### PR Guidelines

- Write clear, descriptive commit messages
- Include tests for new features with appropriate markers
- Update documentation as needed
- Follow the existing code style
- Ensure main CI checks pass (PostgreSQL tests required)
- Enterprise tests are optional but recommended for enterprise features
- Use config fixtures in tests instead of direct FraiseQLConfig creation

## Code Style

- Follow PEP 8 guidelines
- Use type hints for all functions
- Write docstrings for public APIs
- Keep functions focused and small
- Use meaningful variable names

## Testing Guidelines

- Write unit tests for new functionality
- Include integration tests where appropriate
- Aim for high test coverage
- Test edge cases and error conditions
- Use appropriate pytest markers for test categorization
- Prefer config fixtures over direct FraiseQLConfig creation

### Test Organization

FraiseQL uses pytest markers to categorize tests by their dependencies:

```bash
# Run only PostgreSQL tests (fast, reliable)
pytest -m 'requires_postgres'

# Run everything except enterprise features
pytest -m 'not requires_vault and not requires_auth0'

# Run specific test categories
pytest -m 'integration'  # Integration tests
pytest -m 'e2e'         # End-to-end tests
```

### Available Markers

| Marker | Purpose | CI Usage |
|--------|---------|----------|
| `@pytest.mark.requires_postgres` | PostgreSQL database tests | Main CI |
| `@pytest.mark.requires_vault` | HashiCorp Vault KMS tests | Enterprise CI |
| `@pytest.mark.requires_auth0` | Auth0 authentication tests | Enterprise CI |
| `@pytest.mark.integration` | Integration tests | General categorization |

### Config Fixtures

Use pre-configured fixtures instead of creating FraiseQLConfig directly:

```python
# ✅ Preferred: Use fixtures
def test_feature(test_config):
    assert test_config.environment == "testing"

# ❌ Avoid: Direct config creation
def test_feature():
    config = FraiseQLConfig(database_url="...", environment="testing")
```

Available fixtures: `test_config`, `development_config`, `production_config`, `custom_config`

See [docs/testing/config-fixtures.md](docs/testing/config-fixtures.md) for details.

## Documentation

- Update README.md if adding major features
- Add docstrings to all public functions
- Include code examples in documentation
- Update CHANGELOG.md for significant changes

## Reporting Issues

- Use the GitHub issue tracker
- Provide a clear description
- Include steps to reproduce
- Attach relevant error messages
- Specify your environment (Python version, OS, etc.)

## Questions?

- Open a discussion on GitHub
- Check existing issues and PRs
- Read the documentation

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.

---

Thank you for contributing to FraiseQL!
