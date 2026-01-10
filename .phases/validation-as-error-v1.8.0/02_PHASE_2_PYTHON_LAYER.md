# Phase 2: Python Layer Updates

**Timeline:** Week 1, Days 4-5
**Risk Level:** MEDIUM (backward compatibility concerns)
**Dependencies:** Phase 1 (Rust core changes)
**Blocking:** Phases 3-5

---

## Objective

Update Python mutation layer to:
1. Remove `error_as_data_prefixes` concept (all errors are now Error type)
2. Update error config to reflect new mapping
3. Ensure decorator generates union types
4. Update type hints for Success/Error classes
5. Update executor to handle new response structure

---

## Files to Modify

### Critical Files
1. `src/fraiseql/mutations/error_config.py` - Error classification
2. `src/fraiseql/mutations/rust_executor.py` - Rust integration
3. `src/fraiseql/mutations/types.py` - Type definitions
4. `src/fraiseql/mutations/mutation_decorator.py` - Mutation decorator

### Supporting Files
5. `src/fraiseql/mutations/__init__.py` - Exports
6. `src/fraiseql/mutations/result_processor.py` - May need updates

---

## Implementation Steps

### Step 2.1: Update Error Config

**File:** `src/fraiseql/mutations/error_config.py`

**Current (v1.7.x):**
```python
@dataclass
class MutationErrorConfig:
    """Configurable error detection for mutations."""

    success_keywords: set[str] = field(
        default_factory=lambda: {
            "success", "completed", "ok", "done", "new",
            "existing", "updated", "deleted", "synced",
        }
    )

    error_prefixes: set[str] = field(
        default_factory=lambda: {
            "error:", "failed:", "validation_error:",
            "unauthorized:", "forbidden:", "not_found:",
            "timeout:", "conflict:",
        }
    )

    # v1.7.x: These are treated as SUCCESS ❌
    error_as_data_prefixes: set[str] = field(
        default_factory=lambda: {
            "noop:", "blocked:", "skipped:", "ignored:",
        }
    )

    def is_error_status(self, status: str) -> bool:
        """Check if a status should be treated as a GraphQL error."""
        # ... checks error_as_data_prefixes first (returns False for noop:) ...
```

