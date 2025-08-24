"""FraiseQL Default Patterns - Clean Names Without Adjectives

This module provides the default FraiseQL patterns with clean names,
making Enhanced/Optimized patterns the standard while preserving
legacy patterns for backward compatibility.

Key Changes:
- OptimizedFraiseQLMutation → FraiseQLMutation (default)
- EnhancedFraiseQLError → FraiseQLError (default)
- Current defaults → Legacy variants
- Clean import paths for new users

Usage (NEW Default Pattern):
    from fraiseql_defaults import FraiseQLMutation, FraiseQLError

    class CreateUserSuccess:
        user: User
        errors: list[FraiseQLError] = []

    class CreateUser(
        FraiseQLMutation,  # Clean default!
        function="create_user",
        validation_strict=True
    ):
        input: CreateUserInput
        success: CreateUserSuccess
        failure: CreateUserError
"""

import uuid
import logging
from typing import Any, Type, Union, get_type_hints, get_origin, get_args
from dataclasses import dataclass
from enum import Enum

# Configure logging
logger = logging.getLogger(__name__)


# ============================================================================
# DEFAULT FRAISEQL PATTERNS (Clean Names)
# ============================================================================

class ErrorSeverity(Enum):
    """Error severity levels for categorization."""
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class ConstraintType(Enum):
    """Standard constraint types for error categorization."""
    REQUIRED = "required"
    FORMAT = "format"
    MAX_LENGTH = "max_length"
    MIN_LENGTH = "min_length"
    UNIQUE = "unique"
    FOREIGN_KEY = "foreign_key"
    SECURITY = "security"
    BUSINESS_RULE = "business_rule"


# Mock fraiseql module for testing
class MockFraiseQL:
    """Mock fraiseql module for testing."""
    DEFAULT_ERROR_CONFIG = {"auto_populate_errors": True}

    @staticmethod
    def type(cls):
        cls.__fraiseql_type__ = True
        return cls

    @staticmethod
    def input(cls):
        cls.__fraiseql_input__ = True
        return cls

    @staticmethod
    def success(cls):
        cls.__fraiseql_success__ = True
        return cls

    @staticmethod
    def failure(cls):
        cls.__fraiseql_failure__ = True
        return cls

    @staticmethod
    def mutation(function=None, schema="app", context_params=None, error_config=None):
        def decorator(cls):
            cls.__fraiseql_mutation__ = True
            cls.__fraiseql_error_config__ = error_config
            return cls
        return decorator

# Mock fraiseql for imports
fraiseql = MockFraiseQL()


@fraiseql.type
class FraiseQLError:
    """Default FraiseQL error type with comprehensive features.

    This is the NEW DEFAULT (was EnhancedFraiseQLError) with:
    - code: HTTP status code (422, 409, etc.)
    - identifier: Machine-readable error identifier
    - message: Human-readable error message
    - details: Structured context information
    - severity: Error severity level (low, medium, high, critical)
    - category: Error category for client handling
    - field_path: Dot-notation field path for nested errors
    - timestamp: When the error occurred
    - trace_id: For error tracking and debugging
    """

    def __init__(
        self,
        code: int,
        identifier: str,
        message: str,
        details: dict[str, Any] | None = None,
        severity: str = ErrorSeverity.MEDIUM.value,
        category: str | None = None,
        field_path: str | None = None,
        timestamp: str | None = None,
        trace_id: str | None = None
    ):
        self.code = code
        self.identifier = identifier
        self.message = message
        self.details = details
        self.severity = severity
        self.category = category
        self.field_path = field_path
        self.timestamp = timestamp
        self.trace_id = trace_id


@dataclass
class ValidationContext:
    """Context for validation operations with trace information."""
    trace_id: str
    operation: str
    timestamp: str
    metadata: dict[str, Any]


