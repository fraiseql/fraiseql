"""Tests for type mapping and introspection."""

import pytest

from fraiseql.scalars import ID, UUID, Date, DateTime, Decimal, Json, Time, Vector
from fraiseql.types import extract_field_info, extract_function_signature, python_type_to_graphql


def test_python_type_to_graphql_basic() -> None:
    """Test basic Python type to GraphQL type conversion."""
    assert python_type_to_graphql(int) == ("Int", False)
    assert python_type_to_graphql(float) == ("Float", False)
    assert python_type_to_graphql(str) == ("String", False)
    assert python_type_to_graphql(bool) == ("Boolean", False)


def test_python_type_to_graphql_scalars() -> None:
    """Test FraiseQL scalar types to GraphQL type conversion."""
    # Core scalar
    assert python_type_to_graphql(ID) == ("ID", False)

    # Date/time scalars
    assert python_type_to_graphql(DateTime) == ("DateTime", False)
    assert python_type_to_graphql(Date) == ("Date", False)
    assert python_type_to_graphql(Time) == ("Time", False)

    # Complex scalars
    assert python_type_to_graphql(UUID) == ("UUID", False)
    assert python_type_to_graphql(Json) == ("Json", False)
    assert python_type_to_graphql(Decimal) == ("Decimal", False)
    assert python_type_to_graphql(Vector) == ("Vector", False)


def test_python_type_to_graphql_nullable_scalars() -> None:
    """Test nullable FraiseQL scalar types."""
    graphql_type, nullable = python_type_to_graphql(ID | None)
    assert graphql_type == "ID"
    assert nullable is True

    graphql_type, nullable = python_type_to_graphql(DateTime | None)
    assert graphql_type == "DateTime"
    assert nullable is True

    graphql_type, nullable = python_type_to_graphql(Json | None)
    assert graphql_type == "Json"
    assert nullable is True


def test_python_type_to_graphql_list_of_scalars() -> None:
    """Test list of FraiseQL scalar types."""
    graphql_type, nullable = python_type_to_graphql(list[ID])
    assert graphql_type == "[ID!]"
    assert nullable is False

    graphql_type, nullable = python_type_to_graphql(list[DateTime])
    assert graphql_type == "[DateTime!]"
    assert nullable is False


def test_python_type_to_graphql_rich_scalars() -> None:
    """Test rich FraiseQL scalar types."""
    from fraiseql.scalars import Email, IBAN, IPAddress, LTree, Money, PhoneNumber, URL

    # Contact scalars
    assert python_type_to_graphql(Email) == ("Email", False)
    assert python_type_to_graphql(PhoneNumber) == ("PhoneNumber", False)
    assert python_type_to_graphql(URL) == ("URL", False)

    # Financial scalars
    assert python_type_to_graphql(IBAN) == ("IBAN", False)
    assert python_type_to_graphql(Money) == ("Money", False)

    # Networking scalars
    assert python_type_to_graphql(IPAddress) == ("IPAddress", False)

    # Database scalars
    assert python_type_to_graphql(LTree) == ("LTree", False)

    # Nullable rich scalars
    graphql_type, nullable = python_type_to_graphql(Email | None)
    assert graphql_type == "Email"
    assert nullable is True


def test_python_type_to_graphql_custom_scalar() -> None:
    """Test user-defined custom scalar types."""
    from typing import NewType

    # User can define their own custom scalars
    MyCustomScalar = NewType("MyCustomScalar", str)
    CompanyId = NewType("CompanyId", str)

    assert python_type_to_graphql(MyCustomScalar) == ("MyCustomScalar", False)
    assert python_type_to_graphql(CompanyId) == ("CompanyId", False)

    # Nullable custom scalar
    graphql_type, nullable = python_type_to_graphql(MyCustomScalar | None)
    assert graphql_type == "MyCustomScalar"
    assert nullable is True