**New (v1.8.0):**
```python
@dataclass
class MutationErrorConfig:
    """Configurable error detection for mutations.

    v1.8.0 Breaking Change:
    ----------------------
    - Removed `error_as_data_prefixes` (all errors are now Error type)
    - `noop:*` statuses now return Error type with code 422
    - Success type ALWAYS has non-null entity
    - Error type includes REST-like `code` field (422, 404, 409, 500)

    Migration Guide:
    ---------------
    OLD (v1.7.x):
        noop:invalid_contract_id → CreateMachineSuccess with machine=null

    NEW (v1.8.0):
        noop:invalid_contract_id → CreateMachineError with code=422

    See docs/migrations/v1.8.0.md for details.
    """

    # Success keywords (unchanged)
    success_keywords: set[str] = field(
        default_factory=lambda: {
            "success", "completed", "ok", "done", "new",
            "existing", "updated", "deleted", "synced",
            "created", "cancelled",
        }
    )

    # Error prefixes - NOW INCLUDES noop:, blocked:, etc.
    error_prefixes: set[str] = field(
        default_factory=lambda: {
            # v1.8.0: Moved from error_as_data_prefixes
            "noop:",           # Validation/business rule failures (422)
            "blocked:",        # Blocked operations (422)
            "skipped:",        # Skipped operations (422)
            "ignored:",        # Ignored operations (422)

            # Traditional errors
            "failed:",         # System failures (500)
            "unauthorized:",   # Auth failures (401)
            "forbidden:",      # Permission failures (403)
            "not_found:",      # Missing resources (404)
            "timeout:",        # Timeouts (408)
            "conflict:",       # Conflicts (409)
        }
    )

    # REMOVED in v1.8.0: error_as_data_prefixes
    # All errors are now Error type

    # Error keywords (unchanged)
    error_keywords: set[str] = field(
        default_factory=lambda: {
            "error", "failed", "fail", "invalid", "timeout",
        }
    )

    # Custom regex pattern for error detection (optional)
    error_pattern: Pattern[str] | None = None

    # DEPRECATED in v1.8.0: always_return_as_data
    # Use success_keywords and error_prefixes instead
    always_return_as_data: bool = False

    def is_error_status(self, status: str) -> bool:
        """Check if a status should be treated as a GraphQL error.

        v1.8.0: This method now returns True for noop:* statuses.

        Args:
            status: The status string from the mutation result

        Returns:
            True if this should be Error type, False if Success type
        """
        if not status:
            return False

        if self.always_return_as_data:
            # DEPRECATED: For backward compatibility only
            import warnings
            warnings.warn(
                "always_return_as_data is deprecated in v1.8.0. "
                "Use success_keywords and error_prefixes instead.",
                DeprecationWarning,
                stacklevel=2,
            )
            return False

        status_lower = status.lower()

        # Check success keywords first
        if status_lower in self.success_keywords:
            return False

        # v1.8.0: REMOVED error_as_data_prefixes check
        # All non-success prefixes are errors

        # Check error prefixes (includes noop: now)
        for prefix in self.error_prefixes:
            if status_lower.startswith(prefix):
                return True

        # Check error keywords
        if any(keyword in status_lower for keyword in self.error_keywords):
            return True

        # Check custom pattern if provided
        if self.error_pattern and self.error_pattern.match(status):
            return True

        # Default: not an error (unknown statuses are success for backward compat)
        return False

    def get_error_code(self, status: str) -> int:
        """Map status string to REST-like error code.

        v1.8.0: New method for mapping statuses to application-level codes.

        Args:
            status: The status string from the mutation result

        Returns:
            Application-level error code (422, 404, 409, 500, etc.)
        """
        if not status:
            return 500

        status_lower = status.lower()

        # Validation/business rule failures
        if status_lower.startswith(("noop:", "blocked:", "skipped:", "ignored:")):
            return 422  # Unprocessable Entity

        # Resource not found
        if status_lower.startswith("not_found:"):
            return 404  # Not Found

        # Authentication failures
        if status_lower.startswith("unauthorized:"):
            return 401  # Unauthorized

        # Permission failures
        if status_lower.startswith("forbidden:"):
            return 403  # Forbidden

        # Resource conflicts
        if status_lower.startswith("conflict:"):
            return 409  # Conflict

        # Timeouts
        if status_lower.startswith("timeout:"):
            return 408  # Request Timeout

        # System failures
        if status_lower.startswith("failed:"):
            return 500  # Internal Server Error

        # Unknown errors
        return 500


# Updated default configuration
DEFAULT_ERROR_CONFIG = MutationErrorConfig(
    success_keywords={
        "success", "completed", "ok", "done", "new",
        "existing", "updated", "deleted", "synced",
        "created", "cancelled",
    },
    error_prefixes={
        # v1.8.0: Validation/business rule failures
        "noop:", "blocked:", "skipped:", "ignored:",

        # Traditional errors
        "failed:", "unauthorized:", "forbidden:",
        "not_found:", "timeout:", "conflict:",
    },
)

# DEPRECATED in v1.8.0: STRICT_STATUS_CONFIG
# Use DEFAULT_ERROR_CONFIG instead
STRICT_STATUS_CONFIG = DEFAULT_ERROR_CONFIG

# DEPRECATED in v1.8.0: ALWAYS_DATA_CONFIG
# All errors are now Error type
@deprecated("Use DEFAULT_ERROR_CONFIG instead. ALWAYS_DATA_CONFIG is deprecated in v1.8.0.")
def ALWAYS_DATA_CONFIG():
    import warnings
    warnings.warn(
        "ALWAYS_DATA_CONFIG is deprecated in v1.8.0. "
        "All errors now return Error type with appropriate codes.",
        DeprecationWarning,
        stacklevel=2,
    )
    return MutationErrorConfig(always_return_as_data=True)
```

---

### Step 2.2: Update Type Definitions

