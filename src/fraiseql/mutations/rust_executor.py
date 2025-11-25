"""Rust-first mutation executor.

PostgreSQL -> Rust -> HTTP bytes (zero Python parsing)
"""

import json
import logging
from typing import Any

from fraiseql.core.rust_pipeline import RustResponseBytes

logger = logging.getLogger(__name__)


def _get_fraiseql_rs():
    """Lazy-load Rust extension."""
    try:
        from fraiseql import _fraiseql_rs

        return _fraiseql_rs
    except ImportError as e:
        raise ImportError(
            "fraiseql Rust extension not available. "
            "Reinstall: pip install --force-reinstall fraiseql"
        ) from e


async def execute_mutation_rust(
    conn: Any,
    function_name: str,
    input_data: dict[str, Any],
    field_name: str,
    success_type: str,
    error_type: str,
    entity_field_name: str | None = None,
    entity_type: str | None = None,
    context_args: list[Any] | None = None,
    cascade_selections: str | None = None,
) -> RustResponseBytes:
    """Execute mutation via Rust-first pipeline.

    Supports both simple format (just entity JSONB) and full v2 format.
    Rust auto-detects the format based on presence of 'status' field.

    Args:
        conn: PostgreSQL async connection
        function_name: Full function name (e.g., "app.create_user")
        input_data: Mutation input as dict
        field_name: GraphQL field name (e.g., "createUser")
        success_type: GraphQL success type name
        error_type: GraphQL error type name
        entity_field_name: Field name for entity (e.g., "user")
        entity_type: Entity type for __typename (e.g., "User") - REQUIRED for simple format
        context_args: Optional context arguments
        cascade_selections: Optional cascade selections JSON

    Returns:
        RustResponseBytes ready for HTTP response
    """
    fraiseql_rs = _get_fraiseql_rs()

    # Convert input to JSON
    input_json = json.dumps(input_data, separators=(",", ":"))

    # Build SQL query using psycopg placeholders (%s)
    # Wrap with row_to_json() to handle composite type returns as JSON
    if context_args:
        placeholders = ", ".join(["%s"] * len(context_args))
        query = f"SELECT row_to_json({function_name}({placeholders}, %s::jsonb))"
        params = (*context_args, input_json)
    else:
        query = f"SELECT row_to_json({function_name}(%s::jsonb))"
        params = (input_json,)

    # Execute query
    async with conn.cursor() as cursor:
        await cursor.execute(query, params)
        row = await cursor.fetchone()

    # Handle no result
    if not row or row[0] is None:
        error_json = json.dumps(
            {
                "status": "failed:no_result",
                "message": "No result returned from mutation",
                "entity_id": None,
                "entity_type": None,
                "entity": None,
                "updated_fields": None,
                "cascade": None,
                "metadata": None,
            }
        )
        response_bytes = fraiseql_rs.build_mutation_response(
            error_json,
            field_name,
            success_type,
            error_type,
            entity_field_name,
            entity_type,
            None,  # cascade_selections
        )
        return RustResponseBytes(response_bytes)

    # Get mutation result
    mutation_result = row[0]

    # Debug logging
    logger.debug(f"Mutation result type: {type(mutation_result)}, value: {mutation_result}")

    # Handle different result types from psycopg
    if isinstance(mutation_result, dict):
        # psycopg returned a dict (from JSONB or row_to_json composite)
        # Map legacy field names to v2 format
        if "object_data" in mutation_result:
            # Legacy composite type format - convert to v2
            mutation_result = {
                "entity_id": str(mutation_result.get("id")) if mutation_result.get("id") else None,
                "updated_fields": mutation_result.get("updated_fields"),
                "status": mutation_result.get("status"),
                "message": mutation_result.get("message"),
                "entity": mutation_result.get("object_data"),  # object_data -> entity
                "metadata": mutation_result.get("extra_metadata"),  # extra_metadata -> metadata
                "entity_type": (
                    mutation_result.get("extra_metadata", {}).get("entity")
                    if isinstance(mutation_result.get("extra_metadata"), dict)
                    else None
                ),
                "cascade": None,
            }
        mutation_json = json.dumps(mutation_result, separators=(",", ":"), default=str)
    elif isinstance(mutation_result, tuple):
        # psycopg returned a tuple from composite type
        # Expected: (id, updated_fields, status, message, object_data, extra_metadata)
        # Convert to v2 format JSON
        composite_dict = {
            "entity_id": str(mutation_result[0]) if mutation_result[0] else None,
            "updated_fields": list(mutation_result[1]) if mutation_result[1] else None,
            "status": mutation_result[2],
            "message": mutation_result[3],
            "entity": mutation_result[4],  # object_data -> entity
            "metadata": mutation_result[5],  # extra_metadata -> metadata
            "cascade": None,
        }
        # Extract entity_type from metadata if present
        if composite_dict["metadata"] and isinstance(composite_dict["metadata"], dict):
            composite_dict["entity_type"] = composite_dict["metadata"].get("entity")
        mutation_json = json.dumps(composite_dict, separators=(",", ":"), default=str)
    elif isinstance(mutation_result, str):
        # Already a JSON string
        mutation_json = mutation_result
    else:
        # Unknown type - try to convert to JSON
        mutation_json = json.dumps(mutation_result, separators=(",", ":"), default=str)

    # Transform via Rust (auto-detects simple vs v2 format)
    response_bytes = fraiseql_rs.build_mutation_response(
        mutation_json,
        field_name,
        success_type,
        error_type,
        entity_field_name,
        entity_type,
        cascade_selections,
    )

    return RustResponseBytes(response_bytes, schema_type=success_type)
