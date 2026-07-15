"""Tests for @fraiseql.source scheduled-ingress authoring (#573)."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def _clear_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def test_source_registers_with_defaults():
    @fraiseql.source(schedule="*/5 * * * *")
    def poll_orders() -> None:
        pass

    schema = SchemaRegistry.get_schema()
    assert schema["sources"] == [
        {
            "name": "pollOrders",  # camelCased like other SDK names
            "schedule": "*/5 * * * *",
            "function": "pollOrders",  # defaults to the source name
            "enabled": True,
        }
    ]


def test_source_carries_run_as_cursor_and_explicit_function():
    @fraiseql.source(
        schedule="0 * * * *",
        function="stripePull",
        cursor="stripe-cursor",
        enabled=False,
        run_as={"roles": ["ingest_writer"], "scopes": ["write:order"], "tenant": "acme"},
    )
    def stripe_sync() -> None:
        pass

    source = SchemaRegistry.get_schema()["sources"][0]
    assert source == {
        "name": "stripeSync",
        "schedule": "0 * * * *",
        "function": "stripePull",
        "cursor": "stripe-cursor",
        "enabled": False,
        "run_as": {"roles": ["ingest_writer"], "scopes": ["write:order"], "tenant": "acme"},
    }


def test_sources_omitted_when_none_registered():
    assert "sources" not in SchemaRegistry.get_schema()


def test_duplicate_source_name_raises():
    @fraiseql.source(schedule="*/5 * * * *")
    def poll_orders() -> None:
        pass

    with pytest.raises(ValueError, match="already registered"):

        @fraiseql.source(schedule="0 * * * *")
        def poll_orders() -> None:
            pass
