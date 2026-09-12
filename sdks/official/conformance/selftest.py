#!/usr/bin/env python3
"""Self-test for the cross-SDK conformance harness (#1118).

`project.py` used to carry, above `CONSTRUCTS`:

> Adding an entry here is how the suite grows: a new construct fails every SDK that
> has not implemented it until each either implements it or declares the gap. That is
> the intended behaviour, and `test_new_construct_fails_every_sdk` in `selftest.py`
> pins it.

There was no `selftest.py`, and `git log --all` for the path was empty — it had never
existed. The property is real (adding `vector_fields` in #959 did fail every SDK until
each implemented it), but nothing pinned it, and the comment named a file as evidence.
The comment has since been corrected to describe the mechanism that does exist; this
file supplies the evidence it originally claimed.

Each test below is a property the suite's own README states as a guarantee. All four
run over synthetic compiled-schema dicts and manifest fragments — **no language
toolchain, no CLI, no network** — so this belongs in a fast gate rather than the
`sdk-conformance.yml` job that shells out to eleven runtimes.

Run directly:  python3 sdks/official/conformance/selftest.py
"""

from __future__ import annotations

import json
import re
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import project  # noqa: E402
import run  # noqa: E402


class NewConstructFailsEverySdk(unittest.TestCase):
    """A construct in `CONSTRUCTS` with no observation raises, rather than passing.

    This is the growth property: adding an entry must fail every SDK that has not
    implemented it. The mechanism is `project()`'s refusal to return an incomplete
    observation set — without it, a new construct would project to nothing, compare
    equal to nothing, and be silently green everywhere on the day it was added.
    """

    def test_a_construct_with_no_observation_raises(self) -> None:
        original = project.CONSTRUCTS
        project.CONSTRUCTS = (*original, "a_construct_nothing_emits")
        try:
            with self.assertRaises(AssertionError) as caught:
                project.project({})
        finally:
            project.CONSTRUCTS = original
        self.assertIn("a_construct_nothing_emits", str(caught.exception))

    def test_the_shipped_construct_list_is_complete(self) -> None:
        """...and the list as it stands satisfies its own rule.

        Without this, the test above would keep passing while the real
        `CONSTRUCTS` had drifted away from what `project()` emits.
        """
        observations = project.project({})
        self.assertEqual(set(project.CONSTRUCTS) - set(observations), set())


class ADeclaredGapDropsTheConstruct(unittest.TestCase):
    """A construct in a manifest's `unsupported` map is skipped for that SDK only."""

    EXPECTED = {"queries": ["users"], "enums": ["OrderStatus"]}
    ACTUAL_MISSING_ENUMS = {"queries": ["users"], "enums": []}

    def test_an_undeclared_difference_is_reported(self) -> None:
        problems = run.diff_observations(self.EXPECTED, self.ACTUAL_MISSING_ENUMS, {})
        self.assertTrue(any("`enums`" in p for p in problems), problems)

    def test_a_declared_gap_is_skipped(self) -> None:
        problems = run.diff_observations(
            self.EXPECTED, self.ACTUAL_MISSING_ENUMS, {"enums": "no enum surface yet"}
        )
        self.assertEqual(problems, [])

    def test_the_gap_is_scoped_to_the_construct_it_names(self) -> None:
        """Declaring one gap must not suppress a different construct's failure.

        A skip that leaked across constructs would turn one honest declaration into
        blanket immunity — the shape that makes a support matrix a fiction.
        """
        actual = {"queries": [], "enums": []}
        problems = run.diff_observations(
            self.EXPECTED, actual, {"enums": "no enum surface yet"}
        )
        self.assertTrue(any("`queries`" in p for p in problems), problems)
        self.assertFalse(any("`enums`" in p for p in problems), problems)


class AStaleGapIsReported(unittest.TestCase):
    """`check_undeclared_support` catches a declaration that is no longer true.

    The README's "the matrix cannot go stale into a published falsehood" property,
    which until now was asserted only in prose. A stale entry un-gates a construct
    that works today and could regress tomorrow, while publishing that the SDK does
    not support it.
    """

    def test_a_declaration_that_is_no_longer_true_is_reported(self) -> None:
        expected = {"enums": ["OrderStatus"]}
        problems = run.check_undeclared_support(
            expected, expected, {"enums": "no enum surface yet"}
        )
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("`enums`", problems[0])

    def test_a_genuine_gap_is_not_reported(self) -> None:
        problems = run.check_undeclared_support(
            {"enums": ["OrderStatus"]}, {"enums": []}, {"enums": "no enum surface yet"}
        )
        self.assertEqual(problems, [])

    def test_an_unexercised_construct_cannot_look_stale(self) -> None:
        """An empty expectation matches an empty result for reasons unrelated to support.

        The `minimal` fixture leaves most constructs empty on purpose, so without
        `exercised()` every declared gap would be reported stale against it — a false
        positive that would push maintainers to delete honest declarations.
        """
        problems = run.check_undeclared_support(
            {"enums": []}, {"enums": []}, {"enums": "no enum surface yet"}
        )
        self.assertEqual(problems, [])

    def test_type_relay_counts_as_exercised_only_when_something_is_set(self) -> None:
        """`type_relay` is a dict of sub-observations that is never empty."""
        self.assertFalse(run.exercised({"connection": None, "edge": None}))
        self.assertTrue(run.exercised({"connection": "UserConnection", "edge": None}))


