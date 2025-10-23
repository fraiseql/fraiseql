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

    # Map specific keyword patterns first (for backward compatibility)
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

    # Check prefixes (for operations that don't match specific keywords)
    if status_lower.startswith("noop:"):
        return 422  # Unprocessable Entity for no-op operations
    if status_lower.startswith("blocked:"):
        return 422  # Unprocessable Entity for blocked operations
    if status_lower.startswith("skipped:"):
        return 422  # Unprocessable Entity for skipped operations
    if status_lower.startswith("ignored:"):
        return 422  # Unprocessable Entity for ignored operations
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

    # Debug logging
    import logging

    logger = logging.getLogger(__name__)
    logger.info(
        f"Parsing mutation result: status='{mutation_result.status}', status_lower='{status_lower}'"
    )
    logger.info(f"success_keywords: {error_config.success_keywords}")
    logger.info(f"In success_keywords: {status_lower in error_config.success_keywords}")

    # Use success type only for explicit success statuses
    if status_lower in error_config.success_keywords:
        logger.info("Parsing as SUCCESS type")
        return _parse_success(mutation_result, success_cls)
    # Everything else uses error type (including noop:, blocked:, etc.)
    logger.info("Parsing as ERROR type")
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
            continue

        # NEW: Check if this is an entity field that should receive the entire object_data
        # Only do this for single-entity results where object_data is the entity itself
        if (
            result.object_data
            and _is_entity_field(field_name, field_type)
            and _is_single_entity_object_data(result.object_data, annotations)
        ):
            # Clean UNSET values before processing
            from fraiseql.fastapi.json_encoder import clean_unset_values

            cleaned_object_data = clean_unset_values(result.object_data)

            # Try to instantiate the entire object_data as this field
            value = _instantiate_type(field_type, cleaned_object_data)
            if value is not None:
                fields[field_name] = value

    # Handle main entity from object_data if not already mapped
    if result.object_data:
        # Clean UNSET values before processing to prevent serialization issues
        from fraiseql.fastapi.json_encoder import clean_unset_values

        cleaned_object_data = clean_unset_values(result.object_data)

        # Check if we need to map object_data to a main field
        # We have object data but no entity fields have been populated yet
        non_standard_fields = [f for f in fields if f not in ("message", "status")]
        if not non_standard_fields:
            # Try to map object_data to the main field
            main_field = _find_main_field(annotations, result.extra_metadata)
            if main_field and main_field not in fields:
                field_type = annotations[main_field]
                value = _instantiate_type(field_type, cleaned_object_data)
                if value is not None:
                    fields[main_field] = value

    return success_cls(**fields)


