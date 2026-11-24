"""Declarations an SDK must refuse by name, rather than export.

A construct in `project.CONSTRUCTS` is something the compiled schema carries, observed
after the real compile. A refusal is the other half of that contract: a declaration the
compiled schema has no carrier for. An SDK that records it would write something the
compiler refuses as an unknown key, or drop on the way to the compiled schema, so the
SDK refuses it where it is declared, by name.

For each refusal the harness runs the SDK's exporter with
`FRAISEQL_CONFORMANCE_FIXTURE` set to the refusal's name. The exporter declares that one
thing through the SDK's public API, so the SDK must refuse it. The run must exit non-zero, and the
output must name what was refused, matched by the refusal's pattern. Exiting 0, or
failing for any other reason, is a failure.

A refusal counts in the support matrix like a construct, and a gap is declared the same
way, in the manifest's `unsupported` map. A declaration that is no longer true fails
here as it does for a construct: the exporter is run for it, and a refusal by name
reports the declaration stale.
"""

from __future__ import annotations

import re
import tempfile
from collections.abc import Callable
from pathlib import Path

# The pattern the refusal's output must match: the marker's name, and the reason the SDK
# gives for refusing it.
REFUSALS: dict[str, re.Pattern[str]] = {
    "federation_refuses_requires": re.compile(r"(?i)@requires\b.*no carrier in the compiled schema"),
    "federation_refuses_provides": re.compile(r"(?i)@provides\b.*no carrier in the compiled schema"),
    "federation_refuses_override": re.compile(r"(?i)@override\b.*no carrier in the compiled schema"),
    "federation_refuses_second_key": re.compile(r"(?i)@key\b.*one key per entity"),
}

# Runs one SDK's exporter for a fixture name, writing to the path, and returns its
# document; raises (with the exporter's output in the message) when it exits non-zero.
Exporter = Callable[[str, Path], object]


def refusal_outcome(name: str, export: Exporter, failure: type[Exception]) -> str | None:
    """The run's output when it refused `name` by name, else `None`.

    `failure` is the exception `export` raises for a non-zero exit, carrying its output.
    """
    with tempfile.TemporaryDirectory() as tmp:
        try:
            export(name, Path(tmp) / f"{name}.json")
        except failure as exc:
            return str(exc) if REFUSALS[name].search(str(exc)) else None
    return None


def check_refusals(
    unsupported: dict[str, str], export: Exporter, failure: type[Exception]
) -> list[str]:
    """Failure reports for one SDK's refusals, and for its stale refusal gaps."""
    problems = []
    for name, pattern in REFUSALS.items():
        refused = refusal_outcome(name, export, failure)
        if name in unsupported:
            if refused is not None:
                problems.append(
                    f"  refusal `{name}` is declared unsupported ({unsupported[name]!r}) "
                    f"but the SDK refuses it by name. Remove the declaration so the "
                    f"refusal is gated."
                )
        elif refused is None:
            problems.append(
                f"  refusal `{name}`: the exporter did not refuse the declaration by name "
                f"(want output matching {pattern.pattern!r}). An export that succeeds, or "
                f"fails for another reason, records or drops what the compiled schema "
                f"cannot carry."
            )
    return problems
