# Phase Plan: Add default_error_config to FraiseQLConfig

## Objective
Implement `default_error_config` field in `FraiseQLConfig` to allow global default error configuration for all mutations, reducing boilerplate and improving maintainability.

## Context
**Issue**: #159 - Feature request to add global default error configuration

**Current State**:
- `error_config` must be specified on every `@fraiseql.mutation` decorator
- FraiseQL already has `default_mutation_schema` and `default_query_schema` patterns
- `SchemaRegistry` provides global config access via `registry.config`
- Mutation decorator uses lazy resolution pattern for schemas

**Key Files**:
- `src/fraiseql/fastapi/config.py:113-382` - FraiseQLConfig definition
- `src/fraiseql/mutations/mutation_decorator.py:17-277` - MutationDefinition class
- `src/fraiseql/mutations/error_config.py:8-123` - MutationErrorConfig definition
- `src/fraiseql/gql/builders/registry.py:18-40` - SchemaRegistry singleton

**Pattern to Follow**:
The implementation should mirror the existing `default_mutation_schema` pattern:
1. Add field to `FraiseQLConfig`
2. Add lazy resolution property in `MutationDefinition`
3. Use `SchemaRegistry.get_instance().config` to access global config

## Files to Modify

### 1. `src/fraiseql/fastapi/config.py`
Add `default_error_config` field after `default_query_schema` (around line 290)

### 2. `src/fraiseql/mutations/mutation_decorator.py`
Update `MutationDefinition` class:
- Add `_provided_error_config` and `_resolved_error_config` attributes
- Add `error_config` property with lazy resolution
- Add `_resolve_error_config()` method (similar to `_resolve_schema`)

## Files to Create

### Test file: `tests/mutations/test_default_error_config.py`
Comprehensive tests for the new feature

---

# PHASE 1 - RED: Write Failing Test

## Objective
Write a test that verifies `default_error_config` works correctly but will fail because the feature isn't implemented yet.

## Implementation Steps

### Step 1: Create test file structure
**File**: `tests/mutations/test_default_error_config.py`

```python
"""Tests for default_error_config in FraiseQLConfig."""

import pytest
from fraiseql import (
    FraiseQLConfig,
    MutationErrorConfig,
    DEFAULT_ERROR_CONFIG,
    STRICT_STATUS_CONFIG,
    mutation,
)
from fraiseql.gql.builders.registry import SchemaRegistry


class TestDefaultErrorConfig:
    """Test default_error_config resolution in mutations."""

    def setup_method(self):
        """Reset registry before each test."""
        registry = SchemaRegistry.get_instance()
        registry.config = None

    def teardown_method(self):
        """Clean up registry after each test."""
        registry = SchemaRegistry.get_instance()
        registry.config = None

    def test_mutation_uses_global_default_error_config(self):
        """Test that mutations use default_error_config from FraiseQLConfig when not specified."""
        # Setup: Create config with default_error_config
        config = FraiseQLConfig(
            database_url="postgresql://test",
            default_error_config=DEFAULT_ERROR_CONFIG,
        )

        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Define mutation without explicit error_config
        @mutation(function="test_mutation")
        class TestMutation:
            input: dict
            success: dict
            failure: dict

        # Verify: Mutation should use global default
        from fraiseql.gql.builders.registry import SchemaRegistry
        mutation_def = SchemaRegistry.get_instance()._mutations.get("test_mutation")

        assert mutation_def is not None
        # This will fail initially - we need to implement the feature
        assert mutation_def.error_config == DEFAULT_ERROR_CONFIG
```

### Step 2: Run test to confirm it fails
```bash
uv run pytest tests/mutations/test_default_error_config.py::TestDefaultErrorConfig::test_mutation_uses_global_default_error_config -xvs
```

## Expected Output
```
FAILED tests/mutations/test_default_error_config.py::...::test_mutation_uses_global_default_error_config
AssertionError: assert None == MutationErrorConfig(...)
```

