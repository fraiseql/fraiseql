# Ticket: Complete FraiseQL Project Quality Improvements

## Summary
Continue improving FraiseQL project quality by removing dead code, setting up CI/CD, and establishing automated quality gates.

## Background
Initial assessment showed FraiseQL had 29.3% test coverage with critical modules under 20%. We've successfully:
- Increased test coverage by creating 7 comprehensive test files (2,736 lines)
- Reduced linting errors from 540 to ~100 
- Fixed all type errors
- Committed improvements in two batches

## Objectives
1. Safely remove verified dead code
2. Establish CI/CD pipeline with quality gates
3. Set up automated documentation
4. Refactor complex functions

## Tasks

### 1. Dead Code Removal (2-3 hours)
**Priority**: Medium  
**Blockers**: Requires thorough testing before removal

- [ ] Run full test suite with coverage report
- [ ] Verify these files have no hidden dependencies:
  - `gql/graphql_entrypoint.py` (0% coverage, no imports found)
  - `mutations/selection_filter.py` (0% coverage, no imports found)
  - `types/common_inputs.py` (0% coverage, no imports found)
  - `types/common_outputs.py` (0% coverage, no imports found)
  - `types/protocols.py` (0% coverage, no imports found)
  - `utils/introspection.py` (0% coverage, no imports found)
  - `utils/ip_utils.py` (0% coverage, no imports found)
- [ ] DO NOT remove `mutations/decorators_v2.py` and `registry_v2.py` (used in examples)
- [ ] DO NOT remove `core/registry.py` without further investigation
- [ ] Update any affected imports
- [ ] Run tests again to ensure nothing breaks
- [ ] Commit with clear message about what was removed and why

### 2. CI/CD Pipeline Setup (3-4 hours)
**Priority**: High  
**Location**: `.github/workflows/ci.yml`

- [ ] Create GitHub Actions workflow with:
  ```yaml
  - Python 3.11, 3.12, 3.13 matrix
  - PostgreSQL service container
  - Steps: lint, type check, test, coverage
  ```
- [ ] Configure:
  - [ ] Ruff linting with same rules as pyproject.toml
  - [ ] Pyright type checking
  - [ ] Pytest with coverage reporting
  - [ ] Coverage threshold (80% minimum)
  - [ ] Security scanning (bandit/safety)
- [ ] Add status badges to README
- [ ] Set up branch protection requiring CI to pass

### 3. Coverage Requirements (1-2 hours)
**Priority**: High  
**Dependencies**: CI/CD pipeline

- [ ] Add coverage configuration to pyproject.toml:
  ```toml
  [tool.coverage.report]
  fail_under = 80
  exclude_lines = [
    "pragma: no cover",
    "if TYPE_CHECKING:",
    "@abstractmethod",
  ]
  omit = [
    "*/migrations/*",
    "*/strawberry_compat.py",
    "*/_version.py",
  ]
  ```
- [ ] Configure codecov.io or similar service
- [ ] Add coverage badge to README
- [ ] Document coverage expectations in CONTRIBUTING.md

### 4. API Documentation (2-3 hours)
**Priority**: Medium  
**Tools**: Sphinx or MkDocs

- [ ] Set up documentation framework
- [ ] Configure autodoc for API reference
- [ ] Add docstring examples to key functions
- [ ] Create getting started guide
- [ ] Set up GitHub Pages deployment
- [ ] Add documentation badge to README

### 5. Code Quality Improvements (2-3 hours)
**Priority**: Medium  
**Focus**: Maintainability

- [ ] Fix remaining ~100 line length issues
- [ ] Identify functions >100 lines using:
  ```bash
  find src -name "*.py" -exec wc -l {} + | sort -rn
  ```
- [ ] Refactor complex functions in:
  - SQL generation modules
  - GraphQL schema builders
  - Query optimization code
- [ ] Add complexity checking to CI (radon/mccabe)

## Success Criteria
- [ ] Test coverage ≥ 80%
- [ ] All CI checks passing
- [ ] Zero linting errors (or documented exceptions)
- [ ] API documentation published
- [ ] No functions >100 lines
- [ ] Dead code removed (measured by coverage)

## Risks & Mitigations
- **Risk**: Removing files breaks hidden dependencies
  - **Mitigation**: Comprehensive testing before each removal
- **Risk**: CI/CD setup delays other work  
  - **Mitigation**: Use GitHub Actions templates, implement incrementally
- **Risk**: Coverage requirements too strict
  - **Mitigation**: Start at 70%, increase gradually

## Estimated Time
Total: 12-16 hours
- Can be parallelized across multiple developers
- CI/CD setup is highest priority (prevents regression)

## Notes
- The v2 mutations API appears to be a migration path - needs product decision on timeline
- Consider adding performance benchmarks for TurboRouter in future iteration
- Migration modules (strawberry_compat) intentionally excluded from coverage