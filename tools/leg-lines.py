#!/usr/bin/env python3
"""Print the commands a Dagger shard runs, as a local mirror target runs them.

`make test-leg` and `make test-integration-postgres` run what `.dagger/main.go`'s
`Test` and `integrationPostgres` run, on a workstation. The lines are written once,
in main.go; the make targets read them here at run time, so there is no second copy
to fall behind.

    python3 tools/leg-lines.py Test                  # the lines, one per line
    python3 tools/leg-lines.py integrationPostgres

Three kinds of shard line do not carry over, and each is handled by its shape:

* `bash tools/ci-target-canary.sh -- test …` is the #880 stale-target detector in its
  `--no-run` form. It builds, and the shard runs the same suite on its own line, so
  the mirror drops it. The detector exists for the leg's persistent target volume,
  which a workstation does not mount.
* `bash tools/ci-target-canary.sh -- build …` IS the shard's build step; the mirror
  runs the `cargo build …` it wraps.
* The lines in CI_ONLY install the pinned confiture the postgres shard puts on PATH.
  Locally, confiture is whatever `confiture` the PATH resolves.

`set -e` is dropped: the make targets run the lines under `bash -e`.

Any piece of a line this reader cannot resolve (a `fmt.Sprintf`, a local variable, a
computed feature list) is FATAL: dropping it would run a different command than the
shard does, and a mirror that runs less than CI while reporting green is worse than
no mirror.

The cargo test lines printed must also be exactly the ones
tools/check-suite-coverage.py attributes to the function. That gate certifies what
each leg runs; two readers of main.go that disagree mean one of them misreads it,
which is FATAL too.

Its red capability is pinned by `tools/tests/leg_lines_test.sh`. `LEG_LINES_ROOT`
points it at another tree (the test's fixtures).
"""

from __future__ import annotations

import importlib.util
import os
import re
import shlex
import sys
from pathlib import Path

ROOT = Path(os.environ.get("LEG_LINES_ROOT") or Path(__file__).resolve().parent.parent)
TOOLS = Path(__file__).resolve().parent

# The shards a make target mirrors.
SHARDS = ("Test", "integrationPostgres")

CANARY = "bash tools/ci-target-canary.sh -- "

# Shard lines that set up the CI container and have no local counterpart, by prefix.
CI_ONLY = (
    "apt-get install -y --no-install-recommends python3-venv",
    "python3 -m venv /tmp/uv",
    "/tmp/uv/bin/pip install --quiet uv==",
    "/tmp/uv/bin/uv venv --quiet --python ",
    "/tmp/uv/bin/uv pip install --quiet --python /tmp/confiture/bin/python -r ",
    "export PATH=/tmp/confiture/bin:$PATH",
)

GO_CONST = re.compile(r'^\s*([a-zA-Z]\w*)\s*=\s*"([^"]*)"\s*$', re.MULTILINE)
ENV_ASSIGN = re.compile(r"^[A-Za-z_]\w*=")
UNRESOLVED = "\x00"


class Unreadable(Exception):
    pass


def strip_go_comment(line: str) -> str:
    """Drop a trailing `// …` from a Go source line, ignoring `//` inside quotes."""
    in_quote = escaped = False
    for idx in range(len(line)):
        ch = line[idx]
        if escaped:
            escaped = False
        elif ch == "\\":
            escaped = True
        elif ch == '"':
            in_quote = not in_quote
        elif not in_quote and line.startswith("//", idx):
            return line[:idx]
    return line


def split_concat(entry: str) -> list[str]:
    """Split a Go expression on `+`, ignoring a `+` inside a string literal."""
    pieces, buf = [], ""
    in_quote = escaped = False
    for ch in entry:
        if escaped:
            escaped = False
        elif ch == "\\":
            escaped = True
        elif ch == '"':
            in_quote = not in_quote
        elif ch == "+" and not in_quote:
            pieces.append(buf)
            buf = ""
            continue
        buf += ch
    pieces.append(buf)
    return pieces


def resolve(entry: str, consts: dict[str, str]) -> str:
    """One `"…" + const + "…"` element of the script slice, consts substituted."""
    out = ""
    for piece in split_concat(entry.rstrip(",")):
        piece = piece.strip()
        if len(piece) >= 2 and piece.startswith('"') and piece.endswith('"'):
            out += piece[1:-1].replace('\\"', '"').replace("\\\\", "\\")
        elif piece in consts:
            out += consts[piece]
        else:
            out += UNRESOLVED + piece + UNRESOLVED
    return out.strip()


