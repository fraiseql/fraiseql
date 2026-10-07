#!/usr/bin/env python3
"""Which official SDK suites a push must run, as a GitHub Actions matrix.

`.github/workflows/sdk-suites.yml` is ONE unfiltered, required workflow (#1467). The
per-SDK workflows it replaced were `paths:`-filtered, and GitHub reports a filtered-out
required check as "not run" rather than "passed", so none of them could be required and
together they gated nothing.

Unfiltered means the workflow starts on every push; this script decides what it runs.
It diffs the push against its base and emits one matrix entry per (SDK, toolchain
version) for every official SDK whose directory changed. A change to the suite's own
definition (`tools/sdk-suite.sh`, this script, the workflow) runs every SDK.

It fails toward running: an unknown base (a new branch whose base cannot be found, a
force-push past the old head) runs every SDK rather than none.

Usage:
  sdk-suites-matrix.py --base <sha> [--head <sha>]   prints JSON, writes $GITHUB_OUTPUT
  sdk-suites-matrix.py --all                         every SDK (a deliberate dispatch)
  sdk-suites-matrix.py --sdks                        prints the table's SDK keys

Output (also to $GITHUB_OUTPUT when set):
  matrix={"include": [...]}   any=true|false
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys

# key → toolchain versions. The key is the SDK's directory without `fraiseql-`, and
# the argument `tools/sdk-suite.sh` takes. Both ends of each supported range, as the
# per-SDK workflows ran them; `tools/check-sdk-workflow-coverage.py` requires every
# directory under sdks/official/ to have a row here.
VERSIONS: dict[str, list[str]] = {
    "csharp": ["8.0.x", "9.0.x"],
    "dart": ["stable", "3.6.0"],
    "elixir": ["1.15/26", "1.15/27", "1.16/26", "1.16/27", "1.17/26", "1.17/27"],
    "fsharp": ["9.0.x"],
    "go": ["1.23", "1.24"],
    "java": ["21"],
    "php": ["8.2", "8.3"],
    "python": ["3.10", "3.11", "3.12", "3.13"],
    "ruby": ["3.2", "3.3"],
    "rust": ["stable", "beta"],
    "typescript": ["20", "22"],
}

# Extra entries that are not `tools/sdk-suite.sh` runs.
EXTRA: list[dict[str, str]] = [
    # Dialyzer's PLT takes minutes to build, so it is not part of the local suite.
    {"sdk": "elixir", "version": "1.17/27", "task": "dialyzer"},
]

# A change to any of these changes what every suite means.
SUITE_DEFINITION = (
    "tools/sdk-suite.sh",
    "tools/sdk-suites-matrix.py",
    ".github/workflows/sdk-suites.yml",
)

ZERO_SHA = "0" * 40


def git(*args: str) -> str | None:
    proc = subprocess.run(["git", *args], capture_output=True, text=True, check=False)
    return proc.stdout if proc.returncode == 0 else None


def changed_paths(base: str, head: str) -> list[str] | None:
    """Paths changed by the push, or None when the base cannot be resolved."""
    if not base or base == ZERO_SHA:
        # A new branch: compare with where it left the trunk.
        base = (git("merge-base", "origin/dev", head) or "").strip()
        if not base:
            return None
    out = git("diff", "--name-only", f"{base}...{head}")
    if out is None:
        return None
    return [line for line in out.splitlines() if line]


def touched(paths: list[str] | None) -> list[str]:
    if paths is None or any(p in SUITE_DEFINITION for p in paths):
        return sorted(VERSIONS)
    return sorted(
        sdk for sdk in VERSIONS if any(p.startswith(f"sdks/official/fraiseql-{sdk}/") for p in paths)
    )


def entries(sdks: list[str]) -> list[dict[str, str]]:
    out = [{"sdk": s, "version": v, "task": "suite"} for s in sdks for v in VERSIONS[s]]
    out += [dict(e) for e in EXTRA if e["sdk"] in sdks]
    for e in out:
        if e["sdk"] == "elixir":
            e["elixir"], e["otp"] = e["version"].split("/")
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default="")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--sdks", action="store_true")
    args = parser.parse_args()

    if args.sdks:
        print("\n".join(sorted(VERSIONS)))
        return 0

    paths = None if args.all else changed_paths(args.base, args.head)
    sdks = touched(paths)
    include = entries(sdks)
    matrix = json.dumps({"include": include}, separators=(",", ":"))
    any_ = "true" if include else "false"

    if args.all:
        why = "--all: running every SDK"
    elif paths is None:
        why = "base unresolved: running every SDK"
    else:
        why = f"{len(paths)} changed path(s)"
    print(f"{why}; SDKs: {', '.join(sdks) or 'none'}", file=sys.stderr)
    print(matrix)
    if out := os.environ.get("GITHUB_OUTPUT"):
        with open(out, "a", encoding="utf-8") as fh:
            fh.write(f"matrix={matrix}\nany={any_}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
