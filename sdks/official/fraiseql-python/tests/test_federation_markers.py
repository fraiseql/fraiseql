"""Field-level federation markers reach schema.json in the shape the compiler reads.

`@inaccessible` is a field `directives` entry, which the compiler lifts into the compiled
federation block. `@requires`, `@provides` and `@override` have no carrier in the compiled
schema: the SDK refuses each by name where it is declared, rather than recording a marker
the export would drop.
"""

import json
from pathlib import Path
from typing import Annotated

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import ID


@pytest.fixture(autouse=True)
def _clean_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


@pytest.mark.parametrize(
    ("marker", "kwargs"),
    [
        ("requires", {"requires": "weight"}),
        ("provides", {"provides": "name"}),
        ("override_from", {"override_from": "legacy-pricing"}),
    ],
)
def test_a_marker_the_compiler_cannot_carry_is_refused_by_name(
    marker: str, kwargs: dict[str, str]
) -> None:
    with pytest.raises(ValueError, match=rf"{marker}=.*no carrier in the compiled schema"):
        fraiseql.field(**kwargs)


def test_an_inaccessible_field_is_exported_as_a_field_directive(tmp_path: Path) -> None:
    @fraiseql.type
    class Account:
        id: ID
        internal_code: Annotated[str, fraiseql.field(inaccessible=True)]
        name: str

    @fraiseql.query
    def account(id: ID) -> Account:
        """Get an account."""

    output = tmp_path / "schema.json"
    fraiseql.export_schema(str(output), federation=fraiseql.Federation(service_name="accounts"))

    fields = {
        field["name"]: field
        for field in next(t for t in json.loads(output.read_text())["types"])["fields"]
    }
    assert fields["internalCode"]["directives"] == [{"name": "inaccessible"}]
    assert "directives" not in fields["name"]
    assert all("federation" not in field for field in fields.values())