**File:** `src/fraiseql/mutations/types.py`

**Add Error type with code field:**

```python
from dataclasses import dataclass
from typing import Any

@dataclass
class MutationError:
    """Error response for mutations (v1.8.0).

    Attributes:
        code: Application-level error code (422, 404, 409, 500, etc.)
              This is NOT an HTTP status code. HTTP is always 200 OK.
              The code field provides REST-like semantics for DX.
        status: Domain-specific status string (e.g., "noop:invalid_contract_id")
        message: Human-readable error message
        cascade: Optional cascade metadata (if enable_cascade=True)
        errors: Optional detailed error list (legacy compatibility)
    """
    code: int
    status: str
    message: str
    cascade: dict[str, Any] | None = None
    errors: list[dict[str, Any]] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        result = {
            "code": self.code,
            "status": self.status,
            "message": self.message,
        }
        if self.cascade is not None:
            result["cascade"] = self.cascade
        if self.errors:
            result["errors"] = self.errors
        return result


@dataclass
class MutationSuccess:
    """Success response for mutations (v1.8.0).

    v1.8.0: Success type ALWAYS has non-null entity.
    If entity is None, the mutation should return MutationError instead.

    Attributes:
        entity: The created/updated/deleted entity (REQUIRED)
        cascade: Optional cascade metadata (if enable_cascade=True)
        message: Optional success message
        updated_fields: Optional list of updated field names
    """
    entity: Any  # REQUIRED - never None in v1.8.0
    cascade: dict[str, Any] | None = None
    message: str | None = None
    updated_fields: list[str] | None = None

    def __post_init__(self):
        """Validate that entity is not None."""
        if self.entity is None:
            raise ValueError(
                "MutationSuccess requires non-null entity. "
                "For validation failures or errors, use MutationError instead."
            )

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        result = {"entity": self.entity}
        if self.cascade is not None:
            result["cascade"] = self.cascade
        if self.message is not None:
            result["message"] = self.message
        if self.updated_fields is not None:
            result["updated_fields"] = self.updated_fields
        return result


# Legacy type alias for backward compatibility
MutationResult = MutationSuccess | MutationError
```

---

### Step 2.3: Update Rust Executor Integration

**File:** `src/fraiseql/mutations/rust_executor.py`

**Update result processing to handle new error format:**

```python
async def execute_mutation_rust(
    mutation_def: "MutationDefinition",
    input_data: dict[str, Any],
    context: dict[str, Any],
    info: Any,
) -> dict[str, Any]:
    """Execute mutation using Rust pipeline.

    v1.8.0: Updated to handle new error format with code field.
    """
    from fraiseql_rs import execute_mutation_pipeline

    # Execute Rust pipeline
    result = execute_mutation_pipeline(
        sql_function_name=mutation_def.function_name,
        input_data=input_data,
        context=context,
        success_type=mutation_def.success_type.__name__,
        error_type=mutation_def.error_type.__name__,
        enable_cascade=mutation_def.enable_cascade,
        cascade_selections=_extract_cascade_selections(info),
    )

    # v1.8.0: Rust now returns code field for errors
    # Verify response structure
    if "__typename" in result:
        typename = result["__typename"]

        # Success type: entity must be non-null
        if typename == mutation_def.success_type.__name__:
            entity_field = _get_entity_field_name(mutation_def.success_type)
            if entity_field in result and result[entity_field] is None:
                raise ValueError(
                    f"Success type '{typename}' returned null entity. "
                    f"This indicates a logic error in the mutation or Rust pipeline. "
                    f"Validation failures should return Error type, not Success type."
                )

        # Error type: code field must be present
        elif typename == mutation_def.error_type.__name__:
            if "code" not in result:
                raise ValueError(
                    f"Error type '{typename}' missing required 'code' field. "
                    f"Ensure Rust pipeline is updated to v1.8.0."
                )
            if not isinstance(result["code"], int):
                raise ValueError(
                    f"Error type '{typename}' has invalid 'code' type: {type(result['code'])}. "
                    f"Expected int (422, 404, 409, 500)."
                )

    return result
```