## Acceptance Criteria
- [x] Test file created with clear test case
- [x] Test runs and fails with expected assertion error
- [x] Test failure indicates `error_config` is `None` instead of `DEFAULT_ERROR_CONFIG`

## DO NOT
- Do not implement the actual feature yet
- Do not add multiple test cases (save for QA phase)
- Do not modify production code

---

# PHASE 2 - GREEN: Implement Minimum Feature

## Objective
Implement the minimal code necessary to make the RED test pass.

## Implementation Steps

### Step 1: Add field to FraiseQLConfig
**File**: `src/fraiseql/fastapi/config.py`

**Location**: After `default_query_schema` field (line 289)

```python
    default_query_schema: str = Field(
        default="public",
        description=(
            "Default schema for queries when not explicitly specified in the @query decorator. "
            "Individual queries can override this by setting schema='custom_schema'."
        ),
    )

    # NEW FIELD - Add after default_query_schema
    default_error_config: MutationErrorConfig | None = Field(
        default=None,
        description=(
            "Default error configuration for all mutations when not explicitly specified "
            "in the @mutation decorator. Individual mutations can override this by setting "
            "error_config=custom_config. If not set, mutations without explicit error_config "
            "will use None (no error configuration)."
        ),
    )
```

**Import needed at top of file**:
```python
from fraiseql.mutations.error_config import MutationErrorConfig
```

### Step 2: Update MutationDefinition to use lazy resolution
**File**: `src/fraiseql/mutations/mutation_decorator.py`

**Location**: `MutationDefinition.__init__` method (around line 29)

**Change from**:
```python
    def __init__(
        self,
        mutation_class: type,
        function_name: str | None = None,
        schema: str | None = None,
        context_params: dict[str, str] | None = None,
        error_config: MutationErrorConfig | None = None,
        enable_cascade: bool = False,
    ) -> None:
        self.mutation_class = mutation_class
        self._provided_schema = schema
        self._resolved_schema = None
        # ... other code ...
        self.error_config = error_config  # <-- CURRENT: Direct assignment
```

**Change to**:
```python
    def __init__(
        self,
        mutation_class: type,
        function_name: str | None = None,
        schema: str | None = None,
        context_params: dict[str, str] | None = None,
        error_config: MutationErrorConfig | None = None,
        enable_cascade: bool = False,
    ) -> None:
        self.mutation_class = mutation_class
        self._provided_schema = schema
        self._resolved_schema = None
        self._provided_error_config = error_config  # <-- NEW: Store provided value
        self._resolved_error_config = None  # <-- NEW: Lazy resolution
        # ... other code ...
```

### Step 3: Add error_config property with lazy resolution
**File**: `src/fraiseql/mutations/mutation_decorator.py`

**Location**: After the `schema` property (after line 136)

```python
    @property
    def error_config(self) -> MutationErrorConfig | None:
        """Get the error config, resolving it lazily if needed."""
        if self._resolved_error_config is None:
            self._resolved_error_config = self._resolve_error_config(self._provided_error_config)
        return self._resolved_error_config

    def _resolve_error_config(self, provided_error_config: MutationErrorConfig | None) -> MutationErrorConfig | None:
        """Resolve the error config to use, considering defaults from config.

        Resolution order:
        1. Explicit error_config parameter on decorator (highest priority)
        2. default_error_config from FraiseQLConfig
        3. None (no error configuration)
        """
        # If error_config was explicitly provided, use it (even if None)
        if provided_error_config is not None:
            return provided_error_config

        # Try to get default from registry config
        try:
            from fraiseql.gql.builders.registry import SchemaRegistry

            registry = SchemaRegistry.get_instance()

            if registry.config and hasattr(registry.config, "default_error_config"):
                return registry.config.default_error_config
        except ImportError:
            pass

        # Fall back to None (no error configuration)
        return None
```

### Step 4: Run test to verify it passes
```bash
uv run pytest tests/mutations/test_default_error_config.py::TestDefaultErrorConfig::test_mutation_uses_global_default_error_config -xvs
```