def _parse_error(
    result: MutationResult,
    error_cls: type[E],
) -> E:
    """Parse error mutation result."""
    # Debug logging
    import logging

    logger = logging.getLogger(__name__)
    class_name = getattr(error_cls, "__name__", str(error_cls))
    logger.info(f"_parse_error called: status={result.status}, class={class_name}")

    fields = {}
    annotations = getattr(error_cls, "__annotations__", {})
    logger.info(f"Error class annotations: {annotations}")

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
        from fraiseql.utils.casing import to_snake_case

        for field_name, field_type in annotations.items():
            if field_name in ("message", "code"):
                continue

            # Check if field exists in metadata with exact name
            if field_name in result.extra_metadata:
                value = _instantiate_type(field_type, result.extra_metadata[field_name])
                if value is not None:
                    fields[field_name] = value
                    continue

            # Try snake_case version of camelCase field name
            snake_field_name = to_snake_case(field_name)
            if snake_field_name != field_name and snake_field_name in result.extra_metadata:
                value = _instantiate_type(field_type, result.extra_metadata[snake_field_name])
                if value is not None:
                    fields[field_name] = value

    # Handle conflict entity instantiation from errors.details.conflict.conflictObject
    # This fixes the bug where DEFAULT_ERROR_CONFIG doesn't populate conflict_* fields
    _populate_conflict_fields(result, annotations, fields)

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

    # Post-process to auto-populate errors field if it exists and is empty or None
    # FORCE POPULATE for frontend compatibility - Enterprise applications need errors array
    errors_value = getattr(instance, "errors", "NOT_SET")
    if hasattr(instance, "errors") and (errors_value is None or errors_value == []):
        # Auto-populate the errors field with structured error information
        import logging

        logger = logging.getLogger(__name__)
        logger.info(
            f"FORCE-populating errors for status: {result.status}, class: {error_cls.__name__}"
        )

        # Create error structure from the status and message - SIMPLIFIED
        status = getattr(instance, "status", result.status or "unknown")
        message = getattr(instance, "message", result.message or "Unknown error")

        # Extract error code and identifier from status
        if ":" in status:
            error_code = 422  # Unprocessable Entity for noop: statuses
            identifier = status.split(":", 1)[1] if ":" in status else "unknown_error"
        else:
            error_code = 500  # Internal Server Error for other statuses
            identifier = "general_error"

        # Create error object - FORCE POPULATE
        error_obj = {
            "code": error_code,
            "identifier": identifier,
            "message": message,
            "details": {},
        }

        # FORCE set the errors array
        instance.errors = [error_obj]
        logger.info(f"FORCE-populated errors: {[error_obj]}")

        # Also check if we can do the normal population
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
                    from fraiseql.config.schema_config import SchemaConfig
                    from fraiseql.fastapi.json_encoder import clean_unset_values
                    from fraiseql.utils.casing import transform_keys_to_camel_case

                    cleaned_metadata = clean_unset_values(result.extra_metadata)

                    # Transform keys to camelCase if configured
                    config = SchemaConfig.get_instance()
                    if config.camel_case_fields:
                        error_data["details"] = transform_keys_to_camel_case(cleaned_metadata)
                    else:
                        error_data["details"] = cleaned_metadata

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
    """Extract field value from object_data or metadata.

    Supports multiple patterns:
    1. Direct field mapping: object_data[field_name] -> field
    2. Metadata mapping: metadata[field_name] -> field
    3. Whole object mapping: object_data -> field (for single entity results)
    """
    # First check metadata (highest priority for explicit mappings)
    if metadata and field_name in metadata:
        return _instantiate_type(field_type, metadata[field_name])

    # Then check if field exists in object_data by exact name
    if object_data and field_name in object_data:
        # Clean UNSET values before instantiation
        from fraiseql.fastapi.json_encoder import clean_unset_values

        field_data = clean_unset_values(object_data[field_name])
        return _instantiate_type(field_type, field_data)

    # For single-field results, object_data might be the field itself
    # This handles the case where object_data = {id: "...", name: "..."}
    # and we want to map it to field "location" of type Location
    if object_data and _is_matching_type(field_type, object_data):
        from fraiseql.fastapi.json_encoder import clean_unset_values

        cleaned_data = clean_unset_values(object_data)
        return _instantiate_type(field_type, cleaned_data)

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
        # Clean UNSET values from dict data before instantiation
        from fraiseql.fastapi.json_encoder import clean_unset_values

        cleaned_data = clean_unset_values(data)

        # Check if it's a FraiseQL type (decorated with @fraise_type, @success, @failure)
        if (
            hasattr(field_type, "__fraiseql_definition__")
            or hasattr(field_type, "__fraiseql_success__")
            or hasattr(field_type, "__fraiseql_failure__")
        ):
            # Use the constructor directly
            try:
                return field_type(**cleaned_data)
            except TypeError:
                # Special handling for Error objects - provide default values for required fields
                if hasattr(field_type, "__name__") and field_type.__name__ == "Error":
                    # Ensure required Error fields have default values
                    error_data = cleaned_data.copy()
                    if "message" not in error_data:
                        error_data["message"] = "Unknown error"
                    if "code" not in error_data:
                        error_data["code"] = 500
                    if "identifier" not in error_data:
                        error_data["identifier"] = "unknown_error"

                    try:
                        return field_type(**error_data)
                    except TypeError:
                        pass  # Still failed, continue to from_dict fallback

                # If direct construction fails, try from_dict if available
                if hasattr(field_type, "from_dict"):
                    return field_type.from_dict(cleaned_data)

        # Fallback to from_dict if available
        if hasattr(field_type, "from_dict"):
            return field_type.from_dict(cleaned_data)

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


def _is_entity_field(field_name: str, field_type: type) -> bool:
    """Check if a field is likely an entity field that should receive object_data.

    Entity fields are typically:
    - Not standard fields (message, status, errors)
    - Complex types (have annotations or are FraiseQL types)
    - Named after entities (location, machine, user, etc.)
    """
    # Skip standard mutation result fields
    if field_name in ("message", "status", "errors", "code"):
        return False

    # Check if it's a complex type (not primitive)
    if field_type in (str, int, float, bool, type(None)):
        return False

    # Check for Optional/Union types
    origin = get_origin(field_type)
    if origin is Union or origin is types.UnionType:
        # Get the non-None type from Optional[T]
        args = get_args(field_type)
        non_none_types = [t for t in args if t is not type(None)]
        if non_none_types:
            field_type = non_none_types[0]
            origin = get_origin(field_type)

    # Lists of entities are entity fields
    if origin is list:
        return True

    # Check if it's a FraiseQL type or has annotations (complex type)
    if (
        hasattr(field_type, "__annotations__")
        or hasattr(field_type, "__fraiseql_definition__")
        or hasattr(field_type, "__fraiseql_success__")
        or hasattr(field_type, "__fraiseql_failure__")
    ):
        return True

    return False


