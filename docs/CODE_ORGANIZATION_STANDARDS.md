# Code Organization Standards

**Version**: 2.0
**Last Updated**: January 8, 2026
**Enforced**: Yes (via CI/CD checks)

## Overview

FraiseQL maintains consistent code organization to ensure:
- **Clarity**: Easy to navigate and understand
- **Scalability**: Supports growth without degradation
- **Consistency**: Predictable structure across project
- **Maintainability**: Easier to modify and review

---

## File Organization Rules

### Python Source Files

#### Location Rules

```
✅ CORRECT LOCATIONS

src/fraiseql/
  └── module_name/
      ├── __init__.py
      ├── submodule.py
      └── nested/
          └── feature.py

tests/
  ├── unit/
  │   └── module_name/
  │       └── test_submodule.py
  └── integration/
      └── module_name/
          └── test_feature.py

❌ INCORRECT LOCATIONS

src/fraiseql/
  └── MyModule.py              # Wrong: PascalCase filename

tests/
  ├── test_*.py                # Wrong: Root level
  └── unit_tests/              # Wrong: Naming convention
```

#### File Size Guidelines

| Type | Max Size | Action |
|------|----------|--------|
| Source file | 1,500 lines | Break into subpackage |
| Test file | 500 lines | Create new test file |
| __init__.py | 100 lines | Mostly exports |
| Config file | 200 lines | Split complex configs |

**Verification**: Automated in CI/CD

```bash
# Check file sizes
python scripts/check_file_sizes.py
```

---

### Directory Structure

#### Rule: One Responsibility

Each directory should represent one functional area:

```
✅ CORRECT

types/                    # Type system (one responsibility)
├── fraise_type.py
├── fraise_input.py
└── scalars/
    └── standard/

❌ INCORRECT

types/                    # Mixed: types + scalars + utils
├── types.py
├── scalars.py
├── filters.py           # Doesn't belong
└── utils.py             # Doesn't belong
```

#### Rule: Clear Hierarchy

Maximum 3 levels of nesting for most features:

```
✅ CORRECT

src/fraiseql/
  └── enterprise/        # Level 1: Feature area
      └── rbac/          # Level 2: Sub-feature
          └── resolver.py  # Level 3: Implementation

❌ INCORRECT

src/fraiseql/
  └── enterprise/        # Level 1
      └── security/      # Level 2
          └── auth/      # Level 3
              └── jwt/   # Level 4 - Too deep!
```

---

## Naming Conventions

### Files

**Pattern**: `snake_case.py`

```
✅ CORRECT
graphql_type.py
where_generator.py
test_graphql_type.py

❌ INCORRECT
GraphQLType.py
WhereGenerator.py
TestGraphQLType.py
graphql-type.py
```

### Directories

**Pattern**: `lowercase_with_underscores`

```
✅ CORRECT
src/fraiseql/enterprise/rbac/

❌ INCORRECT
src/fraiseql/Enterprise/RBAC/
src/fraiseql/enterprise-rbac/
```

### Classes

**Pattern**: `PascalCase`

```
✅ CORRECT
class GraphQLType:
class WhereGenerator:
class TestGraphQLType:

❌ INCORRECT
class graphql_type:
class GRAPHQL_TYPE:
class graphQLType:
```

### Functions

**Pattern**: `snake_case`

```
✅ CORRECT
def execute_query():
def build_where_clause():

❌ INCORRECT
def executeQuery():
def buildWhereClause():
def Execute_Query():
```

### Test Files

**Pattern**: `test_[module_name].py`

```
✅ CORRECT
test_graphql_type.py
test_where_generator.py

❌ INCORRECT
graphql_type_test.py
TestGraphQLType.py
test_graphqltype.py
```

### Test Classes

**Pattern**: `Test[ComponentName]`

```
✅ CORRECT
class TestGraphQLType:
class TestWhereGenerator:

❌ INCORRECT
class TestFor_GraphQLType:
class GraphQLTypeTest:
class graphql_type_test:
```

### Test Methods

**Pattern**: `test_[specific_behavior]`

```
✅ CORRECT
def test_parses_simple_type():
def test_raises_error_for_invalid_input():
def test_handles_null_values():

❌ INCORRECT
def test():
def testParseSimpleType():
def shouldParseSimpleType():
def test_simple_type_parsing_should_work():
```

