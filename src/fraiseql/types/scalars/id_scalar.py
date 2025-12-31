"""GraphQL ID scalar backed by UUID.

IMPORTANT: FraiseQL uses GraphQL's built-in ID type instead of defining a custom one.
The built-in GraphQLID already handles UUID serialization properly, so we don't need
to redefine it (and GraphQL prevents redefinition of reserved types like 'ID').

For backward compatibility, IDScalar is aliased to GraphQLID.
"""

from __future__ import annotations

from graphql import GraphQLID

from fraiseql.types.definitions import ScalarMarker

# Use GraphQL's built-in ID type (handles UUID serialization)
# This prevents "Redefinition of reserved type 'ID'" errors
IDScalar = GraphQLID


# Python Type Marker
class IDField(str, ScalarMarker):
    """FraiseQL ID marker used for Python-side typing and introspection.

    Represents opaque identifiers, backed by UUID in PostgreSQL.
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """Return a user-friendly type name for introspection and debugging."""
        return "ID"