---

### Step 2.4: Update Mutation Decorator

**File:** `src/fraiseql/mutations/mutation_decorator.py`

**Update to generate union types:**

```python
from dataclasses import dataclass
from typing import Type, Any

@dataclass
class MutationDefinition:
    """Definition of a FraiseQL mutation.

    v1.8.0: All mutations return union types (Success | Error).
    """
    function_name: str
    success_type: Type
    error_type: Type
    enable_cascade: bool = False
    error_config: MutationErrorConfig | None = None

    def get_graphql_return_type(self) -> str:
        """Get GraphQL return type name.

        v1.8.0: Returns union type name.

        Example:
            CreateMachine → CreateMachineResult
        """
        # Extract mutation name from class
        # Assumes mutation class is named like: CreateMachine
        mutation_name = self.success_type.__name__.replace("Success", "")
        return f"{mutation_name}Result"

    def get_graphql_union_definition(self) -> str:
        """Get GraphQL union type definition.

        v1.8.0: All mutations return unions.

        Example:
            union CreateMachineResult = CreateMachineSuccess | CreateMachineError
        """
        mutation_name = self.success_type.__name__.replace("Success", "")
        success_name = self.success_type.__name__
        error_name = self.error_type.__name__
        return f"union {mutation_name}Result = {success_name} | {error_name}"

    def validate_types(self):
        """Validate Success and Error types conform to v1.8.0 requirements."""
        # Validate Success type
        if not hasattr(self.success_type, "__annotations__"):
            raise ValueError(f"Success type {self.success_type.__name__} must have annotations")

        success_annotations = self.success_type.__annotations__

        # Success must have entity field
        entity_field = self._get_entity_field_name()
        if entity_field not in success_annotations:
            raise ValueError(
                f"Success type {self.success_type.__name__} must have '{entity_field}' field. "
                f"v1.8.0 requires Success types to always have non-null entity."
            )

        # Entity field must NOT be Optional
        entity_type = success_annotations[entity_field]
        if _is_optional(entity_type):
            raise ValueError(
                f"Success type {self.success_type.__name__} has nullable entity field. "
                f"v1.8.0 requires entity to be non-null. "
                f"Change '{entity_field}: {entity_type}' to non-nullable type."
            )

        # Validate Error type
        if not hasattr(self.error_type, "__annotations__"):
            raise ValueError(f"Error type {self.error_type.__name__} must have annotations")

        error_annotations = self.error_type.__annotations__

        # Error must have code field (v1.8.0)
        if "code" not in error_annotations:
            raise ValueError(
                f"Error type {self.error_type.__name__} must have 'code: int' field. "
                f"v1.8.0 requires Error types to include REST-like error codes."
            )

        # Code must be int
        code_type = error_annotations["code"]
        if code_type != int:
            raise ValueError(
                f"Error type {self.error_type.__name__} has wrong 'code' type: {code_type}. "
                f"Expected 'int'."
            )

        # Error must have status field
        if "status" not in error_annotations:
            raise ValueError(
                f"Error type {self.error_type.__name__} must have 'status: str' field."
            )

        # Error must have message field
        if "message" not in error_annotations:
            raise ValueError(
                f"Error type {self.error_type.__name__} must have 'message: str' field."
            )

    def _get_entity_field_name(self) -> str:
        """Get entity field name from Success type.

        Looks for common patterns: entity, <lowercase_type>, etc.
        """
        annotations = self.success_type.__annotations__

        # Common patterns
        if "entity" in annotations:
            return "entity"

        # Try lowercase type name (e.g., CreateMachineSuccess → machine)
        mutation_name = self.success_type.__name__.replace("Success", "")
        entity_name_candidate = mutation_name.lower()
        if entity_name_candidate in annotations:
            return entity_name_candidate

        # Fallback: first non-standard field
        standard_fields = {"cascade", "message", "updated_fields", "code", "status"}
        for field in annotations:
            if field not in standard_fields:
                return field

        raise ValueError(
            f"Could not determine entity field name for {self.success_type.__name__}. "
            f"Expected 'entity' or lowercase mutation name."
        )


def _is_optional(type_hint: Any) -> bool:
    """Check if type hint is Optional (includes None)."""
    import typing

    # Check for X | None (Python 3.10+)
    if hasattr(typing, "get_args") and hasattr(typing, "get_origin"):
        origin = typing.get_origin(type_hint)
        if origin is typing.Union:
            args = typing.get_args(type_hint)
            return type(None) in args

    # Check for Optional[X] (older syntax)
    return getattr(type_hint, "__origin__", None) is typing.Union and \
           type(None) in getattr(type_hint, "__args__", [])
```

