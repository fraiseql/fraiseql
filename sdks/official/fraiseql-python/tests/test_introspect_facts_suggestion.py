"""The Python suggestion `fraiseql-cli introspect facts` prints runs against this SDK.

The CLI's suite asserts that it prints exactly `fixtures/introspect_facts_suggestion.txt`
for a table holding every shape it reports, and that the entry in
`fixtures/introspect_facts_registered.json` compiles back to that table. This side runs
the suggestion and checks that it registers that entry.
"""

import json
from pathlib import Path

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry

FIXTURES = Path(__file__).parent / "fixtures"


@pytest.fixture(autouse=True)
def _clear_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def test_the_introspect_facts_suggestion_registers_the_introspected_table():
    suggestion = (FIXTURES / "introspect_facts_suggestion.txt").read_text()

    exec(suggestion, {"fraiseql": fraiseql})

    registered = json.loads((FIXTURES / "introspect_facts_registered.json").read_text())
    assert SchemaRegistry.get_schema()["fact_tables"] == [registered]
