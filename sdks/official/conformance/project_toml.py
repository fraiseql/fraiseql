"""The `fraiseql.toml` a project compiling an exported schema carries (ruling AF 4).

Shared by the conformance harness (`run.py`) and the examples gate (`check_examples.sh`),
so both compile an export the way a project does.

    python3 project_toml.py <schema.json>   # prints the fraiseql.toml
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def declared_scopes(node: object) -> list[str]:
    """Every `requires_scope` value anywhere in an exported schema, sorted."""
    found: set[str] = set()
    stack = [node]
    while stack:
        item = stack.pop()
        if isinstance(item, dict):
            scope = item.get("requires_scope")
            if isinstance(scope, str) and scope:
                found.add(scope)
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)
    return sorted(found)


def project_toml(schema: Path) -> str:
    """The `fraiseql.toml` a project compiling `schema` carries: a role granting each scope
    the schema declares.

    A scope is granted only by a role the project's security section defines, and a schema
    declaring `requires_scope` with no such section is one no server can load — so the
    compiler, which loads what it writes (ruling AF 4), refuses it. Real projects declare
    their roles in `fraiseql.toml`; the harness does the same, deriving the role from the
    export itself so that a scope an SDK drops or misspells still shows up in the diff of
    observations rather than here.
    """
    scopes = declared_scopes(json.loads(schema.read_text()))
    if not scopes:
        return ""
    listed = ", ".join(json.dumps(s) for s in scopes)
    return (
        "[[fraiseql.security.role_definitions]]\n"
        'name = "conformance"\n'
        'description = "Grants every scope the exported schema declares"\n'
        f"scopes = [{listed}]\n"
    )


if __name__ == "__main__":
    sys.stdout.write(project_toml(Path(sys.argv[1])))
