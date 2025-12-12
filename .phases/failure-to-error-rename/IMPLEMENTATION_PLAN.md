# Implementation Plan: Rename @failure to @error

**Objective**: Standardize FraiseQL terminology to align with GraphQL conventions by renaming `@failure` decorator to `@error` throughout the codebase.

**Rationale**:
- GraphQL standard uses "error" terminology (top-level `errors`, union error types)
- FraiseQL already uses `error_type`, `error_config` internally
- Most examples in the wild use `error:` field, not `failure:`
- Eliminates vocabulary confusion for developers

**Scope** (Updated after Phase 0 Audit):
- ✅ **37 occurrences across 19 files** (original estimate of 200+ was incorrect)
- Python source: 10 occurrences (5 files)
- Test files: 18 occurrences (9 files)
- Documentation: 6 occurrences (3-4 files)
- Examples: 0 occurrences (already clean or doesn't exist)
- Rust code: 3 occurrences (comments only)
- No backward compatibility - breaking change for v2.0

**📋 Supporting Documents**:
- **[PHASE_0_AUDIT.md](PHASE_0_AUDIT.md)**: Complete pre-implementation audit with file-by-file breakdown
- **[CHECKPOINTS.md](CHECKPOINTS.md)**: Verification scripts to run after each phase

**⚠️ IMPORTANT**: Run Phase 0 audit BEFORE starting Phase 1, and run checkpoint after EACH phase.

---

## Rollback Plan

If issues are discovered during implementation:

### Phase 1 Breakage (Core Broken)
```bash
# Full rollback - core implementation is broken
git reset --hard HEAD~1
# Or revert the commit
git revert <phase_1_commit_sha>
```

### Phase 2-7 Breakage (Tests/Docs Broken)
```bash
# Fix forward - core works, only tests/docs broken
# Identify issue, fix it, re-run checkpoint
# Don't rollback - too much work invested
```

### Phase 8 QA Failures
```bash
# Fix forward - comprehensive fixes needed
# Create new phase or iterate on existing phases
# Document workarounds in PHASE_0_AUDIT.md
```

**Decision Points**:
- **Phase 1 fails**: ROLLBACK (core is critical)
- **Phase 2-10 fails**: FIX FORWARD (too much invested, core works)

---

## Phase 0: Pre-Implementation Audit ✅ COMPLETED

**Objective**: Discover and catalog ALL occurrences before implementation

**Status**: ✅ Audit completed - see [PHASE_0_AUDIT.md](PHASE_0_AUDIT.md)

**Key Findings**:
- Scope is much smaller than estimated (37 vs 200+ occurrences)
- Examples directory has 0 occurrences (Phase 3 can be skipped)
- Rust changes are comments-only (Phase 5 is trivial)
- 9 test files need updates (not ~40 as estimated)

**Checkpoint**: Run `CHECKPOINTS.md > Checkpoint 0` before Phase 1

---

## Phase 1: Core Python Implementation [RED]

**Objective**: Update the decorator definition and core mutation system

**Files to Modify**:
1. `src/fraiseql/mutations/decorators.py`
   - Rename `def failure()` → `def error()`
   - Rename `_failure_registry` → `_error_registry`
   - Update all internal references
   - Update docstrings

2. `src/fraiseql/mutations/__init__.py`
   - Change export: `from .decorators import failure` → `from .decorators import error`

3. `src/fraiseql/__init__.py`
   - Update public API export: `failure` → `error`

4. `src/fraiseql/__init__.pyi` (type stubs)
   - Update type signature: `def failure(...)` → `def error(...)`

5. `src/fraiseql/mutations/mutation_decorator.py`
   - Line 126-128: Remove support for `failure:` field
   - Keep only `error:` field support
   - Update comments referencing "failure"

**Implementation Steps**:

```python
# Step 1: Rename decorator in decorators.py
# BEFORE:
@dataclass_transform(field_specifiers=(fraise_field,))
@overload
def failure(_cls: None = None) -> Callable[[T], T]: ...
@overload
def failure(_cls: T) -> T: ...

def failure(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type."""
    # ...
    _failure_registry[cls.__name__] = cls
    # ...

# AFTER:
@dataclass_transform(field_specifiers=(fraise_field,))
@overload
def error(_cls: None = None) -> Callable[[T], T]: ...
@overload
def error(_cls: T) -> T: ...

def error(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type."""
    # ...
    _error_registry[cls.__name__] = cls
    # ...

# Step 2: Rename registries
_success_registry: dict[str, type] = {}
_error_registry: dict[str, type] = {}  # was _failure_registry
_union_registry: dict[str, object] = {}

# Step 3: Update helper functions
def _maybe_register_union(_: str) -> None:
    for success_name, success_cls in _success_registry.items():
        error_name = f"{success_name.removesuffix('Success')}Error"
        if error_name in _error_registry:  # was _failure_registry
            error_cls = _error_registry[error_name]  # was _failure_registry
            # ...

    for error_name, error_cls in _error_registry.items():  # was _failure_registry
        success_name = f"{error_name.removesuffix('Error')}Success"
        # ...

# Step 4: Update mutation_decorator.py
# Remove fallback support for 'failure' field
self.error_type = hints.get("error")  # Remove: or hints.get("failure")
```

**Verification**:
```bash
# No references to old name
! grep -r "def failure" src/fraiseql/
! grep -r "_failure_registry" src/fraiseql/
! grep -r "from.*import.*failure" src/fraiseql/

# New decorator exists
grep -r "def error" src/fraiseql/mutations/decorators.py
grep -r "_error_registry" src/fraiseql/mutations/decorators.py
```

**Expected Test Failures**: ALL tests that import or use `@failure`

**✅ Checkpoint**: After completing Phase 1, run `CHECKPOINTS.md > Checkpoint 1` to verify core implementation.

---

## Phase 2: Update All Test Files [GREEN]

**Objective**: Fix all test imports and usages

**Scope** (from Phase 0 Audit): 9 test files, 18 occurrences

**Exact Files to Update** (from audit):
1. `tests/test_mutation_field_selection_integration.py`
2. `tests/mutations/test_canary.py`
3. `tests/integration/graphql/mutations/test_mutation_failure_alias.py`
4. `tests/integration/graphql/mutations/test_decorators.py`
5. `tests/integration/graphql/mutations/test_mutation_decorator.py`
6. `tests/unit/decorators/test_empty_string_to_null.py`
7. `tests/unit/decorators/test_decorators.py`
8. `tests/unit/decorators/test_mutation_decorator.py`
9. `tests/unit/mutations/test_auto_populate_schema.py`

**Search Pattern** (to verify):
```bash
grep -r "from.*failure\|import.*failure\|@failure" tests/
```

**Files to Update** (detailed breakdown):

### 2.1: Unit Tests
1. `tests/unit/decorators/test_decorators.py`
   - Import: `from fraiseql.mutations.decorators import error`
   - Usage: `@error` decorator

2. `tests/unit/decorators/test_mutation_decorator.py`
   - Import change
   - All `@failure` → `@error`
   - Variable names: `SampleError`, `TestError` (keep as-is, just decorator changes)

3. `tests/unit/decorators/test_empty_string_to_null.py`
   - Import and decorator updates

4. `tests/unit/decorators/test_query_descriptions.py`
   - Import and decorator updates

5. `tests/unit/mutations/test_auto_populate_schema.py`
   - Import and decorator updates

6. `tests/unit/mutations/test_real_world_nested_input_scenario.py`
   - Import and decorator updates

7. `tests/unit/core/` tests
   - `test_schema_builder.py`
   - `test_coordinates.py`
   - `test_unset_error_extensions.py`
   - `test_json_field.py`

### 2.2: Integration Tests
1. `tests/integration/graphql/mutations/test_decorators.py`
2. `tests/integration/graphql/mutations/test_mutation_decorator.py`
3. `tests/integration/graphql/mutations/test_mutation_patterns.py`
4. `tests/integration/graphql/mutations/test_mutation_failure_alias.py`
   - **Special**: This file tests the alias functionality - may need to be renamed or removed
5. `tests/integration/graphql/mutations/test_similar_mutation_names_collision_fix.py`
6. `tests/integration/graphql/mutations/test_mutation_dict_responses.py`
7. `tests/integration/graphql/mutations/test_simple_mutation_regression.py`
8. `tests/integration/graphql/schema/test_resolver_wrappers.py`
9. `tests/integration/test_introspection/test_mutation_generation_integration.py`

### 2.3: Regression Tests
1. `tests/regression/test_field_conversion_underscore_number_id_bug.py`
2. `tests/regression/test_graphql_ip_address_scalar_mapping.py`
3. `tests/regression/test_v0717_graphql_validation_bypass_regression.py`
4. `tests/regression/test_printoptim_backend_bug_reproduction.py`
5. `tests/regression/v0_5_0/test_error_arrays.py`

### 2.4: Top-Level Test Files
1. `tests/test_mutation_field_selection_integration.py`
2. `tests/mutations/test_canary.py`
3. `tests/unit/test_introspection/test_metadata_parser.py`

**Batch Update Script**:
```bash
# Find and replace in all test files
find tests/ -type f -name "*.py" -exec sed -i \
  -e 's/from fraiseql.mutations.decorators import failure/from fraiseql.mutations.decorators import error/g' \
  -e 's/from fraiseql import failure/from fraiseql import error/g' \
  -e 's/import failure/import error/g' \
  -e 's/@failure/@error/g' \
  {} \;
```

**Verification**:
```bash
# Run full test suite
uv run pytest tests/ -v

# Specific checks
! grep -r "@failure" tests/
! grep -r "import failure" tests/
grep -c "@error" tests/ # Should show many matches
```

**Acceptance Criteria**:
- [ ] All tests pass
- [ ] No references to `@failure` in test files
- [ ] All error types properly decorated with `@error`

**✅ Checkpoint**: After completing Phase 2, run `CHECKPOINTS.md > Checkpoint 2` to verify all tests pass.

---

## Phase 3: Update Examples [GREEN]

**Objective**: Update all example code to use new decorator

**⚠️ NOTE from Phase 0 Audit**: Examples directory has **0 occurrences** of `@failure`. Either:
- Examples don't exist, or
- Already migrated to `@error`

**Action**: Verify examples/ exists and skip if clean. If examples exist, verify they already use `@error`.

**Files to Update** (if needed):
1. `examples/mutations_demo/demo.py`
2. `examples/blog_simple/models.py`
3. `examples/blog_simple/README.md`
4. `examples/blog_api/models.py`
5. `examples/ecommerce/models.py`
6. `examples/ecommerce_api/mutations.py`
7. `examples/enterprise_patterns/models.py`
8. `examples/where_input_filtering_example.py`
9. `examples/quickstart_5min.py`

**Pattern**:
```python
# BEFORE:
from fraiseql import failure

@failure
class CreateUserError:
    message: str

# AFTER:
from fraiseql import error

@error
class CreateUserError:
    message: str
```

**Batch Update**:
```bash
find examples/ -type f -name "*.py" -exec sed -i \
  -e 's/from fraiseql import failure/from fraiseql import error/g' \
  -e 's/from fraiseql.mutations.decorators import failure/from fraiseql.mutations.decorators import error/g' \
  -e 's/@failure/@error/g' \
  {} \;

find examples/ -type f -name "*.md" -exec sed -i \
  -e 's/@failure/@error/g' \
  -e 's/from fraiseql import failure/from fraiseql import error/g' \
  {} \;
```

**Verification**:
```bash
# Run all examples
for example in examples/*/; do
  echo "Testing $example"
  cd "$example"
  uv run python *.py 2>&1 | head -20
  cd -
done

! grep -r "@failure" examples/
```

**✅ Checkpoint**: After completing Phase 3, run `CHECKPOINTS.md > Checkpoint 3` (or skip if no examples).

---

## Phase 4: Update CLI and Introspection [GREEN]

**Objective**: Update code generation and introspection tools

**Files to Modify**:
1. `src/fraiseql/cli/commands/generate.py`
   - Update mutation template generation
   - Generated code should use `@error`

2. `src/fraiseql/introspection/mutation_generator.py`
   - Update auto-generated mutation scaffolding
   - Use `@error` in generated code

3. `src/fraiseql/utils/introspection.py`
   - Update any references to failure types

**Changes**:
```python
# In generate.py - mutation template
MUTATION_TEMPLATE = """
@fraise_input
class {name}Input:
    # TODO: Add input fields
    pass

@success
class {name}Success:
    # TODO: Add success fields
    pass

@error  # Changed from @failure
class {name}Error:
    message: str
    code: str

@mutation
class {name}:
    input: {name}Input
    success: {name}Success
    error: {name}Error  # Field name stays 'error'
"""
```

**Verification**:
```bash
# Test CLI generation
uv run fraiseql generate mutation TestMutation --output /tmp/test_gen.py
grep "@error" /tmp/test_gen.py
! grep "@failure" /tmp/test_gen.py
```

**✅ Checkpoint**: After completing Phase 4, run `CHECKPOINTS.md > Checkpoint 4` to verify CLI/introspection.

---

## Phase 5: Update Rust Code [GREEN]

**Objective**: Update any Rust code that references failure types

**✅ Confirmed from Phase 0 Audit**: Only 3 occurrences, all in comments:
- `fraiseql_rs/src/mutation/response_builder.rs:433` - "Validation failure or business rule rejection"
- `fraiseql_rs/src/mutation/response_builder.rs:453` - "Internal Server Error (generic failure)"
- `fraiseql_rs/src/mutation/test_status_only.rs:137` - "validation/business rule failure"

**Action**: Update these 3 comments to use "error" terminology for consistency

**Expected Changes**: Comments only - no code compilation changes needed

**Verification**:
```bash
# Check for failure references
! grep -i "failure.*type" fraiseql_rs/src/

# Build Rust code
cd fraiseql_rs
cargo build
cargo test
```

**✅ Checkpoint**: After completing Phase 5, run `CHECKPOINTS.md > Checkpoint 5` to verify Rust builds.

---

## Phase 6: Update Documentation [REFACTOR]

**Objective**: Update all documentation to use `@error` decorator

### 6.1: API Reference Documentation
1. `docs/api-reference/README.md`
   - Update decorator reference
   - Code examples

2. `docs/reference/decorators.md`
   - **PRIMARY DOCUMENT**: Full decorator documentation
   - Rename section: `@failure` → `@error`
   - Update all examples
   - Update description

3. `docs/reference/cli.md`
   - Update generated code examples

### 6.2: Guide Documentation
1. `docs/getting-started/first-hour.md`
   - First-time user experience
   - Update tutorial examples

2. `docs/getting-started/quickstart.md`
   - Update quickstart examples

3. `docs/guides/mutation-sql-requirements.md`
   - Update mutation examples (5 occurrences)

4. `docs/guides/troubleshooting-mutations.md`
   - Update error handling examples (4 occurrences)

5. `docs/guides/error-handling-patterns.md`
   - **CRITICAL**: 6 occurrences
   - This is the main error handling guide

6. `docs/core/queries-and-mutations.md`
   - Core concepts documentation

### 6.3: README and Top-Level Docs
1. `README.md`
   - Update main example
   - Quick start guide

2. `CHANGELOG.md`
   - Add breaking change entry for v2.0:
     ```markdown
     ## [2.0.0] - YYYY-MM-DD

     ### BREAKING CHANGES
     - **Renamed `@failure` decorator to `@error`** to align with GraphQL standards
       - All mutation error types must now use `@error` instead of `@failure`
       - The mutation class field name remains `error:` (unchanged)
       - No backward compatibility - update all `@failure` to `@error`
       - Rationale: GraphQL ecosystem uses "error" terminology universally
     ```

### 6.4: Phase Documentation
Update archived phase docs (for historical accuracy):
1. `.phases/mutation-schema-fix/`
2. `.phases/mutation-schema-fix-v2/`
3. `.phases/auto-populate-mutation-fields/`
4. `.phases/input-normalization/`
5. `.phases/archive/error-field-population/`

**Note**: These are historical records - mark them as "archived" with note:
```markdown
> **Note**: This phase plan uses the old `@failure` decorator.
> As of v2.0, use `@error` instead.
```

**Batch Update Script**:
```bash
# Update all markdown files
find docs/ -type f -name "*.md" -exec sed -i \
  -e 's/@failure/@error/g' \
  -e 's/from fraiseql import failure/from fraiseql import error/g' \
  -e 's/import failure/import error/g' \
  {} \;

# Update README
sed -i \
  -e 's/@failure/@error/g' \
  -e 's/from fraiseql import failure/from fraiseql import error/g' \
  README.md
```

**Verification**:
```bash
# Check documentation
! grep -r "@failure" docs/ --include="*.md" | grep -v "archived" | grep -v "Note:"
! grep -r "import failure" docs/ --include="*.md" | grep -v "archived"

# Verify examples work
python -m doctest docs/getting-started/quickstart.md
```

**✅ Checkpoint**: After completing Phase 6, run `CHECKPOINTS.md > Checkpoint 6` to verify documentation updated.

---

## Phase 7: Update Configuration & Misc Files [REFACTOR]

**Objective**: Update remaining configuration and miscellaneous files

**Files to Update**:
1. `deploy/kubernetes/helm/fraiseql/templates/deployment.yaml`
   - Environment variable references (if any)
   - Comments

2. `src/fraiseql/types/common.py`
   - Type definitions (2 occurrences)

3. `src/fraiseql/types/definitions.py`
   - Type system definitions (1 occurrence)

4. `src/fraiseql/audit/security_logger.py`
   - Logging references (1 occurrence)

**Verification**:
```bash
# Final sweep for any remaining references
grep -r "failure" src/ --include="*.py" | grep -v "__pycache__" | grep -v ".pyc"
grep -r "@failure" . --include="*.py" --include="*.md" | grep -v ".git" | grep -v "__pycache__"
```

**✅ Checkpoint**: After completing Phase 7, run `CHECKPOINTS.md > Checkpoint 7` for final sweep.

---

## Phase 8: Final Verification & QA [QA]

**Objective**: Comprehensive testing and validation

### 8.1: Test Suite
```bash
# Run full test suite
uv run pytest tests/ -v --tb=short

# Run with coverage
uv run pytest tests/ --cov=fraiseql --cov-report=html

# Check for any skipped tests related to failure/error
uv run pytest tests/ -v 2>&1 | grep -i "failure\|error" | head -50
```

### 8.2: Static Analysis
```bash
# Type checking
uv run mypy src/fraiseql

# Linting
uv run ruff check src/fraiseql tests/

# Check imports
uv run python -c "from fraiseql import error; print(error)"
uv run python -c "from fraiseql.mutations.decorators import error; print(error)"

# Verify old import fails
! uv run python -c "from fraiseql import failure" 2>&1 | grep "ImportError"
```

### 8.3: Example Validation
```bash
# Run all examples
for file in examples/**/*.py; do
  echo "=== Testing $file ==="
  uv run python "$file" || echo "FAILED: $file"
done
```

### 8.4: Documentation Validation
```bash
# Check for broken links or references
uv run python scripts/lint_docs.py

# Verify all code blocks in docs are valid
for doc in docs/**/*.md; do
  echo "Checking $doc"
  # Extract and validate Python code blocks
  grep -A 20 '```python' "$doc" | grep -v '```' > /tmp/code_check.py
  uv run python -m py_compile /tmp/code_check.py || echo "Invalid code in $doc"
done
```

### 8.5: Search for Stragglers
```bash
# Final check for any remaining @failure references
echo "=== Checking for @failure in source ==="
grep -r "@failure" src/ --include="*.py" || echo "✓ None found"

echo "=== Checking for failure imports in source ==="
grep -r "import failure\|from.*failure" src/ --include="*.py" || echo "✓ None found"

echo "=== Checking for @failure in tests ==="
grep -r "@failure" tests/ --include="*.py" || echo "✓ None found"

echo "=== Checking for @failure in docs ==="
grep -r "@failure" docs/ --include="*.md" | grep -v "archived" | grep -v "Note:" || echo "✓ None found (except archived)"

echo "=== Checking for @failure in examples ==="
grep -r "@failure" examples/ --include="*.py" || echo "✓ None found"

echo "=== Checking for _failure_registry ==="
grep -r "_failure_registry" src/ --include="*.py" || echo "✓ None found"
```

### 8.6: Integration Test with Real Schema
```bash
# Create a test schema using the new decorator
cat > /tmp/test_new_decorator.py << 'EOF'
from fraiseql import fraise_input, error, success, mutation, type as fraiseql_type
from fraiseql.gql.builders.registry import SchemaRegistry

@fraise_input
class TestInput:
    name: str

@fraiseql_type
class TestEntity:
    id: str
    name: str

@success
class TestSuccess:
    entity: TestEntity

@error  # NEW DECORATOR
class TestError:
    code: str
    message: str

@mutation
class TestMutation:
    input: TestInput
    success: TestSuccess
    error: TestError  # FIELD NAME

# Build schema
registry = SchemaRegistry.get_instance()
schema = registry.build_schema_string()

# Verify
assert "type TestSuccess" in schema, "TestSuccess not in schema"
assert "type TestError" in schema, "TestError not in schema"
assert "union TestMutationResult" in schema, "Union not in schema"
assert "testMutation(" in schema, "Mutation not in schema"

print("✓ Schema generation successful")
print("\nGenerated schema:")
print(schema)
EOF

uv run python /tmp/test_new_decorator.py
```

**Acceptance Criteria**:
- [ ] All tests pass (pytest exit code 0)
- [ ] No `@failure` references in source code
- [ ] No `import failure` in source code
- [ ] No `_failure_registry` in source code
- [ ] All examples run without errors
- [ ] Documentation builds without warnings
- [ ] Type checking passes (mypy)
- [ ] Linting passes (ruff)
- [ ] Schema generation works with new decorator
- [ ] Old `@failure` import raises ImportError

**✅ Checkpoint**: After completing Phase 8, run `CHECKPOINTS.md > Checkpoint 8` - comprehensive QA verification.

---

## Phase 9: Migration Guide & Communication [GREENFIELD]

**Objective**: Create migration guide for users upgrading to v2.0

### 9.1: Create Migration Guide
**File**: `docs/migration/v2.0-failure-to-error.md`

```markdown
# Migration Guide: v1.x to v2.0 - @failure → @error

## Overview

FraiseQL v2.0 renames the `@failure` decorator to `@error` to align with GraphQL
standards and eliminate vocabulary confusion.

## Breaking Changes

### Decorator Name Change

**Before (v1.x)**:
```python
from fraiseql import failure

@failure
class CreateUserError:
    message: str
    code: str
```

**After (v2.0)**:
```python
from fraiseql import error

@error
class CreateUserError:
    message: str
    code: str
```

### Import Changes

| v1.x | v2.0 |
|------|------|
| `from fraiseql import failure` | `from fraiseql import error` |
| `from fraiseql.mutations.decorators import failure` | `from fraiseql.mutations.decorators import error` |

### No Changes Required

These remain unchanged:
- Error class naming convention: `*Error` (e.g., `CreateUserError`)
- Mutation field name: `error: CreateUserError`
- Response fields: `status`, `message`, `code`, `errors`

## Migration Steps

### Automated Migration (Recommended)

Run this command in your project root:

```bash
# Find and replace @failure with @error
find . -type f -name "*.py" -exec sed -i \
  -e 's/from fraiseql import failure/from fraiseql import error/g' \
  -e 's/from fraiseql.mutations.decorators import failure/from fraiseql.mutations.decorators import error/g' \
  -e 's/@failure/@error/g' \
  {} \;
```

### Manual Migration

1. **Update imports**:
   - Search: `from fraiseql import failure`
   - Replace: `from fraiseql import error`

2. **Update decorators**:
   - Search: `@failure`
   - Replace: `@error`

3. **Verify**: Run tests to ensure nothing broke

### Example Migration

```python
# BEFORE (v1.x)
from fraiseql import fraise_input, success, failure, mutation

@fraise_input
class CreateUserInput:
    name: str
    email: str

@success
class CreateUserSuccess:
    user: User
    message: str

@failure
class CreateUserError:
    message: str
    code: str

@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError  # Field name unchanged

# AFTER (v2.0)
from fraiseql import fraise_input, success, error, mutation  # Changed

@fraise_input
class CreateUserInput:
    name: str
    email: str

@success
class CreateUserSuccess:
    user: User
    message: str

@error  # Changed
class CreateUserError:
    message: str
    code: str

@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError  # Field name unchanged
```

## Rationale

This change aligns FraiseQL with GraphQL ecosystem conventions:

1. **GraphQL Standard**: Uses `errors` in response format
2. **Common Practice**: Union types like `UserError`, `ValidationError`
3. **Internal Consistency**: FraiseQL already uses `error_type`, `error_config`
4. **Developer Experience**: "Error" is more intuitive than "Failure"

## FAQ

**Q: Why not support both `@failure` and `@error`?**
A: Clean break prevents confusion. The migration is straightforward (find/replace).

**Q: Does the mutation field name change?**
A: No. It remains `error: YourErrorType` (was already `error`, not `failure`).

**Q: Do I need to rename my error classes?**
A: No. `CreateUserError` is still the recommended naming convention.

**Q: What about old code?**
A: v1.x continues to work with `@failure`. Upgrade to v2.0 when ready to migrate.

## Support

If you encounter issues during migration:
- Check the [troubleshooting guide](../guides/troubleshooting-mutations.md)
- Open an issue: https://github.com/fraiseql/fraiseql/issues
```

### 9.2: Update CHANGELOG.md

Add to the top of CHANGELOG.md:

```markdown
## [2.0.0] - YYYY-MM-DD

### BREAKING CHANGES

#### Decorator Rename: @failure → @error

The `@failure` decorator has been renamed to `@error` to align with GraphQL
standards and eliminate vocabulary confusion.

**Migration Required**:
- Change all `from fraiseql import failure` → `from fraiseql import error`
- Change all `@failure` → `@error`
- The mutation field name `error:` remains unchanged
- See [Migration Guide](docs/migration/v2.0-failure-to-error.md) for details

**Rationale**:
- GraphQL ecosystem uses "error" terminology universally
- FraiseQL internals already use `error_type`, `error_config`
- Eliminates confusion between decorator name and field name

**Automated Migration**:
```bash
find . -type f -name "*.py" -exec sed -i \
  -e 's/from fraiseql import failure/from fraiseql import error/g' \
  -e 's/@failure/@error/g' \
  {} \;
```

### Changed
- Renamed `@failure` decorator to `@error` throughout codebase
- Renamed internal `_failure_registry` to `_error_registry`
- Updated all documentation, examples, and tests

### Removed
- Removed support for `failure:` field in mutation classes (use `error:`)
- Removed `from fraiseql import failure` (use `error`)
```

### 9.3: Update README.md Badges/Version

Update version references:
```markdown
# FraiseQL

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/fraiseql/fraiseql)

## Quick Start

```python
from fraiseql import fraise_input, success, error, mutation  # Note: @error in v2.0
```

**✅ Checkpoint**: After completing Phase 9, run `CHECKPOINTS.md > Checkpoint 9` to verify migration guide.

---

## Summary of All Phases

| Phase | Focus | Files | Verification |
|-------|-------|-------|--------------|
| 1 | Core Python (decorators, imports) | 5 files | Import tests, grep checks |
| 2 | Test files (unit, integration, regression) | ~40 files | pytest success |
| 3 | Examples | 9 files | Examples run |
| 4 | CLI & introspection | 3 files | Generation works |
| 5 | Rust code | ~2 files | Cargo build/test |
| 6 | Documentation | ~15 files | Doc validation |
| 7 | Config & misc | ~4 files | Final grep |
| 8 | QA & verification | N/A | All checks pass |
| 9 | Migration guide | 2 files | Guide published |

**Total Estimated Files**: ~80 files
**Total Estimated Occurrences**: 200+

---

## Execution Order

```bash
# Phase 1: Core Implementation
cd /home/lionel/code/fraiseql
git checkout -b feature/rename-failure-to-error

# Modify decorators.py, __init__.py, etc.
# ... (as detailed in Phase 1)

git add src/
git commit -m "feat(decorators): rename @failure to @error [BREAKING]"

# Phase 2: Tests
# ... (batch update as detailed)
uv run pytest tests/ -v
git add tests/
git commit -m "test: update all tests to use @error decorator"

# Phase 3: Examples
# ... (batch update)
git add examples/
git commit -m "docs(examples): update to use @error decorator"

# Phase 4: CLI
git add src/fraiseql/cli src/fraiseql/introspection src/fraiseql/utils
git commit -m "feat(cli): update code generation to use @error"

# Phase 5: Rust
git add fraiseql_rs/
git commit -m "docs(rust): update comments to reference error not failure"

# Phase 6: Documentation
git add docs/
git commit -m "docs: update all documentation to use @error decorator"

# Phase 7: Misc
git add deploy/ src/fraiseql/types src/fraiseql/audit
git commit -m "chore: update remaining files to use @error terminology"

# Phase 8: QA
uv run pytest tests/ -v
uv run ruff check .
uv run mypy src/

# Phase 9: Migration Guide
git add docs/migration/v2.0-failure-to-error.md CHANGELOG.md README.md
git commit -m "docs(migration): add v2.0 migration guide for @failure → @error"

# Final verification
git log --oneline -10
git diff origin/dev --stat
```

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking existing user code | HIGH | Clear migration guide, major version bump |
| Missed references | MEDIUM | Comprehensive grep/verification in Phase 8 |
| Documentation drift | LOW | Batch updates + final sweep |
| Test failures | MEDIUM | Fix as discovered in Phase 2 |
| Import errors | HIGH | Phase 1 critical - test immediately |

---

## Success Criteria

✅ **Code**:
- No `@failure` in src/, tests/, examples/
- No `import failure` anywhere
- All tests pass
- Type checking passes
- Linting passes

✅ **Documentation**:
- No `@failure` in docs/ (except archived phases with notes)
- Migration guide published
- CHANGELOG updated
- README updated

✅ **Functionality**:
- Schema generation works
- Mutation execution works
- Error responses formatted correctly
- CLI generates correct code

✅ **User Experience**:
- Clear error message if trying to import `failure`
- Migration guide is comprehensive
- Examples demonstrate new pattern

---

## Phase 10: Code Archaeology Cleanup [EVERGREEN]

**Objective**: Remove all comments about the change history to make code appear as if it always used `@error`. This creates "evergreen" code without archaeological layers.

**Philosophy**: Code should represent the current truth, not the journey to get there. Historical context belongs in git history and CHANGELOG, not inline comments.

### 10.1: Remove Change Comments from Phase 1 Implementation

**Files to Clean**:
1. `src/fraiseql/mutations/decorators.py`
   - Remove comments like `# was _failure_registry`
   - Remove `# Changed from @failure`
   - Clean up docstrings that mention the rename

2. `src/fraiseql/mutations/mutation_decorator.py`
   - Remove: `# Support both 'error' and 'failure'` (line 128)
   - Remove: `# Remove: or hints.get("failure")`

**Before**:
```python
_error_registry: dict[str, type] = {}  # was _failure_registry

# Step 4: Update mutation_decorator.py
# Remove fallback support for 'failure' field
self.error_type = hints.get("error")  # Remove: or hints.get("failure")
```

**After**:
```python
_error_registry: dict[str, type] = {}

self.error_type = hints.get("error")
```

### 10.2: Remove Version References

**Pattern to Remove**:
- `# v1.x` / `# v2.0` version markers
- `# NEW in v2.0:`
- `# As of v2.0:`
- `# BREAKING CHANGE v2.0:`
- `# Updated for v2.0:`

**Files with Version Markers** (from grep):
```bash
# Find all version markers
grep -r "# v1\.\|# v2\.\|# NEW in\|# As of\|# BREAKING" src/ --include="*.py"
```

**Action**: Remove these comments, keep only functional documentation.

**Before**:
```python
# NEW in v2.0: Renamed from @failure to @error
@dataclass_transform(field_specifiers=(fraise_field,))
def error(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type."""
```

**After**:
```python
@dataclass_transform(field_specifiers=(fraise_field,))
def error(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type."""
```

### 10.3: Remove Historical/Archaeological Comments

**Patterns to Remove**:
- `# NOTE: This used to be...`
- `# Previously this was...`
- `# Before v2.0:`
- `# Historical note:`
- `# Legacy:` (unless referring to maintained legacy features)
- `# BEFORE:` / `# AFTER:` code comparison blocks
- `# Changed:` / `# Updated:` / `# Modified:`

**Example Clean-ups**:

**Before**:
```python
def _maybe_register_union(_: str) -> None:
    """Register union types for success/error pairs.

    NOTE: Previously used _failure_registry, renamed to _error_registry in v2.0.
    """
    for success_name, success_cls in _success_registry.items():
        error_name = f"{success_name.removesuffix('Success')}Error"
        if error_name in _error_registry:  # was _failure_registry
            error_cls = _error_registry[error_name]  # renamed from failure_cls
```

**After**:
```python
def _maybe_register_union(_: str) -> None:
    """Register union types for success/error pairs."""
    for success_name, success_cls in _success_registry.items():
        error_name = f"{success_name.removesuffix('Success')}Error"
        if error_name in _error_registry:
            error_cls = _error_registry[error_name]
```

### 10.4: Clean Up Docstrings

**Remove Migration References**:
- Docstrings should not mention `@failure` unless for backward compatibility notes
- Remove "renamed from" clauses
- Keep only current behavior documentation

**Before**:
```python
def error(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type.

    This decorator was renamed from @failure in v2.0 to align with GraphQL
    standards. It marks a class as an error response type for mutations.

    Args:
        _cls: The error class to decorate

    Returns:
        The decorated error class

    Example:
        # Before v2.0:
        @failure
        class CreateUserError:
            message: str

        # After v2.0:
        @error
        class CreateUserError:
            message: str
    """
```

**After**:
```python
def error(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type.

    Marks a class as an error response type for mutations, automatically
    adding standard error fields (status, message, code, errors).

    Args:
        _cls: The error class to decorate

    Returns:
        The decorated error class with injected fields

    Example:
        @error
        class CreateUserError:
            message: str
            validation_errors: list[str] | None = None
    """
```

### 10.5: Clean Up Implementation Plan Comments

**This File**: `.phases/failure-to-error-rename/IMPLEMENTATION_PLAN.md`
- Keep as historical record (it's in `.phases/` archive)
- Add note at top:
  ```markdown
  > **Archived Phase Plan**: This plan documents the v2.0 migration from
  > `@failure` to `@error`. The migration is complete. For current usage,
  > see the [decorators reference](../../docs/reference/decorators.md).
  ```

### 10.6: Remove Inline Code Examples with OLD/NEW

**Files with Before/After Examples**:
- Documentation is fine to keep these (shows migration path)
- **Source code** should not have inline before/after examples

**Pattern to Remove from Source**:
```python
# Example migration:
# OLD: @failure
# NEW: @error
```

**Keep only current example**:
```python
# Example:
# @error
# class CreateUserError:
#     message: str
```

### 10.7: Clean Commit Messages References

**Remove inline references to commits**:
```python
# Fixed in commit abc123: renamed failure to error
# See PR #456 for background on this change
```

**Git history is the source of truth** - no need to duplicate in code.

### 10.8: Update Type Hints and Variable Names

**Check for vestiges**:
```python
# Bad - implies there was a different name
error_registry = {}  # formerly failure_registry

# Good - just state what it is
error_registry = {}  # Registry of @error decorated types
```

### 10.9: Automated Cleanup Script

```bash
#!/bin/bash
# cleanup_archaeology.sh - Remove archaeological comments

set -e

echo "=== Phase 10: Removing Code Archaeology ==="

# Function to remove archaeological comments
remove_archaeology() {
    local file=$1

    # Remove "was X" comments
    sed -i 's/  # was _.*//' "$file"

    # Remove "renamed from" comments
    sed -i 's/  # renamed from .*//' "$file"

    # Remove "Changed from" comments
    sed -i 's/  # Changed from .*//' "$file"

    # Remove "Previously" comments
    sed -i '/# Previously .*/d' "$file"

    # Remove "NOTE: This used to" comments
    sed -i '/# NOTE: This used to .*/d' "$file"

    # Remove "Before v2.0" comments
    sed -i '/# Before v2\.0:/d' "$file"

    # Remove "NEW in v2.0" comments
    sed -i 's/# NEW in v2\.0: //' "$file"
    sed -i '/# NEW in v2\.0$/d' "$file"

    # Remove "As of v2.0" comments
    sed -i 's/# As of v2\.0: //' "$file"
    sed -i '/# As of v2\.0$/d' "$file"

    # Remove version markers in comments
    sed -i 's/\(.*\)  # v[0-9]\.[0-9]\.[0-9]/\1/' "$file"

    # Remove "Remove:" comments (from implementation notes)
    sed -i '/# Remove: .*/d' "$file"

    echo "  ✓ Cleaned $file"
}

# Clean Python source files
echo "Cleaning source files..."
remove_archaeology "src/fraiseql/mutations/decorators.py"
remove_archaeology "src/fraiseql/mutations/mutation_decorator.py"
remove_archaeology "src/fraiseql/mutations/__init__.py"
remove_archaeology "src/fraiseql/__init__.py"

# Clean key type files
for file in src/fraiseql/types/*.py; do
    remove_archaeology "$file"
done

# Clean mutation-related files
for file in src/fraiseql/mutations/*.py; do
    remove_archaeology "$file"
done

echo ""
echo "=== Manual Review Required ==="
echo "The following need human review for docstring cleanup:"
echo "  - src/fraiseql/mutations/decorators.py (docstrings)"
echo "  - src/fraiseql/mutations/mutation_decorator.py (docstrings)"
echo ""
echo "Check for:"
echo "  - BEFORE/AFTER example blocks"
echo "  - Migration path documentation"
echo "  - Version-specific notes"
echo ""

# Generate review list
echo "=== Files with potential archaeological comments ==="
grep -r "# was \|# renamed\|# Changed\|Previously\|Before v\|NEW in v\|As of v\|BEFORE:\|AFTER:" \
    src/ --include="*.py" | cut -d: -f1 | sort -u || echo "None found!"

echo ""
echo "Phase 10 automated cleanup complete."
echo "Run manual review of docstrings and then commit."
```

### 10.10: Manual Docstring Review Checklist

After running automated cleanup, manually review:

**Files Requiring Docstring Review**:
1. `src/fraiseql/mutations/decorators.py`
   - [ ] `error()` function docstring - remove migration path
   - [ ] `_maybe_register_union()` - remove historical notes
   - [ ] Module-level docstring - keep concise

2. `src/fraiseql/mutations/mutation_decorator.py`
   - [ ] `MutationDefinition.__init__()` - clean comments
   - [ ] Main `mutation()` decorator docstring - remove old examples

3. `src/fraiseql/__init__.py`
   - [ ] Public API docstring - state current API only

**Review Questions**:
- Does this comment explain *what* or *why* (keep) vs *what changed* (remove)?
- Would a new developer understand this without knowing the history? (goal: yes)
- Is the comment about current behavior (keep) or past behavior (remove)?

### 10.11: Documentation Exceptions

**Keep Historical Context In**:
- ✅ `CHANGELOG.md` - This is a historical record by design
- ✅ `docs/migration/v2.0-failure-to-error.md` - Migration guide needs history
- ✅ `.phases/` files - Archived phase plans are historical records
- ✅ Git commit messages - Source of truth for changes

**Remove Historical Context From**:
- ❌ Inline code comments
- ❌ Docstrings (unless public API deprecation notice)
- ❌ Source file headers
- ❌ Function/class documentation

### 10.12: Verification Commands

```bash
# Check for archaeological artifacts
echo "=== Checking for archaeological comments ==="

# Inline "was X" comments
grep -r " # was " src/ --include="*.py" && echo "❌ Found 'was' comments" || echo "✓ No 'was' comments"

# Renamed comments
grep -r "renamed from\|renamed to" src/ --include="*.py" && echo "❌ Found 'renamed' comments" || echo "✓ No 'renamed' comments"

# Version markers
grep -r "# v[0-9]\.\|# NEW in\|# As of v" src/ --include="*.py" && echo "❌ Found version markers" || echo "✓ No version markers"

# Change history
grep -r "# Changed\|# Updated\|# Modified\|# BEFORE:\|# AFTER:" src/ --include="*.py" && echo "❌ Found change history" || echo "✓ No change history"

# "Previously" comments
grep -r "# Previously\|# Historical\|# Legacy" src/ --include="*.py" && echo "❌ Found historical comments" || echo "✓ No historical comments"

# Old decorator name in comments
grep -r "@failure" src/ --include="*.py" && echo "❌ Found @failure references" || echo "✓ No @failure references"

echo ""
echo "=== Summary ==="
echo "Code should now be 'evergreen' - appears as if it always used @error"
```

### 10.13: Final Code State

**Goal**: Code reads as if `@error` was always the decorator name.

**Example Final State** - `decorators.py`:

```python
"""FraiseQL decorators for mutation result classes and input types."""

import types
from collections.abc import Callable
from typing import TypeVar, Union, get_args, get_origin, overload

from fraiseql.fields import fraise_field

T = TypeVar("T", bound=type[Any])

_success_registry: dict[str, type] = {}
_error_registry: dict[str, type] = {}
_union_registry: dict[str, object] = {}


@dataclass_transform(field_specifiers=(fraise_field,))
@overload
def error(_cls: None = None) -> Callable[[T], T]: ...
@overload
def error(_cls: T) -> T: ...


def error(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type.

    Automatically injects standard error fields: status, message, code, errors.
    Use this decorator to define error response types for GraphQL mutations.
    """
    def wrap(cls: T) -> T:
        # ... implementation ...
        _error_registry[cls.__name__] = cls
        return cls

    return wrap if _cls is None else wrap(_cls)
```

**No mention of**:
- `@failure`
- "renamed"
- "v2.0"
- "was _failure_registry"
- "changed from"

### 10.14: Commit Strategy for Phase 10

```bash
# After manual review
git add src/fraiseql/mutations/decorators.py
git add src/fraiseql/mutations/mutation_decorator.py
git add src/fraiseql/mutations/__init__.py
git add src/fraiseql/types/

git commit -m "refactor: remove archaeological comments from @error implementation

Remove all historical comments referencing the @failure → @error rename.
Code now appears 'evergreen' as if @error was always the decorator name.

Changes:
- Removed 'was X' inline comments
- Removed version markers (v2.0, NEW, etc.)
- Cleaned up docstrings to focus on current behavior
- Removed before/after code examples from source
- Historical context preserved in CHANGELOG and git history

The code now represents the current truth without archaeological layers."
```

**✅ Checkpoint**: After completing Phase 10, run `CHECKPOINTS.md > Checkpoint 10` to verify archaeology cleanup.

---

## Updated Success Criteria (includes Phase 10)

✅ **Code**:
- No `@failure` in src/, tests/, examples/
- No `import failure` anywhere
- All tests pass
- Type checking passes
- Linting passes
- **No archaeological comments** (was, renamed, v2.0 markers, etc.)
- **Code appears evergreen** (as if always used `@error`)

✅ **Documentation**:
- No `@failure` in docs/ (except archived phases with notes)
- Migration guide published
- CHANGELOG updated
- README updated

✅ **Functionality**:
- Schema generation works
- Mutation execution works
- Error responses formatted correctly
- CLI generates correct code

✅ **User Experience**:
- Clear error message if trying to import `failure`
- Migration guide is comprehensive
- Examples demonstrate new pattern
- **Code is clean and focused on current behavior**

---

## Updated Execution Order (with Phase 10)

```bash
# ... Phases 1-9 as before ...

# Phase 10: Archaeology Cleanup
./cleanup_archaeology.sh

# Manual review
vim src/fraiseql/mutations/decorators.py
vim src/fraiseql/mutations/mutation_decorator.py

# Verify
bash -c 'grep -r " # was " src/ --include="*.py"' || echo "✓ Clean"
bash -c 'grep -r "# v2\." src/ --include="*.py"' || echo "✓ Clean"

# Commit
git add src/
git commit -m "refactor: remove archaeological comments from @error implementation"

# Final push
git push origin feature/rename-failure-to-error
```
