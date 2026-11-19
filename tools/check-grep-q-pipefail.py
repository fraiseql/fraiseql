#!/usr/bin/env python3
"""No pipe into `grep -q` where `pipefail` is on.

`grep -q` exits at its first match. If the command writing into the pipe still has
output, its next write raises SIGPIPE and it exits 141; under `pipefail` the pipeline's
status is then 141, not grep's 0. So a match reads as a miss, depending on how far the
writer had got: `if ! … | grep -q` refuses a correct tree, and `if … | grep -q`
skips the branch it guards, which in a gate that refuses on a match passes output it
should refuse (#1870, #1961).

Write `grep -c … >/dev/null` instead: `-c` counts every line, so grep reads to the end
and the writer never takes SIGPIPE, and the exit status is the same (0 on a match).

Where `pipefail` is on, as this check models it:
  - a shell script (`*.sh`, or a `sh`/`bash` shebang) that sets it anywhere, since a
    function defined above the `set` still runs under it, and any file such a script
    sources;
  - a workflow step whose shell is `bash` (GitHub runs it as `bash -eo pipefail`),
    directly or through `defaults.run.shell`, or whose script sets it;
  - a Makefile recipe line that sets it, or every recipe when `.SHELLFLAGS` does;
  - every line of `.dagger/*.go`: its scripts run under `set -euo pipefail`.

Exit 0 when no site is found, 1 naming each site, 2 when the scan reads no shell script.
"""

from __future__ import annotations

import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

ROOT = Path(os.environ.get("GREP_Q_PIPEFAIL_ROOT") or Path(__file__).absolute().parent.parent)

# A pipe into grep (or egrep/fgrep), up to the end of that command.
PIPED_GREP = re.compile(r"(?<![|])\|(?![|])\s*(?:command\s+)?[ef]?grep\b([^|;&]*)")
QUIET = re.compile(r"^(?:-[A-Za-z]*q[A-Za-z]*|--quiet|--silent)$")
PIPEFAIL = re.compile(r"\bset\s+(?:-[A-Za-z]*o\s+pipefail|-o\s+pipefail)\b|\bpipefail\b")
SOURCE = re.compile(r"(?:^|[\s;&|(])(?:source|\.)\s+(.*)")
SCRIPT_NAME = re.compile(r"[\w.-]+\.(?:sh|bash)\b")
SHEBANG = re.compile(r"^#!.*\b(?:ba)?sh\b")


# This check's own pin holds refused lines as fixture data, in quoted strings that span
# lines. It is skipped by its exact path; a file anywhere else is read.
OWN_PIN = "tools/tests/grep_q_pipefail_test.sh"


def tracked() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, text=True, check=False
    ).stdout
    if out:
        return [ROOT / p for p in out.split("\0") if p]
    # No git (a fixture tree or a source mount without `.git`): walk the tree.
    return [p for p in ROOT.rglob("*") if p.is_file() and ".git" not in p.parts]


def code(line: str) -> bool:
    return not line.lstrip().startswith("#")


OPERATORS = {"|", "||", "&&", ";", "&", "(", ")", ";;", "|&"}


def quiet_grep(line: str) -> bool:
    """A pipe into grep with `-q` among its options, wherever it is placed.

    The line is split the way the shell splits it, quotes and operators included, so a
    `|` inside a quoted pattern is part of the pattern. GNU grep takes options after the
    pattern too, so every word up to the end of the command counts. A line the shell
    could not split alone (a quote that continues on the next line) is read with a
    pattern instead.
    """
    try:
        lexer = shlex.shlex(line, posix=True, punctuation_chars=";&|()")
        lexer.whitespace_split = True
        lexer.commenters = "#"
        words = list(lexer)
    except ValueError:
        return any(
            any(QUIET.match(w) for w in m.group(1).split()) for m in PIPED_GREP.finditer(line)
        )
    for i, w in enumerate(words):
        if w not in ("|", "|&"):
            continue
        j = i + 1
        if j < len(words) and words[j] == "command":
            j += 1
        if j >= len(words) or words[j] not in ("grep", "egrep", "fgrep"):
            continue
        for arg in words[j + 1 :]:
            if arg in OPERATORS:
                break
            if QUIET.match(arg):
                return True
    return False


def quiet_grep_in_string(line: str) -> bool:
    """`quiet_grep` for shell held in another language's string literals (Go)."""
    return any(
        any(QUIET.match(w) for w in m.group(1).split()) for m in PIPED_GREP.finditer(line)
    )


def sites(text: str, offset: int = 0, embedded: bool = False) -> list[tuple[int, str]]:
    found = quiet_grep_in_string if embedded else quiet_grep
    return [
        (offset + n, line.strip())
        for n, line in enumerate(text.splitlines(), 1)
        if (embedded or code(line)) and found(line)
    ]


def sets_pipefail(text: str) -> bool:
    return any(code(line) and PIPEFAIL.search(line) for line in text.splitlines())