---

## Module Documentation

### Required Elements

Every module must have:

1. **Module docstring**:
```python
"""Module purpose in one sentence.

Longer description explaining what this module does and when to use it.

Example:
    >>> from fraiseql.module import function
    >>> result = function()

Exports:
    - ClassName: Main class
    - function: Main function
"""
```

2. **Public API definition**:
```python
__all__ = ["ClassName", "function"]
```

3. **Type hints**:
```python
def function(param: str) -> int:
    """Function description."""
    ...
```

### Optional Elements

- **Class docstrings**: For non-trivial classes
- **Function docstrings**: For public functions
- **Inline comments**: For complex logic only

---

## Module Public API

### __init__.py Files

Every package must have `__init__.py` that exports public API:

```python
"""Module name and purpose."""

from .implementation import ClassName, function

__all__ = ["ClassName", "function"]
```

**Rules**:
- Export only public API
- Keep __init__.py < 100 lines
- Import from internal modules, not external packages
- Document exports in module docstring

### Type Hints

All public functions must have complete type hints:

```python
# ✅ CORRECT
def execute_query(query: str) -> dict[str, Any]:
    """Execute GraphQL query."""
    ...

# ❌ INCORRECT
def execute_query(query):  # Missing type hints
    """Execute GraphQL query."""
    ...
```

---

## Test Organization

### Structure

```
tests/
├── unit/
│   ├── [module_name]/
│   │   └── test_[component].py
│   └── conftest.py
├── integration/
│   ├── [module_name]/
│   │   └── test_[feature].py
│   └── conftest.py
├── system/
├── regression/
├── chaos/
├── fixtures/
└── conftest.py (root)
```

### Naming

**Test files**: `test_[module_or_feature].py`

```
✅ CORRECT
tests/unit/graphql/test_parser.py
tests/integration/database/test_repository.py

❌ INCORRECT
tests/unit/graphql/parser_test.py
tests/unit/graphql/TestParser.py
```

**Test classes**: `Test[Component]`

```
✅ CORRECT
class TestGraphQLParser:
class TestRepository:

❌ INCORRECT
class GraphQLParserTest:
class test_graphql_parser:
```

**Test methods**: `test_[behavior]`

```
✅ CORRECT
def test_parses_simple_query():
def test_raises_error_on_invalid_input():

❌ INCORRECT
def should_parse_simple_query():
def testParseSimpleQuery():
```

### Test Markers

All tests must be marked with at least type and feature:

```python
import pytest

@pytest.mark.unit
@pytest.mark.core
def test_parses_simple_type():
    """Test parsing logic."""
    ...

@pytest.mark.integration
@pytest.mark.database
@pytest.mark.regression
def test_where_clause_issue_124():
    """Regression test for Issue #124."""
    ...
```

**Allowed markers** (from `pyproject.toml`):
```
Type: unit, integration, e2e, system, chaos
Feature: core, graphql, database, auth, enterprise
Special: regression, performance, skip_ci, profile
```

---

## Code Quality Rules

### Type Hints

**Requirement**: All public functions must have type hints

```python
# ✅ REQUIRED
def public_function(param: str) -> dict[str, Any]:
    """Public function."""
    ...

# ❌ NOT ALLOWED
def public_function(param):
    """Public function."""
    ...
```

### Imports

**Organization**:
1. Standard library
2. Third-party
3. Internal fraiseql
4. Blank line between groups

```python
# ✅ CORRECT
import json
from typing import Any

import pytest
from pydantic import BaseModel

from fraiseql.core import GraphQLPipeline
from fraiseql.types import fraise_type

# ❌ INCORRECT
from fraiseql.types import fraise_type
import pytest
from fraiseql.core import GraphQLPipeline
import json
```

### Docstrings

**Format**: Google style

```python
def function(param1: str, param2: int) -> bool:
    """Short description.

    Longer description if needed.

    Args:
        param1: Parameter description
        param2: Parameter description

    Returns:
        Return value description

    Raises:
        ValueError: When parameter invalid

    Example:
        >>> function("test", 42)
        True
    """
    ...
```

---

## Deprecation & Legacy Code

### Marking Deprecated Features

```python
import warnings

@deprecated(version="1.8.0", removal_version="2.0.0")
def old_function():
    """Deprecated function.

    .. deprecated:: 1.8.0
        Use :func:`new_function` instead.
    """
    warnings.warn(
        "old_function is deprecated, use new_function",
        DeprecationWarning,
        stacklevel=2
    )
    ...
```

