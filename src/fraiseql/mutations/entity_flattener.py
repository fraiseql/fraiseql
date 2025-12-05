"""Entity field flattening for mutation_response format."""

import logging
from typing import Any, Type

logger = logging.getLogger(__name__)


def should_flatten_entity(success_type: Type) -> bool:
    """Determine if Success type has explicit fields requiring entity flattening.

    Returns True if:
    - Success type has explicit field annotations
    - Has fields beyond just 'message' (indicates custom fields)

    Returns False if:
    - Success type has no annotations (generic type)
    - Only has 'message' field (minimal type)
    """
    if not hasattr(success_type, "__annotations__"):
        return False

    annotations = success_type.__annotations__

    # No annotations = generic success type
    if not annotations:
        return False

    # Only 'message' field = minimal success type, no flattening needed
    if set(annotations.keys()) == {"message"}:
        return False

    # Has explicit fields = flatten entity wrapper
    return True


def get_success_type_fields(success_type: Type) -> set[str]:
    """Get field names from Success type annotations.

    Returns set of field names that should exist at top level.
    """
    if not hasattr(success_type, "__annotations__"):
        return set()

    return set(success_type.__annotations__.keys())


def flatten_entity_wrapper(
    mutation_result: dict[str, Any],
    success_type: Type,
) -> dict[str, Any]:
    """Flatten entity JSONB fields to match Success type schema.

    Args:
        mutation_result: Raw mutation result from PostgreSQL (as dict)
        success_type: Python Success type class with field annotations

    Returns:
        Flattened mutation result with entity fields at top level

    Examples:
        # Before flattening
        {
            "status": "created",
            "message": "Success",
            "entity": {"post": {...}, "extra": "data"},
            "cascade": {...},
            "entity_type": "Post",
            "entity_id": "123"
        }

        # After flattening (Success type has 'post', 'message', 'cascade' fields)
        {
            "status": "created",
            "message": "Success",
            "post": {...},      # from entity.post
            "cascade": {...},   # kept from top-level
            "entity_type": "Post",
            "entity_id": "123"
        }
    """
    # Check if this is mutation_response format (full format with entity field)
    if "entity" not in mutation_result:
        logger.debug("No entity field found - not full format, skipping flattening")
        return mutation_result

    # Check if entity is a dict (JSONB object)
    entity = mutation_result.get("entity")
    if not isinstance(entity, dict):
        logger.debug(f"Entity is not a dict (type: {type(entity)}), skipping flattening")
        return mutation_result

    # Check if Success type has explicit fields
    if not should_flatten_entity(success_type):
        logger.debug(
            f"Success type {success_type.__name__} has no explicit fields, keeping entity wrapper"
        )
        return mutation_result

    # Get expected field names from Success type
    expected_fields = get_success_type_fields(success_type)

    logger.debug(f"Flattening entity fields for {success_type.__name__}")
    logger.debug(f"Expected fields: {expected_fields}")
    logger.debug(f"Entity keys: {entity.keys()}")

    # Create flattened result (copy original dict)
    flattened = mutation_result.copy()

    # Check if Success type expects an 'entity' field
    # If so, don't flatten - keep entity wrapper as-is
    if "entity" in expected_fields:
        logger.debug("Success type has 'entity' field, keeping entity wrapper (no flattening)")
        return mutation_result

    # Check if entity_type hint matches any expected field
    # entity_type is a hint like 'machine', 'post', etc. that indicates which
    # field should receive entity data
    entity_type_hint = mutation_result.get("entity_type")
    if entity_type_hint and entity_type_hint in expected_fields and entity_type_hint != "entity":
        # Custom entity field name detected (e.g., 'machine', 'post')
        # The Rust transformer doesn't fully support custom field names yet,
        # so we skip flattening and let the Python parser handle it
        logger.debug(
            f"Success type has custom entity field '{entity_type_hint}', "
            f"skipping flattening (Rust transformer limitation)"
        )
        return mutation_result

    # Legacy behavior: extract individual fields from entity dict
    # Priority: top-level fields > entity fields (e.g., cascade from top-level wins)
    for field_name in expected_fields:
        # For each field in Success type, try to find it in top-level first, then entity
        if field_name in mutation_result and field_name != "entity":
            # Use top-level fields (including cascade if present) - highest priority
            flattened[field_name] = mutation_result[field_name]
            logger.debug(f"Flattened field '{field_name}' from top-level")
        elif field_name in entity:
            # Fall back to entity field if not at top-level
            flattened[field_name] = entity[field_name]
            logger.debug(f"Flattened field '{field_name}' from entity")

    # Remove entity wrapper but PRESERVE internal fields for Rust parsing
    # Rust needs 'status', 'entity_id', 'entity_type' to detect full format
    internal_fields = {"status", "entity_id", "entity_type", "updated_fields", "metadata"}

    # Remove entity field explicitly
    flattened.pop("entity", None)
    logger.debug("Removed 'entity' wrapper field")

    # Remove any extra fields not in expected_fields or internal_fields
    fields_to_remove = []
    for key in flattened:
        # Keep expected fields AND internal fields
        if key not in expected_fields and key not in internal_fields:
            fields_to_remove.append(key)

    for key in fields_to_remove:
        flattened.pop(key, None)

    logger.debug(f"Removed extra fields: {fields_to_remove}")

    logger.debug(f"Flattened result keys: {flattened.keys()}")

    return flattened
