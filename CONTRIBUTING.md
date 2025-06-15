# Contributing to FraiseQL

Thank you for your interest in contributing to FraiseQL! This guide will help you get started.

## Code of Conduct

By participating in this project, you agree to abide by our Code of Conduct: be respectful, inclusive, and constructive.

## Getting Started

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/fraiseql.git
   cd fraiseql
   ```

3. Set up development environment:
   ```bash
   python -m venv venv
   source venv/bin/activate  # On Windows: venv\Scripts\activate
   pip install -e ".[dev]"
   ```

4. Set up pre-commit hooks:
   ```bash
   pre-commit install
   ```

## Development Workflow

1. Create a feature branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. Make your changes and ensure tests pass:
   ```bash
   pytest
   make typecheck
   ruff check src/ tests/
   ```

3. Commit with descriptive messages:
   ```bash
   git commit -m "feat: add new feature"
   ```

4. Push and create a pull request

## Testing

- Write tests for new features
- Ensure all tests pass: `pytest`
- Maintain test coverage above 80%
- Use Podman for integration tests:
  ```bash
  cd examples/blog_api
  ./test-podman.sh
  ```

## Code Style

- Follow PEP 8
- Use type hints
- Run `ruff` for linting
- Run `black` for formatting
- Use descriptive variable names

## Pull Request Process

1. Update documentation for new features
2. Add tests for bug fixes and features
3. Update CHANGELOG.md
4. Ensure CI passes
5. Request review from maintainers

## Commit Message Convention

Follow conventional commits:
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation changes
- `test:` Test additions/changes
- `chore:` Maintenance tasks
- `perf:` Performance improvements

## Documentation

- Update docs/ for API changes
- Include docstrings for public APIs
- Add examples for new features

## Questions?

- Open an issue for bugs
- Start a discussion for features
- Join our community chat

Thank you for contributing to FraiseQL!
