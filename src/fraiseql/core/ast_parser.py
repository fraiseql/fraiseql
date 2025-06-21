"""Parse GraphQL AST and extract field paths for SQL JSONB query generation."""

from collections.abc import Callable
from typing import NamedTuple

from graphql import (
    DocumentNode,
    FragmentDefinitionNode,
    OperationDefinitionNode,
    SelectionSetNode,
    parse,
)

from fraiseql.core.fragment_resolver import resolve_all_fields


class FieldPath(NamedTuple):
    """Represents a flattened JSON path and alias extracted from a GraphQL selection set."""

    alias: str
    path: list[str]


def parse_query_ast(
    source: str,
) -> tuple[OperationDefinitionNode, dict[str, FragmentDefinitionNode]]:
    """Parse the GraphQL query string and return the root operation and fragment map."""
    doc: DocumentNode = parse(source)
    op = next(
        (defn for defn in doc.definitions if isinstance(defn, OperationDefinitionNode)),
        None,
    )
    if op is None:
        msg = "No operation found in query"
        raise ValueError(msg)

    fragments: dict[str, FragmentDefinitionNode] = {
        defn.name.value: defn
        for defn in doc.definitions
        if isinstance(defn, FragmentDefinitionNode)
    }
    return op, fragments


def extract_flat_paths(
    selection: SelectionSetNode,
    fragments: dict[str, FragmentDefinitionNode],
    path: list[str] | None = None,
    transform_path: Callable[[str], str] | None = None,
) -> list[FieldPath]:
    """Recursively extract flattened JSONB paths from the selection set.

    Args:
        selection: The selection set to process.
        fragments: The fragment definition map.
        path: The current path of JSON keys being built.
        transform_path: An optional function to transform each path segment
            (e.g. camelCase → snake_case).

    Returns:
        A flat list of FieldPath with alias and transformed path list.
    """
    if path is None:
        path = []

    result: list[FieldPath] = []
    all_fields = resolve_all_fields(selection, fragments)

    for field in all_fields:
        name = field.name.value
        transformed = transform_path(name) if transform_path else name
        alias = field.alias.value if field.alias else name
        current_path = [*path, transformed]

        if field.selection_set:
            result.extend(
                extract_flat_paths(
                    field.selection_set,
                    fragments,
                    current_path,
                    transform_path,
                ),
            )
        else:
            result.append(FieldPath(alias=alias, path=current_path))

    return result