def _is_single_entity_object_data(
    object_data: dict[str, Any], annotations: dict[str, type]
) -> bool:
    """Check if object_data represents a single entity rather than multiple named entities.

    Returns True if object_data looks like a single entity (has id, name, etc.)
    Returns False if object_data has keys matching field names in annotations.
    """
    if not object_data:
        return False

    # Get non-standard fields from annotations
    entity_fields = [
        field
        for field in annotations
        if field not in ("message", "status", "errors", "code")
        and _is_entity_field(field, annotations[field])
    ]

    # If object_data has keys matching entity field names, it's a multi-entity result
    for field_name in entity_fields:
        if field_name in object_data:
            return False

    # Check if object_data looks like an entity (has common entity fields)
    entity_indicators = {"id", "uuid", "name", "identifier", "created_at", "updated_at"}
    if any(key in object_data for key in entity_indicators):
        return True

    # If we have exactly one entity field and object_data is a dict, assume it's for that field
    if len(entity_fields) == 1 and isinstance(object_data, dict):
        return True

    return False


def _extract_conflict_from_camel_case_format(
    extra_metadata: dict[str, Any],
) -> dict[str, Any] | None:
    """Extract conflict object from camelCase format: errors.details.conflict.conflictObject."""
    if "errors" not in extra_metadata:
        return None

    errors_list = extra_metadata.get("errors", [])
    if not isinstance(errors_list, list) or len(errors_list) == 0:
        return None

    first_error = errors_list[0]
    if not isinstance(first_error, dict):
        return None

    details = first_error.get("details", {})
    if not isinstance(details, dict) or "conflict" not in details:
        return None

    conflict_data = details["conflict"]
    if not isinstance(conflict_data, dict) or "conflictObject" not in conflict_data:
        return None

    conflict_object = conflict_data["conflictObject"]
    if isinstance(conflict_object, dict):
        logger.debug(
            "Found conflict object in camelCase format: errors.details.conflict.conflictObject"
        )
        return conflict_object

    return None


def _extract_conflict_from_snake_case_format(
    extra_metadata: dict[str, Any],
) -> dict[str, Any] | None:
    """Extract conflict object from snake_case format: conflict.conflict_object."""
    if "conflict" not in extra_metadata:
        return None

    conflict_data = extra_metadata["conflict"]
    if not isinstance(conflict_data, dict) or "conflict_object" not in conflict_data:
        return None

    conflict_object = conflict_data["conflict_object"]
    if isinstance(conflict_object, dict):
        logger.debug("Found conflict object in snake_case format: conflict.conflict_object")
        return conflict_object

    return None


def _populate_conflict_fields(
    result: MutationResult,
    annotations: dict[str, type],
    fields: dict[str, Any],
) -> None:
    """Populate conflict_* fields from conflict object data in multiple formats.

    This function fixes the bug where DEFAULT_ERROR_CONFIG doesn't automatically
    instantiate conflict entities from the nested error structure returned by
    PostgreSQL functions.

    Supports both formats for backward compatibility:
    1. errors.details.conflict.conflictObject (camelCase - API format)
    2. conflict.conflict_object (snake_case - internal format)

    Args:
        result: The parsed mutation result containing extra_metadata
        annotations: Field annotations from the error class
        fields: Dictionary to populate with conflict field values
    """
    if not (result.extra_metadata and isinstance(result.extra_metadata, dict)):
        return

    # Try to extract conflict object from either format
    conflict_object = _extract_conflict_from_camel_case_format(
        result.extra_metadata
    ) or _extract_conflict_from_snake_case_format(result.extra_metadata)

    # If we found a conflict object in either format, process it
    if conflict_object is not None:
        # Map conflict object to all conflict_* fields that haven't been populated yet
        for field_name, field_type in annotations.items():
            if field_name.startswith("conflict_") and field_name not in fields:
                try:
                    # Try to instantiate the conflict entity using the type system
                    value = _instantiate_type(field_type, conflict_object)
                    if value is not None:
                        fields[field_name] = value
                        logger.debug(
                            "Successfully populated conflict field %s with %s",
                            field_name,
                            type(value).__name__,
                        )
                except Exception as e:
                    # If instantiation fails, don't break the entire parsing process
                    # This maintains backward compatibility with existing error handling
                    logger.debug("Failed to instantiate conflict field %s: %s", field_name, e)
                    continue
