"""Tests for the ``timezone`` of a ``cron:`` function (#1658)."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def _clear_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def test_a_cron_function_carries_its_timezone():
    @fraiseql.function(trigger="cron:30 2 * * *", timezone="Europe/Paris")
    def nightly_report() -> None:
        pass

    function = SchemaRegistry.get_schema()["functions"][0]
    assert function["timezone"] == "Europe/Paris"


def test_a_function_without_a_timezone_omits_it():
    @fraiseql.function(trigger="cron:30 2 * * *")
    def nightly_report() -> None:
        pass

    assert "timezone" not in SchemaRegistry.get_schema()["functions"][0]