---

### Step 2.5: Update __init__.py Exports

**File:** `src/fraiseql/mutations/__init__.py`

```python
"""FraiseQL Mutations - v1.8.0

Breaking Changes in v1.8.0:
---------------------------
- Validation failures now return Error type (not Success with null entity)
- Error type includes `code` field (422, 404, 409, 500)
- Success type entity is always non-null
- Removed `error_as_data_prefixes` from error config

See docs/migrations/v1.8.0.md for migration guide.
"""

from .error_config import (
    MutationErrorConfig,
    DEFAULT_ERROR_CONFIG,
    # Deprecated exports
    STRICT_STATUS_CONFIG,  # Use DEFAULT_ERROR_CONFIG
)
from .types import (
    MutationError,
    MutationSuccess,
    MutationResult,
)
from .mutation_decorator import (
    MutationDefinition,
    mutation,
)
from .decorators import (
    success,
    failure,
)

__all__ = [
    # Error configuration
    "MutationErrorConfig",
    "DEFAULT_ERROR_CONFIG",
    "STRICT_STATUS_CONFIG",  # Deprecated

    # Types
    "MutationError",
    "MutationSuccess",
    "MutationResult",

    # Decorators
    "MutationDefinition",
    "mutation",
    "success",
    "failure",
]

# Version check warning
import warnings
warnings.warn(
    "FraiseQL v1.8.0 includes breaking changes to mutation error handling. "
    "See docs/migrations/v1.8.0.md for migration guide.",
    FutureWarning,
    stacklevel=2,
)
```

---

## Testing Strategy

### Step 2.6: Update Python Tests

**File:** `tests/integration/graphql/mutations/test_error_config.py`

```python
import pytest
from fraiseql.mutations import MutationErrorConfig, DEFAULT_ERROR_CONFIG

class TestErrorConfigV190:
    """Test error config behavior in v1.8.0."""

    def test_noop_is_error_v190(self):
        """v1.8.0: noop:* statuses are errors."""
        config = DEFAULT_ERROR_CONFIG
        assert config.is_error_status("noop:invalid_contract_id") is True
        assert config.is_error_status("noop:unchanged") is True
        assert config.is_error_status("NOOP:DUPLICATE") is True

    def test_noop_maps_to_422(self):
        """noop:* statuses map to code 422."""
        config = DEFAULT_ERROR_CONFIG
        assert config.get_error_code("noop:invalid_contract_id") == 422
        assert config.get_error_code("blocked:business_rule") == 422
        assert config.get_error_code("skipped:validation") == 422

    def test_not_found_maps_to_404(self):
        """not_found:* statuses map to code 404."""
        config = DEFAULT_ERROR_CONFIG
        assert config.get_error_code("not_found:machine") == 404
        assert config.get_error_code("not_found:user") == 404

    def test_conflict_maps_to_409(self):
        """conflict:* statuses map to code 409."""
        config = DEFAULT_ERROR_CONFIG
        assert config.get_error_code("conflict:duplicate_serial") == 409

    def test_failed_maps_to_500(self):
        """failed:* statuses map to code 500."""
        config = DEFAULT_ERROR_CONFIG
        assert config.get_error_code("failed:database_error") == 500

    def test_success_not_error(self):
        """Success keywords are not errors."""
        config = DEFAULT_ERROR_CONFIG
        assert config.is_error_status("created") is False
        assert config.is_error_status("updated") is False
        assert config.is_error_status("SUCCESS") is False

    def test_error_as_data_prefixes_removed(self):
        """v1.8.0: error_as_data_prefixes is removed."""
        config = DEFAULT_ERROR_CONFIG
        assert not hasattr(config, "error_as_data_prefixes") or \
               not config.error_as_data_prefixes
```

