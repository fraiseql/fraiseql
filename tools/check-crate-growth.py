#!/usr/bin/env python3
"""No commit grows a crate's production code by more than 300 lines unless it says why.

The crate-size ceilings in the root Cargo.toml (tools/check-crate-sizes.py) are a loose
backstop for slow, cumulative growth. This check bounds growth per commit instead:
each non-merge commit in a range is measured against its first parent, so it holds
inside a stack of commits as well as for one.

Growth is counted with check-crate-sizes.py's rules: production lines of a crate's
`src/`, so tests, docs and blank lines are free. For each crate whose `src/` a commit
changes, its production lines at the commit minus those at the parent (zero where the
crate has no `src/`) may be at most GROWTH_LIMIT. A commit that grows a crate further
carries a trailer, one per crate:

    Crate-Growth: fraiseql-core +1250 the fact-table roots are a new surface of the engine

The trailer counts only as a real git trailer (the closing trailer block of the
message, as `git log --format=%(trailers)` reads it), not a mention in the subject or
the body. Its crate must be the crate that grew, its number must cover the measured
growth, and its reason must be prose: a reason that is only issue references says
nothing to the next reader of the history.

Usage:
  check-crate-growth.py [--base REV] [--head REV] [--fallback REF]

`--head` defaults to HEAD. A `--base` that is empty, all zeros (a push creating a
branch), or not an ancestor of the head (a force-push) is replaced by the merge-base
of the head and `--fallback` (default origin/dev); with no `--base` at all, that
merge-base is the base. An empty range passes and says so. A shallow repository, or a
revision that does not resolve, is FATAL: a check that read no history must not pass.

Overrides, for testing: CRATE_GROWTH_ROOT=<git work tree>.
"""

from __future__ import annotations

import importlib.util
import io
import os
import re
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

GROWTH_LIMIT = 300
TRAILER = "Crate-Growth"
ROOT = Path(os.environ.get("CRATE_GROWTH_ROOT") or Path(__file__).resolve().parent.parent)
TRAILER_VALUE = re.compile(r"^(\S+)\s+\+([\d_]+)\s+(.*)$")
ZERO = re.compile(r"^0+$")

_spec = importlib.util.spec_from_file_location(
    "check_crate_sizes", Path(__file__).resolve().parent / "check-crate-sizes.py"
)
sizes = importlib.util.module_from_spec(_spec)
sys.modules["check_crate_sizes"] = sizes
_spec.loader.exec_module(sizes)


class Fatal(Exception):
    pass


def git(*args: str, check: bool = True) -> str:
    proc = subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise Fatal(f"git {' '.join(args)}: {proc.stderr.strip() or f'exit {proc.returncode}'}")
    return proc.stdout


def resolve(rev: str) -> str:
    return git("rev-parse", "--verify", "--quiet", f"{rev}^{{commit}}").strip()


def resolve_range(base: str | None, head: str, fallback: str) -> tuple[str, str]:
    if git("rev-parse", "--is-shallow-repository").strip() != "false":
        raise Fatal("the repository is shallow; check out the full history (fetch-depth: 0)")
    head_sha = resolve(head)
    if base and not ZERO.match(base):
        base_sha = resolve(base)
        ancestor = subprocess.run(
            ["git", "-C", str(ROOT), "merge-base", "--is-ancestor", base_sha, head_sha]
        ).returncode == 0
        if ancestor:
            return base_sha, head_sha
    return git("merge-base", resolve(fallback), head_sha).strip(), head_sha


_counts: dict[tuple[str, str], int] = {}


def production(rev: str | None, crate: str) -> int:
    """Production lines of `crates/<crate>/src` at `rev`; 0 where it has none."""
    if rev is None:
        return 0
    key = (rev, crate)
    if key not in _counts:
        path = f"crates/{crate}/src"
        if not git("ls-tree", "-d", "--name-only", rev, path).strip():
            _counts[key] = 0
        else:
            data = subprocess.run(
                ["git", "-C", str(ROOT), "archive", rev, path], capture_output=True, check=True
            ).stdout
            with tempfile.TemporaryDirectory() as tmp:
                with tarfile.open(fileobj=io.BytesIO(data)) as tar:
                    tar.extractall(tmp, filter="data")
                files = sizes.crate_files(Path(tmp) / path)
                _counts[key] = sum(c.production for c in files.values())
    return _counts[key]


