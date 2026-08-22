#!/usr/bin/env python3
"""Assert `make preflight` runs everything the Dagger ShellGates leg runs.

`make preflight` prints "mirrors the Dagger preflight leg. Safe to push." A
developer who takes that at face value and pushes over a gate the local target
does not run learns about it from a *required* CI check instead. The two lists
are maintained by hand in two files, so they drift silently — and did, twice:
`test-deadline-gate` (#1135) and, added later in the same program,
`check-feature-chains.sh`.

The check is one-directional: every ShellGates command must be reachable from
the `preflight` target. The reverse is allowed — `preflight` additionally runs
fmt/rustdoc/clippy inline, which Dagger runs as sibling gates (`Fmt`,
`Rustdoc`, `Clippy`) of the same `Preflight` function, and a local target that
is *stricter* than CI costs nothing.

Two commands are matched by name and by the variable assignments they carry, so
a budget that drifts between the two call sites (`UNWRAP_ALLOW_LIMIT=3` here,
something else there) is reported rather than silently accepted.
"""

from __future__ import annotations

import argparse
import re
import shlex
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )

# Commands in the ShellGates script that set up the container rather than gate
# anything, and so have no local counterpart.
SHELLGATES_PREAMBLE = {"set -e", "git init -q . >/dev/null"}

# Go constants interpolated into ShellGates entries via `"…" + name`. Resolved
# from the const block so a changed budget is compared, not skipped.
GO_CONST_RE = re.compile(r"^\s*([a-zA-Z][a-zA-Z0-9_]*)\s*=\s*\"([^\"]*)\"\s*$")


class Command:
    """One gate invocation, normalised so two spellings of it compare equal."""

    def __init__(self, raw: str, kind: str, name: str, assignments: dict[str, str]):
        self.raw = raw
        self.kind = kind  # "make" or "script"
        self.name = name  # target name, or repo-relative script path
        self.assignments = assignments

    @property
    def key(self) -> str:
        return f"{self.kind}:{self.name}"

    def __repr__(self) -> str:
        return self.key


def go_constants(text: str) -> dict[str, str]:
    consts: dict[str, str] = {}
    for line in text.splitlines():
        m = GO_CONST_RE.match(line)
        if m:
            consts[m.group(1)] = m.group(2)
    return consts


def parse_command(raw: str) -> Command | None:
    """Normalise one shell command into a Command, or None if it gates nothing."""
    raw = raw.strip()
    if not raw or raw in SHELLGATES_PREAMBLE:
        return None

    try:
        parts = shlex.split(raw)
    except ValueError:
        return None
    if not parts:
        return None

    # `@$(MAKE) --no-print-directory lint-unwrap UNWRAP_ALLOW_LIMIT=3`
    # and `make lint-unwrap UNWRAP_ALLOW_LIMIT=3` are the same invocation.
    head = parts[0].lstrip("@-+")
    if head in {"make", "$(MAKE)"}:
        rest = [p for p in parts[1:] if not p.startswith("-")]
        target = None
        assignments: dict[str, str] = {}
        for tok in rest:
            if "=" in tok and not tok.startswith("="):
                k, _, v = tok.partition("=")
                assignments[k] = v
            elif target is None:
                target = tok
        if target is None:
            return None
        return Command(raw, "make", target, assignments)

    # `bash tools/check-x.sh`, `python3 tools/check-x.py`, `./tools/check-x.sh`
    for tok in parts:
        cleaned = tok.lstrip("@").lstrip("./")
        if cleaned.startswith("tools/") and cleaned.endswith((".sh", ".py")):
            return Command(raw, "script", cleaned, {})

    return None


def parse_makefile(text: str) -> dict[str, tuple[list[str], list[str]]]:
    """target -> (prerequisites, recipe lines), with continuations joined."""
    targets: dict[str, tuple[list[str], list[str]]] = {}
    lines = text.splitlines()
    i = 0
    current: str | None = None

    target_re = re.compile(r"^([A-Za-z0-9_][A-Za-z0-9_./-]*)\s*:(?!=)\s*(.*)$")

    while i < len(lines):
        line = lines[i]

        if line.startswith("\t"):
            if current is not None:
                recipe = line[1:]
                # Join backslash continuations so one logical command is one entry.
                while recipe.endswith("\\") and i + 1 < len(lines):
                    i += 1
                    recipe = recipe[:-1] + " " + lines[i].strip()
                targets[current][1].append(recipe)
            i += 1
            continue

        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            i += 1
            continue

        m = target_re.match(line)
        if m and not line.startswith(".PHONY"):
            name = m.group(1)
            prereq_text = m.group(2)
            while prereq_text.endswith("\\") and i + 1 < len(lines):
                i += 1
                prereq_text = prereq_text[:-1] + " " + lines[i].strip()
            if name not in targets:
                targets[name] = ([], [])
            targets[name][0].extend(prereq_text.split())
            current = name
        else:
            current = None
        i += 1

    return targets


