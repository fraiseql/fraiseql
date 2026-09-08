#!/usr/bin/env python3
"""GitHub's push ref-filter rule has exactly one implementation.

`branches`/`branches-ignore` define the BRANCH half of a workflow's `push:` ref
filter and `tags`/`tags-ignore` the TAG half, and defining only one half leaves
the other undefined — the workflow then does not run for events affecting the
undefined ref kind at all. It is a two-line rule that reads like a convenience
and is not one, and this repository wrote it four separate times (#1301):

  * `check-sdk-workflow-coverage.py` (#1119) regexed `^\\s*branches:` and
    `^\\s*tags:` out of the raw `push:` text, so `branches-ignore:`/`tags-ignore:`
    matched neither key and inverted the verdict in both directions.
  * `check-workflow-job-reachability.py` (#1206) got it right.
  * `check-suite-coverage.py` (#1289) read a missing `branches:` as "no
    restriction", making sixteen tag-only publish contexts look branch-reaching.
    Any one of them, added to the merge gate, would have wedged `dev`
    permanently. Fixed in #1298.
  * `check-sdk-publication-claims.py` (#1119) got the mirror side right.

Two of four were wrong, and the two that were right were no help to the two that
were not, because nothing connected them. #1301 collapsed all four onto
`push_ref_filter` in `tools/check-suite-coverage.py`; this gate is what stops a
fifth from being written.

What is refused, in any `tools/**/*.py` other than the owner:

  A. a membership test against a ref-filter key — `"branches" in cfg`, the idiom
     copies 2, 3 and 4 all used;
  B. a regular expression matching one of those keys as YAML — `r"^\\s*tags:"`,
     the idiom copy 1 used.

Both forms are how the rule gets re-derived rather than imported. Reading a
`branches:` VALUE once the halves are known is not re-derivation and is not
refused: `check-workflow-job-reachability.py` passes the key names to its own
`_patterns()` helper to build include/exclude lists, which is its own business.

Import the rule the way the other gates do:

    _yaml_module().push_ref_filter(push_cfg).reaches_no_branch()

Overrides, for testing:
  TRIGGER_RULE_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import ast
import os
import re
import subprocess
import sys
from pathlib import Path

# The one file allowed to state the rule, relative to the repo root.
OWNER = "tools/check-suite-coverage.py"

REF_KEYS = frozenset({"branches", "branches-ignore", "tags", "tags-ignore"})

# Form B: a regex source naming one of the keys as a YAML mapping key, e.g.
# `^\s*branches:\s*(.*)$`. Matched against the string a `re.*` call is given,
# never against a source line — see `_findings`.
YAML_KEY_RE = re.compile(r"(?:branches|tags)(?:-ignore)?:")


def _findings(tree: ast.AST) -> list[tuple[int, str, str]]:
    """Re-derivations of the rule, read from the AST rather than from text.

    Scanning source lines would flag this file's own docstring, and "skip the
    gate\'s own file" is the shape that lets a real copy hide in it. An AST walk
    sees only executable code, so prose quoting the idiom costs nothing and code
    using it is caught wherever it lives.
    """
    out: list[tuple[int, str, str]] = []
    for node in ast.walk(tree):
        # Form A: `"branches" in cfg`, `"tags-ignore" not in push`.
        if isinstance(node, ast.Compare):
            for op, left in zip(node.ops, [node.left, *node.comparators]):
                if not isinstance(op, (ast.In, ast.NotIn)):
                    continue
                if isinstance(left, ast.Constant) and left.value in REF_KEYS:
                    out.append((node.lineno, ast.unparse(node), "membership test"))
        # Form B: `re.search(r"^\s*branches:\s*(.*)$", push, re.M)`.
        if isinstance(node, ast.Call):
            fn = node.func
            if not (isinstance(fn, ast.Attribute) and isinstance(fn.value, ast.Name)
                    and fn.value.id == "re"):
                continue
            for arg in node.args:
                if (isinstance(arg, ast.Constant) and isinstance(arg.value, str)
                        and YAML_KEY_RE.search(arg.value)):
                    out.append((node.lineno, ast.unparse(node), "regex over raw YAML"))
    return sorted(set(out))


def repo_root() -> Path:
    env = os.environ.get("TRIGGER_RULE_ROOT")
    if env:
        return Path(env)
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def main() -> int:
    root = repo_root()
    tools = root / "tools"
    if not tools.is_dir():
        print(f"trigger-rule-copies: FAIL — no {tools}", file=sys.stderr)
        return 1

    owner = root / OWNER
    if not owner.is_file():
        print(
            f"trigger-rule-copies: FAIL — the owner {OWNER} is missing; the rule has "
            "no home and this gate would pass vacuously",
            file=sys.stderr,
        )
        return 1

    # The owner must actually still define the rule. A gate that only refuses
    # copies would go quietly green if the original were deleted or renamed.
    owner_text = owner.read_text(encoding="utf-8")
    for symbol in ("def push_ref_filter(", "class PushRefFilter("):
        if symbol not in owner_text:
            print(
                f"trigger-rule-copies: FAIL — {OWNER} no longer defines `{symbol}`. "
                "The shared rule moved or was deleted; point this gate at its new home.",
                file=sys.stderr,
            )
            return 1

    scanned = 0
    findings: list[tuple[str, int, str, str]] = []
    for path in sorted(tools.rglob("*.py")):
        if "__pycache__" in path.parts:
            continue
        rel = path.relative_to(root).as_posix()
        if rel == OWNER:
            continue
        scanned += 1
        text = path.read_text(encoding="utf-8")
        try:
            tree = ast.parse(text, filename=rel)
        except SyntaxError as exc:
            print(
                f"trigger-rule-copies: FAIL — cannot parse {rel}: {exc}. A file this "
                "gate cannot read is a file it cannot clear.",
                file=sys.stderr,
            )
            return 1
        for lineno, snippet, kind in _findings(tree):
            findings.append((rel, lineno, snippet, kind))

    if not scanned:
        print(
            "trigger-rule-copies: FAIL — scanned zero files under tools/; the layout "
            "changed and this gate went blind",
            file=sys.stderr,
        )
        return 1

    if findings:
        print("trigger-rule-copies: FAIL", file=sys.stderr)
        print(
            "\nA second implementation of GitHub's push ref-filter rule. Two of the "
            "four\nthat existed before #1301 were wrong, in opposite directions, and "
            "neither\nwas found by a check:\n",
            file=sys.stderr,
        )
        for rel, lineno, text, kind in findings:
            print(f"  {rel}:{lineno}  ({kind})", file=sys.stderr)
            print(f"      {text}", file=sys.stderr)
        print(
            f"\n  Import it instead — {OWNER} defines `push_ref_filter(cfg)`, and\n"
            "  `_yaml_module()` in this same directory is how three gates already "
            "load it:\n\n"
            "      _yaml_module().push_ref_filter(push_cfg).reaches_no_branch()\n",
            file=sys.stderr,
        )
        return 1

    print(
        f"trigger-rule-copies: OK — {scanned} tool(s) scanned; GitHub's push "
        f"ref-filter rule is stated once, in {OWNER}."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
