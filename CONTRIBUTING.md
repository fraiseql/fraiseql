# Contributing to FraiseQL

Thank you for your interest in contributing to FraiseQL!

> **💡 Project Philosophy**: FraiseQL values clarity, correctness, and craft. See [docs/development/PHILOSOPHY.md](docs/development/PHILOSOPHY.md) to understand the project's design principles and collaborative approach.

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

### Running Tests

```bash
# Run all tests
pytest

# Run specific test file
pytest tests/path/to/test_file.py

# Run with coverage
pytest --cov=src/fraiseql
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
- Include tests for new features
- Update documentation as needed
- Follow the existing code style
- Ensure all CI checks pass

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
