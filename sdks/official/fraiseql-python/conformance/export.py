#!/usr/bin/env python3
"""Author the cross-SDK conformance fixture with the Python SDK's public API.

Driven by `sdks/official/conformance/run.py`; see `sdks/official/conformance/README.md`
for what the fixture contains and why.

The one rule for every SDK's copy of this file: **author through the SDK, never
hand-assemble the JSON.** Six of the eleven pre-existing parity generators built the
expected bytes by hand and were therefore structurally incapable of failing, which is
how nine export defects shipped at once.
"""

from __future__ import annotations

import importlib
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

import fraiseql  # noqa: E402
from fraiseql.registry import SchemaRegistry  # noqa: E402


def main() -> None:
    fixture = os.environ["FRAISEQL_CONFORMANCE_FIXTURE"]
    out = os.environ["FRAISEQL_CONFORMANCE_OUT"]

    SchemaRegistry.clear()
    # The fixtures are separate modules because Python's decorators resolve return-type
    # annotations against the *module* namespace: a type declared inside a function is
    # invisible to `list[User]`, and the SDK's pre-export validation rejects it. Module
    # scope is also how a user's own `schema.py` is written.
    importlib.import_module(f"conformance.fixture_{fixture}")
    fraiseql.export_schema(out)


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    main()