def resolve(
    target: str,
    targets: dict[str, tuple[list[str], list[str]]],
    seen: set[str] | None = None,
) -> list[Command]:
    """Every gate command reachable from `target`, following prerequisites."""
    if seen is None:
        seen = set()
    if target in seen or target not in targets:
        return []
    seen.add(target)

    prereqs, recipe = targets[target]
    out: list[Command] = [Command(target, "make", target, {})]

    for p in prereqs:
        out.append(Command(p, "make", p, {}))
        out.extend(resolve(p, targets, seen))

    for line in recipe:
        cmd = parse_command(line)
        if cmd is None:
            continue
        out.append(cmd)
        if cmd.kind == "make":
            out.extend(resolve(cmd.name, targets, seen))

    return out


def shellgates_commands(text: str) -> list[Command]:
    """The gate commands in `.dagger/main.go`'s ShellGates script literal."""
    start = text.index("func (m *FraiseqlCi) ShellGates(")
    body = text[start:]
    literal_start = body.index("script := strings.Join([]string{")
    literal_end = body.index('}, "\\n")', literal_start)
    literal = body[literal_start:literal_end]

    consts = go_constants(text)
    commands: list[Command] = []

    for line in literal.splitlines():
        stripped = line.strip()
        if not stripped.startswith('"'):
            continue

        # `"make lint-unwrap UNWRAP_ALLOW_LIMIT=" + unwrapAllowLimit,`
        raw = ""
        for piece in stripped.rstrip(",").split("+"):
            piece = piece.strip()
            if piece.startswith('"') and piece.endswith('"'):
                raw += piece[1:-1]
            elif piece in consts:
                raw += consts[piece]
            else:
                raw += ""
        cmd = parse_command(raw)
        if cmd is not None:
            commands.append(cmd)

    return commands


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="tree to check; defaults to the git toplevel. Used by the self-test "
        "in tools/tests/preflight_parity_test.sh to prove the gate goes red.",
    )
    args = parser.parse_args()
    root = args.root if args.root is not None else repo_root()

    makefile_text = (root / "Makefile").read_text(encoding="utf-8")
    dagger_text = (root / ".dagger" / "main.go").read_text(encoding="utf-8")

    targets = parse_makefile(makefile_text)
    if "preflight" not in targets:
        print("preflight-parity: FAIL — no `preflight` target in Makefile", file=sys.stderr)
        return 1

    required = shellgates_commands(dagger_text)
    if not required:
        print(
            "preflight-parity: FAIL — parsed zero commands out of ShellGates; "
            "the script literal's shape changed and this gate went blind",
            file=sys.stderr,
        )
        return 1

    local = resolve("preflight", targets)

    # A make target that wraps a script is satisfied by either spelling, so index
    # the local side by both its target names and the scripts they reach.
    local_keys: set[str] = set()
    local_assignments: dict[str, dict[str, str]] = {}
    for cmd in local:
        local_keys.add(cmd.key)
        if cmd.assignments:
            local_assignments.setdefault(cmd.key, {}).update(cmd.assignments)
        if cmd.kind == "make":
            for reached in resolve(cmd.name, targets, set()):
                local_keys.add(reached.key)

    missing: list[Command] = []
    drifted: list[str] = []

    for cmd in required:
        satisfied = cmd.key in local_keys
        if not satisfied and cmd.kind == "make":
            # ShellGates may name a target the Makefile spells as a direct
            # script call, and vice versa.
            for reached in resolve(cmd.name, targets, set()):
                if reached.key in local_keys:
                    satisfied = True
                    break
        if not satisfied:
            missing.append(cmd)
            continue

        for var, value in cmd.assignments.items():
            local_value = local_assignments.get(cmd.key, {}).get(var)
            if local_value is not None and local_value != value:
                drifted.append(
                    f"  {cmd.name}: {var}={local_value} locally, {var}={value} in ShellGates"
                )

    if missing or drifted:
        print("preflight-parity: FAIL", file=sys.stderr)
        if missing:
            print(
                f"\n`make preflight` does not run {len(missing)} command(s) the Dagger "
                "ShellGates leg runs:\n",
                file=sys.stderr,
            )
            for cmd in missing:
                print(f"  {cmd.raw}", file=sys.stderr)
            print(
                "\nAdd each to the `preflight:` prerequisite list (or its recipe) in "
                "the Makefile.\nUntil then, preflight's "
                '"mirrors the Dagger preflight leg. Safe to push." is false.',
                file=sys.stderr,
            )
        if drifted:
            print("\nSame gate, different budget on each side:\n", file=sys.stderr)
            for row in drifted:
                print(row, file=sys.stderr)
        return 1

    print(
        f"preflight-parity: OK — all {len(required)} ShellGates commands are "
        "reachable from `make preflight`."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