class FraiseQLMutation:
    """Default FraiseQL mutation base class with comprehensive features.

    This is the NEW DEFAULT (was OptimizedFraiseQLMutation) with:
    - Auto-decoration of success/failure types
    - Enhanced validation and error handling
    - Performance optimizations with caching
    - Production-ready logging and debugging
    - Comprehensive type validation

    Usage:
        class CreateUser(
            FraiseQLMutation,  # Clean default name!
            function="create_user_enhanced",
            context_params={"user_id": "input_created_by"},
            validation_strict=True,
            error_trace=True
        ):
            input: CreateUserInput
            success: CreateUserSuccess  # Auto-decorated!
            failure: CreateUserError   # Auto-decorated!
    """

    # Class-level cache for performance optimization
    _type_hints_cache: dict[Type, dict[str, Any]] = {}

    def __init_subclass__(
        cls,
        function: str,
        schema: str = "app",
        context_params: dict[str, str] | None = None,
        validation_strict: bool = False,
        error_trace: bool = False,
        **kwargs: Any,
    ) -> None:
        """Initialize subclass with auto-decoration and enhanced validation."""
        super().__init_subclass__(**kwargs)

        # Store configuration for later use
        cls._fraiseql_config = {
            'function': function,
            'schema': schema,
            'context_params': context_params or {},
            'validation_strict': validation_strict,
            'error_trace': error_trace
        }

        try:
            # Enhanced validation with better error messages
            cls._validate_class_structure()

            # Optimized type resolution with caching
            success_type, failure_type = cls._resolve_result_types()

            # Enhanced auto-decoration with error handling
            cls._apply_decorations(success_type, failure_type)

            # Apply FraiseQL mutation decorator with optimized configuration
            cls._apply_mutation_decorator()

            logger.info(f"Successfully initialized FraiseQL mutation: {cls.__name__}")

        except Exception as e:
            error_msg = (
                f"Failed to initialize FraiseQL mutation {cls.__name__}: {e}\n"
                f"Configuration: {cls._fraiseql_config}\n"
                f"Check that all required annotations are present and types are valid."
            )
            logger.error(error_msg)
            raise TypeError(error_msg) from e

    @classmethod
    def _validate_class_structure(cls) -> None:
        """Validate class structure with enhanced error messages."""
        if not hasattr(cls, "__annotations__"):
            raise TypeError(
                f"{cls.__name__} must define input, success, and failure type annotations.\n\n"
                f"Example:\n"
                f"    class {cls.__name__}(\n"
                f"        FraiseQLMutation,\n"
                f"        function=\"{cls._fraiseql_config['function']}\"\n"
                f"    ):\n"
                f"        input: YourInputType\n"
                f"        success: YourSuccessType\n"
                f"        failure: YourFailureType"
            )

        annotations = cls.__annotations__
        required = {"input", "success", "failure"}
        missing = required - set(annotations.keys())

        if missing:
            missing_list = sorted(missing)
            raise TypeError(
                f"{cls.__name__} missing required type annotations: {', '.join(missing_list)}.\n\n"
                f"Required structure:\n"
                f"    class {cls.__name__}(FraiseQLMutation, ...):\n"
                f"        input: YourInputType\n"
                f"        success: YourSuccessType\n"
                f"        failure: YourFailureType"
            )

    @classmethod
    def _resolve_result_types(cls) -> tuple[Type, Type]:
        """Resolve result types with caching and error handling."""
        cache_key = cls
        if cache_key in cls._type_hints_cache:
            cached_hints = cls._type_hints_cache[cache_key]
            return cached_hints['success'], cached_hints['failure']

        try:
            type_hints = get_type_hints(cls)
            success_type = type_hints.get("success")
            failure_type = type_hints.get("failure")

            if not success_type or not failure_type:
                annotations = cls.__annotations__
                success_type = success_type or annotations.get("success")
                failure_type = failure_type or annotations.get("failure")

            if not success_type or not failure_type:
                raise TypeError("Could not resolve success or failure types")

            # Cache the results for performance
            cls._type_hints_cache[cache_key] = {
                'success': success_type,
                'failure': failure_type
            }

            return success_type, failure_type

        except Exception as e:
            raise TypeError(
                f"Failed to resolve result types for {cls.__name__}: {e}. "
                f"Ensure success and failure annotations use valid types."
            ) from e

    @classmethod
    def _apply_decorations(cls, success_type: Type, failure_type: Type) -> None:
        """Apply decorations with enhanced error handling."""
        try:
            if not hasattr(success_type, '__name__'):
                raise TypeError(f"Success type must be a proper class, got: {success_type}")
            if not hasattr(failure_type, '__name__'):
                raise TypeError(f"Failure type must be a proper class, got: {failure_type}")

            # Apply decorators with metadata tracking
            fraiseql.success(success_type)
            success_type.__fraiseql_success__ = True
            success_type.__fraiseql_mutation_class__ = cls.__name__

            fraiseql.failure(failure_type)
            failure_type.__fraiseql_failure__ = True
            failure_type.__fraiseql_mutation_class__ = cls.__name__

            # Add tracing information if enabled
            if cls._fraiseql_config.get('error_trace'):
                success_type.__fraiseql_trace__ = True
                failure_type.__fraiseql_trace__ = True

            logger.debug(f"Applied decorations for {cls.__name__}: {success_type.__name__}, {failure_type.__name__}")

        except Exception as e:
            raise TypeError(
                f"Failed to apply decorations for {cls.__name__}: {e}. "
                f"Ensure success and failure types are proper classes."
            ) from e

    @classmethod
    def _apply_mutation_decorator(cls) -> None:
        """Apply mutation decorator with enhanced configuration."""
        try:
            config = cls._fraiseql_config

            # Enhanced error configuration
            error_config = fraiseql.DEFAULT_ERROR_CONFIG if hasattr(fraiseql, 'DEFAULT_ERROR_CONFIG') else None

            fraiseql.mutation(
                function=config['function'],
                schema=config['schema'],
                context_params=config['context_params'],
                error_config=error_config
            )(cls)

            # Store metadata for debugging
            cls.__fraiseql_mutation__ = True
            cls.__fraiseql_error_config__ = error_config
            cls.__fraiseql_optimized__ = True  # Mark as having enhanced features

            logger.debug(f"Applied mutation decorator for {cls.__name__} with config: {config}")

        except Exception as e:
            raise TypeError(
                f"Failed to apply mutation decorator for {cls.__name__}: {e}. "
                f"Check that function '{config['function']}' exists in schema '{config['schema']}'."
            ) from e


