"""GraphQL selection set extraction and response filtering for APQ caching.

This module provides utilities to parse GraphQL queries and filter response data
based on the query's field selection. This ensures that cached APQ responses
only contain the fields that were actually requested by the client.

Key functions:
- extract_selection_set: Parse a query and extract its SelectionSet
- filter_response_by_selection: Filter response data to match the selection

Example usage:
    query = "query { user { id name } }"
    selection_set = extract_selection_set(query)

    full_response = {"data": {"user": {"id": 1, "name": "John", "email": "x"}}}
    filtered = filter_response_by_selection(full_response, selection_set)
    # Result: {"data": {"user": {"id": 1, "name": "John"}}}
"""

from typing import Any

from graphql import (
    DocumentNode,
    FieldNode,
    FragmentDefinitionNode,
    FragmentSpreadNode,
    InlineFragmentNode,
    OperationDefinitionNode,
    parse,
)
from graphql.language.ast import SelectionSetNode


def extract_selection_set(
    query_text: str,
    operation_name: str | None = None,
) -> SelectionSetNode | None:
    """Parse query and extract the selection set for the operation.

    Args:
        query_text: GraphQL query string
        operation_name: Optional operation name (for multi-operation documents)

    Returns:
        SelectionSetNode or None if parsing fails
    """
    try:
        document: DocumentNode = parse(query_text)

        for definition in document.definitions:
            if not isinstance(definition, OperationDefinitionNode):
                continue

            # Match by operation name if provided
            if operation_name is not None:
                if definition.name and definition.name.value == operation_name:
                    return definition.selection_set
            else:
                # Return first operation if no name specified
                return definition.selection_set

        return None
    except Exception:
        return None


def extract_fragments(query_text: str) -> dict[str, FragmentDefinitionNode]:
    """Extract all fragment definitions from a query.

    Args:
        query_text: GraphQL query string

    Returns:
        Dictionary mapping fragment names to their definitions
    """
    try:
        document: DocumentNode = parse(query_text)
        fragments: dict[str, FragmentDefinitionNode] = {}

        for definition in document.definitions:
            if isinstance(definition, FragmentDefinitionNode):
                fragments[definition.name.value] = definition

        return fragments
    except Exception:
        return {}


def filter_response_by_selection(
    response: dict[str, Any],
    selection_set: SelectionSetNode,
    fragments: dict[str, FragmentDefinitionNode] | None = None,
) -> dict[str, Any]:
    """Filter response data to only include fields in the selection set.

    Args:
        response: Full GraphQL response dict with "data" key
        selection_set: The query's selection set
        fragments: Optional dictionary of fragment definitions

    Returns:
        Filtered response with only requested fields
    """
    if "data" not in response or response["data"] is None:
        return response

    filtered_data = _filter_data_by_selection(
        response["data"],
        selection_set,
        fragments or {},
    )

    # Preserve other response keys (errors, extensions, etc.)
    return {**response, "data": filtered_data}


def _filter_data_by_selection(
    data: dict[str, Any] | list[Any] | Any,
    selection_set: SelectionSetNode,
    fragments: dict[str, FragmentDefinitionNode],
) -> dict[str, Any] | list[Any] | Any:
    """Recursively filter data based on selection set.

    Args:
        data: The data to filter (dict, list, or scalar)
        selection_set: The selection set to apply
        fragments: Dictionary of fragment definitions

    Returns:
        Filtered data matching the selection set
    """
    # Handle lists - filter each item
    if isinstance(data, list):
        return [_filter_data_by_selection(item, selection_set, fragments) for item in data]

    # Non-dict data passes through unchanged
    if not isinstance(data, dict):
        return data

    filtered: dict[str, Any] = {}

    for selection in selection_set.selections:
        if isinstance(selection, FieldNode):
            _handle_field_selection(data, selection, fragments, filtered)

        elif isinstance(selection, FragmentSpreadNode):
            # Handle named fragment spread: ...FragmentName
            fragment = fragments.get(selection.name.value)
            if fragment and fragment.selection_set:
                nested = _filter_data_by_selection(data, fragment.selection_set, fragments)
                if isinstance(nested, dict):
                    filtered.update(nested)

        elif isinstance(selection, InlineFragmentNode) and selection.selection_set:
            # Handle inline fragment: ... on Type { fields }
            nested = _filter_data_by_selection(data, selection.selection_set, fragments)
            if isinstance(nested, dict):
                filtered.update(nested)

    return filtered


def _handle_field_selection(
    data: dict[str, Any],
    selection: FieldNode,
    fragments: dict[str, FragmentDefinitionNode],
    filtered: dict[str, Any],
) -> None:
    """Handle a single field selection.

    Args:
        data: Source data dictionary
        selection: The field selection node
        fragments: Dictionary of fragment definitions
        filtered: Output dictionary to populate
    """
    # Use alias if present, otherwise field name
    response_key = selection.alias.value if selection.alias else selection.name.value
    field_name = selection.name.value

    # Try response key first (handles aliases), then field name
    if response_key in data:
        value = data[response_key]
    elif field_name in data:
        value = data[field_name]
    else:
        # Field not present in data, skip it
        return

    if selection.selection_set:
        # Recurse for nested selections
        filtered[response_key] = _filter_data_by_selection(
            value, selection.selection_set, fragments
        )
    else:
        # Scalar field - copy value directly
        filtered[response_key] = value
