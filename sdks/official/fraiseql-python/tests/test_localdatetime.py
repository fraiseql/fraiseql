"""``fraiseql.LocalDateTime``: a date and time with no offset, a PostgreSQL ``timestamp``.

A Python ``datetime`` is naive or aware by its value, not its type, so it stays
``DateTime``; a field whose values carry no offset says so with the marker. Published as
``DateTime``, such a value reads as an instant it never was.
"""

import datetime

import fraiseql
from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import ID, LocalDateTime
from fraiseql.types import python_type_to_graphql


def test_the_marker_is_localdatetime_and_datetime_stays_datetime() -> None:
    assert python_type_to_graphql(LocalDateTime) == ("LocalDateTime", False)
    assert python_type_to_graphql(LocalDateTime | None) == ("LocalDateTime", True)
    assert python_type_to_graphql(list[LocalDateTime]) == ("[LocalDateTime!]", False)
    assert python_type_to_graphql(datetime.datetime) == ("DateTime", False)
    assert fraiseql.LocalDateTime is LocalDateTime


def test_a_localdatetime_field_and_argument_are_the_built_in_scalar() -> None:
    """Exported as ``LocalDateTime``, and never declared as a custom scalar of that name."""
    SchemaRegistry.clear()

    @fraiseql.type(sql_source="v_shift")
    class Shift:
        id: ID
        starts_at: LocalDateTime
        ends_at: LocalDateTime | None
        breaks_at: list[LocalDateTime]

    @fraiseql.query(sql_source="v_shift")
    def shifts(starts_after: LocalDateTime) -> list[Shift]:
        pass

    schema = SchemaRegistry.get_schema()
    fields = {f["name"]: f for f in schema["types"][0]["fields"]}
    assert fields["startsAt"]["type"] == "LocalDateTime"
    assert fields["endsAt"]["type"] == "LocalDateTime"
    assert fields["endsAt"]["nullable"] is True
    assert fields["breaksAt"]["type"] == "[LocalDateTime!]"
    assert schema["queries"][0]["arguments"][0]["type"] == "LocalDateTime"
    assert "LocalDateTime" not in [s["name"] for s in schema.get("custom_scalars", [])]
    SchemaRegistry.clear()
