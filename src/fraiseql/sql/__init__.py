"""SQL generation and GraphQL where type utilities."""

from .where_generator import safe_create_where_type


# Lazy imports to avoid circular dependencies
def __getattr__(name):
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
    raise AttributeError(f"module '{__name__}' has no attribute '{name}'")


__all__ = [
    "BooleanFilter",
    "DateFilter",
    "DateTimeFilter",
    "DecimalFilter",
    "FloatFilter",
    "IntFilter",
    "StringFilter",
    "UUIDFilter",
    "create_graphql_where_input",
    "safe_create_where_type",
]