class ACasingNearMissIsNamed(unittest.TestCase):
    """An operation published under another convention is reported as that, not as absent.

    `project()` indexes the operation constructs by the names the canonical fixture
    declares, so an SDK publishing `tenant_orders` where the fixture expects
    `tenantOrders` yields an observation with the key missing entirely. Without the note
    the diff reads "this SDK registered no such query", and the reader goes looking for a
    registration that is in fact there under another spelling — a detour this cost twice
    before it was added (#1255).
    """

    EXPECTED = {"queries": {"tenantOrders": {"sql_source": "v_order"}}}
    ACTUAL = {"queries": {}}

    def test_the_note_names_both_spellings(self) -> None:
        compiled = {"queries": [{"name": "tenant_orders", "sql_source": "v_order"}]}
        problems = run.diff_observations(self.EXPECTED, self.ACTUAL, {}, compiled)
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("`tenantOrders` published as `tenant_orders`", problems[0])

    def test_a_genuinely_absent_operation_gets_no_note(self) -> None:
        """The note must not fire when nothing was published under any spelling.

        Claiming a convention mismatch where the operation is simply missing would send
        the reader after a rename that never happened — the same failure one level over.
        """
        problems = run.diff_observations(self.EXPECTED, self.ACTUAL, {}, {"queries": []})
        self.assertEqual(len(problems), 1, problems)
        self.assertNotIn("different convention", problems[0])

    def test_the_note_is_diagnostic_only(self) -> None:
        """It may never change whether a construct is reported, only what the report says.

        Same expected/actual, with and without the compiled schema: one problem either
        way. A hint that could suppress a finding would be a gate that fails to fail.
        """
        compiled = {"queries": [{"name": "tenant_orders"}]}
        with_hint = run.diff_observations(self.EXPECTED, self.ACTUAL, {}, compiled)
        without = run.diff_observations(self.EXPECTED, self.ACTUAL, {}, None)
        self.assertEqual(len(with_hint), len(without), (with_hint, without))

    def test_an_exact_match_is_not_reported_as_a_near_miss(self) -> None:
        """A name published exactly as expected is not a near-miss of itself."""
        actual = {"queries": {"tenantOrders": {"sql_source": "v_user"}}}
        compiled = {"queries": [{"name": "tenantOrders", "sql_source": "v_user"}]}
        problems = run.diff_observations(self.EXPECTED, actual, {}, compiled)
        self.assertEqual(len(problems), 1, problems)
        self.assertNotIn("different convention", problems[0])


class AnUnknownUnsupportedKeyFails(unittest.TestCase):
    """A typo in a manifest's `unsupported` map is reported, not silently ignored.

    `check_sdk` implements this; nothing tested it. A misspelled key disables no gate
    while reading as a declared gap, so the construct is neither tested nor reported
    as missing — the worst of both.
    """

    def test_an_unknown_construct_key_is_reported(self) -> None:
        failures = run.check_sdk(
            "demo",
            {"unsupported": {"enumz": "typo for enums"}},
            Path("/nonexistent/fraiseql-cli"),
            {},
        )
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("enumz", failures[0])
        self.assertIn("project.CONSTRUCTS", failures[0])

    def test_a_known_construct_key_is_not_reported_as_unknown(self) -> None:
        """The negative direction: a real key must reach the run, not be rejected here.

        A false positive would block a legitimate declaration, so it needs its own
        assertion. The guard returns EARLY, before any exporter runs — so proving the
        key was accepted means proving execution got past it. With a deliberately
        incomplete spec (no `dir`), that shows up as a `KeyError` from `run_exporter`:
        an exception here is the evidence, and an early return would produce none.
        """
        with self.assertRaises(KeyError):
            run.check_sdk(
                "demo",
                {"unsupported": {"enums": "no enum surface yet"}},
                Path("/nonexistent/fraiseql-cli"),
                {},
            )


