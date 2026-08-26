#!/usr/bin/env python3
"""check-deny-lint-flags.py — every `cargo deny check` invocation that covers the
`bans` check must escalate cargo-deny's two unmatched-skip lints to errors.

Background (#1020, and #933 before it). `deny.toml` carries six `[[bans.skip-tree]]`
entries and several `[[bans.skip]]` entries, all pinned to an exact version. An exact
pin stops matching the moment the crate is bumped, and the entry then covers nothing —
the next `cargo deny check bans` reports every duplicate under that tree at once,
naming crates that look entirely unrelated to the one that moved.

cargo-deny DOES detect this. It emits `unmatched-skip-root` (for `[[bans.skip-tree]]`)
and `unmatched-skip` (for `[[bans.skip]]`) — but both default to WARN, so the run still
exits 0. Measured here on cargo-deny 0.19.0, with a skip-tree naming a crate that is not
in the graph and nothing else changed:

    default lint levels     → exit 0, "bans ok"      ← the silent fail-open
    -D unmatched-skip-root  → exit 2, "bans FAILED"

In #933 the warning WAS printed and was invisible: one line above 22 `error[duplicate]`
entries. That is why the issue recorded that cargo-deny does not warn. It does; nobody
had escalated it. So the durable fix is the flags plus this gate, not a re-implementation
of a check the tool already performs against the real resolved graph.

The lint level cannot be set in deny.toml — cargo-deny 0.19 refuses `unmatched-skip`
and `unmatched-skip-root` as `[bans]` fields ("failed to deserialize config"). So the
flags live on the command line, at three separate call sites, which is exactly the drift
shape this repo keeps hitting (#1110, #1129, #1169). Hence a gate rather than a comment.

Override for testing: DENY_FLAGS_ROOT=<dir>
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

# Anchored to a COMMAND position: start of line, or after `&&` / `;` / `|`, allowing the
# Makefile `@`, the YAML `run:` and the Dockerfile `RUN` prefixes.
#
# ⚠ An unanchored search is not usable here. The first draft of this gate reported
# `deny.toml:432` — the `reason` string of the wasmtime skip-tree entry, which quotes
# `cargo deny check bans` while warning about this very failure mode. A gate that fails
# on its own documentation gets disabled.
CMD_RE = re.compile(
    r"(?:^|&&|;|\|)\s*(?:@|-\s|run:\s*|RUN\s+)?\s*cargo[- ]deny\s+check\b"
)
# The Go string-slice spelling, in .dagger/security.go.
GO_RE = re.compile(r'"cargo-deny"\s*,\s*"check"')

COMMENT_RE = re.compile(r"^\s*(#|//|--)")

# ⚠ `unmatched-skip` is a SUBSTRING of `unmatched-skip-root`. A plain containment test
# for the former is satisfied by the latter, so both are matched as whole tokens.
ROOT_FLAG_RE = re.compile(r"unmatched-skip-root\b")
SKIP_FLAG_RE = re.compile(r"unmatched-skip(?![-\w])")

# An invocation naming only non-bans checks cannot trip these lints, so demanding the
# flags there would be noise the next author deletes.
SCOPED_RE = re.compile(r"check[\"',\s]+(advisories|licenses|sources)\b")
COVERS_BANS_RE = re.compile(r"\b(bans|ban|all)\b")

SKIP_DIRS = {"target", ".git", "node_modules", "vendor", ".venv", "__pycache__"}
SKIP_SUFFIXES = {".md", ".lock", ".snap", ".png", ".svg", ".jpg", ".gz", ".zip"}

MAX_WINDOW = 12

SELF_FILES = {"check-deny-lint-flags.py", "deny_lint_flags_test.sh"}


def command_text(lines: list[str], start: int, is_go: bool) -> str:
    """The full invocation whose command name sits on `lines[start]`.

    A command is not always one line, and the two spellings end differently — so they
    are extracted differently rather than by one heuristic. Reading only the matched
    line reports "flag missing" for an invocation carrying it on the next line, a false
    RED that trains people to delete the gate; reading a fixed forward window instead
    lets flags from a NEIGHBOURING command satisfy this one, a false GREEN. Both were
    observed while writing this.

    Go: the command is the enclosing `[]string{ … }` literal. The opening brace can sit
    on an earlier line than the `"cargo-deny"` element, so the scan starts there and
    runs to the matching close.

    Shell: the command ends at end of line unless continued with a trailing backslash.
    """
    if is_go:
        begin = start
        for j in range(start, max(-1, start - 4), -1):
            if "[]string{" in lines[j]:
                begin = j
                break
        text = ""
        depth = 0
        for k in range(begin, min(len(lines), begin + MAX_WINDOW)):
            text += lines[k] + "\n"
            depth += lines[k].count("{") - lines[k].count("}")
            if depth <= 0:
                break
        return text

    text = lines[start]
    i = start
    while i + 1 < len(lines) and i - start < MAX_WINDOW and text.rstrip().endswith("\\"):
        i += 1
        text += "\n" + lines[i]
    return text


def main() -> int:
    root = Path(os.environ.get("DENY_FLAGS_ROOT", ".")).resolve()
    findings: list[tuple[str, int, str, bool, bool]] = []
    exempt: list[tuple[str, int]] = []

    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fname in filenames:
            path = Path(dirpath) / fname
            # This gate's own source, and its self-test — whose fixtures are
            # deliberately flagless invocations, being red-capability assertions.
            # Excluded by exact name rather than by directory, so a real invocation
            # added anywhere under tools/tests/ is still gated.
            if path.suffix in SKIP_SUFFIXES or path.name in SELF_FILES:
                continue
            try:
                lines = path.read_text(encoding="utf-8").splitlines()
            except (UnicodeDecodeError, OSError):
                continue
            for idx, line in enumerate(lines):
                if COMMENT_RE.match(line):
                    continue
                if not (CMD_RE.search(line) or GO_RE.search(line)):
                    continue
                text = command_text(lines, idx, is_go=bool(GO_RE.search(line)))
                rel = str(path.relative_to(root))
                if SCOPED_RE.search(text) and not COVERS_BANS_RE.search(text):
                    exempt.append((rel, idx + 1))
                    continue
                findings.append(
                    (
                        rel,
                        idx + 1,
                        line.strip(),
                        bool(ROOT_FLAG_RE.search(text)),
                        bool(SKIP_FLAG_RE.search(text)),
                    )
                )

    # A discovery scan that finds nothing must fail loudly. An empty file list reading
    # as success is how three gates in this repo shipped unable to reject anything
    # (#1075).
    if not findings and not exempt:
        print(
            "✗ check-deny-lint-flags: found NO cargo-deny invocation to check.\n"
            "  This gate cannot pass vacuously. Either the scan is broken, or every\n"
            "  cargo-deny call site was removed — both need a human.",
            file=sys.stderr,
        )
        return 1

    bad = [f for f in findings if not (f[3] and f[4])]
    for rel, lineno, snippet, has_root, has_skip in sorted(findings):
        mark = "✓" if (has_root and has_skip) else "✗"
        print(f"  {mark} {rel}:{lineno}")
    for rel, lineno in sorted(exempt):
        print(f"  – {rel}:{lineno} — scoped check, bans not covered (exempt)")

    if bad:
        print("", file=sys.stderr)
        for rel, lineno, snippet, has_root, has_skip in sorted(bad):
            print(
                f"✗ {rel}:{lineno} — cargo-deny invocation covers 'bans' but does not "
                "deny its unmatched-skip lints.",
                file=sys.stderr,
            )
            if not has_root:
                print(
                    "    missing: -D unmatched-skip-root   "
                    "(a stale [[bans.skip-tree]] pin skips NOTHING, silently)",
                    file=sys.stderr,
                )
            if not has_skip:
                print(
                    "    missing: -D unmatched-skip        "
                    "(a stale [[bans.skip]] pin skips NOTHING, silently)",
                    file=sys.stderr,
                )
            print(f"    line: {snippet}", file=sys.stderr)
        print(
            "\n  Both lints default to WARN in cargo-deny 0.19, so the run still exits 0\n"
            "  and the stale entry stays invisible until an unrelated-looking duplicate\n"
            "  storm (#933).",
            file=sys.stderr,
        )
        return 1

    print(
        f"OK: {len(findings)} cargo-deny invocation(s) deny both unmatched-skip lints; "
        f"{len(exempt)} scoped invocation(s) exempt."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
