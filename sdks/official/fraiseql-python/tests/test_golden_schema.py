"""Compare Python SDK-generated schema against golden fixtures."""

import json
import pathlib

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry

GOLDEN_DIR = pathlib.Path(__file__).parents[2] / "tests" / "fixtures" / "golden"


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test."""
    SchemaRegistry.clear()


def _normalize(obj: object) -> object:
    """Recursively sort dict keys for order-independent comparison."""
    if isinstance(obj, dict):
        return {k: _normalize(v) for k, v in sorted(obj.items())}
    if isinstance(obj, list):
        return [_normalize(item) for item in obj]
    return obj


def test_golden_python_sdk_01_basic_query_mutation() -> None:
    """Python SDK must produce JSON matching python-sdk-01-basic.json golden fixture."""

    @fraiseql.type
    class User:
        """A user in the system."""

        id: int
        email: str

    @fraiseql.query(sql_source="v_user", description="List all users", auto_params=True)
    def users(limit: int = 10) -> list[User]:
        pass

    @fraiseql.mutation(
        sql_source="fn_create_user",
        description="Create a new user",
        operation="insert",
    )
    def create_user(email: str, name: str) -> User:
        pass

    generated = SchemaRegistry.get_schema()
    golden_path = GOLDEN_DIR / "python-sdk-01-basic.json"
    golden = json.loads(golden_path.read_text())

    assert _normalize(generated["types"]) == _normalize(golden["types"])
    assert _normalize(generated["queries"]) == _normalize(golden["queries"])
    assert _normalize(generated["mutations"]) == _normalize(golden["mutations"])


def test_golden_python_sdk_02_embedded() -> None:
    """Python SDK must emit an embedded value object (#687) matching the golden fixture.

    ``Money`` is embedded under an ``Order`` returned by a ``cascade=True`` mutation. The
    fixture pins the authoring half of (a): ``Money`` carries ``"embedded": true`` and — the
    load-bearing part — declares **no** ``sql_source`` (the synthesized ``v_money`` is exactly
    what would misclassify it as a cascade entity). The compile half (this same shape compiles
    clean, ``Money`` is not a ``CascadeNode``) is proven separately by the Rust convert e2e.
    """

    @fraiseql.type(embedded=True)
    class Money:
        """A monetary amount embedded on an order — no independent identity."""

        amount: int
        currency: str

    @fraiseql.type(sql_source="v_order")
    class Order:
        """An order whose total is an embedded Money value object."""

        id: str
        total: Money

    @fraiseql.mutation(sql_source="fn_create_order", operation="insert", cascade=True)
    def create_order(reference: str) -> Order:
        """Create an order (returns a cascade payload)."""

    generated = SchemaRegistry.get_schema()
    golden_path = GOLDEN_DIR / "python-sdk-02-embedded.json"
    golden = json.loads(golden_path.read_text())

    assert _normalize(generated["types"]) == _normalize(golden["types"])
    assert _normalize(generated["mutations"]) == _normalize(golden["mutations"])

    # The point of the fixture: Money is emitted embedded, with no synthesized source.
    money = next(t for t in generated["types"] if t["name"] == "Money")
    assert money["embedded"] is True
    assert "sql_source" not in money
