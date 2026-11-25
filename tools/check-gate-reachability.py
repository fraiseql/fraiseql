#!/usr/bin/env python3
"""Every gate script on disk is run by a CI leg, or carries an exemption saying why not.

A gate that exists and runs nowhere is worse than no gate: it reads as coverage. Five
self-tests (the ones for check-fuzz-compiles, check-example-crates-compile,
check-r-examples-parse, check-phases-citations and check-image-context) ran in
`make preflight` and in no CI leg from the day they were added, and the check that
compared preflight with the Dagger list could not see it: it asked whether every CI
gate was local, never whether every local gate was CI.

So this check starts from the files, not from either list:

* **What exists:** every `tools/check-*.sh`, `tools/check-*.py` and
  `tools/tests/*_test.sh`.
* **What CI runs:** every `dagger call <fn>` in a workflow's text names a root. From
  there, the Go functions and constants those roots reference, the make targets
  their strings invoke (with prerequisites and recipe lines), and the scripts those
  invoke in turn. Plus every script and make target a workflow invokes directly.

Only an *invocation* counts: `bash tools/x.sh`, `python3 tools/x.py`, a path in
command position, `make <target>`, or `"bash", "tools/x.sh"` in a Go string list. A
path in an `echo`, a `case` arm, a `paths:` filter or a comment is a mention, and a
mention runs nothing. Edges out of a `tools/tests/*_test.sh` are not followed: a
self-test runs its gate over fixtures, which says the gate can fail, not that it
gates this tree. A Dagger function no workflow reaches is not CI either.

An exemption names the file and the reason it runs in no leg
(`tools/gate-reachability-exemptions.toml`). An exemption for a file CI does reach,
or for a file that does not exist, fails: it would otherwise outlive its reason.

Runs in preflight ShellGates and locally via `make lint-gate-reachability`. Its red
capability is pinned by `tools/tests/gate_reachability_test.sh`.
`GATE_REACHABILITY_ROOT` points it at another tree (the test's fixtures).
"""

from __future__ import annotations

import os
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(
    os.environ.get("GATE_REACHABILITY_ROOT") or Path(__file__).resolve().parent.parent
)
EXEMPTIONS = ROOT / "tools" / "gate-reachability-exemptions.toml"

SEP = r"""["', \t]+"""  # one line: a separator never spans two commands
PATH = r"""(?:\$\(CURDIR\)/|/src/|\./)?(tools/[\w./-]+?\.(?:sh|py))\b"""
# `bash tools/x.sh`, `python3 -I tools/x.py`, `"bash", "tools/x.sh"`.
INTERPRETED = re.compile(
    r"\b(?:bash|sh|python3?|exec|source)" + SEP + r"(?:-[\w-]+" + SEP + r")*" + PATH
)
# A script run by path, in command position.
# A `case` arm (`tools/x.sh) ...`) names a path without running it.
DIRECT = re.compile(
    r"(?:^|[;&|(!]|\bthen\b|\bdo\b|\bif\b)\s*[\"']?" + PATH + r"(?![\"']?\s*[|)])", re.MULTILINE
)
# A sibling run by its own directory: `"$(dirname "${BASH_SOURCE[0]}")/x.py"`.
SIBLING = re.compile(r'\$\(dirname "\$\{BASH_SOURCE\[0\]\}"\)/([\w.-]+\.(?:sh|py))')
# `make a b`, `$(MAKE) --no-print-directory a VAR=1`, `"make", "a"`.
MAKE = re.compile(r"(?:\$\(MAKE\)|\bmake\b)((?:" + SEP + r"[^\s\"',;&|>]+)+)")
DAGGER_CALL = re.compile(r"\bdagger\s+call\s+([a-z][\w-]*)")

GO_UNIT = re.compile(r"^(?:func\s+(?:\([^)]*\)\s*)?(\w+)|const\b|var\b|type\s+(\w+))", re.M)
GO_CONST_NAME = re.compile(r"^\s*(\w+)\s*(?:\w+\s*)?=", re.M)
GO_STRING = re.compile(r'"((?:[^"\\\n]|\\.)*)"|`([^`]*)`')
IDENT = re.compile(r"\b[A-Za-z_]\w*\b")


