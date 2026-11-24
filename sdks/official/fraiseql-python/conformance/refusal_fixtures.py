"""The conformance refusals, declared with the Python SDK's public API.

Each function declares one thing the compiled schema has no carrier for; the SDK must
refuse it by name (`sdks/official/conformance/refusals.py`). A second key is not here:
`key_fields` is one field set, so the SDK has no way to write one.
"""

from __future__ import annotations

import fraiseql


def federation_refuses_requires() -> None:
    fraiseql.field(requires="weight")


def federation_refuses_provides() -> None:
    fraiseql.field(provides="name")


def federation_refuses_override() -> None:
    fraiseql.field(override_from="legacy-pricing")