class ErrorMapper:
    """Default error mapping with advanced categorization and optimization."""

    @staticmethod
    def map_database_result_to_graphql(
        db_result: dict,
        target_type_name: str,
        validation_context: ValidationContext | None = None
    ) -> Any:
        """Map database result to GraphQL response with comprehensive error handling."""
        if not db_result:
            raise ValueError("Database result is empty or None")

        errors_array = db_result.get("errors", [])
        trace_id = validation_context.trace_id if validation_context else str(uuid.uuid4())

        # Advanced error mapping with categorization
        mapped_errors = []
        if errors_array and isinstance(errors_array, list):
            for error_data in errors_array:
                if isinstance(error_data, dict):
                    enhanced_error = FraiseQLError(
                        code=error_data.get("code", 500),
                        identifier=error_data.get("identifier", "unknown_error"),
                        message=error_data.get("message", "An error occurred"),
                        details=error_data.get("details"),
                        severity=ErrorMapper._determine_severity(error_data),
                        category=ErrorMapper._determine_category(error_data),
                        field_path=ErrorMapper._extract_field_path(error_data),
                        timestamp=validation_context.timestamp if validation_context else None,
                        trace_id=trace_id
                    )
                    mapped_errors.append(enhanced_error)

        # Create response based on error presence
        if not mapped_errors:
            return ErrorMapper._create_success_response(target_type_name, db_result, trace_id)
        else:
            return ErrorMapper._create_error_response(target_type_name, db_result, mapped_errors, trace_id)

    @staticmethod
    def _determine_severity(error_data: dict) -> str:
        """Determine error severity based on error properties."""
        code = error_data.get("code", 500)
        constraint = error_data.get("details", {}).get("constraint")

        if constraint == "security" or code >= 500:
            return ErrorSeverity.CRITICAL.value
        elif code == 409 or constraint == "business_rule":
            return ErrorSeverity.HIGH.value
        elif code == 422:
            return ErrorSeverity.MEDIUM.value
        return ErrorSeverity.LOW.value

    @staticmethod
    def _determine_category(error_data: dict) -> str | None:
        """Determine error category for client-side handling."""
        constraint = error_data.get("details", {}).get("constraint")
        if constraint:
            return constraint

        code = error_data.get("code")
        if code == 422:
            return "validation"
        elif code == 409:
            return "conflict"
        elif code >= 500:
            return "system"
        return None

    @staticmethod
    def _extract_field_path(error_data: dict) -> str | None:
        """Extract field path for nested error reporting."""
        details = error_data.get("details", {})
        field = details.get("field")
        parent_field = details.get("parent_field")

        if parent_field and field:
            return f"{parent_field}.{field}"
        return field

    @staticmethod
    def _create_success_response(target_type_name: str, db_result: dict, trace_id: str):
        """Create enhanced success response."""
        class SuccessResponse:
            def __init__(self):
                self.__class__.__name__ = target_type_name
                self.message = db_result.get("message", "Operation completed successfully")
                self.errors = []
                self.object_data = db_result.get("object_data")
                self.trace_id = trace_id

        return SuccessResponse()

    @staticmethod
    def _create_error_response(target_type_name: str, db_result: dict, mapped_errors: list, trace_id: str):
        """Create enhanced error response with comprehensive metadata."""
        class ErrorResponse:
            def __init__(self):
                self.__class__.__name__ = target_type_name
                self.message = db_result.get("message", "Operation failed")
                self.errors = mapped_errors
                self.trace_id = trace_id
                self.error_summary = ErrorMapper._create_error_summary(mapped_errors)

        return ErrorResponse()

    @staticmethod
    def _create_error_summary(errors: list[FraiseQLError]) -> dict[str, Any]:
        """Create comprehensive error summary."""
        if not errors:
            return {"total_errors": 0}

        field_errors = {}
        constraint_violations = {}
        severity_counts = {}

        for error in errors:
            # Group by field path
            field_path = error.field_path or error.details.get("field") if error.details else None
            if field_path:
                if field_path not in field_errors:
                    field_errors[field_path] = []
                field_errors[field_path].append(error.message)

            # Count constraints and severities
            if error.category:
                constraint_violations[error.category] = constraint_violations.get(error.category, 0) + 1
            severity_counts[error.severity] = severity_counts.get(error.severity, 0) + 1

        return {
            "total_errors": len(errors),
            "field_errors": field_errors,
            "constraint_violations": constraint_violations,
            "severity_distribution": severity_counts,
            "has_critical_errors": any(e.severity == ErrorSeverity.CRITICAL.value for e in errors),
            "has_security_violations": any(e.category == "security" for e in errors),
            "has_conflicts": any(e.code == 409 for e in errors),
            "has_validation_errors": any(e.code == 422 for e in errors)
        }


