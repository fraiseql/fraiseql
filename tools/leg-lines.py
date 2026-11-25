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

Two more modes serve coverage. fraiseql-core and fraiseql-db are measured by
`CoveragePostgres` in main.go, over the tests their legs run, database-backed ones
included; `.github/workflows/coverage.yml` measures the other crates:

    python3 tools/leg-lines.py --const coreTestFeatures   # one const's value
    python3 tools/leg-lines.py --coverage                 # check both

`--const` prints a const of main.go; a name main.go does not define is FATAL.
`--coverage` holds the split. In the workflow's `run:` blocks (a YAML or shell comment,
or an `echo`, runs nothing and is not read), no `--features` value may be a literal,
every `--const` must name a const, and no `cargo llvm-cov` command may measure a crate
of COVERAGE_CRATES: none may name one with `-p`, and a `--workspace` one must
`--exclude` each. In main.go, `CoveragePostgres` must read its lines through
`leg-lines.py --crates` naming every crate of COVERAGE_CRATES.

One more serves `CoveragePostgres` in main.go, which measures fraiseql-core and
fraiseql-db over the services `integrationPostgres` binds:

    python3 tools/leg-lines.py --crates fraiseql-core,fraiseql-db Test integrationPostgres

`--crates` prints, from the named shards in order, the cargo test lines whose `-p` names
one of the crates, as the shard runs them. Nothing else of the shard is printed. A crate
no shard tests is FATAL, and so is a crate none of the named shards prints a line for:
coverage measured over no line of a crate is a number about nothing.

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

# The crates CoveragePostgres measures over their legs' lines, and coverage.yml does not.
COVERAGE_CRATES = ("fraiseql-core", "fraiseql-db")
COVERAGE_FUNC = "CoveragePostgres"
CRATES_READ = re.compile(r"leg-lines\.py\s+--crates\s+([\w,-]+)")
COVERAGE_WORKFLOW = Path(".github") / "workflows" / "coverage.yml"
RUN_KEY = re.compile(r"^(\s*)(?:-\s+)?run:\s*(.*)$")
CONST_READ = re.compile(r"leg-lines\.py\s+--const\s+([A-Za-z_]\w*)")

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


def flag_values(toks: list[str], *flags: str) -> set[str]:
    """The values of `flags` among `toks`, as `--flag v` or `--flag=v`."""
    found = set()
    for idx, tok in enumerate(toks):
        if tok in flags and idx + 1 < len(toks):
            found.add(toks[idx + 1])
        elif tok.startswith(tuple(f"{flag}=" for flag in flags)):
            found.add(tok.split("=", 1)[1])
    return found


def packages(line: str) -> set[str]:
    """The `-p`/`--package` crates of a cargo test line; empty for any other line."""
    if canonical(line) is None:
        return set()
    return flag_values(shlex.split(line), "-p", "--package")


def crate_lines(crates: list[str], shards: list[str]) -> list[str]:
    """The cargo test lines of `shards`, in order, that test one of `crates`."""
    read = {shard: leg_lines(shard) for shard in SHARDS}
    tested = set().union(*(packages(line) for lines in read.values() for line in lines))
    unknown = [crate for crate in crates if crate not in tested]
    if unknown:
        raise Unreadable(
            "no shard tests " + ", ".join(f"`{c}`" for c in unknown)
            + f" (the crates {', '.join(SHARDS)} test: {', '.join(sorted(tested))})"
        )
    out = [line for shard in shards for line in read[shard] if packages(line) & set(crates)]
    for crate in crates:
        if not any(crate in packages(line) for line in out):
            raise Unreadable(f"`{', '.join(shards)}` run no cargo test line for `{crate}`")
    return out


def go_consts() -> dict[str, str]:
    return dict(GO_CONST.findall((ROOT / ".dagger" / "main.go").read_text(encoding="utf-8")))


def const_value(name: str) -> str:
    consts = go_consts()
    if name not in consts:
        raise Unreadable(f"`{name}` is not a const of .dagger/main.go")
    return consts[name]


def run_blocks(workflow: str) -> list[tuple[int, str]]:
    """The shell of each `run:` step, with the line of its `run:` key: a block
    scalar's lines (those indented past the key), or the inline value."""
    lines = workflow.splitlines()
    blocks = []
    i = 0
    while i < len(lines):
        m = RUN_KEY.match(lines[i])
        i += 1
        if not m:
            continue
        key_line, indent, value = i, len(m.group(1)), m.group(2).strip()
        if not value.startswith(("|", ">")):
            blocks.append((key_line, value))
            continue
        body = []
        while i < len(lines) and (
            not lines[i].strip() or len(lines[i]) - len(lines[i].lstrip()) > indent
        ):
            body.append(lines[i])
            i += 1
        blocks.append((key_line, "\n".join(body)))
    return blocks


def shell_commands(block: str) -> list[str]:
    """A run block's commands: `\\` continuations joined, comment lines dropped. As in
    bash, a comment line is not continued by a trailing `\\` and ends the command it
    sits in, so the line after it is a command of its own."""
    out, buf = [], ""
    for raw in block.splitlines():
        line = raw.strip()
        if line.startswith("#"):
            if buf:
                out.append(buf)
                buf = ""
            continue
        if not buf and not line:
            continue
        if line.endswith("\\"):
            buf += line[:-1] + " "
            continue
        out.append(buf + line)
        buf = ""
    if buf:
        out.append(buf)
    return out


