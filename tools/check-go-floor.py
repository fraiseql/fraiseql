#!/usr/bin/env python3
"""The Go SDK's floor is stated once, and every place that pins or claims a Go agrees.

The floor is the `go` line of `sdks/official/fraiseql-go/go.mod`. It drifted because
several files pin Go independently (#2278): go.mod said 1.23, the SDK matrix tested
1.23 and 1.24, the conformance container ran 1.24, and the README promised 1.22. Each
of these must agree with go.mod:

  A. the oldest Go in the `go` row of `tools/sdk-suites-matrix.py`: the floor is
     tested;
  B. every literal `go-version:` in `.github/workflows/*.yml`: a leg that sets up one
     Go runs the floor (a `${{ matrix.version }}` reads the matrix, checked by A);
  C. the Go SDK's conformance container in `sdks/official/conformance/manifest.json`;
  D. every floor the docs state ("Go 1.26+", "Go 1.26 or later", "Go Version: 1.26+")
     in the Go SDK's README, CONTRIBUTING and implementation summary, and in
     `sdks/official/README.md`.

The floor follows the Go releases upstream supports, and moving it is a decision; this
gate cannot know the upstream support window, only that every copy says the same thing.

Exit codes: 0 = clean, 1 = findings, 2 = FATAL (an input the gate cannot read).

Overrides, for testing:
  GO_FLOOR_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(os.environ.get("GO_FLOOR_ROOT", Path(__file__).resolve().parent.parent))
GO_MOD = "sdks/official/fraiseql-go/go.mod"
MATRIX = "tools/sdk-suites-matrix.py"
MANIFEST = "sdks/official/conformance/manifest.json"
DOCS = (
    "sdks/official/fraiseql-go/README.md",
    "sdks/official/fraiseql-go/CONTRIBUTING.md",
    "sdks/official/fraiseql-go/implementation-summary.md",
    "sdks/official/README.md",
)
DOC_FLOOR = re.compile(r"\bGo(?: [Vv]ersion:)? (\d+\.\d+)(?:\+| or later)")
WORKFLOW_PIN = re.compile(
    r"""^\s*go-version:\s*['"]?([0-9][0-9.]*)['"]?\s*$""", re.MULTILINE
)


def fatal(message: str) -> None:
    print(f"go-floor: FATAL — {message}", file=sys.stderr)
    sys.exit(2)


def read(relative: str) -> str:
    path = ROOT / relative
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        fatal(f"cannot read {relative}: {error}")
        raise


def minor(version: str) -> tuple[int, int]:
    """`1.26`, `1.26.3` → (1, 26)."""
    parts = version.split(".")
    if len(parts) < 2 or not all(p.isdigit() for p in parts[:2]):
        fatal(f"not a Go version: {version!r}")
    return int(parts[0]), int(parts[1])


def floor() -> tuple[int, int]:
    found = re.search(r"^go\s+(\S+)\s*$", read(GO_MOD), re.MULTILINE)
    if not found:
        fatal(f"{GO_MOD} declares no `go` line")
    return minor(found.group(1))


def matrix_versions() -> list[str]:
    found = re.search(r'"go":\s*\[([^\]]*)\]', read(MATRIX))
    if not found:
        fatal(f"{MATRIX} has no `go` row")
    versions = re.findall(r'"([^"]+)"', found.group(1))
    if not versions:
        fatal(f"{MATRIX}: the `go` row is empty")
    return versions


def container() -> str:
    try:
        manifest = json.loads(read(MANIFEST))
        image = manifest["sdks"]["go"]["container"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        fatal(f"{MANIFEST}: no Go container ({error})")
        raise
    if not isinstance(image, str) or not image.startswith("golang:"):
        fatal(f"{MANIFEST}: the Go container is not a golang image: {image!r}")
    return image.removeprefix("golang:").split("-")[0]


def show(version: tuple[int, int]) -> str:
    return f"{version[0]}.{version[1]}"


def main() -> int:
    go_floor = floor()
    findings: list[str] = []

    matrix = [minor(v) for v in matrix_versions()]
    if min(matrix) != go_floor:
        findings.append(
            f"{MATRIX}: the oldest Go tested is {show(min(matrix))}, and {GO_MOD} "
            f"declares {show(go_floor)}"
        )

    workflows = sorted((ROOT / ".github/workflows").glob("*.yml"))
    for workflow in workflows:
        text = workflow.read_text(encoding="utf-8")
        for pin in WORKFLOW_PIN.finditer(text):
            if minor(pin.group(1)) != go_floor:
                line = text.count("\n", 0, pin.start()) + 1
                findings.append(
                    f"{workflow.relative_to(ROOT)}:{line}: go-version {pin.group(1)}, "
                    f"and {GO_MOD} declares {show(go_floor)}"
                )

    image = container()
    if minor(image) != go_floor:
        findings.append(
            f"{MANIFEST}: the Go container is golang:{image}, and {GO_MOD} declares "
            f"{show(go_floor)}"
        )

    for doc in DOCS:
        text = read(doc)
        for claim in DOC_FLOOR.finditer(text):
            if minor(claim.group(1)) != go_floor:
                line = text.count("\n", 0, claim.start()) + 1
                findings.append(
                    f"{doc}:{line}: states {claim.group(0)!r}, and {GO_MOD} declares "
                    f"{show(go_floor)}"
                )

    if findings:
        print(
            "go-floor: FAIL — the Go SDK's floor is stated more than one way:",
            file=sys.stderr,
        )
        for finding in findings:
            print(f"  - {finding}", file=sys.stderr)
        print(
            f"Move every copy with {GO_MOD}; the floor is the oldest Go release upstream "
            "supports (#2278).",
            file=sys.stderr,
        )
        return 1
    print(f"go-floor: OK — Go {show(go_floor)} everywhere it is pinned or stated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
