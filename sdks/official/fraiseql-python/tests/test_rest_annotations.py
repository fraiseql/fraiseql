"""Tests for REST annotation support in @fraiseql.query and @fraiseql.mutation."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def _clear_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


# -- Query REST annotations ----------------------------------------------------


def test_query_rest_path_and_method():
    @fraiseql.query(sql_source="v_user", rest_path="/api/users", rest_method="GET")
    def users() -> list["User"]:
        pass

    schema = SchemaRegistry.get_schema()
    query = schema["queries"][0]
    assert query["rest"] == {"path": "/api/users", "method": "GET"}


def test_query_rest_default_method_is_get():
    @fraiseql.query(sql_source="v_user", rest_path="/api/users")
    def users() -> list["User"]:
        pass

    schema = SchemaRegistry.get_schema()
    query = schema["queries"][0]
    assert query["rest"]["method"] == "GET"


def test_query_without_rest_omits_rest_block():
    @fraiseql.query(sql_source="v_user")
    def users() -> list["User"]:
        pass

    schema = SchemaRegistry.get_schema()
    query = schema["queries"][0]
    assert "rest" not in query


def test_query_rest_method_case_insensitive():
    @fraiseql.query(sql_source="v_user", rest_path="/api/users", rest_method="post")
    def users() -> list["User"]:
        pass

    schema = SchemaRegistry.get_schema()
    query = schema["queries"][0]
    assert query["rest"]["method"] == "POST"


def test_query_rest_invalid_method_raises():
    with pytest.raises(ValueError, match="not valid"):

        @fraiseql.query(sql_source="v_user", rest_path="/api/users", rest_method="INVALID")
        def users() -> list["User"]:
            pass


def test_query_rest_method_without_path_raises():
    with pytest.raises(ValueError, match="no effect"):

        @fraiseql.query(sql_source="v_user", rest_method="GET")
        def users() -> list["User"]:
            pass


def test_query_rest_empty_path_raises():
    with pytest.raises(ValueError, match="non-empty string"):

        @fraiseql.query(sql_source="v_user", rest_path="")
        def users() -> list["User"]:
            pass


# -- Mutation REST annotations -------------------------------------------------


def test_mutation_rest_path_and_method():
    @fraiseql.mutation(
        sql_source="fn_create_user",
        operation="CREATE",
        rest_path="/api/users",
        rest_method="POST",
    )
    def create_user(name: str) -> "User":
        pass

    schema = SchemaRegistry.get_schema()
    mutation = schema["mutations"][0]
    assert mutation["rest"] == {"path": "/api/users", "method": "POST"}


def test_mutation_rest_default_method_is_post():
    @fraiseql.mutation(
        sql_source="fn_create_user",
        operation="CREATE",
        rest_path="/api/users",
    )
    def create_user(name: str) -> "User":
        pass

    schema = SchemaRegistry.get_schema()
    mutation = schema["mutations"][0]
    assert mutation["rest"]["method"] == "POST"


def test_mutation_rest_delete_method():
    @fraiseql.mutation(
        sql_source="fn_delete_user",
        operation="DELETE",
        rest_path="/api/users/{id}",
        rest_method="DELETE",
    )
    def delete_user(id: int) -> "User":
        pass

    schema = SchemaRegistry.get_schema()
    mutation = schema["mutations"][0]
    assert mutation["rest"] == {"path": "/api/users/{id}", "method": "DELETE"}


def test_mutation_without_rest_omits_rest_block():
    @fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
    def create_user(name: str) -> "User":
        pass

    schema = SchemaRegistry.get_schema()
    mutation = schema["mutations"][0]
    assert "rest" not in mutation


def test_mutation_rest_method_without_path_raises():
    with pytest.raises(ValueError, match="no effect"):

        @fraiseql.mutation(
            sql_source="fn_create_user", operation="CREATE", rest_method="POST"
        )
        def create_user(name: str) -> "User":
            pass