def script_lines(main_go: str, func: str) -> list[str]:
    """The commands of a shard's `script := strings.Join([]string{…}, "\\n")`, in
    order. An element may span source lines (`+` binds across a newline)."""
    head = re.search(rf"^func \(m \*FraiseqlCi\) {re.escape(func)}\(", main_go, re.MULTILINE)
    if not head:
        raise Unreadable(f"no `{func}` function in .dagger/main.go")
    body = main_go[head.start() :]
    nxt = re.search(r"^func ", body[1:], re.MULTILINE)
    body = body[: nxt.start() + 1] if nxt else body
    start = body.find("script := strings.Join([]string{")
    end = body.find('}, "\\n")', start)
    if start < 0 or end < 0:
        raise Unreadable(f"`{func}` has no `script := strings.Join([]string{{…}}, \"\\n\")`")
    consts = dict(GO_CONST.findall(main_go))
    lines: list[str] = []
    entry = ""
    for raw in body[start:end].splitlines()[1:]:
        stripped = strip_go_comment(raw).strip()
        if not stripped or (not entry and not stripped.startswith('"')):
            continue
        entry += stripped
        if entry.endswith("+"):
            continue
        lines.append(resolve(entry, consts))
        entry = ""
    if entry:
        lines.append(resolve(entry, consts))
    if not lines:
        raise Unreadable(f"read no line out of `{func}`'s script")
    return lines


def local_lines(lines: list[str], func: str) -> list[str]:
    out = []
    for line in lines:
        if UNRESOLVED in line and line.startswith("echo "):
            # A progress line: what it prints changes nothing the shard runs.
            line = re.sub(f"{UNRESOLVED}(.*?){UNRESOLVED}", r"<\1>", line)
        if UNRESOLVED in line:
            piece = line.split(UNRESOLVED)[1]
            raise Unreadable(
                f"`{func}` builds a line from `{piece}`, which this reader cannot resolve: "
                f"{line.replace(UNRESOLVED, '')}"
            )
        if line == "set -e" or line.startswith(CI_ONLY):
            continue
        if line.startswith(CANARY):
            args = line[len(CANARY) :].strip()
            if args.startswith("test "):
                continue
            if args.startswith("build "):
                out.append("cargo " + args)
                continue
            raise Unreadable(f"`{func}`: a canary line of neither form: {line}")
        out.append(line)
    return out


def canonical(cmd: str) -> str | None:
    """A cargo test/nextest command with env prefixes dropped and feature lists
    sorted; None for any other command."""
    toks = shlex.split(cmd)
    while toks and ENV_ASSIGN.match(toks[0]):
        toks.pop(0)
    if len(toks) < 2 or toks[0] != "cargo" or toks[1] not in ("test", "nextest"):
        return None
    out = []
    feats = False
    for tok in toks:
        if feats:
            tok = ",".join(sorted(f for f in tok.split(",") if f))
        feats = tok == "--features"
        out.append(tok)
    return " ".join(out)


def coverage_lines(main_go: str, func: str) -> list[str]:
    spec = importlib.util.spec_from_file_location("suite_coverage", TOOLS / "check-suite-coverage.py")
    cov = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cov)
    invocations, _ = cov.extract_invocations(main_go)
    return [inv.cmd for inv in invocations if inv.leg == func]


def leg_lines(func: str) -> list[str]:
    main_go = (ROOT / ".dagger" / "main.go").read_text(encoding="utf-8")
    lines = local_lines(script_lines(main_go, func), func)
    ours = sorted(c for c in (canonical(line) for line in lines) if c)
    theirs = sorted(c for c in (canonical(cmd) for cmd in coverage_lines(main_go, func)) if c)
    if ours != theirs:
        only_ours = [c for c in ours if c not in theirs]
        only_theirs = [c for c in theirs if c not in ours]
        raise Unreadable(
            f"`{func}`: this reader and check-suite-coverage.py read different cargo test "
            "lines out of .dagger/main.go\n"
            + "".join(f"  only here:            {c}\n" for c in only_ours)
            + "".join(f"  only in the coverage: {c}\n" for c in only_theirs)
        )
    if not ours:
        raise Unreadable(f"`{func}`: no cargo test line read; the reader went blind")
    return lines


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in SHARDS:
        print(f"usage: leg-lines.py {{{'|'.join(SHARDS)}}}", file=sys.stderr)
        return 2
    try:
        lines = leg_lines(sys.argv[1])
    except Unreadable as exc:
        print(f"leg-lines: FAIL — {exc}", file=sys.stderr)
        return 1
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
