"""Module defining protocols for FraiseQL input and output types.

This module contains protocol definitions for types used in FraiseQL, specifying
the structure and metadata required for input and output types in the system.
"""

from typing import Any, Protocol

from fraiseql.types.definitions import FraiseQLTypeDefinition


class FraiseQLOutputType(Protocol):
    """Protocol defining the structure of an output type in FraiseQL.

    This protocol specifies the necessary attributes for a type to be considered
    a valid output type in the FraiseQL system. It includes metadata such as the
    GraphQL typename, table, where type, fields, and a FraiseQL type definition.
    """

    __gql_typename__: str
    __gql_table__: str | None
    __gql_where_type__: object
    __gql_fields__: dict[str, Any]
    __fraiseql_definition__: FraiseQLTypeDefinition


class FraiseQLInputType(Protocol):
    """Protocol defining the structure of an input type in FraiseQL.

    This protocol specifies the necessary attributes for a type to be considered
    a valid input type in the FraiseQL system. It includes metadata such as the
    GraphQL typename, fields, and a FraiseQL type definition.
    """

    __gql_typename__: str
    __gql_fields__: dict[str, Any]
    __fraiseql_definition__: FraiseQLTypeDefinition
