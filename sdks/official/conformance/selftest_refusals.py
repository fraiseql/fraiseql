"""Self-test for the refusal half of the conformance harness (`refusals.py`).

Run through `selftest.py`, which imports these cases: synthetic exporters, no language
toolchain, no CLI.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import refusals  # noqa: E402

REQUIRES = "federation_refuses_requires"


class ExporterFailed(Exception):
    """Stands in for `run.ConformanceFailure`."""


def exporter(outputs: dict[str, str | None]):
    """An exporter that succeeds for a name mapped to `None`, else fails with its text."""

    def export(name: str, _out: Path) -> object:
        text = outputs.get(name)
        if text is None:
            return {}
        raise ExporterFailed(text)

    return export


def refusing_everything() -> dict[str, str | None]:
    return {
        "federation_refuses_requires": "ValueError: @requires has no carrier in the compiled schema",
        "federation_refuses_provides": "Error: @Provides: no carrier in the compiled schema",
        "federation_refuses_override": "ValueError: @override has no carrier in the compiled schema",
        "federation_refuses_second_key": "Error: @Key on type A: one key per entity",
    }


class RefusalsAreGated(unittest.TestCase):
    def check(self, outputs: dict[str, str | None], unsupported: dict[str, str]) -> list[str]:
        return refusals.check_refusals(unsupported, exporter(outputs), ExporterFailed)

    def test_a_refusal_by_name_passes(self) -> None:
        self.assertEqual(self.check(refusing_everything(), {}), [])

    def test_an_export_that_succeeds_fails(self) -> None:
        outputs = {**refusing_everything(), REQUIRES: None}
        problems = self.check(outputs, {})
        self.assertEqual(len(problems), 1)
        self.assertIn(REQUIRES, problems[0])

    def test_a_failure_that_names_something_else_fails(self) -> None:
        outputs = {**refusing_everything(), REQUIRES: "error: unknown fixture"}
        problems = self.check(outputs, {})
        self.assertEqual(len(problems), 1)
        self.assertIn(REQUIRES, problems[0])

    def test_a_declared_gap_that_is_refused_by_name_is_stale(self) -> None:
        problems = self.check(refusing_everything(), {REQUIRES: "no federation surface"})
        self.assertEqual(len(problems), 1)
        self.assertIn("declared unsupported", problems[0])

    def test_a_genuine_gap_is_not_reported(self) -> None:
        outputs = {**refusing_everything(), REQUIRES: None}
        self.assertEqual(self.check(outputs, {REQUIRES: "no federation surface"}), [])


if __name__ == "__main__":
    unittest.main()
