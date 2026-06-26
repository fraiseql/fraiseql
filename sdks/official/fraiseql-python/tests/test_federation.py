"""Tests for federation support in export_schema() and @fraiseql.type."""

import json
from pathlib import Path
from typing import Annotated

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


# ---------------------------------------------------------------------------
# #496: derive per-entity / type-level federation directives from decorators
# ---------------------------------------------------------------------------


def test_type_shareable_stored():
    @fraiseql.type(shareable=True)
    class Money:
        amount: int
        currency: str

    schema = SchemaRegistry.get_schema()
    assert schema["types"][0]["shareable"] is True


def test_type_not_shareable_by_default():
    @fraiseql.type
    class Money:
        amount: int

    schema = SchemaRegistry.get_schema()
    assert "shareable" not in schema["types"][0]


def test_type_shareable_with_key_fields_rejected():
    # A type is either a keyless @shareable value type or a keyed entity — never
    # both. (For a shareable field on an entity, use field(shareable=True).)
    with pytest.raises(ValueError, match="shareable"):

        @fraiseql.type(key_fields=["id"], shareable=True)
        class Bad:
            id: ID


def test_error_accepts_shareable_kwarg():
    @fraiseql.error(shareable=True)
    class MutationError:
        message: str

    schema = SchemaRegistry.get_schema()
    assert schema["types"][0]["is_error"] is True
    assert schema["types"][0]["shareable"] is True


def test_error_without_parens_still_works():
    @fraiseql.error
    class NotFound:
        message: str

    schema = SchemaRegistry.get_schema()
    assert schema["types"][0]["is_error"] is True
    assert "shareable" not in schema["types"][0]


def test_federation_derives_extends():
    @fraiseql.type(key_fields=["id"], extends=True)
    class Product:
        id: ID

    fed = fraiseql.Federation(service_name="reviews")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    product = next(e for e in block["entities"] if e["name"] == "Product")
    assert product["extends"] is True


def test_federation_derives_external_fields():
    @fraiseql.type(key_fields=["id"], extends=True)
    class Product:
        id: Annotated[ID, fraiseql.field(external=True)]
        weight: Annotated[float, fraiseql.field(external=True)]
        reviews: str

    fed = fraiseql.Federation(service_name="reviews")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    product = next(e for e in block["entities"] if e["name"] == "Product")
    assert set(product["external_fields"]) == {"id", "weight"}
    assert "reviews" not in product["external_fields"]


def test_federation_derives_shareable_fields():
    @fraiseql.type(key_fields=["id"])
    class Product:
        id: ID
        name: Annotated[str, fraiseql.field(shareable=True)]

    fed = fraiseql.Federation(service_name="catalog")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    product = next(e for e in block["entities"] if e["name"] == "Product")
    assert product["shareable_fields"] == ["name"]


def test_federation_shareable_type_goes_to_shareable_types_not_entities():
    @fraiseql.type
    class User:
        id: ID

    @fraiseql.type(shareable=True)
    class Money:
        amount: int
        currency: str

    fed = fraiseql.Federation(service_name="catalog")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    entity_names = [e["name"] for e in block["entities"]]
    assert "User" in entity_names
    assert "Money" not in entity_names  # keyless value type, not an entity
    assert block["shareable_types"] == ["Money"]


def test_federation_shareable_error_type_is_shareable_value_type():
    @fraiseql.type
    class User:
        id: ID

    @fraiseql.error(shareable=True)
    class MutationError:
        message: str

    fed = fraiseql.Federation(service_name="catalog")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    entity_names = [e["name"] for e in block["entities"]]
    assert "MutationError" not in entity_names
    assert block["shareable_types"] == ["MutationError"]


def test_federation_plain_entity_has_no_directive_keys():
    # Guards the byte-exact entity contract (see test_export_schema_federation_json):
    # an entity with no directives stays exactly {name, key_fields}.
    @fraiseql.type
    class User:
        id: ID
        name: str

    fed = fraiseql.Federation(service_name="users")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    user = next(e for e in block["entities"] if e["name"] == "User")
    assert user == {"name": "User", "key_fields": ["id"]}


def test_federation_no_shareable_types_key_when_none():
    @fraiseql.type
    class User:
        id: ID

    fed = fraiseql.Federation(service_name="users")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    assert "shareable_types" not in block


def test_federation_emits_version_string():
    # The Rust core FederationConfig reads `version` (the @link spec URL); the legacy
    # int `apollo_version` is ignored there. Emit both.
    @fraiseql.type
    class User:
        id: ID

    fed = fraiseql.Federation(service_name="users")
    block = fraiseql.get_schema_dict(federation=fed)["federation"]
    assert block["version"] == "v2"
