"""snake_case → camelCase surface naming, including digit boundaries.

Mirrors FraiseQL v1 + the engine's canonical `to_camel_case`: a digit segment
collapses onto the previous word (`phone_1` → `phone1`). The Rust runtime's
`to_snake_case` reinserts the boundary so the round trip is bijective.
"""

import pytest

from fraiseql.registry import SchemaRegistry, _snake_to_camel


@pytest.mark.parametrize(
    ("snake", "expected"),
    [
        # The bug: digit segments must collapse, not stay snake.
        ("phone_1", "phone1"),
        ("phone_2", "phone2"),
        ("address_2", "address2"),
        ("dns_1_id", "dns1Id"),
        ("line_2_content", "line2Content"),
        # Regression guard for ordinary fields.
        ("user_id", "userId"),
        ("http_response", "httpResponse"),
        ("created_at_timestamp", "createdAtTimestamp"),
        # Idempotent on already-camel input.
        ("already", "already"),
        ("alreadyCamel", "alreadyCamel"),
    ],
)
def test_snake_to_camel_digit_boundaries(snake: str, expected: str) -> None:
    assert _snake_to_camel(snake) == expected


def test_build_field_def_camelizes_digit_field_name() -> None:
    """The field `name` emitted into schema.json collapses digit segments."""
    field = SchemaRegistry._build_field_def("phone_1", {"type": "String", "nullable": True})
    assert field["name"] == "phone1"


def test_repro_issue_field_surface() -> None:
    """The original report: phone_1/phone_2 must camelize like every other field."""
    names = [
        SchemaRegistry._build_field_def(n, {"type": "String", "nullable": True})["name"]
        for n in ("phone_1", "phone_2", "user_id", "http_response")
    ]
    assert names == ["phone1", "phone2", "userId", "httpResponse"]
