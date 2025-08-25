import pytest
from graphql import FragmentDefinitionNode, parse

from fraiseql.core.fragment_resolver import resolve_all_fields


def get_fragments(query: str) -> dict[str, FragmentDefinitionNode]:
    doc = parse(query)
    return {
        defn.name.value: defn
        for defn in doc.definitions
        if isinstance(defn, FragmentDefinitionNode)
    }


@pytest.mark.unit
def test_named_fragment_resolution() -> None:
    query = """
    query {
        viewer {
            ...UserFields
        }
    }

    fragment UserFields on User {
        id
        username
    }
    """
    doc = parse(query)
    op = next(d for d in doc.definitions if d.kind == "operation_definition")
    viewer_sel = op.selection_set.selections[0]
    fragments = get_fragments(query)

    fields = resolve_all_fields(viewer_sel.selection_set, fragments)
    assert {f.name.value for f in fields} == {"id", "username"}


def test_inline_fragment_typename_match() -> None:
    query = """
    query {
        node {
            ... on User {
                email
            }
        }
    }
    """
    doc = parse(query)
    op = next(d for d in doc.definitions if d.kind == "operation_definition")
    node_sel = op.selection_set.selections[0]

    fields = resolve_all_fields(node_sel.selection_set, fragments={}, typename="User")
    assert len(fields) == 1
    assert fields[0].name.value == "email"


def test_inline_fragment_typename_mismatch() -> None:
    query = """
    query {
        node {
            ... on User {
                email
            }
        }
    }
    """
    doc = parse(query)
    op = next(d for d in doc.definitions if d.kind == "operation_definition")
    node_sel = op.selection_set.selections[0]

    fields = resolve_all_fields(node_sel.selection_set, fragments={}, typename="Post")
    assert fields == []


def test_deduplication_by_alias() -> None:
    query = """
    query {
        thing {
            name
            display: name
            id
        }
    }
    """
    doc = parse(query)
    op = next(d for d in doc.definitions if d.kind == "operation_definition")
    thing_sel = op.selection_set.selections[0]

    fields = resolve_all_fields(thing_sel.selection_set, fragments={})
    aliases = [f.alias.value if f.alias else f.name.value for f in fields]
    assert aliases == ["name", "display", "id"]
