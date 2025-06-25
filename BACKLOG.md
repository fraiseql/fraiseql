# FraiseQL Development Backlog

## Overview

This document tracks the current state of development tasks, issues, and improvements for the FraiseQL project.

Last Updated: 2025-06-23

## Immediate Priority (P0) - CI/CD Blockers

### ✅ Completed

- [x] Add OpenTelemetry as optional dependency group
- [x] Fix OpenTelemetry test imports for missing dependencies
- [x] Update GitHub Actions to install tracing dependencies
- [x] Fix postgres_container fixture yielding issue
- [x] Update test_with_podman.sh for virtual environment support

### 🔴 Critical - Blocking CI

- [ ] Fix Black formatting issues in:
  - `tests/deployment/test_docker.py`
  - `tests/monitoring/test_metrics.py`

## High Priority (P1) - Test Failures

### Database Tests (10 failures)

- [ ] Fix `test_db_integration_simple.py` - FraiseQLRepository integration tests
- [ ] Fix `test_sql_injection_real_db.py` - SQL injection prevention tests
- [ ] Fix pagination tests in `test_pagination.py`
- [ ] Investigate "postgres_container did not yield a value" errors

### OpenTelemetry/Tracing (14 failures)

- [ ] Fix `FraiseQLTracer` implementation - tracer is None
- [ ] Fix `TracingMiddleware` initialization - missing 'app' parameter
- [ ] Update test fixtures for proper middleware initialization
- [ ] Consider making tracing tests optional if feature is optional

### Other Test Failures

- [ ] Fix Docker deployment tests (3 failures)
- [ ] Fix FastAPI dataloader integration test
- [ ] Fix minimal test failure

## Medium Priority (P2) - Infrastructure & Quality

### Testing Infrastructure

- [ ] Document unified container testing approach in main README
- [ ] Add pytest markers for different test categories
- [ ] Improve test isolation and reduce flakiness
- [ ] Add test coverage badges to README
- [ ] Create integration test guide

### Code Quality

- [ ] Address TODO comments throughout codebase
- [ ] Review and reduce ignored linting rules in pyproject.toml
- [ ] Add type hints to remaining untyped functions
- [ ] Improve error messages and logging

### Documentation

- [ ] Create comprehensive testing guide
- [ ] Document Podman support prominently
- [ ] Add troubleshooting section for common test failures
- [ ] Create architecture decision records (ADRs)

## Low Priority (P3) - Enhancements

### Features

- [ ] Complete CLI command implementations (init, generate)
- [ ] Add more example applications
- [ ] Implement missing GraphQL features (interfaces, unions)
- [ ] Add performance benchmarks

### Developer Experience

- [ ] Add pre-commit hooks for Black formatting
- [ ] Create development environment setup script
- [ ] Add VS Code recommended extensions
- [ ] Improve error messages for common issues

## Technical Debt

### Refactoring Opportunities

- [ ] Consolidate SQL generation logic
- [ ] Simplify type system implementation
- [ ] Reduce coupling between modules
- [ ] Improve test fixture organization

### Performance

- [ ] Optimize dataloader batching
- [ ] Add query result caching
- [ ] Implement connection pooling best practices
- [ ] Profile and optimize hot paths

## Future Considerations

### Major Features

- [ ] Multi-database support (MySQL, SQLite)
- [ ] GraphQL subscriptions implementation
- [ ] Schema migration tooling
- [ ] Admin interface generator

### Ecosystem

- [ ] Create FraiseQL VS Code extension
- [ ] Build online playground
- [ ] Develop migration tool from other ORMs
- [ ] Create performance comparison suite

## Notes

- The unified container testing system is working well but needs better documentation
- OpenTelemetry integration is optional but tests need proper handling
- Database tests are the most critical for the project's core functionality
- Consider using GitHub Issues for more granular task tracking

## Contributing

When working on items from this backlog:

1. Update the status as you begin work
2. Create a feature branch from `dev`
3. Add tests for new functionality
4. Update documentation as needed
5. Mark items as completed with the date

---

To update this backlog, please ensure you:

- Keep items organized by priority
- Add new items in the appropriate section
- Update completion status promptly
- Include any relevant issue/PR numbers
