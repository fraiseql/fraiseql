"""Tests for minimal types.json export (refactored TOML-based workflow).

This test verifies the new minimal export behavior where Python decorators
only generate types.json (not complete schema.json with queries, mutations,
federation, security, observers, analytics).

All configuration moves to fraiseql.toml instead.
"""

import json
import sys
import tempfile
from enum import Enum
from pathlib import Path

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test."""
    SchemaRegistry.clear()


def test_export_types_minimal_single_type() -> None:
    """export_types() should create minimal types.json with only types."""

    @fraiseql.type
    class User:
        """A user in the system."""

        id: str
        name: str
        email: str

    with tempfile.TemporaryDirectory() as tmpdir:
        output_path = Path(tmpdir) / "user_types.json"
        fraiseql.export_types(str(output_path))

        # Load and verify output
        with open(output_path) as f:
            schema = json.load(f)

        # Should have types section
        assert "types" in schema
        assert len(schema["types"]) == 1

        # Should have User type
        user_type = schema["types"][0]
        assert user_type["name"] == "User"
        assert user_type["description"] == "A user in the system."
        assert len(user_type["fields"]) == 3

        # Verify fields
        field_names = {f["name"] for f in user_type["fields"]}
        assert field_names == {"id", "name", "email"}

        # IMPORTANT: No queries, mutations, federation, security, observers, analytics
        assert "queries" not in schema or len(schema.get("queries", [])) == 0
        assert "mutations" not in schema or len(schema.get("mutations", [])) == 0
        assert "federation" not in schema or schema.get("federation") is None
        assert "security" not in schema or schema.get("security") is None
        assert "observers" not in schema or schema.get("observers") is None
        assert "analytics" not in schema or schema.get("analytics") is None


def test_export_types_multiple_types() -> None:
    """export_types() should handle multiple types correctly."""

    @fraiseql.type
    class User:
        """User type."""

        id: str
        name: str

    @fraiseql.type
    class Product:
        """Product type."""

        id: str
        title: str
        price: float

    with tempfile.TemporaryDirectory() as tmpdir:
        output_path = Path(tmpdir) / "schema_types.json"
        fraiseql.export_types(str(output_path))

        with open(output_path) as f:
            schema = json.load(f)

        assert len(schema["types"]) == 2
        type_names = {t["name"] for t in schema["types"]}
        assert type_names == {"User", "Product"}


def test_export_types_with_enums() -> None:
    """export_types() should include enums in output."""

    @fraiseql.enum
    class Status(Enum):
        """Status enum."""

        ACTIVE = "active"
        INACTIVE = "inactive"

    @fraiseql.type
    class User:
        """User with status."""

        id: str
        status: Status

    with tempfile.TemporaryDirectory() as tmpdir:
        output_path = Path(tmpdir) / "schema_types.json"
        fraiseql.export_types(str(output_path))

        with open(output_path) as f:
            schema = json.load(f)

        # Should have both enum and type
        assert "enums" in schema
        assert len(schema["enums"]) == 1
        assert schema["enums"][0]["name"] == "Status"

        assert len(schema["types"]) == 1
        assert schema["types"][0]["name"] == "User"


def test_an_enum_exports_each_members_value_when_it_is_not_its_name() -> None:
    """#1580: a member is stored as the value Python gives it, a str or an int.

    `.value` is the member's data in Python's own serializers (Pydantic, `json` for an
    `IntEnum`/`StrEnum`) and in FraiseQL v1, and Python keeps no trace of `auto()` on a
    member, so an `auto()` member is stored as the integer it is, as `LOW = 1` is. A member
    whose value is its name is stored as its name.
    """
    from enum import IntEnum, auto

    @fraiseql.enum
    class OrderStatus(Enum):
        PENDING = "pending"
        SHIPPED = "shipped"
        LOST = "LOST"

    @fraiseql.enum
    class Priority(Enum):
        LOW = 1
        HIGH = 3

    @fraiseql.enum
    class Level(Enum):
        LOW = auto()
        HIGH = auto()

    @fraiseql.enum
    class Severity(IntEnum):
        MINOR = 10

    with tempfile.TemporaryDirectory() as tmpdir:
        output_path = Path(tmpdir) / "schema_types.json"
        fraiseql.export_types(str(output_path))
        schema = json.loads(output_path.read_text())

    by_name = {e["name"]: e["values"] for e in schema["enums"]}
    assert by_name["OrderStatus"] == [
        {"name": "PENDING", "value": "pending"},
        {"name": "SHIPPED", "value": "shipped"},
        {"name": "LOST"},
    ]
    assert by_name["Priority"] == [{"name": "LOW", "value": 1}, {"name": "HIGH", "value": 3}]
    assert by_name["Level"] == [{"name": "LOW", "value": 1}, {"name": "HIGH", "value": 2}]
    assert by_name["Severity"] == [{"name": "MINOR", "value": 10}]


@pytest.mark.skipif(sys.version_info < (3, 11), reason="enum.StrEnum is Python 3.11+")
def test_a_str_enum_auto_member_exports_the_lowercase_name_python_gives_it() -> None:
    """A `StrEnum`'s `auto()` value is the member's name in lower case, and is stored as it."""
    from enum import StrEnum, auto

    @fraiseql.enum
    class Colour(StrEnum):
        RED = auto()

    with tempfile.TemporaryDirectory() as tmpdir:
        output_path = Path(tmpdir) / "schema_types.json"
        fraiseql.export_types(str(output_path))
        schema = json.loads(output_path.read_text())

    by_name = {e["name"]: e["values"] for e in schema["enums"]}
    assert by_name["Colour"] == [{"name": "RED", "value": "red"}]


@pytest.mark.parametrize("value", [1.5, True, (1, 2), None])
def test_an_enum_value_that_is_neither_a_str_nor_an_int_is_refused(value: object) -> None:
    """A stored value is a string or an integer; anything else is refused where it is
    declared, with the member named, rather than exported as its name."""

    with pytest.raises(TypeError, match=r"Odd\.X.*str or an int"):

        @fraiseql.enum
        class Odd(Enum):
            X = value


def test_export_types_with_input_types() -> None:
    """export_types() should include input types in output."""

    @fraiseql.input
    class CreateUserInput:
        """Input for creating a user."""

        name: str
        email: str

    @fraiseql.type
    class User:
        """User type."""

        id: str
        name: str

    with tempfile.TemporaryDirectory() as tmpdir:
        output_path = Path(tmpdir) / "schema_types.json"
        fraiseql.export_types(str(output_path))

        with open(output_path) as f:
            schema = json.load(f)

        # Should have both input type and type
        assert "input_types" in schema
        assert len(schema["input_types"]) == 1
        assert schema["input_types"][0]["name"] == "CreateUserInput"

        assert len(schema["types"]) == 1
        assert schema["types"][0]["name"] == "User"


def test_export_types_no_queries_or_mutations() -> None:
    """export_types() should NOT include queries or mutations in output."""

    @fraiseql.type
    class User:
        """User type."""

        id: str
        name: str

    # Queries and mutations are defined but should NOT appear in types.json
    @fraiseql.query(sql_source="v_user")
    def get_user(user_id: str) -> User:
        """Get a user (query config moves to TOML)."""
        ...

    @fraiseql.mutation(sql_source="m_create_user")
    def create_user(name: str, email: str) -> User:
        """Create a user (mutation config moves to TOML)."""
        ...

    with tempfile.TemporaryDirectory() as tmpdir:
        output_path = Path(tmpdir) / "schema_types.json"
        fraiseql.export_types(str(output_path))

        with open(output_path) as f:
            schema = json.load(f)

        # Should only have the type
        assert len(schema["types"]) == 1
        assert schema["types"][0]["name"] == "User"

        # Queries and mutations should NOT be in types.json
        # They move to fraiseql.toml [queries] and [mutations] sections
        assert "queries" not in schema or len(schema.get("queries", [])) == 0
        assert "mutations" not in schema or len(schema.get("mutations", [])) == 0


def test_embedded_type_emits_flag_and_suppresses_source() -> None:
    """An @fraiseql.type(embedded=True) value object emits ``embedded: true`` and NO
    ``sql_source`` — the synthesized ``v_money`` is exactly what misclassifies a value
    object as a cascade entity (#687), so the SDK must suppress it."""

    @fraiseql.type(embedded=True)
    class Money:
        """A monetary amount — an embedded value object, no independent identity."""

        amount: int
        currency: str

    schema = SchemaRegistry.get_schema()
    money = next(t for t in schema["types"] if t["name"] == "Money")

    assert money["embedded"] is True
    assert "sql_source" not in money, (
        "an embedded value object must declare no source — a synthesized v_money is what "
        "misclassifies it as a cascade entity"
    )


def test_non_embedded_type_is_unchanged() -> None:
    """A default (non-embedded) type keeps its synthesized ``sql_source`` and carries NO
    ``embedded`` key (skip-when-false ⇒ byte-identical to pre-#687)."""

    @fraiseql.type
    class User:
        """A user."""

        id: str
        name: str

    schema = SchemaRegistry.get_schema()
    user = next(t for t in schema["types"] if t["name"] == "User")

    assert user["sql_source"] == "v_user"
    assert "embedded" not in user


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
