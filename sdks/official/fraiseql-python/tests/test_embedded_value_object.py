"""Rejection tests for the ``@fraiseql.type(embedded=True)`` value-object marker (#687).

An embedded value object has no independent identity and no backing view, so two
combinations are contradictions the SDK refuses at authoring time — the compiler only
``warn!``s on the hand-authored form because the SDK is supposed to make it unreachable.
"""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test."""
    SchemaRegistry.clear()


def test_embedded_with_sql_source_raises() -> None:
    """A value object has no backing view — ``embedded=True`` + an explicit ``sql_source``
    is a contradiction (the SDK suppresses the source for an embedded type)."""
    with pytest.raises(ValueError, match="backing view"):

        @fraiseql.type(embedded=True, sql_source="v_money")
        class Money:
            amount: int
            currency: str


def test_embedded_with_cascade_raises() -> None:
    """A value object cannot originate a cascade — ``embedded=True`` + ``cascade=True``
    is a contradiction (only a keyed entity mutation may cascade)."""
    with pytest.raises(ValueError, match="cannot originate a cascade"):

        @fraiseql.type(embedded=True, cascade=True)
        class Money:
            amount: int
            currency: str


def test_embedded_value_object_alone_is_accepted() -> None:
    """The non-contradictory form registers cleanly (guards against over-rejection)."""

    @fraiseql.type(embedded=True)
    class Money:
        amount: int
        currency: str

    money = next(t for t in SchemaRegistry.get_schema()["types"] if t["name"] == "Money")
    assert money["embedded"] is True
    assert "sql_source" not in money


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