def shell_scripts(files: list[Path]) -> dict[Path, str]:
    scripts = {}
    for f in files:
        if f.suffix in (".sh", ".bash") or f.name.endswith(".sh.tmpl"):
            scripts[f] = f.read_text(encoding="utf-8", errors="replace")
        elif f.suffix == "" and f.stat().st_size < 1_000_000:
            try:
                head = f.open(encoding="utf-8", errors="replace").readline()
            except OSError:
                continue
            if SHEBANG.match(head):
                scripts[f] = f.read_text(encoding="utf-8", errors="replace")
    return scripts


def check_scripts(scripts: dict[Path, str]) -> list[str]:
    under = {f for f, t in scripts.items() if sets_pipefail(t)}
    # A sourced file runs under its caller's options. Match on the basename the
    # `source` line names; a variable path (`"$dir/lib.sh"`) still ends in it.
    by_name: dict[str, list[Path]] = {}
    for f in scripts:
        by_name.setdefault(f.name, []).append(f)
    changed = True
    while changed:
        changed = False
        for f in list(under):
            for line in scripts[f].splitlines():
                if not code(line):
                    continue
                for m in SOURCE.finditer(line):
                    names = SCRIPT_NAME.findall(m.group(1))
                    for lib in (lib for name in names for lib in by_name.get(name, [])):
                        if lib not in under:
                            under.add(lib)
                            changed = True
    return [
        f"{f.relative_to(ROOT)}:{n}: {line}"
        for f in sorted(under)
        for n, line in sites(scripts[f])
    ]


def indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def check_workflows(files: list[Path]) -> list[str]:
    """Sites in steps that run under `bash -eo pipefail`.

    Read from the text (the gate's container has no YAML library). A step is the list
    item enclosing the site; its shell is a `shell:` key at the item's own level. A
    `defaults: run: shell: bash` anywhere in the file is taken to cover every step.
    """
    out = []
    for f in sorted(files):
        rel = f.relative_to(ROOT).as_posix()
        if not (rel.startswith(".github/workflows/") and f.suffix in (".yml", ".yaml")):
            continue
        lines = f.read_text(encoding="utf-8").splitlines()
        default_bash = bool(
            re.search(r"^\s*defaults:\s*\n\s+run:\s*\n(?:\s+\S.*\n)*?\s+shell:\s*bash\b",
                      "\n".join(lines) + "\n", re.MULTILINE)
        )
        for n, line in enumerate(lines, 1):
            if not (code(line) and quiet_grep(line)):
                continue
            level = indent(line)
            start = next(
                (i for i in range(n - 1, 0, -1)
                 if re.match(r"^\s*- ", lines[i - 1]) and indent(lines[i - 1]) < level),
                None,
            )
            if start is None:
                continue
            item = indent(lines[start - 1])
            end = next(
                (i for i in range(start + 1, len(lines) + 1)
                 if lines[i - 1].strip() and indent(lines[i - 1]) <= item),
                len(lines) + 1,
            )
            step = lines[start - 1 : end - 1]
            key = item + 2
            shell_bash = any(
                re.match(r"^\s*(?:- )?shell:\s*bash\b", l)
                and (indent(l) == key or (l.lstrip().startswith("- ") and indent(l) == item))
                for l in step
            )
            if default_bash or shell_bash or sets_pipefail("\n".join(step)):
                out.append(f"{rel}:{n}: {line.strip()}")
    return out


def check_makefile(path: Path) -> list[str]:
    if not path.is_file():
        return []
    text = path.read_text(encoding="utf-8")
    everywhere = any(
        re.match(r"^\.SHELLFLAGS\s*[:+]?=.*pipefail", l) for l in text.splitlines()
    )
    out = []
    # A recipe's logical line: physical lines joined at a trailing backslash.
    logical: list[tuple[int, str]] = []
    start, buf = 0, []
    for n, line in enumerate(text.splitlines(), 1):
        if not buf:
            start = n
        buf.append(line)
        if not line.endswith("\\"):
            logical.append((start, "\n".join(buf)))
            buf = []
    for n, block in logical:
        if not block.startswith("\t"):
            continue
        if everywhere or sets_pipefail(block):
            out += [f"{path.name}:{m}: {line}" for m, line in sites(block, n - 1)]
    return out


def check_dagger(files: list[Path]) -> list[str]:
    return [
        f"{f.relative_to(ROOT)}:{n}: {line}"
        for f in sorted(files)
        if f.relative_to(ROOT).as_posix().startswith(".dagger/") and f.suffix == ".go"
        for n, line in sites(f.read_text(encoding="utf-8"), embedded=True)
        if not line.startswith("//")
    ]


def main() -> int:
    files = [f for f in tracked() if f.is_file() and f.relative_to(ROOT).as_posix() != OWN_PIN]
    scripts = shell_scripts(files)
    if not scripts:
        print("check-grep-q-pipefail: read no shell script; the scan found nothing to check.")
        return 2
    failures = (
        check_scripts(scripts)
        + check_workflows(files)
        + check_makefile(ROOT / "Makefile")
        + check_dagger(files)
    )
    if failures:
        print("❌ a pipe into `grep -q` where `pipefail` is on: a match can read as a miss.")
        print("   Write `grep -c … >/dev/null`, which reads all its input.\n")
        for f in failures:
            print(f"  {f}")
        print(f"\n{len(failures)} site(s).")
        return 1
    print(f"✅ no pipe into `grep -q` under `pipefail` ({len(scripts)} shell scripts read).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
