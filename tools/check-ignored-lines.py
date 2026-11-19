#!/usr/bin/env python3
"""Every `-- --ignored` line names a target that holds an `#[ignore]` test.

`cargo test … -- --ignored` runs ONLY the tests marked `#[ignore]`. A line pointed at a
target with none prints `test result: ok. 0 passed; … N filtered out` and exits 0, so it
reads as a suite that ran (#2103). Five Makefile lines did that, among them
`--test integration_test -- --ignored` in `test-full` and `test-integration`.

The rule is conservative on purpose: a line fails only when NO source it can compile
contains an `#[ignore]` attribute at all, whatever features are on. So it never fails a
line that runs a test. It does not catch an `#[ignore]` that the line's features compile
out, nor a name filter that matches none of them.

Lines are read from the three places that run them:
  - `.dagger/main.go` and `.github/workflows/*`, through tools/check-suite-coverage.py's
    own parser, so the two gates read the same commands;
  - the Makefile, one recipe line at a time, `VAR=value` prefixes dropped.

Exit 0 when every line has an `#[ignore]` test in scope, 1 when one does not, 2 when the
Makefile yields no `--ignored` line at all (a parser that reads nothing passes nothing).
"""

from __future__ import annotations

import fnmatch
import re
import runpy
import shlex
import sys
from pathlib import Path

REPO = Path(__file__).absolute().parent.parent
COV = runpy.run_path(str(REPO / "tools" / "check-suite-coverage.py"), run_name="check_ignored_lines")
IGNORE_ATTR = re.compile(r"#\[ignore\b")
ENV_PREFIX = re.compile(r"^(?:[A-Za-z_][A-Za-z0-9_]*=(?:\"[^\"]*\"|'[^']*'|\S*)\s+)+")


def makefile_lines(path: Path) -> list[tuple[str, str]]:
    """(`Makefile:N`, command) for each recipe line that runs `-- --ignored`."""
    out = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        s = line.strip()
        if s.startswith("#") or "cargo" not in s or "--ignored" not in s.split():
            continue
        s = ENV_PREFIX.sub("", s.lstrip("@"))
        out.append((f"{path.name}:{n}", s))
    return out


def lib_ignored(crate_dir: Path) -> int:
    total = 0
    for f in sorted((crate_dir / "src").rglob("*.rs")):
        code = COV["scrub_source"](f.read_text(encoding="utf-8", errors="replace"), blank_strings=True)
        total += len(IGNORE_ATTR.findall(code))
    return total


def main() -> int:
    crate_dirs = {}
    binaries: dict[str, list] = {}
    for cd in sorted(p for p in (REPO / "crates").iterdir() if (p / "Cargo.toml").exists()):
        manifest = COV["parse_cargo_toml"](cd)
        name = manifest["package"]["name"]
        crate_dirs[name] = cd
        binaries[name] = COV["discover_binaries"](cd, name, manifest)

    dagger, _ = COV["extract_invocations"]((REPO / ".dagger" / "main.go").read_text(encoding="utf-8"))
    workflows, _ = COV["extract_workflow_invocations"]()
    lines = [(f"main.go {inv.leg}", inv) for inv in dagger]
    lines += [(inv.leg, inv) for inv in workflows]
    make = makefile_lines(REPO / "Makefile")
    if not make:
        print("ERROR: the Makefile yielded no `-- --ignored` line; the parser read nothing.")
        return 2
    for where, cmd in make:
        inv = COV["Invocation"](where, cmd)
        COV["parse_cmd"](inv)
        lines.append((where, inv))

    checked, failures = 0, []
    for where, inv in lines:
        if not inv.ignored_mode or inv.include_ignored:
            continue
        checked += 1
        runner = shlex.split(inv.cmd.split(" -- ")[0])
        crates = [inv.crate] if inv.crate else [c for c in crate_dirs if c not in inv.excludes]
        selectors = {"--lib", "--tests", "--test", "--bins", "--bin", "--examples", "--benches", "--doc"}
        any_selector = bool(selectors & set(runner))
        with_lib = inv.lib_only or "--tests" in runner or not any_selector
        all_bins = "--tests" in runner or not any_selector
        found = 0
        for c in crates:
            if c not in crate_dirs:
                failures.append(f"{where}: `-p {c}` names no crate under crates/")
                break
            if with_lib:
                found += lib_ignored(crate_dirs[c])
            for b in binaries[c]:
                if all_bins or any(fnmatch.fnmatchcase(b.name, t) for t in inv.tests):
                    found += b.n_ignored
        else:
            if found == 0:
                failures.append(
                    f"{where}: `{inv.cmd.strip()}` runs only `#[ignore]` tests, and nothing it "
                    f"can compile has one: it runs 0 tests and exits 0. Drop `--ignored`, or "
                    f"delete the line if another one runs the suite."
                )

    if failures:
        for f in failures:
            print(f"❌ {f}")
        print(f"\n{len(failures)} of {checked} `--ignored` lines select no `#[ignore]` test.")
        return 1
    print(f"✅ all {checked} `--ignored` lines select at least one `#[ignore]` test "
          f"({len(make)} from the Makefile).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
