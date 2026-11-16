"""`scoped_like_parent` on a field that embeds a scoped type (#1737).

A type whose own query is scoped by `inject` does not compile inside another type's
document unless the embedding field declares it scoped like its parent. The compiler
checks the claim; the SDK's part is to carry the declaration, and only where it was made.
"""

from typing import Annotated

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import ID
from fraiseql.schema import get_schema_dict


@pytest.fixture(autouse=True)
def clear_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def _fields(type_name: str) -> dict[str, dict]:
    types = {t["name"]: t for t in get_schema_dict()["types"]}
    return {f["name"]: f for f in types[type_name]["fields"]}


def test_the_declaration_is_exported_on_the_field_that_makes_it():
    @fraiseql.type(sql_source="v_order_line")
    class OrderLine:
        id: ID

    @fraiseql.type(sql_source="v_order")
    class Order:
        id: ID
        lines: Annotated[list[OrderLine], fraiseql.field(scoped_like_parent=True)]
        first_line: OrderLine

    fields = _fields("Order")
    assert fields["lines"]["scoped_like_parent"] is True
    assert "scoped_like_parent" not in fields["firstLine"]
    assert "scoped_like_parent" not in fields["id"]


def test_an_input_field_cannot_declare_it():
    with pytest.raises(TypeError, match="scoped_like_parent"):

        @fraiseql.input
        class OrderInput:
            line_id: Annotated[ID, fraiseql.field(scoped_like_parent=True)]
