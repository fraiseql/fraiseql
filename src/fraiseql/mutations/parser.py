"""Parser for mutation results from PostgreSQL functions."""

import logging
import types
from typing import Any, TypeVar, Union, get_args, get_origin

from fraiseql.mutations.error_config import MutationErrorConfig
from fraiseql.mutations.types import MutationResult

logger = logging.getLogger(__name__)

# Type variables for Success and Error types
S = TypeVar("S")  # Success type
E = TypeVar("E")  # Error type


def _status_to_error_code(status: str) -> int:
    """Convert a status string to an appropriate HTTP error code.

    This is a basic implementation that can be overridden by projects.
    """
    if not status:
        return 500

    status_lower = status.lower()

    # Map common statuses to HTTP codes
    if "not_found" in status_lower:
        return 404
    if "unauthorized" in status_lower:
        return 401
    if "forbidden" in status_lower:
        return 403
    if "conflict" in status_lower or "duplicate" in status_lower or "exists" in status_lower:
        return 409
    if "validation" in status_lower or "invalid" in status_lower:
        return 422
    if "timeout" in status_lower:
        return 408
    if status_lower.startswith("noop:"):
        return 422  # Unprocessable Entity for no-op operations
    if status_lower.startswith("blocked:"):
        return 422  # Unprocessable Entity for blocked operations
    if status_lower.startswith("failed:"):
        return 500  # Internal error for failures
    return 500  # Default to internal server error


def _status_to_identifier(status: str) -> str:
    """Convert a status string to an error identifier.

    Extracts the meaningful part of the status for use as an identifier.
    """
    if not status:
        return "unknown_error"

    # Handle prefixed statuses (e.g., "noop:already_exists" -> "already_exists")
    if ":" in status:
        parts = status.split(":", 1)
        if len(parts) > 1 and parts[1]:
            return parts[1]

    # Use the full status as identifier, replacing spaces with underscores
    return status.lower().replace(" ", "_").replace("-", "_")


def parse_mutation_result(
    result: dict[str, Any],
    success_cls: type[S],
    error_cls: type[E],
    error_config: MutationErrorConfig | None = None,
) -> S | E:
    """Parse mutation result from PostgreSQL into typed Success or Error.

    Args:
        result: Raw result from PostgreSQL function
        success_cls: Success type class
        error_cls: Error type class
        error_config: Optional error detection configuration

    Returns:
        Instance of either success_cls or error_cls
    """
    # Convert to MutationResult for easier access
    mutation_result = MutationResult.from_db_row(result)

    # For parsing, we need to determine which type to use based on the data structure
    # and status. This is separate from whether it's a GraphQL error.

    # If no config provided, use the original behavior for backward compatibility
    if error_config is None:
        is_error = _is_error_status(mutation_result.status)
        if is_error:
            return _parse_error(mutation_result, error_cls)
        return _parse_success(mutation_result, success_cls)

    # With config, use more sophisticated logic
    status_lower = mutation_result.status.lower() if mutation_result.status else ""

    # Use success type only for explicit success statuses
    if status_lower in error_config.success_keywords:
        return _parse_success(mutation_result, success_cls)
    # Everything else uses error type (including noop:, blocked:, etc.)
    return _parse_error(mutation_result, error_cls)


def _is_error_status(status: str) -> bool:
    """Check if status indicates an error."""
    if not status:
        return False

    status_lower = status.lower()

    # Success statuses
    success_statuses = {"success", "completed", "ok", "done"}
    if status_lower in success_statuses:
        return False

    # Error indicators
    error_keywords = {
        "error",
        "failed",
        "fail",
        "not_found",
        "forbidden",
        "unauthorized",
        "conflict",
        "validation_error",
        "invalid",
        "email_exists",
        "exists",
        "duplicate",
        "timeout",
    }

    # Check if status contains any error keywords
    return any(keyword in status_lower for keyword in error_keywords)


def _parse_success(
    result: MutationResult,
    success_cls: type[S],
) -> S:
    """Parse successful mutation result."""
    # Get fields from success class
    fields = {}
    annotations = getattr(success_cls, "__annotations__", {})

    # Always include message if present
    if "message" in annotations:
        fields["message"] = result.message

    # Include status if present
    if "status" in annotations:
        fields["status"] = result.status

    # Process each field in the success type
    for field_name, field_type in annotations.items():
        if field_name in ("message", "status"):
            continue

        # Try to get value from different sources
        value = _extract_field_value(
            field_name,
            field_type,
            result.object_data,
            result.extra_metadata,
        )

        if value is not None:
            fields[field_name] = value

    # Handle main entity from object_data if not already mapped
    if result.object_data:
        # Check if we need to map object_data to a main field
        # We have object data but no entity fields have been populated yet
        non_standard_fields = [f for f in fields if f not in ("message", "status")]
        if not non_standard_fields:
            # Try to map object_data to the main field
            main_field = _find_main_field(annotations, result.extra_metadata)
            if main_field and main_field not in fields:
                field_type = annotations[main_field]
                value = _instantiate_type(field_type, result.object_data)
                if value is not None:
                    fields[main_field] = value

    return success_cls(**fields)


