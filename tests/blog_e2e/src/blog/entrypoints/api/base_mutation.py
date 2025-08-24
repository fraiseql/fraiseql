"""Base mutation class for Blog Demo Application.

Following PrintOptim Backend mutation patterns for consistent, enterprise-grade
error handling and mutation processing.
"""

import uuid
from typing import Any, Dict, List, Optional, Union, Type
from abc import ABC

import fraiseql
from fraiseql import FraiseQLMutation, FraiseQLError
from fraiseql.cqrs import CQRSRepository

from ...core.exceptions import BlogException
from .config.mutation_error_config import DEFAULT_ERROR_CONFIG


class BlogMutationBase(FraiseQLMutation, ABC):
    """Base mutation class following PrintOptim Backend patterns.

    Provides:
    - Standardized error handling with rich error context
    - Automatic context parameter extraction
    - Database function integration
    - Consistent mutation result processing
    """

    # Default configuration - can be overridden in subclasses
    validation_strict: bool = True
    error_trace: bool = True

    @classmethod
    def get_context_params(cls) -> Dict[str, str]:
        """Get context parameter mappings.

        Following PrintOptim patterns for context extraction.
        """
        return {
            "user_id": "current_user_id",
            "organization_id": "current_organization_id",
            "tenant_id": "tenant_id",
        }

    @classmethod
    async def resolve_mutation(
        cls,
        info: Any,
        input_data: Any
    ) -> Union[Any, Any]:  # Success or Failure type
        """Resolve mutation with comprehensive error handling."""
        try:
            # Extract context parameters
            context = info.context
            repo: CQRSRepository = context.get("db")

            if not repo:
                return cls._create_error_response(
                    "Database repository not available",
                    "REPOSITORY_ERROR"
                )

            # Get function name from class metadata
            function_name = getattr(cls, "function", None)
            if not function_name:
                return cls._create_error_response(
                    "Function name not configured for mutation",
                    "CONFIGURATION_ERROR"
                )

            # Prepare function parameters
            params = cls._prepare_function_params(input_data, context)

            # Execute database function
            result = await repo.call_function(function_name, **params)

            # Process result according to PrintOptim patterns
            return cls._process_mutation_result(result, input_data)

        except BlogException as e:
            return cls._create_error_response(
                e.message,
                e.code,
                details=e.details,
                original_payload=input_data.__dict__ if hasattr(input_data, "__dict__") else None
            )
        except Exception as e:
            # Log unexpected errors
            import logging
            logging.exception(f"Unexpected error in {cls.__name__}")

            return cls._create_error_response(
                "An unexpected error occurred",
                "INTERNAL_ERROR",
                details={"exception_type": type(e).__name__} if cls.error_trace else None
            )

    @classmethod
    def _prepare_function_params(cls, input_data: Any, context: Dict[str, Any]) -> Dict[str, Any]:
        """Prepare parameters for database function call."""
        params = {}

        # Add context parameters
        context_mappings = cls.get_context_params()
        for param_name, context_key in context_mappings.items():
            if context_key in context:
                params[f"input_{param_name}"] = context[context_key]

        # Add input payload
        if hasattr(input_data, "__dict__"):
            params["input_payload"] = cls._serialize_input(input_data)
        else:
            params["input_payload"] = input_data

        return params

    @classmethod
    def _serialize_input(cls, input_data: Any) -> Dict[str, Any]:
        """Serialize input data for database function."""
        if hasattr(input_data, "to_dict"):
            return input_data.to_dict()
        elif hasattr(input_data, "__dict__"):
            result = {}
            for key, value in input_data.__dict__.items():
                if isinstance(value, uuid.UUID):
                    result[key] = str(value)
                elif hasattr(value, "value"):  # Enum
                    result[key] = value.value
                else:
                    result[key] = value
            return result
        else:
            return input_data

    @classmethod
    def _process_mutation_result(cls, result: Dict[str, Any], original_input: Any) -> Union[Any, Any]:
        """Process database function result into appropriate response type."""
        # Following PrintOptim Backend mutation_result pattern
        if not result:
            return cls._create_error_response(
                "No result returned from database function",
                "DATABASE_ERROR"
            )

        status = result.get("status", "unknown")

        # Handle different status types
        if status == "new" or status == "updated":
            return cls._create_success_response(result, original_input)
        elif status.startswith("noop:"):
            return cls._create_noop_response(result, original_input, status)
        else:
            return cls._create_error_response(
                result.get("message", "Operation failed"),
                result.get("error_code", "DATABASE_ERROR"),
                details=result.get("extra_metadata", {}),
                original_payload=original_input.__dict__ if hasattr(original_input, "__dict__") else None
            )

    @classmethod
    def _create_success_response(cls, result: Dict[str, Any], original_input: Any):
        """Create success response from database result."""
        # Get success type from class annotation
        success_type = getattr(cls, "success", None)
        if not success_type:
            raise ValueError(f"Success type not defined for {cls.__name__}")

        # Create success instance
        success_data = {
            "message": result.get("message", "Operation completed successfully"),
            "original_payload": cls._serialize_input(original_input) if original_input else None,
        }

        # Add entity data if available
        if result.get("object_data"):
            # Assume the success type has an attribute matching the entity name
            entity_field = cls._get_entity_field_name()
            if entity_field:
                success_data[entity_field] = result["object_data"]

        return success_type(**success_data)

    @classmethod
    def _create_error_response(
        cls,
        message: str,
        error_code: str,
        details: Optional[Dict[str, Any]] = None,
        original_payload: Optional[Dict[str, Any]] = None
    ):
        """Create error response with comprehensive error information."""
        # Get failure type from class annotation
        failure_type = getattr(cls, "failure", None)
        if not failure_type:
            raise ValueError(f"Failure type not defined for {cls.__name__}")

        # Create FraiseQL errors following clean patterns
        errors = [FraiseQLError(
            message=message,
            code=error_code,
            details=details or {}
        )]

        error_data = {
            "message": message,
            "errors": errors,
            "error_code": error_code,
            "original_payload": original_payload,
        }

        # Add error-specific fields based on error code
        if error_code == "DUPLICATE_ERROR" and details:
            error_data["conflict_entity"] = details.get("existing_entity")
        elif error_code == "NOT_FOUND" and details:
            error_data["missing_entity"] = details.get("missing_entity")
        elif error_code == "VALIDATION_ERROR" and details:
            error_data["field_errors"] = details.get("field_errors", {})

        return failure_type(**error_data)

    @classmethod
    def _create_noop_response(cls, result: Dict[str, Any], original_input: Any, status: str):
        """Create NOOP response for idempotent operations."""
        # Check if class has noop type defined
        noop_type = getattr(cls, "noop", None)
        if not noop_type:
            # Fall back to success response for NOOPs
            return cls._create_success_response(result, original_input)

        noop_data = {
            "message": result.get("message", "No operation performed (idempotent)"),
            "reason": status,
            "original_payload": cls._serialize_input(original_input) if original_input else None,
        }

        return noop_type(**noop_data)

    @classmethod
    def _get_entity_field_name(cls) -> Optional[str]:
        """Get the entity field name for success responses."""
        # Simple heuristic based on class name
        class_name = cls.__name__.replace("Create", "").replace("Update", "").replace("Delete", "")
        return class_name.lower()


# Convenience classes for specific mutation types

class BlogCreateMutation(BlogMutationBase):
    """Base class for create mutations."""
    pass


class BlogUpdateMutation(BlogMutationBase):
    """Base class for update mutations."""
    pass


class BlogDeleteMutation(BlogMutationBase):
    """Base class for delete mutations."""
    pass
