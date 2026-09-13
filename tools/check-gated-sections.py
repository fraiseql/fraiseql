#!/usr/bin/env python3
"""Assert every compiled-schema section is classified as servable-everywhere or feature-gated (#1326).

WHY THIS GATE EXISTS
--------------------
`functions` was loaded, validated, and then silently discarded by any build without
`functions-runtime` — which is every published image, since `docker-build.yml` builds
`rest,arrow`. The server booted clean and every declared function never fired. #1326 is
that defect; the fix is `refuse_unservable_sections` in
`crates/fraiseql-server/src/schema/loader.rs`, which refuses a section this build cannot
serve instead of dropping it.

A refusal list is only worth having while nothing can be added outside it. The failure
mode this gate prevents is the original one repeating: someone adds a top-level section
to `CompiledSchema`, wires its only consumer behind a Cargo feature, and a lean build
goes back to dropping it in silence. Nothing about that is visible in review — the
silent-drop is the *absence* of code.

WHAT IT CHECKS
--------------
Discovery is the wire-visible field set of `CompiledSchema` (every `pub` field not
carrying `#[serde(skip)]`), plus the keys `load_extended` handles that are not
`CompiledSchema` fields at all (`functions`, `storage`, `realtime`).

Every discovered key must be *classified*, in one of two places:

  - named in `gated_sections()` in the server's loader — a section only a feature-gated
    subsystem can serve, refused when that feature is absent; or
  - named in SERVABLE_EVERYWHERE below, with the reason it needs no gate.

A key in neither fails this gate. That is deliberately a human decision rather than an
inferred one: whether a section's consumers are all feature-gated is a question about
call graphs and `cfg` arms that a regex answers badly, and answering it badly in the
permissive direction reintroduces exactly the defect. So the gate does not guess — it
refuses to let a new section pass unclassified.

It is also checked in the other direction: a classified key that no longer exists is a
stale entry, and a stale entry silently stops covering anything.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

# Sections whose consumers are compiled into every build, so a declaration can always be
# served and no refusal applies. The reason is recorded because "it needs no gate" is a
# claim about where the code runs, and a later reader has to be able to re-check it.
SERVABLE_EVERYWHERE: dict[str, str] = {
    "types": "the GraphQL type system — the executor's core, ungated",
    "enums": "type system",
    "input_types": "type system",
    "interfaces": "type system",
    "unions": "type system",
    "queries": "read dispatch, ungated",
    "mutations": "write dispatch, ungated",
    "subscriptions": "the subscription manager is built in every server",
    "directives": "type system",
    "fact_tables": "aggregate/window dispatch in fraiseql-core, ungated",
    "observers": (
        "observer DEFINITIONS are matched by the change-log reader in core; the "
        "`observers` feature gates the action transports, which `observers_config` covers"
    ),
    "subscribable": "read by the subscription manager, ungated",
    "operation_cost_weights": "the #379 cost estimator in fraiseql-core, ungated",
    "security": "field filters, RLS, role and actor gates all live in fraiseql-core",
    "auth": "`auth` is a default feature; a build without it mounts no auth surface at all",
    "subscriptions_config": "read by the subscription manager, ungated",
    "validation_config": "the #379 executor gate in fraiseql-core, ungated",
    "debug_config": "read by AppState in every build",
    "changelog": "the Change-Spine outbox write lives in fraiseql-core, ungated",
    "session_variables": "resolved in fraiseql-core on every request",
    "hierarchies_config": "hierarchy resolution in fraiseql-core, ungated",
    "naming_convention": "casing resolution in fraiseql-core, ungated",
    "naming_acronyms": "casing resolution in fraiseql-core, ungated",
    "fraiseql_version": "the #1304 build-identity stamp, checked in every build",
    "schema_sdl": "served by introspection, ungated",
    # Handled by `load_extended` with their own posture, both already pinned by tests.
    "storage": "refused outright (#1008) — configured in the server config file, not here",
    "realtime": "warned and ignored (#605) — the subsystem was removed",
}

COMPILED_SCHEMA = Path("crates/fraiseql-core/src/schema/compiled/schema.rs")
LOADER = Path("crates/fraiseql-server/src/schema/loader.rs")

# Keys `load_extended` reads that are not `CompiledSchema` fields.
LOADER_ONLY = {"functions", "storage", "realtime"}


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def wire_keys(src: str) -> set[str]:
    """Every `pub` field of `CompiledSchema` that is not `#[serde(skip)]`."""
    start = src.index("pub struct CompiledSchema {")
    depth = 0
    end = len(src)
    for i in range(start, len(src)):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    body = src[start:end]
    keys = set()
    for m in re.finditer(r"((?:\s*#\[[^\]]*\]\s*)*)\s+pub\s+([a-z_][a-z0-9_]*)\s*:", body):
        attrs, name = m.group(1), m.group(2)
        if re.search(r"serde\(\s*skip\s*\)", attrs):
            continue
        keys.add(name)
    return keys


def gated_names(src: str) -> set[str]:
    """The `name:` values inside `gated_sections()`."""
    start = src.index("pub(crate) fn gated_sections()")
    end = src.index("\n}\n", start)
    return set(re.findall(r'name:\s*"([^"]+)"', src[start:end]))


def main() -> int:
    root = repo_root()
    schema_src = (root / COMPILED_SCHEMA).read_text(encoding="utf-8")
    loader_src = (root / LOADER).read_text(encoding="utf-8")

    discovered = wire_keys(schema_src) | LOADER_ONLY
    gated = gated_names(loader_src)
    classified = gated | set(SERVABLE_EVERYWHERE)

    failures: list[str] = []

    unclassified = sorted(discovered - classified)
    if unclassified:
        failures.append(
            "compiled-schema section(s) with no classification: "
            + ", ".join(unclassified)
            + "\n\n"
            "  Decide, for each, whether a build can always serve it:\n"
            "    - only a feature-gated subsystem serves it → add it to `gated_sections()` in\n"
            f"      {LOADER}, with the feature, an activity predicate and a remedy;\n"
            "    - every build serves it → add it to SERVABLE_EVERYWHERE in this file, with the\n"
            "      reason.\n"
            "  Leaving it unclassified is how #1326 happened: the section loads, validates, and\n"
            "  a lean build drops it in silence."
        )

    stale = sorted(classified - discovered)
    if stale:
        failures.append(
            "classified section(s) that no longer exist: "
            + ", ".join(stale)
            + "\n  A stale entry covers nothing. Remove it from `gated_sections()` or from\n"
            "  SERVABLE_EVERYWHERE."
        )

    overlap = sorted(gated & set(SERVABLE_EVERYWHERE))
    if overlap:
        failures.append(
            "section(s) classified BOTH ways: "
            + ", ".join(overlap)
            + "\n  A section is either always servable or feature-gated, not both — and the\n"
            "  permissive entry would win by accident."
        )

    if failures:
        print("ERROR: compiled-schema sections are not fully classified (#1326):\n")
        for f in failures:
            print("  " + f + "\n")
        return 1

    print(
        f"OK: all {len(discovered)} compiled-schema sections are classified "
        f"({len(gated)} feature-gated and refused when absent, "
        f"{len(SERVABLE_EVERYWHERE)} servable in every build)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