## Expected Output
```
tests/mutations/test_default_error_config.py::TestDefaultErrorConfig::test_mutation_uses_global_default_error_config PASSED
```

## Acceptance Criteria
- [x] `default_error_config` field added to `FraiseQLConfig`
- [x] `MutationDefinition` uses lazy resolution for `error_config`
- [x] Test passes successfully
- [x] No existing tests broken

## DO NOT
- Do not add extra features or optimizations
- Do not write additional tests yet
- Do not update documentation yet

---

# PHASE 3 - REFACTOR: Clean Up Implementation

## Objective
Review and clean up the implementation for consistency and code quality.

## Implementation Steps

### Step 1: Verify consistency with schema resolution pattern
- Ensure `_resolve_error_config` follows same pattern as `_resolve_schema`
- Check that property naming is consistent (`_provided_*`, `_resolved_*`)
- Verify error handling matches schema resolution

### Step 2: Check for edge cases in resolution logic
Review the resolution logic:
```python
if provided_error_config is not None:
    return provided_error_config
```

**Question**: Should this be `if provided_error_config is not None:` or just `if provided_error_config:`?
- With `is not None`: Explicit `error_config=None` on decorator will skip global default
- Without: Explicit `None` would fall through to global default

**Decision**: Use `is not None` for consistency with schema pattern - this allows explicit override to `None`.

**BUT WAIT** - there's a subtle issue here. Let me check the mutation decorator signature...

Looking at the decorator, the default is `error_config: MutationErrorConfig | None = None`.

**Problem**: We can't distinguish between:
- User explicitly passing `error_config=None` (should use `None`)
- User not passing `error_config` at all (should use global default)

**Solution**: Check how schema handles this...

Looking at schema resolution (line 118-136), it has the same "issue" but it works because:
- When user doesn't pass `schema`, it's `None` → falls through to default
- When user passes `schema="custom"`, it's not `None` → uses custom
- User can't explicitly pass `schema=None` to skip default

**For error_config**, we need the same behavior:
- Don't pass `error_config` → use global default
- Pass `error_config=CUSTOM_CONFIG` → use custom config
- Can't explicitly pass `None` to skip default (this is OK, matches schema behavior)

**Current implementation is correct** - matches schema pattern exactly.

### Step 3: Review imports and exports
- Verify `MutationErrorConfig` is imported in `config.py`
- Check if we need to export anything new from `__init__.py` (probably not)

### Step 4: Run full mutation test suite
```bash
uv run pytest tests/mutations/ -xvs
```

## Expected Output
```
All tests pass
```

## Acceptance Criteria
- [x] Implementation follows existing patterns consistently
- [x] No regressions in existing mutation tests
- [x] Code is clean and well-commented
- [x] Edge cases are handled correctly

## DO NOT
- Do not add new features
- Do not change existing behavior
- Do not optimize prematurely

---

# PHASE 4 - QA: Comprehensive Testing

## Objective
Add comprehensive tests covering all scenarios and edge cases.

## Implementation Steps

### Step 1: Add test for explicit override
**File**: `tests/mutations/test_default_error_config.py`

Add test method:
```python
    def test_explicit_error_config_overrides_default(self):
        """Test that explicit error_config on decorator overrides global default."""
        # Setup: Config with DEFAULT_ERROR_CONFIG as default
        config = FraiseQLConfig(
            database_url="postgresql://test",
            default_error_config=DEFAULT_ERROR_CONFIG,
        )

        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Define mutation WITH explicit error_config (different from default)
        @mutation(function="test_override", error_config=STRICT_STATUS_CONFIG)
        class TestMutation:
            input: dict
            success: dict
            failure: dict

        # Verify: Should use explicit config, not global default
        mutation_def = SchemaRegistry.get_instance()._mutations.get("test_override")
        assert mutation_def is not None
        assert mutation_def.error_config == STRICT_STATUS_CONFIG
        assert mutation_def.error_config != DEFAULT_ERROR_CONFIG
```