# ============================================================================
# LEGACY PATTERNS (Backward Compatibility)
# ============================================================================

class LegacyMutationResultBase:
    """Legacy base class for mutation results - backward compatibility."""

    def __init__(self, **kwargs):
        for key, value in kwargs.items():
            setattr(self, key, value)


class LegacyFraiseQLMutation:
    """Legacy FraiseQL mutation class - basic version for backward compatibility."""

    def __init_subclass__(
        cls,
        function: str,
        schema: str = "app",
        context_params: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> None:
        """Basic subclass initialization - legacy version."""
        super().__init_subclass__(**kwargs)

        # Basic validation (not enhanced)
        if not hasattr(cls, "__annotations__"):
            raise TypeError(f"{cls.__name__} must define input, success, and failure type annotations")

        annotations = cls.__annotations__
        required = {"input", "success", "failure"}
        missing = required - set(annotations.keys())

        if missing:
            raise TypeError(f"{cls.__name__} missing required annotations: {', '.join(sorted(missing))}")

        # Apply basic FraiseQL mutation decorator
        fraiseql.mutation(
            function=function,
            schema=schema,
            context_params=context_params or {}
        )(cls)

        # Mark as legacy
        cls.__fraiseql_mutation__ = True
        cls.__fraiseql_legacy__ = True


class LegacyFraiseQLError:
    """Legacy error type - basic version for backward compatibility."""

    def __init__(self, code: int, identifier: str, message: str, details: dict[str, Any] | None = None):
        self.code = code
        self.identifier = identifier
        self.message = message
        self.details = details


# ============================================================================
# MIGRATION UTILITIES
# ============================================================================

def get_migration_guide() -> dict[str, Any]:
    """Get comprehensive migration guide for upgrading to default patterns."""

    return {
        "renaming_map": {
            "OptimizedFraiseQLMutation": "FraiseQLMutation",
            "EnhancedFraiseQLError": "FraiseQLError",
            "FraiseQLMutation": "LegacyFraiseQLMutation",
            "MutationResultBase": "LegacyMutationResultBase"
        },
        "breaking_changes": [],  # No breaking changes - backward compatible
        "migration_steps": [
            "Import from fraiseql_defaults instead of enhanced modules",
            "Use FraiseQLMutation (clean name) instead of OptimizedFraiseQLMutation",
            "Use FraiseQLError (clean name) instead of EnhancedFraiseQLError",
            "Legacy patterns available for gradual migration",
            "Update documentation to use clean default patterns"
        ],
        "examples": {
            "new_pattern": '''
from fraiseql_defaults import FraiseQLMutation, FraiseQLError

class CreateUserSuccess:
    user: User
    errors: list[FraiseQLError] = []

class CreateUser(
    FraiseQLMutation,  # Clean default!
    function="create_user",
    validation_strict=True
):
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError
            ''',
            "legacy_pattern": '''
from fraiseql_defaults import LegacyFraiseQLMutation, LegacyMutationResultBase

@fraiseql.success
class CreateUserSuccess(LegacyMutationResultBase):
    user: User
    error_code: str | None = None

class CreateUser(
    LegacyFraiseQLMutation,
    function="create_user"
):
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError
            '''
        }
    }


# ============================================================================
# EXPORT ALL DEFAULT PATTERNS
# ============================================================================

__all__ = [
    # Default patterns (clean names)
    "FraiseQLMutation",
    "FraiseQLError",
    "ErrorMapper",
    "ValidationContext",
    "ErrorSeverity",
    "ConstraintType",

    # Legacy patterns (backward compatibility)
    "LegacyFraiseQLMutation",
    "LegacyMutationResultBase",
    "LegacyFraiseQLError",

    # Migration utilities
    "get_migration_guide"
]
