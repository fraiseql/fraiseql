"""TOML configuration loader for inject_defaults.

Reads ``[inject_defaults]``, ``[inject_defaults.queries]``, and
``[inject_defaults.mutations]`` sections from a ``fraiseql.toml`` file
and applies them to the global :class:`SchemaRegistry`.

The file is parsed as TOML 1.1 by the standard library's ``tomllib`` (Python 3.15), as
``fraiseql-cli compile`` parses it: an inline table may span lines and end with a comma,
a basic string accepts the ``\\e`` and ``\\xHH`` escapes, and a time may omit its seconds.

Example ``fraiseql.toml``::

    [inject_defaults]
    tenant_id = "jwt:tenant_id"

    [inject_defaults.queries]
    read_scope = "jwt:scope"

    [inject_defaults.mutations]
    user_id = "jwt:sub"
"""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Any

from fraiseql.registry import SchemaRegistry


def load_config(toml_path: str | Path) -> dict[str, Any]:
    """Load a ``fraiseql.toml`` file and apply inject_defaults to the registry.

    Args:
        toml_path: Path to the TOML configuration file.

    Returns:
        The parsed TOML document as a dictionary.

    Raises:
        FileNotFoundError: If the file does not exist.
        tomllib.TOMLDecodeError: If the file is not valid TOML.
    """
    path = Path(toml_path)
    with path.open("rb") as f:
        doc = tomllib.load(f)

    inject = doc.get("inject_defaults", {})
    if not isinstance(inject, dict):
        return doc

    # Base defaults (keys at top level of [inject_defaults])
    base: dict[str, str] = {}
    queries: dict[str, str] = {}
    mutations: dict[str, str] = {}

    for key, value in inject.items():
        if isinstance(value, str):
            base[key] = value
        elif isinstance(value, dict) and key == "queries":
            queries = {k: v for k, v in value.items() if isinstance(v, str)}
        elif isinstance(value, dict) and key == "mutations":
            mutations = {k: v for k, v in value.items() if isinstance(v, str)}

    if base or queries or mutations:
        SchemaRegistry.set_inject_defaults(base, queries, mutations)

    return doc