class SupportMatrixMatchesTheManifest(unittest.TestCase):
    """`sdks/official/README.md`'s table agrees with `manifest.json` and `CONSTRUCTS`.

    The conformance README says the declared gap reasons *are* the support matrix in
    `../README.md`, which makes the table a published claim about eleven SDKs. Nothing
    checked it, and it had drifted twice by the time #1266 arrived: every row read
    `N/19` while the fixture carried 23 constructs, and Go's `type_crud` gap was absent
    from the table entirely — so a reader was told Go implements a construct it declares
    it does not.

    The score is the only part checked mechanically. The reason text is prose and stays
    a human's job; what cannot be allowed to rot silently is the arithmetic, which
    changes every single time a construct is added.
    """

    ROW = re.compile(
        r"^\|\s*`fraiseql-(?P<sdk>[a-z]+)/`\s*\|[^|]*\|[^|]*\|\s*"
        r"(?P<satisfied>\d+)/(?P<total>\d+)\s*\|",
        re.M,
    )

    def test_every_row_matches(self) -> None:
        manifest = json.loads((HERE / "manifest.json").read_text())["sdks"]
        readme = (HERE.parent / "README.md").read_text()

        rows = {m["sdk"]: m for m in self.ROW.finditer(readme)}
        self.assertEqual(
            sorted(rows), sorted(manifest),
            "the support matrix and the manifest list different SDKs",
        )

        total = len(project.CONSTRUCTS)
        for sdk, spec in manifest.items():
            expected = total - len(spec.get("unsupported", {}))
            row = rows[sdk]
            self.assertEqual(
                (int(row["satisfied"]), int(row["total"])),
                (expected, total),
                f"{sdk}: README says {row['satisfied']}/{row['total']}, manifest and "
                f"CONSTRUCTS say {expected}/{total}",
            )


NUMBER_WORDS = {
    1: "one", 2: "two", 3: "three", 4: "four", 5: "five", 6: "six",
    7: "seven", 8: "eight", 9: "nine", 10: "ten", 11: "eleven", 12: "twelve",
}


class CommunityReadmeMatchesTheDirectory(unittest.TestCase):
    """`sdks/community/README.md`'s count, table and dedup note agree with the tree.

    The community tier is unmaintained and untested, so nothing there fails when a
    directory is deleted — and two were. The README went on claiming **nine** SDKs and
    listing `fraiseql-dart` and `fraiseql-elixir` in its table, while its "Deduplication
    Note (AB2)" named three SDKs as duplicated in `official/` when only one was: three
    disagreements with the tree in one file (#1318).

    That is the same rot `SupportMatrixMatchesTheManifest` was written for one directory
    over, and the same fix: the arithmetic is checked, the prose stays a human's job.
    The tier is scheduled for removal in v3.0.0 and this check is deleted with it.
    """

    COMMUNITY = HERE.parent.parent / "community"
    OFFICIAL = HERE.parent
    ROW = re.compile(r"^\|\s*`(?P<sdk>fraiseql-[a-z0-9]+)`\s*\|", re.M)
    COUNT = re.compile(r"contains \*\*(?P<word>[a-z]+) community-contributed SDKs\*\*")
    DEDUP = re.compile(
        r"^(?P<word>\w+) SDKs? in this directory \((?P<names>[^)]*)\) also\s*\n?exists?",
        re.M,
    )

    @staticmethod
    def _dirs(root: Path) -> set[str]:
        return {d.name for d in root.iterdir() if d.is_dir() and d.name.startswith("fraiseql-")}

    def setUp(self) -> None:
        if not self.COMMUNITY.is_dir():
            # The v3.0.0 removal happened. This check has no subject left; delete it
            # rather than letting it raise a FileNotFoundError nobody can read.
            self.fail(
                "sdks/community/ is gone — the tier was removed. Delete "
                "CommunityReadmeMatchesTheDirectory; it exists only to keep that "
                "directory's README honest while it lives."
            )
        self.readme = (self.COMMUNITY / "README.md").read_text()
        self.community = self._dirs(self.COMMUNITY)

    def test_table_lists_exactly_the_directories(self) -> None:
        listed = {m["sdk"] for m in self.ROW.finditer(self.readme)}
        self.assertEqual(
            listed, self.community,
            "the SDK List table and sdks/community/ disagree: "
            f"table-only={sorted(listed - self.community)} "
            f"tree-only={sorted(self.community - listed)}",
        )

    def test_headline_count_matches(self) -> None:
        m = self.COUNT.search(self.readme)
        self.assertIsNotNone(m, "README no longer states a community SDK count")
        expected = NUMBER_WORDS[len(self.community)]
        self.assertEqual(
            m["word"], expected,
            f"README says {m['word']} community SDKs, the tree has {expected} "
            f"({len(self.community)})",
        )

    def test_dedup_note_names_the_actual_overlap(self) -> None:
        overlap = self.community & self._dirs(self.OFFICIAL)
        m = self.DEDUP.search(self.readme)
        self.assertIsNotNone(m, "README no longer carries a deduplication note")
        named = set(re.findall(r"`(fraiseql-[a-z0-9]+)`", m["names"]))
        self.assertEqual(
            named, overlap,
            f"the dedup note names {sorted(named)}, the tree overlaps on {sorted(overlap)}",
        )
        self.assertEqual(
            m["word"].lower(), NUMBER_WORDS[len(overlap)],
            f"the dedup note counts '{m['word']}', the tree overlaps on {len(overlap)}",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
