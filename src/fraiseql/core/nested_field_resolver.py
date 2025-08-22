"""Smart field resolver for nested objects with sql_source.

This module provides a field resolver that handles nested objects based on their
resolve_nested setting:

- resolve_nested=False (default): Assumes data is embedded in parent's JSONB
- resolve_nested=True: Makes separate queries to the nested type's sql_source

The default behavior prioritizes performance by avoiding N+1 queries and works
well with PostgreSQL views that pre-join related data into JSONB columns.
"""

import logging
from typing import Any

from graphql import GraphQLResolveInfo

logger = logging.getLogger(__name__)


def create_smart_nested_field_resolver(field_name: str, field_type: Any):
    """Create a field resolver that handles nested objects based on resolve_nested setting.

    This resolver is only used when resolve_nested=True is set on the field type.
    It provides intelligent resolution by:

    1. First checking if the field data is already present in the parent object (embedded)
    2. If data is present, returns it directly without any database query
    3. If data is missing, attempts to query the nested type's sql_source
    4. Falls back to None if neither approach works

    The resolver tries to be smart about parameters:
    - Looks for foreign key fields (e.g., field_name + "_id")
    - Passes tenant_id from context if available
    - Handles errors gracefully if required parameters are missing

    Args:
        field_name: The name of the field being resolved (e.g., "department")
        field_type: The type of the field (must have resolve_nested=True and sql_source)

    Returns:
        An async resolver function for GraphQL that handles the nested object resolution

    Note:
        This function is only called when resolve_nested=True. For the default behavior
        (resolve_nested=False), FraiseQL uses the standard field resolver that assumes
        data is embedded in the parent's JSONB column.
    """

    async def resolve_nested_field(parent: Any, info: GraphQLResolveInfo, **kwargs) -> Any:
        """Resolve a nested field, preferring embedded data over separate queries."""
        # First, check if the data is already present in the parent object
        value = getattr(parent, field_name, None)

        if value is not None:
            # Data is embedded - return it directly
            logger.debug(
                f"Field '{field_name}' has embedded data, "
                f"returning directly without querying sql_source"
            )

            # If it's a dict and the field type is a FraiseQL type, convert it
            if isinstance(value, dict):
                # Extract actual type from Optional if needed
                actual_field_type = field_type
                import types
                from typing import Union, get_args, get_origin

                origin = get_origin(field_type)
                if origin is Union or origin is types.UnionType:
                    args = get_args(field_type)
                    non_none_types = [t for t in args if t is not type(None)]
                    if non_none_types:
                        actual_field_type = non_none_types[0]

                # Check if the field type is a FraiseQL type
                if hasattr(actual_field_type, "__fraiseql_definition__"):
                    if hasattr(actual_field_type, "from_dict"):
                        return actual_field_type.from_dict(value)
                    # Try direct instantiation
                    try:
                        return actual_field_type(**value)
                    except Exception as e:
                        logger.debug(f"Could not convert dict to {actual_field_type.__name__}: {e}")

            return value

        # Data is not embedded - check if we should query sql_source
        actual_field_type = field_type
        import types
        from typing import Union, get_args, get_origin

        origin = get_origin(field_type)
        if origin is Union or origin is types.UnionType:
            args = get_args(field_type)
            non_none_types = [t for t in args if t is not type(None)]
            if non_none_types:
                actual_field_type = non_none_types[0]

        # Check if the field type has sql_source and we have the necessary context
        if (
            hasattr(actual_field_type, "__gql_table__")
            and actual_field_type.__gql_table__
            and "db" in info.context
        ):
            # Check if we have the required parameters for querying
            # This is where the tenant_id issue occurs - we need to handle it gracefully

            # Try to get foreign key from parent for relationship
            fk_field = f"{field_name}_id"  # Common pattern: organization -> organization_id
            fk_value = getattr(parent, fk_field, None)

            if fk_value:
                try:
                    # Attempt to query the related entity
                    db = info.context["db"]
                    table = actual_field_type.__gql_table__

                    # Build query parameters based on what's available in context
                    query_params = {"id": fk_value}

                    # Add tenant_id if available and the table requires it
                    if "tenant_id" in info.context:
                        query_params["tenant_id"] = info.context["tenant_id"]

                    logger.debug(
                        f"Attempting to query {table} for field '{field_name}' "
                        f"with params: {query_params}"
                    )

                    # Use find_one if available
                    if hasattr(db, "find_one"):
                        result = await db.find_one(table, **query_params)
                        if result:
                            if hasattr(actual_field_type, "from_dict"):
                                return actual_field_type.from_dict(result)
                            return actual_field_type(**result)

                except Exception as e:
                    logger.warning(
                        f"Failed to query {table} for field '{field_name}': {e}. "
                        f"This may be expected if the data should be embedded."
                    )

        # No data found - return None
        return None

    return resolve_nested_field


def should_use_nested_resolver(field_type: Any) -> bool:
    """Check if a field type should use a nested resolver.

    This now checks the resolve_nested flag. By default (False),
    we assume data is embedded and don't create a separate resolver.

    Args:
        field_type: The type to check

    Returns:
        True only if the type explicitly requests nested resolution
    """
    import types
    from typing import Union, get_args, get_origin

    # Extract actual type from Optional if needed
    actual_type = field_type
    origin = get_origin(field_type)
    if origin is Union or origin is types.UnionType:
        args = get_args(field_type)
        non_none_types = [t for t in args if t is not type(None)]
        if non_none_types:
            actual_type = non_none_types[0]

    # Check if it's a type with sql_source AND resolve_nested=True
    if hasattr(actual_type, "__fraiseql_definition__"):
        definition = actual_type.__fraiseql_definition__
        # Only use nested resolver if explicitly requested
        return getattr(definition, "resolve_nested", False)

    return False