### Archive Guidelines

Code is archived when:
- Marked deprecated for 2+ minor versions
- Not actively used
- Considered historical interest

Archive location: `.archive/deprecated/`

---

## CI/CD Checks

### Automated Validation

The following checks run automatically on every commit:

#### File Structure Check
- ✅ No new files in `tests/` root level
- ✅ Test files in proper subdirectories
- ✅ Proper naming conventions

#### File Size Check
- ✅ Source files < 1,500 lines
- ✅ Test files < 500 lines
- ✅ __init__.py < 100 lines

#### Naming Convention Check
- ✅ Files: `snake_case.py`
- ✅ Classes: `PascalCase`
- ✅ Functions: `snake_case`
- ✅ Test classes: `Test*`

#### Documentation Check
- ✅ Module docstrings present
- ✅ Public API exported in __init__.py
- ✅ Type hints on public functions

#### Import Organization
- ✅ Proper import grouping
- ✅ No circular imports

#### Test Organization
- ✅ All tests marked with type
- ✅ All tests marked with feature
- ✅ Tests in proper directories

### Running Checks Locally

```bash
# Run all organization checks
make check-organization

# Individual checks
python scripts/check_file_structure.py
python scripts/check_file_sizes.py
python scripts/check_naming.py
python scripts/check_documentation.py
python scripts/check_imports.py
python scripts/check_test_organization.py
```

---

## Adding New Code

### Checklist

Before committing new code:

- [ ] **Location**: Is module in correct directory?
- [ ] **Naming**: Does filename follow `snake_case.py`?
- [ ] **Documentation**: Does module have docstring?
- [ ] **Public API**: Are exports defined in `__init__.py`?
- [ ] **Type hints**: Do public functions have types?
- [ ] **Tests**: Are unit tests included?
- [ ] **Test naming**: Are tests properly marked?
- [ ] **File size**: Is file < 1,500 lines?

### New Feature Workflow

1. **Create feature branch**:
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Create module directory**:
   ```bash
   mkdir -p src/fraiseql/feature_name
   touch src/fraiseql/feature_name/__init__.py
   ```

3. **Create tests directory**:
   ```bash
   mkdir -p tests/unit/feature_name
   mkdir -p tests/integration/feature_name
   ```

4. **Write code following standards**

5. **Run checks**:
   ```bash
   make check-organization
   make test
   make lint
   ```

6. **Commit with message**:
   ```bash
   git add .
   git commit -m "feat: description of feature"
   ```

---

## Enforcement

### Local

Run before committing:
```bash
make check-organization
```

### Pre-commit Hook

Runs on `git commit`:
```bash
prek run --all
```

### CI/CD

Runs on every push:
- GitHub Actions validates organization
- Failing check blocks merge

---

## Exceptions

Some code is exempt from standard organization:

### Archive Code (`.archive/`)
- ✅ Can have any structure (not maintained)
- ✅ Not enforced by checks

### Generated Code
- ✅ Can have any structure (auto-generated)
- ✅ Marked as `# Auto-generated`

### Legacy Code (marked `# legacy`)
- ⚠️ Slowly migrate to standards
- ⚠️ Blocks new patterns, not retroactive

---

## Questions & Clarification

**Q: Can I put test files at `tests/` root level?**
A: No, they must be in `unit/`, `integration/`, `system/`, or other subdirectory.

**Q: How long can my module be?**
A: Maximum 1,500 lines. Beyond that, break into subpackage or separate modules.

**Q: Can I skip type hints for private functions?**
A: Yes, but public functions must have complete type hints.

**Q: Where do I put a utility function?**
A: In appropriate module (e.g., `sql/` for SQL utils, `utils/` for general).

**Q: Can I create a new top-level module?**
A: Only with clear justification. Check `docs/ORGANIZATION.md` first.

---

## See Also

- **Organization guide**: `docs/ORGANIZATION.md`
- **Deprecation policy**: `docs/DEPRECATION_POLICY.md`
- **Module structure**: `src/fraiseql/[module]/STRUCTURE.md`

---

**Last Updated**: January 8, 2026
**Status**: Enforced in CI/CD
**Next Review**: v2.1 release
