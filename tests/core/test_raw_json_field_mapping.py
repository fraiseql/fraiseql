import pytest

"""Test raw JSON field mapping functionality."""

from graphql import OperationDefinitionNode, parse

from fraiseql.core.ast_parser import FieldPath, extract_flat_paths
from fraiseql.sql.sql_generator import build_sql_query
from fraiseql.utils.casing import to_snake_case



@pytest.mark.unit
def test_extract_field_paths():
    """Test extracting field paths from GraphQL query."""
    query = """
    query {
        user {
            id
            firstName
            lastName
            emailAddress
            profile {
                avatarUrl
                bio
            }
        }
    }
    """
    # Parse the query
    doc = parse(query)
    operation = next(
        (defn for defn in doc.definitions if isinstance(defn, OperationDefinitionNode)), None
    )

    assert operation is not None
    assert operation.selection_set is not None

    # Get the user field
    user_field = operation.selection_set.selections[0]
    assert user_field.selection_set is not None

    # Extract field paths with snake_case transformation
    field_paths = extract_flat_paths(user_field.selection_set, {}, transform_path=to_snake_case)

    # Verify the extracted paths
    expected_paths = [
        ("id", ["id"]),
        ("firstName", ["first_name"]),
        ("lastName", ["last_name"]),
        ("emailAddress", ["email_address"]),
        ("avatarUrl", ["profile", "avatar_url"]),
        ("bio", ["profile", "bio"]),
    ]

    assert len(field_paths) == len(expected_paths)

    for fp, (expected_alias, expected_path) in zip(field_paths, expected_paths, strict=False):
        assert fp.alias == expected_alias
        assert fp.path == expected_path


def test_sql_generation_with_field_mapping():
    """Test SQL generation with camelCase field mapping."""
    # Create field paths
    field_paths = [
        FieldPath(alias="id", path=["id"]),
        FieldPath(alias="firstName", path=["first_name"]),
        FieldPath(alias="lastName", path=["last_name"]),
        FieldPath(alias="emailAddress", path=["email_address"]),
    ]

    # Generate SQL with field mapping
    sql_query = build_sql_query(
        table="user_view",
        field_paths=field_paths,
        where_clause=None,
        json_output=True,
        typename="User",
        raw_json_output=True,
        auto_camel_case=True,
    )

    # Convert to string for verification
    sql_str = sql_query.as_string(None)

    # Verify the query contains expected elements
    assert "jsonb_build_object" in sql_str
    assert "'id', data->>'id'" in sql_str
    assert "'firstName', data->>'first_name'" in sql_str
    assert "'lastName', data->>'last_name'" in sql_str
    assert "'emailAddress', data->>'email_address'" in sql_str
    assert "'__typename', 'User'" in sql_str
    assert "::text AS result" in sql_str
    assert 'FROM "user_view"' in sql_str


def test_sql_generation_with_nested_fields():
    """Test SQL generation with nested field paths."""
    field_paths = [
        FieldPath(alias="id", path=["id"]),
        FieldPath(alias="firstName", path=["first_name"]),
        FieldPath(alias="avatarUrl", path=["profile", "avatar_url"]),
        FieldPath(alias="city", path=["address", "city"]),
    ]

    sql_query = build_sql_query(
        table="user_view",
        field_paths=field_paths,
        where_clause=None,
        json_output=True,
        typename="User",
        raw_json_output=True,
        auto_camel_case=True,
    )

    sql_str = sql_query.as_string(None)

    # Verify nested field extraction
    assert "'avatarUrl', data->'profile'->>'avatar_url'" in sql_str
    assert "'city', data->'address'->>'city'" in sql_str


def test_sql_generation_without_typename():
    """Test SQL generation without typename."""
    field_paths = [FieldPath(alias="id", path=["id"]), FieldPath(alias="name", path=["name"])]

    sql_query = build_sql_query(
        table="test_view",
        field_paths=field_paths,
        where_clause=None,
        json_output=True,
        typename=None,  # No typename
        raw_json_output=True,
        auto_camel_case=True,
    )

    sql_str = sql_query.as_string(None)

    # Verify no __typename field
    assert "'__typename'" not in sql_str
    assert "'id', data->>'id'" in sql_str
    assert "'name', data->>'name'" in sql_str