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
    from graphql import GraphQLField, GraphQLObjectType, GraphQLUnionType

    # Get Success type from mutation definition
    success_cls = mutation_def.success_type

    if isinstance(union_type, GraphQLUnionType):
        # Find and modify the Success type IN PLACE
        for member_type in union_type.types:
            if (
                isinstance(member_type, GraphQLObjectType)
                and member_type.name == success_cls.__name__
            ):
                # Check if cascade field already exists
                if "cascade" in member_type.fields:
                    continue

                # Add cascade field with resolver directly to the existing type
                # This modifies the GraphQL type during schema construction
                # which is safe despite GraphQL types being immutable
                def resolve_cascade(obj: Any, info: Any) -> Any:
                    """Resolve cascade field from __cascade__ attribute."""
                    return getattr(obj, "__cascade__", None)

                cascade_field = GraphQLField(
                    type_=get_cascade_graphql_type(),
                    resolve=resolve_cascade,
                    description="Cascade data with side effects and invalidations",
                )

                # Directly mutate the fields dict (safe during schema construction)
                member_type.fields["cascade"] = cascade_field

        # Return the original union with modified members
        return union_type

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
