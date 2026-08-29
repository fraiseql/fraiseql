#!/usr/bin/env python3
"""Derive the feature-check matrix from `.dagger/feature-combos.go`.

The matrix is typed Go data consumed by a Dagger function that runs on the
self-hosted runner. `make lint-feature-matrix` needs the same list on the host,
and a hand-maintained second copy would drift — the exact failure #1135 recorded
for the preflight/ShellGates lists. So this reads the Go source and reproduces
`featureCombo.cargoArgs()`.

Two properties keep the derivation honest rather than merely convenient:

* **Every declared combo parses, or nothing runs.** A literal this parser cannot
  read in full is a hard error naming the line. It can never yield a shorter list
  than the file declares, which is what a local gate silently covering a subset
  would do.
* **An unknown field is an error.** A new `featureCombo` field that changes the
  invocation (an env var, a toolchain pin) would otherwise be dropped here and the
  local run would quietly stop matching the leg.

The mirrored `cargoArgs()` body is pinned by hash for the same reason: a change to
how the leg builds its command must be a change here too, and a pin is the only
thing that says so at the moment it happens.

Usage:
    tools/feature-combos.py            # NAME<TAB>ARGS…, one combo per line
    tools/feature-combos.py --json     # the same as a JSON array
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

# The normalised `cargoArgs()` body this parser mirrors. Recompute with
# `tools/feature-combos.py --print-cargo-args-hash` after an intentional change,
# and check that the reproduction below still matches.
CARGO_ARGS_HASH = "8b497f24e04253bce67aba81a91182fc148c3a0491a8123604342b3c2b11d3e0"

GO_SOURCE = Path(".dagger/feature-combos.go")

VAR_BLOCK_RE = re.compile(
    r"^var featureCombos = \[\]featureCombo\{$(?P<body>.*?)^\}$",
    re.MULTILINE | re.DOTALL,
)
CARGO_ARGS_RE = re.compile(
    r"^func \(c featureCombo\) cargoArgs\(\) \[\]string \{$(?P<body>.*?)^\}$",
    re.MULTILINE | re.DOTALL,
)
LITERAL_RE = re.compile(r"^\s*\{name:.*\},\s*$")

STRING_FIELD_RE = re.compile(r'\b(name|crate): "([^"]*)"')
BOOL_FIELD_RE = re.compile(r"\b(noDefaultFeatures|clippy): (true|false)")
LIST_FIELD_RE = re.compile(r"\b(features): \[\]string\{([^}]*)\}")
ELEMENT_RE = re.compile(r'"([^"]*)"')
# Every `key:` in the literal, so a field this parser does not model is reported
# instead of dropped.
ANY_FIELD_RE = re.compile(r"\b([a-zA-Z][a-zA-Z0-9_]*): ")

KNOWN_FIELDS = {"name", "crate", "features", "noDefaultFeatures", "clippy"}


class ParseError(Exception):
    pass


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def normalise(body: str) -> str:
    """Strip comments and collapse whitespace, so only semantic edits move the hash."""
    lines = []
    for line in body.splitlines():
        line = re.sub(r"//.*$", "", line).strip()
        if line:
            lines.append(line)
    return " ".join(lines)


def cargo_args_hash(source: str) -> str:
    m = CARGO_ARGS_RE.search(source)
    if not m:
        raise ParseError(
            f"{GO_SOURCE}: could not find `func (c featureCombo) cargoArgs() []string`. "
            "This parser mirrors that function; it cannot verify itself without it."
        )
    return hashlib.sha256(normalise(m.group("body")).encode()).hexdigest()


def parse_literal(line: str, lineno: int) -> dict:
    """Parse one `featureCombo` struct literal into a dict, or raise."""
    fields = set(ANY_FIELD_RE.findall(line))
    unknown = fields - KNOWN_FIELDS
    if unknown:
        raise ParseError(
            f"{GO_SOURCE}:{lineno}: unknown featureCombo field(s) "
            f"{sorted(unknown)} — tools/feature-combos.py models "
            f"{sorted(KNOWN_FIELDS)} and would silently drop the rest, so the local "
            f"run would stop matching the leg. Teach it the field, then re-pin "
            f"CARGO_ARGS_HASH if cargoArgs() also changed.\n  {line.strip()}"
        )

    combo: dict = {"features": [], "noDefaultFeatures": False, "clippy": False}
    for key, value in STRING_FIELD_RE.findall(line):
        combo[key] = value
    for key, value in BOOL_FIELD_RE.findall(line):
        combo[key] = value == "true"
    for key, value in LIST_FIELD_RE.findall(line):
        combo[key] = ELEMENT_RE.findall(value)

    for required in ("name", "crate"):
        if required not in combo:
            raise ParseError(
                f"{GO_SOURCE}:{lineno}: literal has no `{required}:` this parser "
                f"could read.\n  {line.strip()}"
            )
    return combo


def cargo_args(combo: dict) -> list[str]:
    """Reproduce `featureCombo.cargoArgs()` from .dagger/feature-combos.go."""
    sub = "clippy" if combo["clippy"] else "check"
    args = ["cargo", sub, "-p", combo["crate"]]
    if combo["noDefaultFeatures"]:
        args.append("--no-default-features")
    if combo["features"]:
        args += ["--features", ",".join(combo["features"])]
    if combo["clippy"]:
        args += ["--all-targets", "--", "-D", "warnings"]
    return args


def load(root: Path) -> list[dict]:
    path = root / GO_SOURCE
    source = path.read_text()

    actual = cargo_args_hash(source)
    if actual != CARGO_ARGS_HASH:
        raise ParseError(
            f"{GO_SOURCE}: cargoArgs() has changed.\n"
            f"  pinned: {CARGO_ARGS_HASH}\n"
            f"  actual: {actual}\n"
            "tools/feature-combos.py reproduces that function. Check the "
            "reproduction in cargo_args() still matches, then update "
            "CARGO_ARGS_HASH."
        )

    m = VAR_BLOCK_RE.search(source)
    if not m:
        raise ParseError(f"{GO_SOURCE}: could not find `var featureCombos = []featureCombo{{`.")

    block_start = source[: m.start("body")].count("\n") + 1
    body = m.group("body")

    # Count what the file declares BEFORE parsing, so a literal this parser cannot
    # read is a failure rather than a shorter list.
    declared = sum(1 for line in body.splitlines() if "{name:" in line)

    combos = []
    for offset, line in enumerate(body.splitlines()):
        if "{name:" not in line:
            continue
        lineno = block_start + offset
        if not LITERAL_RE.match(line):
            raise ParseError(
                f"{GO_SOURCE}:{lineno}: a featureCombo literal that is not one "
                "self-contained `{name: …},` line. This parser reads single-line "
                "literals only; split or reformat it, or teach the parser.\n"
                f"  {line.strip()}"
            )
        combos.append(parse_literal(line, lineno))

    if len(combos) != declared:
        raise ParseError(
            f"{GO_SOURCE}: declares {declared} combos, parsed {len(combos)}."
        )

    names = [c["name"] for c in combos]
    dupes = sorted({n for n in names if names.count(n) > 1})
    if dupes:
        raise ParseError(f"{GO_SOURCE}: duplicate combo name(s): {dupes}")

    return combos


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--json", action="store_true", help="emit a JSON array")
    ap.add_argument(
        "--print-cargo-args-hash",
        action="store_true",
        help="print the current cargoArgs() hash and exit (for re-pinning)",
    )
    ap.add_argument("--root", type=Path, default=None, help="repository root")
    args = ap.parse_args()

    root = args.root or repo_root()

    if args.print_cargo_args_hash:
        try:
            print(cargo_args_hash((root / GO_SOURCE).read_text()))
        except ParseError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        return 0

    try:
        combos = load(root)
    except ParseError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(
            json.dumps(
                [{"name": c["name"], "args": cargo_args(c), "clippy": c["clippy"]} for c in combos],
                indent=2,
            )
        )
    else:
        for c in combos:
            print(f"{c['name']}\t{' '.join(cargo_args(c))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