def test_python_type_to_graphql_nullable() -> None:
    """Test nullable type conversion."""
    # Using | None syntax
    graphql_type, nullable = python_type_to_graphql(str | None)
    assert graphql_type == "String"
    assert nullable is True


def test_python_type_to_graphql_list() -> None:
    """Test list type conversion."""
    graphql_type, nullable = python_type_to_graphql(list[int])
    assert graphql_type == "[Int!]"
    assert nullable is False

    graphql_type, nullable = python_type_to_graphql(list[str])
    assert graphql_type == "[String!]"
    assert nullable is False


def test_python_type_to_graphql_custom_class() -> None:
    """Test custom class type conversion."""

    class User:
        pass

    graphql_type, nullable = python_type_to_graphql(User)
    assert graphql_type == "User"
    assert nullable is False


def test_extract_field_info() -> None:
    """Test field extraction from class annotations."""

    class User:
        """User type."""

        id: int
        name: str
        email: str | None
        age: int

    fields = extract_field_info(User)

    assert len(fields) == 4

    assert fields["id"]["type"] == "Int"
    assert fields["id"]["nullable"] is False

    assert fields["name"]["type"] == "String"
    assert fields["name"]["nullable"] is False

    assert fields["email"]["type"] == "String"
    assert fields["email"]["nullable"] is True

    assert fields["age"]["type"] == "Int"
    assert fields["age"]["nullable"] is False


def test_extract_function_signature_simple() -> None:
    """Test function signature extraction."""

    def users(limit: int = 10) -> list[str]:
        pass

    sig = extract_function_signature(users)

    assert len(sig["arguments"]) == 1
    assert sig["arguments"][0]["name"] == "limit"
    assert sig["arguments"][0]["type"] == "Int"
    assert sig["arguments"][0]["nullable"] is False
    assert sig["arguments"][0]["default"] == 10

    assert sig["return_type"]["type"] == "[String!]"
    assert sig["return_type"]["nullable"] is False
    assert sig["return_type"]["is_list"] is True


def test_extract_function_signature_multiple_args() -> None:
    """Test function with multiple arguments."""

    class User:
        pass

    def search_users(name: str, age: int | None = None, limit: int = 10) -> list[User]:
        pass

    sig = extract_function_signature(search_users)

    assert len(sig["arguments"]) == 3

    # Check name arg
    name_arg = next(a for a in sig["arguments"] if a["name"] == "name")
    assert name_arg["type"] == "String"
    assert name_arg["nullable"] is False
    assert "default" not in name_arg

    # Check age arg
    age_arg = next(a for a in sig["arguments"] if a["name"] == "age")
    assert age_arg["type"] == "Int"
    assert age_arg["nullable"] is True
    assert age_arg["default"] is None

    # Check limit arg
    limit_arg = next(a for a in sig["arguments"] if a["name"] == "limit")
    assert limit_arg["type"] == "Int"
    assert limit_arg["nullable"] is False
    assert limit_arg["default"] == 10

    # Check return type
    assert sig["return_type"]["type"] == "[User!]"
    assert sig["return_type"]["is_list"] is True


def test_extract_function_signature_nullable_return() -> None:
    """Test function with nullable return type."""

    class User:
        pass

    def user(id: int) -> User | None:
        pass

    sig = extract_function_signature(user)

    assert sig["return_type"]["type"] == "User"
    assert sig["return_type"]["nullable"] is True
    assert sig["return_type"]["is_list"] is False


def test_missing_type_annotation() -> None:
    """Test error on missing type annotation."""

    def bad_function(x) -> int:  # type: ignore[no-untyped-def]
        pass

    with pytest.raises(ValueError, match="missing type annotation"):
        extract_function_signature(bad_function)


def test_missing_return_type() -> None:
    """Test error on missing return type."""

    def bad_function(x: int):  # type: ignore[no-untyped-def]
        pass

    with pytest.raises(ValueError, match="missing return type annotation"):
        extract_function_signature(bad_function)