def _parse_error(
    result: MutationResult,
    error_cls: type[E],
) -> E:
    """Parse error mutation result."""
    fields = {}
    annotations = getattr(error_cls, "__annotations__", {})

    # Always include message
    if "message" in annotations:
        fields["message"] = result.message

    # Include status as code if field exists
    if "code" in annotations:
        fields["code"] = result.status

    # Also include raw status if field exists
    if "status" in annotations:
        fields["status"] = result.status

    # Process other fields from metadata
    if result.extra_metadata:
        for field_name, field_type in annotations.items():
            if field_name in ("message", "code"):
                continue

            # Check if field exists in metadata
            if field_name in result.extra_metadata:
                value = _instantiate_type(field_type, result.extra_metadata[field_name])
                if value is not None:
                    fields[field_name] = value

    # Try to populate remaining fields from object_data
    if result.object_data:
        for field_name, field_type in annotations.items():
            if field_name in fields:  # Skip already populated fields
                continue
            if field_name in ("message", "code", "status", "errors"):  # Skip standard fields
                continue

            # Try to extract from object_data
            value = _extract_field_value(
                field_name,
                field_type,
                result.object_data,
                None,  # Don't re-check metadata
            )
            if value is not None:
                fields[field_name] = value

    # Ensure errors field exists if it's in annotations but not populated
    if "errors" in annotations and "errors" not in fields:
        fields["errors"] = None

    # Create instance first
    instance = error_cls(**fields)

    # Post-process to auto-populate errors field if it exists and is None
    if hasattr(instance, "errors") and getattr(instance, "errors", "NOT_SET") is None:
        # Auto-populate the errors field with structured error information
        error_list_type = annotations.get("errors")
        if error_list_type:
            # Handle Optional[list[Error]] or list[Error] | None
            origin = get_origin(error_list_type)
            args = get_args(error_list_type)

            # If it's a Union type (Optional), extract the non-None type
            if origin is Union or origin is types.UnionType:
                # Find the list type among the union members
                for arg in args:
                    if get_origin(arg) is list:
                        error_list_type = arg
                        break

            # Now check if we have a list type
            origin = get_origin(error_list_type)
            if origin is list:
                # Get the Error type from list[Error]
                error_item_type = get_args(error_list_type)[0]

                # Try to create an error instance
                # This is a basic implementation - projects can customize via error_config
                error_data = {
                    "message": result.message or f"Operation failed: {result.status}",
                    "code": _status_to_error_code(result.status),
                    "identifier": _status_to_identifier(result.status),
                }

                # Add details if available (clean UNSET values to prevent serialization issues)
                if result.extra_metadata:
                    from fraiseql.fastapi.json_encoder import clean_unset_values

                    error_data["details"] = clean_unset_values(result.extra_metadata)

                try:
                    # Instantiate the error type
                    error_instance = _instantiate_type(error_item_type, error_data)
                    if error_instance is not None:
                        instance.errors = [error_instance]
                except Exception as e:
                    # If we can't instantiate, leave as None
                    logger.debug("Failed to auto-populate errors field: %s", e)

    return instance


def _extract_field_value(
    field_name: str,
    field_type: type,
    object_data: dict[str, Any] | None,
    metadata: dict[str, Any] | None,
) -> Any:
    """Extract field value from object_data or metadata."""
    # First check metadata
    if metadata and field_name in metadata:
        return _instantiate_type(field_type, metadata[field_name])

    # Then check object_data
    if object_data and field_name in object_data:
        return _instantiate_type(field_type, object_data[field_name])

    # For single-field results, object_data might be the field itself
    if object_data and _is_matching_type(field_type, object_data):
        return _instantiate_type(field_type, object_data)

    return None


def _instantiate_type(field_type: type, data: Any) -> Any:
    """Instantiate a typed object from data."""
    if data is None:
        return None

    # Handle primitive types
    if field_type in (str, int, float, bool):
        return field_type(data)

    # Handle Optional types (Union with None)
    origin = get_origin(field_type)
    if origin is Union or origin is types.UnionType:
        args = get_args(field_type)
        # For Optional[T], try to instantiate T
        non_none_type = next((t for t in args if t is not type(None)), None)
        if non_none_type:
            return _instantiate_type(non_none_type, data)

    # Handle List types
    if origin is list:
        item_type = get_args(field_type)[0]
        if isinstance(data, list):
            return [_instantiate_type(item_type, item) for item in data]

    # Handle dict types first (before checking for from_dict)
    if origin is dict or field_type is dict:
        return data

    # Handle FraiseQL types - check for both from_dict and __fraiseql_definition__
    if isinstance(data, dict):
        # Check if it's a FraiseQL type (decorated with @fraise_type, @success, @failure)
        if (
            hasattr(field_type, "__fraiseql_definition__")
            or hasattr(field_type, "__fraiseql_success__")
            or hasattr(field_type, "__fraiseql_failure__")
        ):
            # Use the constructor directly
            try:
                return field_type(**data)
            except TypeError:
                # If direct construction fails, try from_dict if available
                if hasattr(field_type, "from_dict"):
                    return field_type.from_dict(data)

        # Fallback to from_dict if available
        if hasattr(field_type, "from_dict"):
            return field_type.from_dict(data)

    # Return as-is for unhandled types
    return data


def _find_main_field(
    annotations: dict[str, type],
    metadata: dict[str, Any] | None,
) -> str | None:
    """Find the main field name for object_data."""
    # Check for entity hint in metadata
    if metadata and "entity" in metadata:
        entity = metadata["entity"]
        # Try exact match
        if entity in annotations:
            return entity
        # Try with common suffixes
        for suffix in ("", "s", "_list", "_data"):
            field = f"{entity}{suffix}"
            if field in annotations:
                return field

    # Find first non-message field
    for field in annotations:
        if field != "message":
            return field

    return None


def _is_matching_type(field_type: type, data: Any) -> bool:
    """Check if data could match the field type."""
    origin = get_origin(field_type)

    # For lists, check if data is a list
    if origin is list:
        return isinstance(data, list)

    # For complex types, check if data is a dict with expected fields
    if hasattr(field_type, "__annotations__") and isinstance(data, dict):
        # Simple heuristic: if data has any of the expected fields
        expected_fields = getattr(field_type, "__annotations__", {})
        return any(field in data for field in expected_fields)

    return False
