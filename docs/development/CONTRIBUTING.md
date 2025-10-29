# Contributing to FraiseQL

🔴 **Contributor** - Development setup, code standards, and contribution guidelines.

## FraiseQL Craft Code

FraiseQL is designed, written, and maintained by a single developer.
In the age of AI, this is a feature — not a bug.
It allows FraiseQL to stay coherent, elegant, and deeply considered at every level.

### Principles

- **Clarity.** Code should be readable, predictable, and shaped by intent.
- **Correctness.** Type safety, explicitness, and well-defined behavior are non-negotiable.
- **Care.** Quality emerges from attention, not from scale.
- **Respect.** All collaborators and users deserve consideration, curiosity, and honesty.
- **Frugality.** Simplicity and restraint are virtues — unnecessary complexity is not.

### Collaboration

FraiseQL welcomes discussion, feedback, and contributions that uphold these principles.
Contributions that compromise clarity, correctness, or coherence will be declined — kindly but firmly.

### The Spirit of FraiseQL

FraiseQL is a work of craft.
It values depth over breadth, signal over noise, and thoughtful architecture over endless abstraction.
The goal is not to build a community of many, but a foundation of quality that endures.

---

*Inspired by the Contributor Covenant, reimagined for the era of individual craft.*

---

## 🚀 Quick Start

### Development Setup
1. **Fork and Clone**: Fork the repository and clone your fork
2. **Environment**: Set up Python 3.10+ and PostgreSQL 13+
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

### Code Quality (AI-Maintainability Standards)

FraiseQL maintains **exceptional code quality** to ensure AI maintainability:

- **Type Safety** (CRITICAL): All code must pass `pyright` with **0 errors**
  ```bash
  uv run pyright  # Must show: 0 errors, 0 warnings
  ```
- **Type Hints**: Full type annotations for all functions (no `Any` without justification)
- **Documentation**: Document public APIs with Google-style docstrings
- **Testing**: Maintain comprehensive test coverage (currently 3,448 tests)
- **Style**: Code is automatically formatted with `ruff`

**Why this matters**: FraiseQL is designed to be AI-maintainable. Perfect type safety means AI assistants (Claude Code, Copilot, Cursor) can understand and maintain the codebase reliably.

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

## 🏆 Recognition

Contributors are recognized in:
- **Changelog**: All contributors mentioned in release notes
- **Contributors**: GitHub contributors page
- **Documentation**: Contributor acknowledgments in docs

---

Thank you for helping make FraiseQL better! Every contribution, no matter how small, is valuable and appreciated. 💙