def changed_crates(parent: str | None, commit: str) -> list[str]:
    if parent is None:
        names = git("ls-tree", "-r", "--name-only", commit).split()
    else:
        names = git("diff-tree", "--no-commit-id", "--name-only", "-r", parent, commit).split()
    return sorted({n.split("/")[1] for n in names if re.match(r"crates/[^/]+/src/", n)})


def trailers(commit: str) -> tuple[dict[str, tuple[int, str]], list[str]]:
    """The commit's Crate-Growth trailers by crate, and what is wrong with any of them."""
    values = git(
        "log", "-1", f"--format=%(trailers:key={TRAILER},valueonly,separator=%x00)", commit
    ).strip("\n")
    found: dict[str, tuple[int, str]] = {}
    problems = []
    for value in filter(None, (v.strip() for v in values.split("\0"))):
        m = TRAILER_VALUE.match(value)
        if not m:
            problems.append(f"`{TRAILER}: {value}` is not `{TRAILER}: <crate> +<lines> <reason>`")
            continue
        crate, lines, reason = m.group(1), int(m.group(2).replace("_", "")), m.group(3).strip()
        words = re.findall(r"[A-Za-z]{2,}", re.sub(r"\S*#\d+\S*", " ", reason))
        if len(words) < 3:
            problems.append(
                f"`{TRAILER}: {value}` gives no reason in words; say what the growth is for"
            )
            continue
        if crate in found:
            problems.append(f"two `{TRAILER}` trailers name `{crate}`")
            continue
        found[crate] = (lines, reason)
    return found, problems


def check(base: str, head: str) -> tuple[list[str], int, int]:
    errors: list[str] = []
    checked = merges = 0
    for line in git("rev-list", "--reverse", "--parents", f"{base}..{head}").splitlines():
        commit, *parents = line.split()
        if len(parents) > 1:
            merges += 1
            continue
        checked += 1
        parent = parents[0] if parents else None
        subject = git("log", "-1", "--format=%s", commit).strip()
        where = f"{commit[:12]} ({subject[:60]})"
        declared, problems = trailers(commit)
        errors.extend(f"{where}: {p}" for p in problems)
        for crate in changed_crates(parent, commit):
            before, after = production(parent, crate), production(commit, crate)
            grown = after - before
            print(f"  {commit[:12]} {crate:<24} {grown:+6d} ({before} → {after})")
            if grown <= GROWTH_LIMIT:
                continue
            said = declared.get(crate)
            if said is None:
                errors.append(
                    f"{where}: {crate} grows by {grown} production lines ({before} → {after}), "
                    f"over the {GROWTH_LIMIT} a commit may add without a "
                    f"`{TRAILER}: {crate} +{grown} <reason>` trailer"
                )
            elif said[0] < grown:
                errors.append(
                    f"{where}: {crate} grows by {grown} production lines ({before} → {after}), "
                    f"more than the +{said[0]} its `{TRAILER}` trailer admits"
                )
    return errors, checked, merges


def main(argv: list[str]) -> int:
    opts = {"--base": None, "--head": "HEAD", "--fallback": "origin/dev"}
    args = list(argv)
    while args:
        flag = args.pop(0)
        if flag not in opts or not args:
            print(__doc__.split("Usage:")[1].split("\n\n")[0], file=sys.stderr)
            return 2
        opts[flag] = args.pop(0)
    try:
        base, head = resolve_range(opts["--base"], opts["--head"], opts["--fallback"])
        errors, checked, merges = check(base, head)
    except (Fatal, subprocess.CalledProcessError) as exc:
        print(f"crate growth: FAIL — {exc}", file=sys.stderr)
        return 1
    span = f"{base[:12]}..{head[:12]}"
    skipped = f", {merges} merge(s) skipped" if merges else ""
    if errors:
        print("\n".join(f"crate growth: FAIL — {e}" for e in errors), file=sys.stderr)
        print(f"crate growth: {checked} commit(s) checked in {span}{skipped}", file=sys.stderr)
        return 1
    print(
        f"crate growth: ok — {checked} commit(s) checked in {span}{skipped}; none grows a "
        f"crate by more than {GROWTH_LIMIT} production lines without a `{TRAILER}` trailer"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
