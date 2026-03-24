"""TOML configuration loader for inject_defaults.

Reads ``[inject_defaults]``, ``[inject_defaults.queries]``, and
``[inject_defaults.mutations]`` sections from a ``fraiseql.toml`` file
and applies them to the global :class:`SchemaRegistry`.

Requires Python 3.11+ (``tomllib``) or the ``tomli`` backport for 3.10.

Example ``fraiseql.toml``::

    [inject_defaults]
    tenant_id = "jwt:tenant_id"

    [inject_defaults.queries]
    read_scope = "jwt:scope"

    [inject_defaults.mutations]
    user_id = "jwt:sub"
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

from fraiseql.registry import SchemaRegistry

if sys.version_info >= (3, 11):
    import tomllib
else:
    try:
        import tomli as tomllib  # type: ignore[no-redef]
    except ModuleNotFoundError as exc:
        msg = (
            "Python 3.10 requires the 'tomli' package to load TOML config. "
            "Install it with: pip install tomli"
        )
        raise ImportError(msg) from exc


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
