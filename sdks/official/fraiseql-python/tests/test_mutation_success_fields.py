"""#1397: a cascade mutation declares typed success fields.

``@fraiseql.mutation(..., success_fields={"recovered_items": int})`` exports each as a field
the compiler adds to the mutation's payload; the function returns them in
a ``result jsonb`` column of its row, keyed by the stored (snake_case) name.
"""

from enum import Enum

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def _clear() -> None:
    SchemaRegistry.clear()


@fraiseql.enum
class Recovery(Enum):
    FULL = "FULL"
    PARTIAL = "PARTIAL"


def _order() -> type:
    @fraiseql.type(sql_source="v_order")
    class Order:
        id: str
        total: int

    return Order


def test_success_fields_export_as_typed_fields() -> None:
    order = _order()

    @fraiseql.mutation(
        sql_source="fn_create_order",
        operation="insert",
        cascade=True,
        success_fields={"recovered_items": int, "recovery": Recovery | None},
    )
    def create_order(total: int) -> order:  # type: ignore[valid-type]
        """Create an order and re-attach orphaned lines."""

    mutation = SchemaRegistry.get_schema()["mutations"][0]
    assert mutation["success_fields"] == [
        {"name": "recoveredItems", "type": "Int", "nullable": False},
        {"name": "recovery", "type": "Recovery", "nullable": True},
    ]


def test_success_fields_without_cascade_are_refused() -> None:
    order = _order()
    with pytest.raises(ValueError, match=r"success_fields.*cascade=True"):

        @fraiseql.mutation(
            sql_source="fn_create_order", operation="insert", success_fields={"n": int}
        )
        def create_order(total: int) -> order:  # type: ignore[valid-type]
            """No payload to carry them."""


def test_success_fields_must_be_a_mapping_of_names_to_types() -> None:
    order = _order()
    for bad in (["recovered_items"], {"": int}, {3: int}):
        with pytest.raises((TypeError, ValueError), match="success_fields"):

            @fraiseql.mutation(
                sql_source="fn_create_order",
                operation="insert",
                cascade=True,
                success_fields=bad,
            )
            def create_order(total: int) -> order:  # type: ignore[valid-type]
                """Malformed."""
