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

# A Dagger function whose only gate command is a make target. Its exec is pinned to the
# exact string: a `|| true`, an `-i`, another target or a line moved into a comment would
# each leave the leg green over a failing gate, with every gate still "reached".
MAKE_EXECS = {
    "ShellGates": "git init -q . >/dev/null && make -k --no-print-directory shell-gates",
}
# Make targets named as gates. Each is reached by a CI leg, or wraps only scripts a leg
# runs some other way, or is exempted.
GATE_TARGET = re.compile(r"^(?:lint-.+|test-.+-gate)$")
# A recipe or command that carries on past a failure.
# A command ends at `;`, `&&`, a shell comment or the end of the line.
SWALLOW_TAIL = re.compile(r"(?:\|\|\s*(?:true|:)|;\s*true)\s*(?:;|&&|#|\\?$)")
IGNORE_FLAG = re.compile(r"(?:^|\s)(?:-(?!-)[A-Za-z]*i[A-Za-z]*|--ignore-errors)(?=\s|$)")
MAKE_RUN = re.compile(r"(?:\$\(MAKE\)|\bmake\b)([^;&|]*)")
REASON = "# Reason:"

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


def exec_problems(units: dict[str, str], reached_units: set[str]) -> list[str]:
    """Each MAKE_EXECS function runs its make command exactly, and nothing else that
    names make, in a function a workflow reaches."""
    problems = []
    for fn, want in MAKE_EXECS.items():
        if fn not in units:
            problems.append(f"{fn}: no such function in .dagger/*.go")
            continue
        if fn not in reached_units:
            problems.append(f"{fn}: no workflow reaches it")
        lits = [a if a is not None else b for a, b in GO_STRING.findall(units[fn])]
        makes = [lit for lit in lits if re.search(r"\bmake\b", lit)]
        if makes != [want]:
            problems.append(
                f"{fn}: must run exactly `{want}`, and no other make command; "
                f"found {makes or 'none'}"
            )
    return problems


def swallow_problems(
    makefile: str, targets: dict[str, tuple[list[str], list[str]]], reached: set[str]
) -> list[str]:
    """A target CI runs may not carry on past a failure: no `-` recipe prefix, no
    `|| true` / `|| :` / `; true` ending a command, no `.IGNORE`, no `-i` in MAKEFLAGS or
    in a make it runs. A recipe line saying `# Reason:` why its failure is expected is
    exempt; `.IGNORE` and MAKEFLAGS have no exemption."""
    problems = []
    for name in sorted(reached):
        for line in targets[name][1]:
            body = line.lstrip("\t")
            if REASON in body:
                continue
            prefix = re.match(r"[@+\-\s]*", body).group(0)
            if "-" in prefix:
                problems.append(f"make {name}: a recipe line ignores its failure (`-`): {body.strip()}")
            if SWALLOW_TAIL.search(body):
                problems.append(f"make {name}: a command ends by discarding its failure: {body.strip()}")
            for m in MAKE_RUN.finditer(body):
                if IGNORE_FLAG.search(m.group(1)):
                    problems.append(f"make {name}: runs make with -i: {body.strip()}")
    for line in makefile.replace("\\\n", " ").splitlines():
        m = re.match(r"^\.IGNORE\s*:(.*)$", line)
        if m:
            named = m.group(1).split("#", 1)[0].split()
            if not named:
                problems.append("Makefile: `.IGNORE:` with no prerequisites ignores every failure")
            for t in named:
                if t in reached:
                    problems.append(f"Makefile: `.IGNORE:` names {t}, which a CI leg runs")
        m = re.match(r"^\s*(?:export\s+|override\s+)*MAKEFLAGS\s*[:+?!]*=(.*)$", line)
        if m and any(
            w == "--ignore-errors" or re.fullmatch(r"-?[A-Za-z]*i[A-Za-z]*", w)
            for w in m.group(1).split("#", 1)[0].split()
        ):
            problems.append(f"Makefile: MAKEFLAGS carries -i: {line.strip()}")
    return problems


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

    makefile_text = (ROOT / "Makefile").read_text(encoding="utf-8")
    targets = parse_makefile(makefile_text)
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
    exempt_targets: dict[str, str] = {}
    if EXEMPTIONS.is_file():
        for row in tomllib.loads(EXEMPTIONS.read_text(encoding="utf-8")).get("exempt", []):
            name = row.get("path") or row.get("target")
            if not row.get("reason", "").strip():
                fail(f"exemption for {name} gives no reason")
            (exempt if "path" in row else exempt_targets)[name] = row["reason"]

    problems = exec_problems(units, reached_units)
    problems += swallow_problems(makefile_text, targets, reached_targets)
    for name in sorted(t for t in targets if GATE_TARGET.match(t)):
        wrapped = [
            os.path.normpath(x) for line in targets[name][1] for x in scripts_in(line)
        ]
        covered = name in reached_targets or (
            wrapped and all(x in reached_scripts for x in wrapped)
        )
        if covered and name in exempt_targets:
            problems.append(f"make {name}: exempted, but a CI leg runs it; drop the exemption")
        elif not covered and name not in exempt_targets:
            problems.append(f"make {name}: no CI leg runs it")
    for name in sorted(set(exempt_targets) - set(targets)):
        problems.append(f"make {name}: exempted, but no such target exists")
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