### Step 2: Add test for no global default
```python
    def test_no_default_error_config_returns_none(self):
        """Test that mutations get None when no global default is set."""
        # Setup: Config WITHOUT default_error_config
        config = FraiseQLConfig(
            database_url="postgresql://test",
            # default_error_config not set (None)
        )

        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Define mutation without explicit error_config
        @mutation(function="test_no_default")
        class TestMutation:
            input: dict
            success: dict
            failure: dict

        # Verify: Should be None
        mutation_def = SchemaRegistry.get_instance()._mutations.get("test_no_default")
        assert mutation_def is not None
        assert mutation_def.error_config is None
```

### Step 3: Add test for no config at all
```python
    def test_no_config_returns_none(self):
        """Test that mutations get None when registry has no config."""
        # Setup: No config in registry
        registry = SchemaRegistry.get_instance()
        registry.config = None

        # Define mutation without explicit error_config
        @mutation(function="test_no_config")
        class TestMutation:
            input: dict
            success: dict
            failure: dict

        # Verify: Should be None
        mutation_def = SchemaRegistry.get_instance()._mutations.get("test_no_config")
        assert mutation_def is not None
        assert mutation_def.error_config is None
```

### Step 4: Add test for different error configs
```python
    def test_different_default_error_configs(self):
        """Test that different global defaults work correctly."""
        test_cases = [
            (DEFAULT_ERROR_CONFIG, "default"),
            (STRICT_STATUS_CONFIG, "strict"),
            (ALWAYS_DATA_CONFIG, "always_data"),
        ]

        for expected_config, suffix in test_cases:
            # Setup
            config = FraiseQLConfig(
                database_url="postgresql://test",
                default_error_config=expected_config,
            )
            registry = SchemaRegistry.get_instance()
            registry.config = config

            # Define mutation
            function_name = f"test_{suffix}"
            @mutation(function=function_name)
            class TestMutation:
                input: dict
                success: dict
                failure: dict

            # Verify
            mutation_def = registry._mutations.get(function_name)
            assert mutation_def is not None
            assert mutation_def.error_config == expected_config
```

### Step 5: Run all new tests
```bash
uv run pytest tests/mutations/test_default_error_config.py -xvs
```

### Step 6: Run full test suite to ensure no regressions
```bash
uv run pytest tests/ -x
```

## Expected Output
```
All tests pass
No regressions
```

## Acceptance Criteria
- [x] Test covers global default usage
- [x] Test covers explicit override
- [x] Test covers no default set (None)
- [x] Test covers no config in registry
- [x] Test covers all pre-configured error configs
- [x] All tests pass
- [x] No regressions in existing tests

## DO NOT
- Do not modify production code unless tests reveal bugs
- Do not add unrelated tests

---

## Verification Commands

### Phase 1 (RED)
```bash
uv run pytest tests/mutations/test_default_error_config.py::TestDefaultErrorConfig::test_mutation_uses_global_default_error_config -xvs
# Expected: FAIL
```

### Phase 2 (GREEN)
```bash
uv run pytest tests/mutations/test_default_error_config.py::TestDefaultErrorConfig::test_mutation_uses_global_default_error_config -xvs
# Expected: PASS

uv run pytest tests/mutations/ -x
# Expected: All pass
```

### Phase 3 (REFACTOR)
```bash
uv run pytest tests/mutations/ -xvs
# Expected: All pass
```

### Phase 4 (QA)
```bash
uv run pytest tests/mutations/test_default_error_config.py -xvs
# Expected: All new tests pass

uv run pytest tests/ -x
# Expected: Full suite passes
```

## Overall Acceptance Criteria

- [x] `default_error_config` field added to `FraiseQLConfig`
- [x] Field has proper type hint: `MutationErrorConfig | None`
- [x] Field has descriptive docstring
- [x] `MutationDefinition` uses lazy resolution pattern
- [x] Resolution order: explicit > global default > None
- [x] Pattern matches existing `default_mutation_schema` implementation
- [x] Comprehensive tests cover all scenarios
- [x] No regressions in existing tests
- [x] Implementation is backward compatible

