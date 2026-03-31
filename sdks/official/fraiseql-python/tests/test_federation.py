"""Tests for federation support in export_schema() and @fraiseql.type."""

import json
from pathlib import Path

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import ID


@pytest.fixture(autouse=True)
def _clean_registry():
    """Clear the registry before each test."""
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


# ---------------------------------------------------------------------------
# Federation class
# ---------------------------------------------------------------------------


def test_federation_defaults():
    fed = fraiseql.Federation(service_name="users")
    assert fed.service_name == "users"
    assert fed.version == "v2"
    assert fed.default_key_fields == ["id"]


def test_federation_custom_default_key():
    fed = fraiseql.Federation(service_name="x", default_key_fields=["pk"])
    assert fed.default_key_fields == ["pk"]


# ---------------------------------------------------------------------------
# @fraiseql.type with key_fields
# ---------------------------------------------------------------------------


def test_type_key_fields_stored():
    @fraiseql.type(key_fields=["id", "region"])
    class Order:
        id: ID
        region: str
        total: int

    schema = SchemaRegistry.get_schema()
    order_type = schema["types"][0]
    assert order_type["key_fields"] == ["id", "region"]


def test_type_extends_stored():
    @fraiseql.type(key_fields=["id"], extends=True)
    class User:
        id: ID

    schema = SchemaRegistry.get_schema()
    assert schema["types"][0]["extends"] is True


def test_type_no_key_fields_by_default():
    @fraiseql.type
    class Post:
        id: ID
        title: str

    schema = SchemaRegistry.get_schema()
    assert "key_fields" not in schema["types"][0]


def test_type_key_fields_validation_not_list():
    with pytest.raises(TypeError, match="must be a list of strings"):

        @fraiseql.type(key_fields="id")  # type: ignore[arg-type]
        class Bad:
            id: ID


def test_type_key_fields_validation_empty():
    with pytest.raises(ValueError, match="must not be empty"):

        @fraiseql.type(key_fields=[])
        class Bad:
            id: ID


# ---------------------------------------------------------------------------
# get_schema_dict with federation
# ---------------------------------------------------------------------------


def test_get_schema_dict_without_federation():
    @fraiseql.type
    class User:
        id: ID
        name: str

    schema = fraiseql.get_schema_dict()
    assert "federation" not in schema


def test_get_schema_dict_with_federation():
    @fraiseql.type
    class User:
        id: ID
        name: str

    @fraiseql.type(key_fields=["id", "region"])
    class Order:
        id: ID
        region: str

    fed = fraiseql.Federation(service_name="my-subgraph")
    schema = fraiseql.get_schema_dict(federation=fed)

    assert "federation" in schema
    block = schema["federation"]
    assert block["enabled"] is True
    assert block["service_name"] == "my-subgraph"
    assert block["apollo_version"] == 2

    entities = {e["name"]: e["key_fields"] for e in block["entities"]}
    # User has no explicit key_fields → defaults to ["id"]
    assert entities["User"] == ["id"]
    # Order has explicit key_fields
    assert entities["Order"] == ["id", "region"]


def test_federation_skips_error_types():
    @fraiseql.type
    class User:
        id: ID

    @fraiseql.error
    class NotFound:
        message: str

    fed = fraiseql.Federation(service_name="svc")
    schema = fraiseql.get_schema_dict(federation=fed)
    entity_names = [e["name"] for e in schema["federation"]["entities"]]
    assert "User" in entity_names
    assert "NotFound" not in entity_names


# ---------------------------------------------------------------------------
# export_schema with federation → JSON file
# ---------------------------------------------------------------------------


def test_export_schema_federation_json(tmp_path: Path):
    @fraiseql.type
    class Product:
        id: ID
        name: str

    @fraiseql.query
    def product(id: ID) -> Product:
        """Get product."""

    output = tmp_path / "schema.json"
    fraiseql.export_schema(
        str(output),
        federation=fraiseql.Federation(service_name="products"),
    )

    data = json.loads(output.read_text())
    assert data["federation"]["enabled"] is True
    assert data["federation"]["service_name"] == "products"
    assert len(data["federation"]["entities"]) == 1
    assert data["federation"]["entities"][0] == {
        "name": "Product",
        "key_fields": ["id"],
    }


def test_export_schema_no_federation(tmp_path: Path):
    @fraiseql.type
    class Item:
        id: ID

    @fraiseql.query
    def item(id: ID) -> Item:
        """Get item."""

    output = tmp_path / "schema.json"
    fraiseql.export_schema(str(output))

    data = json.loads(output.read_text())
    assert "federation" not in data


# ---------------------------------------------------------------------------
# Rust-compatible JSON structure
# ---------------------------------------------------------------------------


def test_federation_json_matches_rust_format():
    """The federation JSON must match what the Rust IntermediateSchema parser expects:
    {"enabled": bool, "entities": [{"name": str, "key_fields": [str]}]}
    """

    @fraiseql.type
    class User:
        id: ID
        name: str

    fed = fraiseql.Federation(service_name="users")
    schema = fraiseql.get_schema_dict(federation=fed)
    block = schema["federation"]

    # Required keys
    assert isinstance(block["enabled"], bool)
    assert isinstance(block["entities"], list)
    for entity in block["entities"]:
        assert isinstance(entity["name"], str)
        assert isinstance(entity["key_fields"], list)
        assert all(isinstance(k, str) for k in entity["key_fields"])