**File:** `tests/integration/graphql/mutations/test_mutation_types.py`

```python
import pytest
from fraiseql.mutations import MutationError, MutationSuccess

class TestMutationTypesV190:
    """Test mutation type validation in v1.8.0."""

    def test_success_requires_entity(self):
        """Success type requires non-null entity."""
        with pytest.raises(ValueError, match="requires non-null entity"):
            MutationSuccess(entity=None)

    def test_success_with_entity_succeeds(self):
        """Success type with entity succeeds."""
        success = MutationSuccess(
            entity={"id": "123", "name": "Test"},
            message="Created successfully",
        )
        assert success.entity is not None

    def test_error_has_code_field(self):
        """Error type has code field."""
        error = MutationError(
            code=422,
            status="noop:invalid_contract_id",
            message="Contract not found",
        )
        assert error.code == 422
        assert error.status == "noop:invalid_contract_id"

    def test_error_to_dict_includes_code(self):
        """Error to_dict includes code field."""
        error = MutationError(
            code=404,
            status="not_found:machine",
            message="Machine not found",
        )
        result = error.to_dict()
        assert result["code"] == 404
        assert result["status"] == "not_found:machine"
        assert result["message"] == "Machine not found"
```

---

## Verification Checklist

### Code Changes
- [ ] `error_config.py` - Remove `error_as_data_prefixes`
- [ ] `error_config.py` - Move `noop:`, `blocked:` to `error_prefixes`
- [ ] `error_config.py` - Add `get_error_code()` method
- [ ] `error_config.py` - Update `is_error_status()` logic
- [ ] `types.py` - Add `MutationError` with `code` field
- [ ] `types.py` - Add validation to `MutationSuccess.__post_init__()`
- [ ] `rust_executor.py` - Validate error responses have `code` field
- [ ] `mutation_decorator.py` - Add `validate_types()` method
- [ ] `mutation_decorator.py` - Check entity is non-nullable in Success
- [ ] `__init__.py` - Add deprecation warnings

### Testing
- [ ] All error config tests pass
- [ ] New test: `test_noop_is_error_v190`
- [ ] New test: `test_noop_maps_to_422`
- [ ] New test: `test_success_requires_entity`
- [ ] All integration tests pass

### Documentation
- [ ] Docstrings updated with v1.8.0 notes
- [ ] Deprecation warnings added
- [ ] Type hints accurate

---

## Expected Behavior After Phase 2

**Python decorator:**
```python
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Non-nullable (enforced)
    cascade: Cascade | None = None

@fraiseql.failure
class CreateMachineError:
    code: int         # ✅ Required in v1.8.0
    status: str
    message: str
    cascade: Cascade | None = None

@fraiseql.mutation(
    function="create_machine",
    enable_cascade=True,
    error_config=DEFAULT_ERROR_CONFIG,  # ✅ noop: is error
)
class CreateMachine:
    input: CreateMachineInput
    success: CreateMachineSuccess
    failure: CreateMachineError
```

**Error config behavior:**
```python
config = DEFAULT_ERROR_CONFIG

# v1.8.0: noop is error
assert config.is_error_status("noop:invalid_contract_id") is True
assert config.get_error_code("noop:invalid_contract_id") == 422

# Success keywords unchanged
assert config.is_error_status("created") is False
```

---

## Next Steps

Once Phase 2 is complete:
1. Run Python test suite: `uv run pytest`
2. Verify type checking: `uv run mypy src/fraiseql`
3. Commit changes: `git commit -m "feat(mutations)!: validation as Error type (v1.8.0) [PYTHON]"`
4. Proceed to Phase 3: Schema Generation

**Blocking:** Schema generation (Phase 3) depends on these Python changes.
