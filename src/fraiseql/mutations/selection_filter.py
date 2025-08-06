"""GraphQL mutation result filtering based on selection set.

This module provides utilities to filter mutation result data based on the
GraphQL query's requested fields, as captured in the GraphQLResolveInfo object.

It enables post-processing of Python mutation result dictionaries to return only
the fields explicitly requested by the client. This is useful for decoupling SQL-level
response shaping from GraphQL-level field resolution.

The main entry point is `filter_mutation_result`, which takes the raw data dictionary
and a `GraphQLResolveInfo` instance and returns a filtered dictionary respecting
the GraphQL selection set.

Features:
- Supports nested field filtering.
- Skips fields not present in the original data.
- Safe to use with dataclass-serialized output.

Intended for use in FraiseQL mutation resolvers to ensure efficient, flexible
GraphQL responses without overfetching.

Example usage:
---------------
    result_dict = dataclasses.asdict(mutation_result)
    filtered = filter_mutation_result(result_dict, info)
    return type(mutation_result)(**filtered)
"""

from collections.abc import Mapping
from typing import Any

from graphql import FieldNode, GraphQLResolveInfo, SelectionSetNode


def filter_mutation_result(data: Mapping[str, Any], info: GraphQLResolveInfo) -> dict[str, Any]:
    """Filters the mutation result data to only include fields selected in the GraphQL query.

    This supports top-level selection sets and nested structures.

    Args:
        data: The raw mutation result dictionary (e.g. from a dataclass).
        info: The GraphQL execution info, used to inspect selected fields.

    Returns:
        A new dictionary with only selected fields from the original result.
    """
    if not info.field_nodes:
        # No field nodes means no selection info - return data as-is
        return dict(data) if isinstance(data, Mapping) else {}

    top_field = info.field_nodes[0]
    selection = top_field.selection_set
    if selection is None:
        return {}  # Or handle as appropriate, e.g., return data as-is
    return _filter_selected_fields(data, selection)


def _filter_selected_fields(
    data: Mapping[str, Any],
    selection_set: SelectionSetNode,
) -> dict[str, Any]:
    filtered = {}
    for selection in selection_set.selections:
        if not isinstance(selection, FieldNode):
            continue

        field_name = selection.alias.value if selection.alias else selection.name.value
        if field_name not in data:
            continue

        value = data[field_name]

        if selection.selection_set and isinstance(value, dict):
            # Recurse for nested selection sets
            filtered[field_name] = _filter_selected_fields(value, selection.selection_set)
        else:
            filtered[field_name] = value

    return filtered
