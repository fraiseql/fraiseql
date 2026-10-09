"""`fraiseql.Localized[str]` (#1513): a field, argument or input field stored as a locale map."""

from typing import Annotated

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import ID


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear the registry around each test."""
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def _schema() -> dict:
    return SchemaRegistry.get_schema()


def _field(type_name: str, field: str) -> dict:
    sections = _schema()["types"] + _schema()["input_types"]
    owner = next(t for t in sections if t["name"] == type_name)
    return next(f for f in owner["fields"] if f["name"] == field)


def test_a_localized_field_is_a_string_marked_localized():
    @fraiseql.type(sql_source="tv_product")
    class Product:
        id: ID
        name: fraiseql.Localized[str]
        description: fraiseql.Localized[str] | None
        sku: str

    assert _field("Product", "name") == {
        "name": "name",
        "type": "String",
        "nullable": False,
        "localized": True,
    }
    assert _field("Product", "description") == {
        "name": "description",
        "type": "String",
        "nullable": True,
        "localized": True,
    }
    assert "localized" not in _field("Product", "sku")


def test_field_metadata_still_applies_to_a_localized_field():
    @fraiseql.type(sql_source="tv_product")
    class Product:
        id: ID
        name: Annotated[fraiseql.Localized[str] | None, fraiseql.field(description="Label")]

    assert _field("Product", "name") == {
        "name": "name",
        "type": "String",
        "nullable": True,
        "localized": True,
        "description": "Label",
    }


def test_a_localized_input_field_and_argument_are_marked_localized():
    @fraiseql.type(sql_source="tv_product")
    class Product:
        id: ID

    @fraiseql.input
    class CreateProductInput:
        name: fraiseql.Localized[str]

    @fraiseql.mutation(sql_source="fn_rename_product", operation="update")
    def rename_product(id: ID, name: fraiseql.Localized[str] | None) -> Product:
        pass

    assert _field("CreateProductInput", "name")["localized"] is True
    mutation = next(m for m in _schema()["mutations"] if m["name"] == "renameProduct")
    name = next(a for a in mutation["arguments"] if a["name"] == "name")
    assert name == {"name": "name", "type": "String", "nullable": True, "localized": True}


def test_only_a_string_can_be_localized():
    with pytest.raises(TypeError, match=r"Localized\[int\] is not supported"):

        @fraiseql.type(sql_source="tv_product")
        class Product:
            id: ID
            stock: fraiseql.Localized[int]