## DO NOT (Global)

- Do not change behavior of existing `error_config` parameter
- Do not modify unrelated code
- Do not add features beyond the scope of this issue
- Do not change public API exports (no new exports needed)
- Do not modify query decorator (out of scope)

---

# PHASE 5 - DOCS: Update Documentation

## Objective
Update documentation to reflect the new `default_error_config` feature in FraiseQLConfig.

## Files to Modify

### 1. `docs/reference/config.md`
Add documentation for `default_error_config` field

### 2. `docs/reference/decorators.md`
Update `@fraiseql.mutation` documentation to mention global default

## Implementation Steps

### Step 1: Add default_error_config to config.md
**File**: `docs/reference/config.md`

**Location**: After "Schema Settings" section (after line 762)

Add new section:

```markdown
## Mutation Error Handling Settings

### default_error_config

- **Type**: `MutationErrorConfig | None`
- **Default**: `None`
- **Description**: Default error configuration for all mutations when not explicitly specified in the `@mutation` decorator

**Impact**:
- When set, all mutations without an explicit `error_config` parameter will use this global default
- Individual mutations can override the global default by specifying `error_config` in the decorator
- Only used in non-HTTP mode (direct GraphQL execution); HTTP mode uses [status string taxonomy](../mutations/status-strings.md)

**Available Configurations**:

| Configuration | Description |
|---------------|-------------|
| `DEFAULT_ERROR_CONFIG` | Standard error handling with common error keywords and prefixes |
| `STRICT_STATUS_CONFIG` | Strict prefix-based error detection, fewer keywords |
| `ALWAYS_DATA_CONFIG` | Returns all statuses as data (never raises GraphQL errors) |
| Custom `MutationErrorConfig` | Define your own error detection rules |

**Examples**:

```python
from fraiseql import FraiseQLConfig, DEFAULT_ERROR_CONFIG, STRICT_STATUS_CONFIG

# Development: Use standard error handling globally
dev_config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="development",
    default_error_config=DEFAULT_ERROR_CONFIG,
)

# Production: Use stricter error handling globally
prod_config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="production",
    default_error_config=STRICT_STATUS_CONFIG,
)

# Custom error configuration
from fraiseql import MutationErrorConfig

custom_config = MutationErrorConfig(
    success_keywords={"success", "ok", "done"},
    error_prefixes={"error:", "failed:"},
    always_return_as_data=False,
)

config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    default_error_config=custom_config,
)
```

**With Mutations**:

```python
from fraiseql import mutation, FraiseQLConfig, DEFAULT_ERROR_CONFIG, STRICT_STATUS_CONFIG

# Global config with default error handling
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    default_error_config=DEFAULT_ERROR_CONFIG,  # Applied to all mutations by default
)

# Mutation uses global default (no error_config specified)
@mutation(function="create_user")
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError
    # Uses DEFAULT_ERROR_CONFIG from config

# Mutation overrides global default
@mutation(
    function="delete_user",
    error_config=STRICT_STATUS_CONFIG,  # Override: Use stricter config for deletions
)
class DeleteUser:
    input: DeleteUserInput
    success: DeleteUserSuccess
    failure: DeleteUserError
    # Uses STRICT_STATUS_CONFIG (explicit override)
```

**Resolution Order**:
1. Explicit `error_config` in `@mutation` decorator (highest priority)
2. `default_error_config` from `FraiseQLConfig`
3. `None` (no error configuration, uses default behavior)

**Benefits**:
- **DRY Principle**: Set error handling once, apply everywhere
- **Environment-aware**: Different configs for dev/staging/prod
- **Maintainability**: Change error strategy in one place
- **Flexibility**: Override per-mutation when needed

**See Also**:
- [Mutation Decorator](./decorators.md#fraiseqlmutation) - Mutation decorator reference
- [Status Strings](../mutations/status-strings.md) - Status string conventions (HTTP mode)
- [MutationErrorConfig](../api-reference/README.md) - Error config API reference
```