def command_tokens(command: str) -> list[str]:
    """A run command's words; empty for an `echo`, which runs nothing it names."""
    try:
        toks = shlex.split(command, comments=True)
    except ValueError as exc:
        raise Unreadable(f"coverage.yml: cannot split `{command}`: {exc}") from exc
    return [] if not toks or toks[0] in ("echo", "printf") else toks


def measured_crates(toks: list[str]) -> set[str]:
    """The COVERAGE_CRATES a `cargo llvm-cov` command measures or reports."""
    if toks[:2] != ["cargo", "llvm-cov"] or toks[2:3] in (["clean"], ["show-env"]):
        return set()
    named = flag_values(toks, "-p", "--package")
    if "--workspace" in toks or "--all" in toks:
        named |= set(COVERAGE_CRATES) - flag_values(toks, "--exclude")
    return named & set(COVERAGE_CRATES)


def literal_features(command: str) -> list[str]:
    """The `--features` values of a command that are literals (expand no `$`)."""
    toks = command_tokens(command)
    found = []
    for idx, tok in enumerate(toks):
        if tok.startswith("--features="):
            value = tok.split("=", 1)[1]
        elif tok == "--features" and idx + 1 < len(toks):
            value = toks[idx + 1]
        else:
            continue
        if "$" not in value:
            found.append(value)
    return found


def function_body(main_go: str, func: str) -> str:
    head = re.search(rf"^func \(m \*FraiseqlCi\) {re.escape(func)}\(", main_go, re.MULTILINE)
    if not head:
        raise Unreadable(f"no `{func}` function in .dagger/main.go")
    body = main_go[head.start() :]
    nxt = re.search(r"^func ", body[1:], re.MULTILINE)
    return body[: nxt.start() + 1] if nxt else body


def check_coverage() -> list[str]:
    path = ROOT / COVERAGE_WORKFLOW
    if not path.is_file():
        raise Unreadable(f"{COVERAGE_WORKFLOW} not found")
    main_go = (ROOT / ".dagger" / "main.go").read_text(encoding="utf-8")
    consts = dict(GO_CONST.findall(main_go))
    errors = []
    blocks = run_blocks(path.read_text(encoding="utf-8"))
    if not blocks:
        raise Unreadable(f"{COVERAGE_WORKFLOW}: no `run:` step read; the reader went blind")
    for lineno, block in blocks:
        for command in shell_commands(block):
            for value in literal_features(command):
                errors.append(
                    f"{COVERAGE_WORKFLOW}:{lineno}: a literal `--features {value}`; read the "
                    "set from .dagger/main.go with `python3 tools/leg-lines.py --const <name>`"
                )
            for name in CONST_READ.findall(command):
                if name not in consts:
                    errors.append(
                        f"{COVERAGE_WORKFLOW}:{lineno}: `--const {name}` is not a const of "
                        ".dagger/main.go"
                    )
            for crate in sorted(measured_crates(command_tokens(command))):
                errors.append(
                    f"{COVERAGE_WORKFLOW}:{lineno}: measures `{crate}`, which "
                    f"{COVERAGE_FUNC} measures over its legs' lines and the services they need"
                )
    read = set()
    for crates in CRATES_READ.findall(function_body(main_go, COVERAGE_FUNC)):
        read |= set(crates.split(","))
    errors.extend(
        f".dagger/main.go: {COVERAGE_FUNC} does not read `{crate}`'s lines through "
        "`leg-lines.py --crates`; nothing measures it"
        for crate in COVERAGE_CRATES
        if crate not in read
    )
    return errors


def main() -> int:
    args = sys.argv[1:]
    try:
        if len(args) == 2 and args[0] == "--const":
            print(const_value(args[1]))
            return 0
        if len(args) >= 3 and args[0] == "--crates":
            crates = [c for c in args[1].split(",") if c]
            shards = args[2:]
            bad = [shard for shard in shards if shard not in SHARDS]
            if not crates or bad:
                raise Unreadable(
                    "usage: --crates CRATE[,CRATE…] SHARD…, with SHARD one of "
                    + ", ".join(SHARDS)
                    + (f" (not a shard: {', '.join(bad)})" if bad else "")
                )
            print("\n".join(crate_lines(crates, shards)))
            return 0
        if args == ["--coverage"]:
            errors = check_coverage()
            for error in errors:
                print(f"leg-lines: FAIL — {error}", file=sys.stderr)
            return 1 if errors else 0
    except Unreadable as exc:
        print(f"leg-lines: FAIL — {exc}", file=sys.stderr)
        return 1
    if len(args) != 1 or args[0] not in SHARDS:
        print(
            f"usage: leg-lines.py {{{'|'.join(SHARDS)}}} | --const NAME | --coverage"
            " | --crates CRATE[,CRATE…] SHARD…",
            file=sys.stderr,
        )
        return 2
    try:
        lines = leg_lines(args[0])
    except Unreadable as exc:
        print(f"leg-lines: FAIL — {exc}", file=sys.stderr)
        return 1
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
