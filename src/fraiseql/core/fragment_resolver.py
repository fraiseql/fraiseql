"""Resolve GraphQL selection sets by expanding fragments and deduplicating fields."""

from typing import cast

from graphql import (
    FieldNode,
    FragmentDefinitionNode,
    FragmentSpreadNode,
    InlineFragmentNode,
    SelectionNode,
    SelectionSetNode,
)


def resolve_all_fields(
    selection_set: SelectionSetNode,
    fragments: dict[str, FragmentDefinitionNode],
    typename: str | None = None,
) -> list[FieldNode]:
    """Resolve all fields from a selection set, including fragments.

    This function recursively expands both named and inline fragments
    within the given selection set. It ensures that fields from fragments
    are included alongside explicitly selected fields. When a `typename` is
    provided, it filters inline fragments to only include those matching
    the type condition, helping to accurately reflect the queried GraphQL
    schema's polymorphic behavior.

    Args:
        selection_set: The selection set node to resolve fields from.
        fragments: A dictionary of named fragment definitions by name.
        typename: Optional GraphQL type name to filter inline fragments.

    Returns:
        A list of unique FieldNode instances, combining explicit fields and
        expanded fragments, with duplicates removed based on alias or name.
    """
    result: list[FieldNode] = []

    def resolve(sel: SelectionNode) -> None:
        if sel.kind == "field":
            field_node = cast("FieldNode", sel)
            result.append(field_node)

        elif sel.kind == "fragment_spread":
            frag_spread = cast("FragmentSpreadNode", sel)
            name = frag_spread.name.value
            if name not in fragments:
                msg = f"Fragment '{name}' not found"
                raise ValueError(msg)
            frag = fragments[name]
            for frag_sel in frag.selection_set.selections:
                resolve(frag_sel)

        elif sel.kind == "inline_fragment":
            inline_frag = cast("InlineFragmentNode", sel)
            type_condition = (
                inline_frag.type_condition.name.value if inline_frag.type_condition else None
            )
            if typename is None or type_condition is None or type_condition == typename:
                for frag_sel in inline_frag.selection_set.selections:
                    resolve(frag_sel)

    for sel in selection_set.selections:
        resolve(sel)

    return deduplicate_fields(result)


def deduplicate_fields(fields: list[FieldNode]) -> list[FieldNode]:
    """Remove duplicated fields by alias (or name if alias is not present).

    Preserves the first occurrence of each field.
    """
    seen: set[str] = set()
    deduped: list[FieldNode] = []

    for field in fields:
        key = field.alias.value if field.alias else field.name.value
        if key not in seen:
            seen.add(key)
            deduped.append(field)

    return deduped