### Step 2: Update decorators.md to mention global default
**File**: `docs/reference/decorators.md`

**Location**: In the mutation decorator parameters table (around line 237)

**Change from**:
```markdown
| error_config | MutationErrorConfig \| None | None | **DEPRECATED** - Only used in non-HTTP mode. HTTP mode uses [status string taxonomy](../mutations/status-strings.md) |
```

**Change to**:
```markdown
| error_config | MutationErrorConfig \| None | None | Error configuration for this mutation. If not specified, uses `default_error_config` from `FraiseQLConfig` (if set). **DEPRECATED** - Only used in non-HTTP mode. HTTP mode uses [status string taxonomy](../mutations/status-strings.md) |
```

**Add note after parameters table** (after line 238):

```markdown
**Global Default**: If you don't specify `error_config` on a mutation, FraiseQL will use `default_error_config` from your `FraiseQLConfig` (if set). This allows you to set a global error handling strategy and override it per-mutation when needed.

```python
from fraiseql import FraiseQLConfig, DEFAULT_ERROR_CONFIG, STRICT_STATUS_CONFIG

# Set global default
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    default_error_config=DEFAULT_ERROR_CONFIG,
)

# Uses global default
@fraiseql.mutation(function="create_user")
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError

# Overrides global default
@fraiseql.mutation(
    function="delete_user",
    error_config=STRICT_STATUS_CONFIG,  # Override
)
class DeleteUser:
    input: DeleteUserInput
    success: DeleteUserSuccess
    failure: DeleteUserError
```

**See**: [FraiseQLConfig.default_error_config](./config.md#default_error_config) for details.
```

### Step 3: Verify documentation links work
```bash
# Check that all referenced docs exist
ls -la docs/mutations/status-strings.md
ls -la docs/reference/config.md
ls -la docs/reference/decorators.md
```

### Step 4: Build and review docs locally (if mkdocs is set up)
```bash
# Optional: If mkdocs is configured
mkdocs serve
# Then review http://localhost:8000
```

## Expected Output
- `docs/reference/config.md` has new "Mutation Error Handling Settings" section
- `docs/reference/decorators.md` updated with global default information
- All internal links resolve correctly
- Documentation is clear and includes examples

## Acceptance Criteria
- [x] `default_error_config` documented in config.md
- [x] New section follows existing config.md format
- [x] Examples show common use cases (dev vs prod, override)
- [x] Resolution order clearly documented
- [x] Benefits section explains value proposition
- [x] Decorators.md mentions global default behavior
- [x] Links between config.md and decorators.md work
- [x] References to related docs (status-strings.md) included

## DO NOT
- Do not rewrite existing sections unrelated to this feature
- Do not change documentation structure or navigation
- Do not add examples for unrelated features
- Do not modify code examples in other sections

---

## Verification Commands (Updated)

### Phase 1 (RED)
```bash
uv run pytest tests/mutations/test_default_error_config.py::TestDefaultErrorConfig::test_mutation_uses_global_default_error_config -xvs
# Expected: FAIL
```

### Phase 2 (GREEN)
```bash
uv run pytest tests/mutations/test_default_error_config.py::TestDefaultErrorConfig::test_mutation_uses_global_default_error_config -xvs
# Expected: PASS

uv run pytest tests/mutations/ -x
# Expected: All pass
```

### Phase 3 (REFACTOR)
```bash
uv run pytest tests/mutations/ -xvs
# Expected: All pass
```

### Phase 4 (QA)
```bash
uv run pytest tests/mutations/test_default_error_config.py -xvs
# Expected: All new tests pass

uv run pytest tests/ -x
# Expected: Full suite passes
```

### Phase 5 (DOCS)
```bash
# Verify doc files exist and are valid markdown
cat docs/reference/config.md | grep -A 5 "default_error_config"
cat docs/reference/decorators.md | grep -A 5 "Global Default"

# Check for broken internal links (optional)
# markdown-link-check docs/reference/config.md
# markdown-link-check docs/reference/decorators.md
```
