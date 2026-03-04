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
