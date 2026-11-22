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


def declares_localized(node: object) -> bool:
    """Whether an exported schema marks anything `localized` (#1513)."""
    stack = [node]
    while stack:
        item = stack.pop()
        if isinstance(item, dict):
            if item.get("localized") is True:
                return True
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)
    return False


def linked_hierarchies(node: object) -> list[str]:
    """Every hierarchy name a field or fact-table filter of an exported schema links (#1909),
    sorted."""
    found: set[str] = set()
    stack = [node]
    while stack:
        item = stack.pop()
        if isinstance(item, dict):
            name = item.get("hierarchy")
            if isinstance(name, str) and name:
                found.add(name)
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)
    return sorted(found)


def hierarchies_section(names: list[str]) -> str:
    """A `[hierarchies.<name>]` table for each name: an SDK names a hierarchy and never
    defines it, so the project config declaring it is the only home it has."""
    return "".join(
        f"[hierarchies.{json.dumps(name)}]\n"
        f'table = "tb_{name}"\npath_column = "path"\n\n'
        for name in names
    )


def declares_observers(exported: object) -> bool:
    """Whether an exported schema declares at least one observer."""
    return isinstance(exported, dict) and bool(exported.get("observers"))


LOCALE_SECTION = '[locale]\ndefault = "en-US"\nallowed = ["en-US", "fr-FR"]\n'
# Declared observers are stored under the project's name, so the compiler refuses a
# schema that declares one where the project writes none.
OBSERVERS_SECTION = '[project]\nname = "conformance"\n\n[observers]\nenabled = true\n'


def project_toml(schema: Path) -> str:
    """The `fraiseql.toml` a project compiling `schema` carries: a role granting each scope
    the schema declares, a `[locale]` when it declares a localized field, and a
    `[project] name` with an enabled `[observers]` when it declares observers, and a
    `[hierarchies.<name>]` for each hierarchy it links.

    A scope is granted only by a role the project's security section defines, and a schema
    declaring `requires_scope` with no such section is one no server can load — so the
    compiler, which loads what it writes (ruling AF 4), refuses it. Real projects declare
    their roles in `fraiseql.toml`; the harness does the same, deriving the role from the
    export itself so that a scope an SDK drops or misspells still shows up in the diff of
    observations rather than here.
    """
    exported = json.loads(schema.read_text())
    scopes = declared_scopes(exported)
    # A localized field needs a `[locale]` to resolve through, or the compiler refuses it;
    # derived from the export for the same reason as the roles below.
    locale = LOCALE_SECTION if declares_localized(exported) else ""
    # Declared observers fire only where the observer runtime is enabled, under a project
    # name, and the compiler refuses them otherwise. Top-level tables go before the role array so neither is read
    # as a key of it.
    observers = OBSERVERS_SECTION if declares_observers(exported) else ""
    # A linked hierarchy must be declared, or the compiler refuses the link; derived from
    # the export for the reason the roles are, so a misspelled name reaches the diff.
    hierarchies = hierarchies_section(linked_hierarchies(exported))
    if not scopes:
        return locale + observers + hierarchies
    listed = ", ".join(json.dumps(s) for s in scopes)
    return locale + observers + hierarchies + (
        "[[security.role_definitions]]\n"
        'name = "conformance"\n'
        'description = "Grants every scope the exported schema declares"\n'
        f"scopes = [{listed}]\n"
    )


if __name__ == "__main__":
    sys.stdout.write(project_toml(Path(sys.argv[1])))
