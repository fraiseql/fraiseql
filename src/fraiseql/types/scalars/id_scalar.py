"""GraphQL ID scalar type with UUID validation.

This module provides:
- ID: NewType for Python type annotations (Strawberry-style syntax)
- IDScalar: Alias for GraphQL's built-in ID scalar
- IDField: Type compatibility alias

FraiseQL follows GraphQL spec: ID is the standard identifier type.
UUID validation happens at the resolver/input level, not at the scalar level.

Note: We use GraphQL's built-in ID scalar to avoid "Redefinition of reserved type 'ID'"
error from graphql-core. Custom UUID enforcement is done via input validation.
"""

from typing import NewType

from graphql import GraphQLID

# Use GraphQL's built-in ID scalar (avoids reserved type conflict)
# UUID validation is handled at input/resolver level via SchemaConfig.id_policy
IDScalar = GraphQLID

# Python type annotation (Strawberry-style)
ID = NewType("ID", str)
"""GraphQL ID type annotation.

Usage:
    @fraiseql.type
    class User:
        id: ID  # Standard GraphQL ID
        name: str

When SchemaConfig.id_policy is IDPolicy.UUID (default), IDs are validated
as UUIDs at the input validation layer. When IDPolicy.OPAQUE, any string
is accepted.
"""

# Type compatibility alias
IDField = ID
