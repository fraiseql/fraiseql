"""Tests for FraiseQL decorators."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test."""
    SchemaRegistry.clear()


def test_type_decorator() -> None:
    """Test @fraiseql.type decorator."""

    @fraiseql.type
    class User:
        """User type."""

        id: int
        name: str
        email: str | None

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 1

    user_type = schema["types"][0]
    assert user_type["name"] == "User"
    assert user_type["description"] == "User type."
    assert len(user_type["fields"]) == 3

    # Check id field
    id_field = next(f for f in user_type["fields"] if f["name"] == "id")
    assert id_field["type"] == "Int"
    assert id_field["nullable"] is False

    # Check name field
    name_field = next(f for f in user_type["fields"] if f["name"] == "name")
    assert name_field["type"] == "String"
    assert name_field["nullable"] is False

    # Check email field (nullable)
    email_field = next(f for f in user_type["fields"] if f["name"] == "email")
    assert email_field["type"] == "String"
    assert email_field["nullable"] is True


def test_query_decorator_simple() -> None:
    """Test @fraiseql.query decorator."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query(sql_source="v_user")
    def users(limit: int = 10) -> list[User]:
        """Get all users."""
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["queries"]) == 1

    users_query = schema["queries"][0]
    assert users_query["name"] == "users"
    assert users_query["return_type"] == "User"
    assert users_query["returns_list"] is True
    assert users_query["nullable"] is False
    assert users_query["description"] == "Get all users."
    assert users_query["sql_source"] == "v_user"

    # Check arguments
    assert len(users_query["arguments"]) == 1
    limit_arg = users_query["arguments"][0]
    assert limit_arg["name"] == "limit"
    assert limit_arg["type"] == "Int"
    assert limit_arg["nullable"] is False
    assert limit_arg["default"] == 10


def test_query_decorator_single_result() -> None:
    """Test @fraiseql.query with single result."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query(sql_source="v_user")
    def user(id: int) -> User | None:
        """Get user by ID."""
        pass

    schema = SchemaRegistry.get_schema()
    users_query = schema["queries"][0]

    assert users_query["name"] == "user"
    assert users_query["return_type"] == "User"
    assert users_query["returns_list"] is False
    assert users_query["nullable"] is True


def test_mutation_decorator() -> None:
    """Test @fraiseql.mutation decorator."""

    @fraiseql.type
    class User:
        id: int
        name: str
        email: str

    @fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
    def create_user(name: str, email: str) -> User:
        """Create a new user."""
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["mutations"]) == 1

    create_mutation = schema["mutations"][0]
    assert create_mutation["name"] == "create_user"
    assert create_mutation["return_type"] == "User"
    assert create_mutation["returns_list"] is False
    assert create_mutation["nullable"] is False
    assert create_mutation["description"] == "Create a new user."
    assert create_mutation["sql_source"] == "fn_create_user"
    assert create_mutation["operation"] == "CREATE"

    # Check arguments
    assert len(create_mutation["arguments"]) == 2
    name_arg = next(a for a in create_mutation["arguments"] if a["name"] == "name")
    assert name_arg["type"] == "String"
    assert name_arg["nullable"] is False

    email_arg = next(a for a in create_mutation["arguments"] if a["name"] == "email")
    assert email_arg["type"] == "String"
    assert email_arg["nullable"] is False


def test_multiple_types() -> None:
    """Test registering multiple types."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.type
    class Post:
        id: int
        title: str
        author_id: int

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 2

    type_names = [t["name"] for t in schema["types"]]
    assert "User" in type_names
    assert "Post" in type_names


def test_nested_types() -> None:
    """Test nested type references."""

    @fraiseql.type
    class Address:
        street: str
        city: str

    @fraiseql.type
    class User:
        id: int
        name: str
        address: Address

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 2

    user_type = next(t for t in schema["types"] if t["name"] == "User")
    address_field = next(f for f in user_type["fields"] if f["name"] == "address")
    assert address_field["type"] == "Address"


def test_list_types() -> None:
    """Test list type handling."""

    @fraiseql.type
    class User:
        id: int
        tags: list[str]

    schema = SchemaRegistry.get_schema()
    user_type = schema["types"][0]

    tags_field = next(f for f in user_type["fields"] if f["name"] == "tags")
    assert tags_field["type"] == "[String!]"


def test_export_schema(tmp_path: pytest.TempPathFactory) -> None:  # type: ignore[name-defined]
    """Test schema export to JSON file."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query(sql_source="v_user")
    def users() -> list[User]:
        pass

    output_file = tmp_path / "schema.json"  # type: ignore[operator]
    fraiseql.export_schema(str(output_file))

    # Verify file exists and is valid JSON
    assert output_file.exists()

    import json

    with open(output_file, encoding="utf-8") as f:
        schema = json.load(f)

    assert "types" in schema
    assert "queries" in schema
    assert "mutations" in schema
    assert len(schema["types"]) == 1
    assert len(schema["queries"]) == 1


def test_decorator_without_parentheses() -> None:
    """Test decorators used without parentheses."""

    @fraiseql.type
    class User:
        id: int
        name: str

    @fraiseql.query
    def users() -> list[User]:
        pass

    @fraiseql.mutation
    def create_user(name: str) -> User:
        pass

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 1
    assert len(schema["queries"]) == 1
    assert len(schema["mutations"]) == 1


def test_clear_registry() -> None:
    """Test registry clearing."""

    @fraiseql.type
    class User:
        id: int

    assert len(SchemaRegistry.get_schema()["types"]) == 1

    SchemaRegistry.clear()

    schema = SchemaRegistry.get_schema()
    assert len(schema["types"]) == 0
    assert len(schema["queries"]) == 0
    assert len(schema["mutations"]) == 0