def fail(msg: str) -> None:
    print(f"gate-reachability: FAIL — {msg}", file=sys.stderr)
    sys.exit(1)


def strip_hash_comments(text: str) -> str:
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))


def make_targets_in(text: str) -> list[str]:
    out = []
    for m in MAKE.finditer(text):
        for word in re.split(SEP, m.group(1)):
            if not word or word.startswith("-") or "=" in word:
                continue
            if not re.fullmatch(r"[a-z0-9][\w.-]*", word):
                break
            out.append(word)
    return out


def scripts_in(text: str, here: Path | None = None) -> list[str]:
    found = [m.group(1) for m in INTERPRETED.finditer(text)]
    found += [m.group(1) for m in DIRECT.finditer(text)]
    if here is not None:
        found += [(here.parent / m.group(1)).as_posix() for m in SIBLING.finditer(text)]
    return found


def parse_makefile(text: str) -> dict[str, tuple[list[str], list[str]]]:
    """target -> (prerequisites, recipe lines), continuations joined."""
    text = text.replace("\\\n", " ")
    targets: dict[str, tuple[list[str], list[str]]] = {}
    current: list[str] = []
    rule = re.compile(r"^([A-Za-z0-9][\w .%-]*?)\s*::?(?!=)\s*(.*)$")
    for line in text.splitlines():
        if line.startswith("\t"):
            for name in current:
                targets[name][1].append(line)
            continue
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        m = rule.match(line)
        if m and "=" not in m.group(1):
            names = [n for n in m.group(1).split() if not n.startswith(".")]
            body = m.group(2).split("#", 1)[0]
            if body.lstrip().startswith("export ") or re.match(r"\s*\w+\s*:?=", body):
                current = []  # a target-specific variable, not a rule
                continue
            for n in names:
                targets.setdefault(n, ([], []))[0].extend(body.split())
            current = names
        else:
            current = []
    return targets


def go_units(text: str) -> dict[str, str]:
    """Top-level declaration name -> its source, full-line `//` comments dropped.

    A `const (...)` or `var (...)` block is one unit under each name it declares, so a
    function that names one constant reaches the strings of the whole block."""
    text = "\n".join(
        line for line in text.splitlines() if not line.lstrip().startswith("//")
    )
    starts = list(GO_UNIT.finditer(text))
    ends = [m.start() for m in starts[1:]] + [len(text)]
    units: dict[str, str] = {}
    for m, end in zip(starts, ends, strict=True):
        body = text[m.start() : end]
        name = m.group(1) or m.group(2)
        if name:
            units[name] = body
            continue
        header = body.partition("\n")[0]
        decls = body.partition("(")[2] if "(" in header else body
        for const in GO_CONST_NAME.findall(decls):
            units[const] = body
    return units


def go_text(body: str) -> str:
    """The unit's string literals, each on its own and as one space-joined line per
    source line they start on, so `"bash", "tools/x.sh"` reads as `bash tools/x.sh`.
    A raw string may span lines; it is matched over the whole body, not per line."""
    by_line: dict[int, list[str]] = {}
    for m in GO_STRING.finditer(body):
        by_line.setdefault(body.count("\n", 0, m.start()), []).append(m.group(1) if m.group(1) is not None else m.group(2))
    parts = []
    for lits in by_line.values():
        parts.append(" ".join(lits))
        parts.extend(lits)
    return "\n".join(parts)


def camel_key(name: str) -> str:
    return name.replace("-", "").lower()


