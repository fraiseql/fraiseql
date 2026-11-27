#!/usr/bin/env python3
"""Prove that every crate the doctest leg skips has no doctest, and print the list.

`tools/check-doctest-feature-sets.sh` runs `cargo test --doc` once per workspace
library. A run compiles the crate's whole dependency graph with that crate's own
features, and for a crate with no doctest it compiles everything and tests nothing.
SKIP names those crates. A skip list is a claim about the source, true when it was
written. A doctest added later to a skipped crate would never run, so this script
reads the source of every skipped crate and refuses the list when it is no longer
true.

The reading is conservative: anything that could be a doctest counts as one.
In a skipped crate, a finding is:

  - a fence (``` or ~~~, any length) opened in a `///` or `//!` comment, unless its
    info string is exactly one word from NOT_RUST. rustdoc compiles an empty info
    string, `rust`, `no_run`, `should_panic`, `compile_fail` and `edition…`, and an
    `ignore`d block is one word away from running, so all of them count, as does
    any word this script does not know;
  - an indented code block: a doc line outside a fence whose text starts with four
    spaces or a tab. Markdown makes it code, and rustdoc compiles it;
  - a `/**` or `/*!` block comment, a `doc = …` attribute (`include_str!` of a
    README, a string, a `cfg_attr`) and a `#[path = …]` module: doc text or source
    this line scan does not read;
  - a `[lib]` `path` in the crate's Cargo.toml: a library root outside `src/`,
    the one directory this script reads. rustdoc tests the library only, so
    `tests/`, `benches/` and `examples/` hold no doctest;
  - a skipped name with no crate directory.

On success it prints the skipped crates, one per line, and exits 0.
Exit codes: 0 = clean, 1 = findings, 2 = FATAL (a skip list with nothing to read).

Overrides, for testing:
  DOCTEST_SKIP_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

# Workspace libraries with no doctest, which the doctest leg does not build.
SKIP = (
    "fraiseql-federation",
    "fraiseql-guard",
    "fraiseql-jwks",
    "fraiseql-webhooks",
)

# Info strings rustdoc does not compile: a block whose info string is one of these
# words, alone, is documentation. Anything else counts as a doctest.
NOT_RUST = frozenset(
    {
        "bash",
        "console",
        "graphql",
        "json",
        "sh",
        "shell",
        "sql",
        "text",
        "toml",
        "yaml",
    }
)

DOC_LINE = re.compile(r"^\s*//[/!](?![/!])(?P<text>.*)$")
FENCE = re.compile(r"^\s{0,3}(?P<mark>`{3,}|~{3,})(?P<info>.*)$")
UNREAD = (
    (re.compile(r"/\*[*!]"), "a `/**` or `/*!` block comment"),
    (re.compile(r"\bdoc\s*="), "a `doc = …` attribute"),
    (re.compile(r"#\[\s*path\s*="), "a `#[path = …]` module"),
)


def scan_root() -> Path:
    env = os.environ.get("DOCTEST_SKIP_ROOT")
    return Path(env) if env else Path(__file__).absolute().parent.parent


def is_rust(info: str) -> bool:
    words = [w for w in re.split(r"[\s,]+", info.strip().lower()) if w]
    return not (len(words) == 1 and words[0] in NOT_RUST)


def scan_file(path: Path, shown: str) -> list[str]:
    findings: list[str] = []
    fence: str | None = None
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        for pattern, what in UNREAD:
            if pattern.search(line):
                findings.append(f"{shown}:{n}: {what}")
        doc = DOC_LINE.match(line)
        if doc is None:
            fence = None
            continue
        text = doc.group("text")
        text = text.removeprefix(" ")
        mark = FENCE.match(text)
        if fence is not None:
            if (
                mark
                and mark.group("mark")[0] == fence[0]
                and len(mark.group("mark")) >= len(fence)
                and not mark.group("info").strip()
            ):
                fence = None
            continue
        if mark:
            fence = mark.group("mark")
            if is_rust(mark.group("info")):
                findings.append(
                    f"{shown}:{n}: a code block rustdoc compiles ({text.strip()})"
                )
        elif text.startswith(("    ", "\t")):
            findings.append(f"{shown}:{n}: an indented code block")
    return findings


def scan_crate(root: Path, name: str) -> list[str]:
    crate = root / "crates" / name
    manifest = crate / "Cargo.toml"
    if not manifest.is_file():
        return [f"{name}: no crates/{name}/Cargo.toml; remove it from SKIP"]
    findings: list[str] = []
    in_lib = False
    for n, line in enumerate(manifest.read_text(encoding="utf-8").splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("["):
            in_lib = stripped == "[lib]"
        elif in_lib and re.match(r"path\s*=", stripped):
            findings.append(f"crates/{name}/Cargo.toml:{n}: a `[lib]` `path`")
    for path in sorted((crate / "src").rglob("*.rs")):
        findings.extend(scan_file(path, str(path.relative_to(root))))
    return findings


def main() -> int:
    root = scan_root()
    if not SKIP:
        print("FATAL: SKIP is empty; delete this script instead", file=sys.stderr)
        return 2
    findings = [f for name in SKIP for f in scan_crate(root, name)]
    if findings:
        print(
            "check-doctest-skip: a crate the doctest leg skips may hold a doctest. "
            "Remove it from SKIP in tools/check-doctest-skip.py, or change the text:",
            file=sys.stderr,
        )
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1
    print("\n".join(SKIP))
    return 0


if __name__ == "__main__":
    sys.exit(main())
