"""Tests for minimal types.json export (refactored TOML-based workflow).

This test verifies the new minimal export behavior where Python decorators
only generate types.json (not complete schema.json with queries, mutations,
federation, security, observers, analytics).

All configuration moves to fraiseql.toml instead.
"""

import json
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