def main() -> int:
    workflows = sorted((ROOT / ".github" / "workflows").glob("*.y*ml"))
    roots: set[str] = set()
    seed_text: list[tuple[str, Path | None]] = []
    for wf in workflows:
        text = strip_hash_comments(wf.read_text(encoding="utf-8"))
        roots.update(camel_key(n) for n in DAGGER_CALL.findall(text))
        seed_text.append((text, None))
    if not roots:
        fail("no `dagger call` in any workflow; the workflow parser went blind")

    units: dict[str, str] = {}
    for go in sorted((ROOT / ".dagger").glob("*.go")):
        units.update(go_units(go.read_text(encoding="utf-8")))
    by_key = {camel_key(n): n for n in units}
    missing_roots = sorted(r for r in roots if r not in by_key)
    stack = [by_key[r] for r in roots if r in by_key]
    if not stack:
        fail("no workflow's `dagger call` names a function in .dagger/*.go")
    reached_units: set[str] = set()
    while stack:
        name = stack.pop()
        if name in reached_units:
            continue
        reached_units.add(name)
        body = units[name]
        stack.extend(i for i in set(IDENT.findall(body)) if i in units and i not in reached_units)
        seed_text.append((go_text(body), None))
    if missing_roots:
        # A `dagger call` of a function the module does not define fails in CI on its
        # own; it adds nothing here, so it is noted rather than fatal.
        print(f"gate-reachability: note — no Go function for {', '.join(missing_roots)}")

    targets = parse_makefile((ROOT / "Makefile").read_text(encoding="utf-8"))
    reached_targets: set[str] = set()
    reached_scripts: set[str] = set()
    pending_targets: list[str] = []
    pending_scripts: list[tuple[str, Path | None]] = []

    def take(text: str, here: Path | None) -> None:
        pending_targets.extend(make_targets_in(text))
        pending_scripts.extend((s, None) for s in scripts_in(text, here))

    for text, here in seed_text:
        take(text, here)
    if not pending_targets and not pending_scripts:
        fail("CI's text invokes no make target and no script; the parser went blind")

    while pending_targets or pending_scripts:
        while pending_targets:
            t = pending_targets.pop()
            if t in reached_targets or t not in targets:
                continue
            reached_targets.add(t)
            prereqs, recipe = targets[t]
            pending_targets.extend(prereqs)
            for line in recipe:
                take(line, None)
        while pending_scripts:
            s, _ = pending_scripts.pop()
            s = os.path.normpath(s)
            if s in reached_scripts:
                continue
            reached_scripts.add(s)
            path = ROOT / s
            if s.startswith("tools/tests/") or not path.is_file():
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            if path.suffix == ".sh":
                text = strip_hash_comments(text)
            take(text, Path(s))

    discovered = sorted(
        p.relative_to(ROOT).as_posix()
        for pattern in ("tools/check-*.sh", "tools/check-*.py", "tools/tests/*_test.sh")
        for p in ROOT.glob(pattern)
    )
    if not discovered:
        fail("found no gate script under tools/; the discovery went blind")

    exempt: dict[str, str] = {}
    if EXEMPTIONS.is_file():
        for row in tomllib.loads(EXEMPTIONS.read_text(encoding="utf-8")).get("exempt", []):
            if not row.get("reason", "").strip():
                fail(f"exemption for {row.get('path')} gives no reason")
            exempt[row["path"]] = row["reason"]

    problems = []
    for path in discovered:
        if path in reached_scripts and path in exempt:
            problems.append(f"{path}: exempted, but a CI leg runs it; drop the exemption")
        elif path not in reached_scripts and path not in exempt:
            problems.append(f"{path}: no CI leg runs it")
    for path in sorted(set(exempt) - set(discovered)):
        problems.append(f"{path}: exempted, but no such gate script exists")

    if problems:
        for p in problems:
            print(f"gate-reachability: {p}", file=sys.stderr)
        print(
            "\nRun it from a leg (a ShellGates line in .dagger/main.go, or a make target a "
            "leg runs), or add it to tools/gate-reachability-exemptions.toml with the "
            "reason it runs in none.",
            file=sys.stderr,
        )
        return 1
    print(
        f"gate-reachability: OK — {len(discovered)} gate scripts, "
        f"{len(discovered) - len(exempt)} run by a CI leg, {len(exempt)} exempted "
        f"({len(reached_units)} Go declarations and {len(reached_targets)} make targets reached)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
