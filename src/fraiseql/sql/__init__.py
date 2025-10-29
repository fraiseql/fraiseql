"""SQL generation and GraphQL where/order by type utilities."""

from typing import Any

from .where_generator import safe_create_where_type


# Lazy imports to avoid circular dependencies
def __getattr__(name: str) -> Any:
    if name == "create_graphql_where_input":
        from .graphql_where_generator import create_graphql_where_input

        return create_graphql_where_input
    if name == "StringFilter":
        from .graphql_where_generator import StringFilter

        return StringFilter
    if name == "IntFilter":
        from .graphql_where_generator import IntFilter

        return IntFilter
    if name == "FloatFilter":
        from .graphql_where_generator import FloatFilter

        return FloatFilter
    if name == "DecimalFilter":
        from .graphql_where_generator import DecimalFilter

        return DecimalFilter
    if name == "BooleanFilter":
        from .graphql_where_generator import BooleanFilter

        return BooleanFilter
    if name == "UUIDFilter":
        from .graphql_where_generator import UUIDFilter

        return UUIDFilter
    if name == "DateFilter":
        from .graphql_where_generator import DateFilter

        return DateFilter
    if name == "DateTimeFilter":
        from .graphql_where_generator import DateTimeFilter

        return DateTimeFilter
    # Order by related imports
    if name == "create_graphql_order_by_input":
        from .graphql_order_by_generator import create_graphql_order_by_input

        return create_graphql_order_by_input
    if name == "create_graphql_order_by_list_input":
        from .graphql_order_by_generator import create_graphql_order_by_list_input

        return create_graphql_order_by_list_input
    if name == "OrderDirection":
        from .graphql_order_by_generator import OrderDirection

        return OrderDirection
    if name == "OrderByItem":
        from .graphql_order_by_generator import OrderByItem

        return OrderByItem
    if name == "OrderBy":
        from .order_by_generator import OrderBy

        return OrderBy
    if name == "OrderBySet":
        from .order_by_generator import OrderBySet

        return OrderBySet
    raise AttributeError(f"module '{__name__}' has no attribute '{name}'")


# pyright: reportUnsupportedDunderAll=false
__all__ = [
    "BooleanFilter",
    "DateFilter",
    "DateTimeFilter",
    "DecimalFilter",
    "FloatFilter",
    "IntFilter",
    "OrderBy",
    "OrderByItem",
    "OrderBySet",
    "OrderDirection",
    "StringFilter",
    "UUIDFilter",
    "create_graphql_order_by_input",
    "create_graphql_order_by_list_input",
    "create_graphql_where_input",
    "safe_create_where_type",
]
