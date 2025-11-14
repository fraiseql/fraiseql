"""Helper functions for adding cascade field to GraphQL types."""

from typing import Any

from graphql import GraphQLOutputType

from fraiseql.core.graphql_type import convert_type_to_graphql_output
from fraiseql.mutations.types import Cascade


def add_cascade_to_union_type(
    union_type: GraphQLOutputType,
    mutation_def: Any,  # MutationDefinition - avoid circular import
) -> GraphQLOutputType:
    """Add cascade field to Success branch of mutation return union.

    Takes a Union[Success, Error] type and adds a cascade field to the Success type.

    Args:
        union_type: The GraphQL union type (typically GraphQLUnionType)
        mutation_def: The MutationDefinition with success_type and error_type

    Returns:
        Modified union type with cascade field in Success
    """
    # Get Success type from mutation definition
    success_cls = mutation_def.success_type

    # Check if success type already has cascade field
    if hasattr(success_cls, "__annotations__") and "cascade" in success_cls.__annotations__:
        # Already has cascade, no modification needed
        return union_type

    # Create modified Success type with cascade field
    modified_success_type = _add_cascade_field_to_type(success_cls)

    # Rebuild the union with modified Success type
    # Get the union types
    from graphql import GraphQLUnionType

    if isinstance(union_type, GraphQLUnionType):
        # Find and replace the Success type
        new_types = []
        for member_type in union_type.types:
            if member_type.name == success_cls.__name__:
                # Replace with modified version
                new_types.append(convert_type_to_graphql_output(modified_success_type))
            else:
                new_types.append(member_type)

        # Create new union
        return GraphQLUnionType(
            name=union_type.name, types=new_types, resolve_type=union_type.resolve_type
        )

    # If not a union, just return the type (shouldn't happen but safe fallback)
    return union_type


def _add_cascade_field_to_type(success_cls: type) -> type:
    """Create a new type with cascade field added.

    Args:
        success_cls: Original Success type class

    Returns:
        New type class with cascade field
    """
    from typing import Optional

    # Create new class with cascade field
    annotations = getattr(success_cls, "__annotations__", {}).copy()
    annotations["cascade"] = Optional[Cascade]

    # Create new type - keep same name to avoid duplicate registration
    new_cls = type(
        success_cls.__name__,  # Use same name, not WithCascade suffix
        (success_cls,),
        {
            "__annotations__": annotations,
            "__doc__": success_cls.__doc__,
            "__module__": success_cls.__module__,  # Preserve module
        },
    )

    # Copy fraiseql metadata
    if hasattr(success_cls, "__fraiseql_success__"):
        new_cls.__fraiseql_success__ = success_cls.__fraiseql_success__

    # Copy other metadata that might exist
    for attr in ["__fraiseql_type__", "__fraiseql_definition__"]:
        if hasattr(success_cls, attr):
            setattr(new_cls, attr, getattr(success_cls, attr))

    return new_cls


def get_cascade_graphql_type() -> GraphQLOutputType:
    """Get the GraphQL type for Cascade.

    Returns:
        GraphQL ObjectType for Cascade
    """
    from fraiseql.mutations.types import (
        Cascade,
    )

    # Convert our Python types to GraphQL types
    return convert_type_to_graphql_output(Cascade)
