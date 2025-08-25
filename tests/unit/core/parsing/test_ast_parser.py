import pytest

from fraiseql.core.ast_parser import FieldPath, extract_flat_paths, parse_query_ast


@pytest.mark.unit
def test_simple_flat_selection() -> None:
    query = """
    query {
        user {
            id
            name
        }
    }
    """
    op, fragments = parse_query_ast(query)
    paths = extract_flat_paths(op.selection_set, fragments)
    assert paths == [
        FieldPath(alias="id", path=["user", "id"]),
        FieldPath(alias="name", path=["user", "name"]),
    ]


def test_aliasing_and_nested() -> None:
    query = """
    query {
        account {
            user: profile {
                age
                nickname: username
            }
        }
    }
    """
    op, fragments = parse_query_ast(query)
    paths = extract_flat_paths(op.selection_set, fragments)
    assert paths == [
        FieldPath(alias="age", path=["account", "profile", "age"]),
        FieldPath(alias="nickname", path=["account", "profile", "username"]),
    ]


def test_named_fragment() -> None:
    query = """
    query {
        person {
            ...personFields
        }
    }

    fragment personFields on Person {
        id
        name
    }
    """
    op, fragments = parse_query_ast(query)
    paths = extract_flat_paths(op.selection_set, fragments)
    assert paths == [
        FieldPath(alias="id", path=["person", "id"]),
        FieldPath(alias="name", path=["person", "name"]),
    ]


def test_inline_fragment() -> None:
    query = """
    query {
        node {
            __typename
            ... on User {
                id
            }
        }
    }
    """
    op, fragments = parse_query_ast(query)
    paths = extract_flat_paths(op.selection_set, fragments)
    assert paths == [
        FieldPath(alias="__typename", path=["node", "__typename"]),
        FieldPath(alias="id", path=["node", "id"]),
    ]
