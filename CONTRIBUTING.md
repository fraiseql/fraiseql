# Contributing to FraiseQL

Thank you for your interest in contributing to FraiseQL! This document provides guidelines for contributing to the project.

## 🚀 Quick Start

### Development Setup
1. **Fork and Clone**: Fork the repository and clone your fork
2. **Environment**: Set up Python 3.13+ and PostgreSQL
3. **Dependencies**: Install development dependencies with `pip install -e ".[dev]"`
4. **Database**: Set up test database with `./scripts/development/test-db-setup.sh`
5. **Pre-commit**: Install pre-commit hooks with `pre-commit install`

### Making Changes
1. **Create Branch**: `git checkout -b feature/your-feature-name`
2. **Write Code**: Follow existing patterns and conventions
3. **Add Tests**: Write tests for new functionality (see `tests/README.md`)
4. **Run Tests**: `pytest tests/` to ensure everything passes
5. **Format Code**: `make lint` to format and check code style

### Submitting Changes
1. **Push Changes**: Push your branch to your fork
2. **Create PR**: Create a pull request using the provided template
3. **Address Review**: Respond to feedback and make requested changes
4. **Celebrate**: Once approved, your changes will be merged! 🎉

## 📋 Development Guidelines

### Code Quality
- **Type Hints**: All code must include type hints
- **Documentation**: Document public APIs with docstrings
- **Testing**: Maintain >95% test coverage for new code
- **Style**: Code is automatically formatted with `black` and `ruff`

### Testing Strategy
- **Unit Tests**: Add unit tests in `tests/unit/` for logic components
- **Integration Tests**: Add integration tests in `tests/integration/` for API changes
- **Examples**: Update examples in `examples/` if adding new features

### Commit Messages
- Use descriptive commit messages
- Reference issue numbers when applicable
- Follow conventional commit format when possible

## 🐛 Reporting Issues

### Bug Reports
- Use the bug report template in `.github/ISSUE_TEMPLATE/bug_report.md`
- Include steps to reproduce, expected vs actual behavior
- Provide Python and PostgreSQL versions

### Feature Requests
- Use the feature request template in `.github/ISSUE_TEMPLATE/feature_request.md`
- Describe the use case and proposed solution
- Consider backward compatibility impact

## 📚 Resources

- **Documentation**: [https://fraiseql.readthedocs.io](https://fraiseql.readthedocs.io)
- **Examples**: Check the `examples/` directory for usage patterns
- **API Reference**: See `docs/api-reference/` for detailed API documentation
- **Architecture**: Review `docs/architecture/` to understand the system design

## 🤝 Community

### Getting Help
- **Questions**: Open a GitHub Discussion or issue
- **Chat**: Join our community discussions in GitHub Discussions
- **Email**: Contact maintainer at lionel.hamayon@evolution-digitale.fr

### Code of Conduct
We are committed to providing a welcoming and inclusive community. By participating in this project, you agree to abide by our Code of Conduct (treating everyone with respect and kindness).

## 🏆 Recognition

Contributors are recognized in:
- **Changelog**: All contributors mentioned in release notes
- **Contributors**: GitHub contributors page
- **Documentation**: Contributor acknowledgments in docs

---

Thank you for helping make FraiseQL better! Every contribution, no matter how small, is valuable and appreciated. 💙
