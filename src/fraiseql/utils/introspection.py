"""FraiseQL introspection utilities."""

from collections.abc import Mapping
from typing import Any

from fraiseql.fields import FraiseQLField
from fraiseql.types.definitions import FraiseQLTypeDefinition


def describe_type(cls: type[Any]) -> dict[str, Any]:
    """Generate an introspection-style dictionary from a FraiseQL-decorated type.

    Args:
        cls: A class decorated with @fraise_input, @success, @failure, or @fraise_type.

    Returns:
        A structured dict describing the type's name, input/output mode,
        SQL binding, and all field names with metadata like type, default, etc.
    """
    definition = getattr(cls, "__fraiseql_definition__", None)

    if not isinstance(definition, FraiseQLTypeDefinition):
        msg = f"{cls.__name__} is not a valid FraiseQL type (missing __fraiseql_definition__)"
        raise TypeError(msg)

    return {
        "typename": cls.__name__,
        "is_input": definition.is_input,
        "is_output": definition.is_output,
        "is_frozen": definition.is_frozen,
        "kw_only": definition.kw_only,
        "sql_source": definition.sql_source,
        "fields": _describe_fields(definition.fields, definition.type_hints),
    }


def _describe_fields(
    fields: Mapping[str, FraiseQLField],
    type_hints: Mapping[str, type],
) -> dict[str, dict[str, Any]]:
    """Return metadata for each field in the type definition."""
    result: dict[str, dict[str, Any]] = {}
    for name, field in fields.items():
        result[name] = {
            "type": type_hints.get(name),
            "purpose": field.purpose,
            "default": None if not field.has_default() else field.default,
            "default_factory": field.default_factory,
            "description": field.description,
        }
    return result
