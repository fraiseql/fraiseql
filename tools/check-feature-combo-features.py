#!/usr/bin/env python3
"""Each feature combo builds the features it names, and no others.

`.dagger/feature-combos.go` lists crate + feature sets the feature-matrix leg compiles.
A combo is only worth its line if cargo builds what the line says. Cargo unifies
features across everything one invocation builds, so a combo that also builds test
targets (`allTargets`, or `clippy`, which adds `--all-targets`) gets the features its
dev-dependencies ask for too. Five `--no-default-features` server combos were compiled
with `auth`, `cli` and the defaults switched back on this way (#2096): the
`fraiseql-cli` dev-dependency's `run-server` depended on this crate with its defaults.
Each combo's name said "no auth"; each build had it.

For every combo this compares the crate's own features that cargo enables
(`cargo tree -e features -i <crate>`) with the features the combo requests, closed
over the crate's `[features]` table, plus `default` when the defaults are on. Test
targets bring the dev-dependencies' edges, so a combo that builds them is read with
every edge kind; one that builds the library and binaries only, without the
dev-dependency edges. Any difference fails, naming the combo and the features.

Nothing compiles: `cargo metadata` and `cargo tree` only, under `--locked`.

Usage:
    tools/check-feature-combo-features.py [--offline]
"""

from __future__ import annotations

import argparse
import json
import re
import runpy
import subprocess
import sys
from pathlib import Path

NODE_RE = re.compile(r'^(?P<crate>\S+) feature "(?P<feature>[^"]+)"')
ROOT_RE = re.compile(r"^(?P<crate>\S+) v\d")


class CheckError(Exception):
    pass


def load_combos_module(root: Path) -> dict:
    """The globals of `tools/feature-combos.py`, which parses the Go list the leg runs."""
    return runpy.run_path(str(root / "tools" / "feature-combos.py"))


def cargo(root: Path, args: list[str], cargo_flags: list[str]) -> str:
    out = subprocess.run(
        ["cargo", *args, *cargo_flags],
        cwd=root,
        capture_output=True,
        text=True,
        stdin=subprocess.DEVNULL,
        check=False,
    )
    if out.returncode != 0:
        raise CheckError(
            f"`cargo {' '.join(args)}` exited {out.returncode}:\n{out.stderr}"
        )
    return out.stdout


def feature_tables(
    root: Path, cargo_flags: list[str]
) -> dict[str, dict[str, list[str]]]:
    meta = json.loads(
        cargo(root, ["metadata", "--no-deps", "--format-version", "1"], cargo_flags)
    )
    return {p["name"]: p["features"] for p in meta["packages"]}


def implied(table: dict[str, list[str]], requested: list[str]) -> set[str]:
    """`requested`, and every feature of the same crate they enable."""
    enabled: set[str] = set()
    todo = list(requested)
    while todo:
        feature = todo.pop()
        if feature in enabled:
            continue
        if feature not in table:
            # `default` on a crate that declares none; anything else is cargo's to
            # refuse, and `cargo tree` will.
            continue
        enabled.add(feature)
        for item in table[feature]:
            if item.startswith("dep:"):
                continue
            if "/" in item:
                # `name/feat` enables `name` too, when it is not weak (`name?/feat`).
                # Only a name that is also one of this crate's features matters here.
                name = item.split("/", 1)[0]
                if not name.endswith("?"):
                    todo.append(name)
            else:
                todo.append(item)
    return enabled


def builds_test_targets(combo: dict) -> bool:
    return bool(combo["allTargets"] or combo["clippy"])


def tree_args(combo: dict) -> list[str]:
    crate = combo["crate"]
    edges = "features" if builds_test_targets(combo) else "normal,build,features"
    args = [
        "tree",
        "--color",
        "never",
        "--prefix",
        "none",
        "-p",
        crate,
        "-e",
        edges,
        "-i",
        crate,
    ]
    if combo["noDefaultFeatures"]:
        args.append("--no-default-features")
    if combo["features"]:
        args += ["--features", ",".join(combo["features"])]
    return args


def enabled(
    root: Path, combo: dict, table: dict[str, list[str]], cargo_flags: list[str]
) -> set[str]:
    crate = combo["crate"]
    out = cargo(root, tree_args(combo), cargo_flags)
    lines = out.splitlines()
    if not any(
        (m := ROOT_RE.match(line)) and m.group("crate") == crate for line in lines
    ):
        # Without the crate's own line the feature set below would be empty, and an
        # empty set matches a combo that requests nothing: refuse rather than pass.
        raise CheckError(
            f"{combo['name']}: `cargo tree` printed no line for {crate}:\n{out}"
        )
    return {
        m.group("feature")
        for line in lines
        if (m := NODE_RE.match(line))
        and m.group("crate") == crate
        # An optional dependency's name is a node too; only the crate's features count.
        and m.group("feature") in table
    }


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--offline", action="store_true", help="pass --offline to cargo")
    ap.add_argument("--root", type=Path, default=None, help="repository root")
    args = ap.parse_args()

    cargo_flags = ["--locked", *(["--offline"] if args.offline else [])]
    try:
        module = load_combos_module(
            args.root or Path(__file__).absolute().parent.parent
        )
    except (CheckError, OSError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    try:
        root = args.root or module["repo_root"]()
        combos = module["load"](root)
        tables = feature_tables(root, cargo_flags)
        bad = []
        for combo in combos:
            crate = combo["crate"]
            if crate not in tables:
                raise CheckError(f"{combo['name']}: no workspace package named {crate}")
            table = tables[crate]
            want = implied(
                table,
                [
                    *combo["features"],
                    *([] if combo["noDefaultFeatures"] else ["default"]),
                ],
            )
            got = enabled(root, combo, table, cargo_flags)
            if got != want:
                bad.append((combo, sorted(got - want), sorted(want - got)))
    except (CheckError, module["ParseError"]) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    for combo, extra, missing in bad:
        targets = (
            "with its test targets"
            if builds_test_targets(combo)
            else "library and binaries"
        )
        print(
            f"❌ {combo['name']} ({combo['crate']}, {targets}): "
            f"cargo enables {extra or 'nothing'} beyond what it names, and leaves out "
            f"{missing or 'nothing'} it names."
        )
    if bad:
        print(
            f"\n{len(bad)} of {len(combos)} combo(s) do not build the features they name. "
            "A dev-dependency that enables a feature of the crate does so for every build "
            "of its test targets; see the comment on `allTargets` in .dagger/feature-combos.go."
        )
        return 1
    print(f"✅ all {len(combos)} combos build exactly the features they name")
    return 0


if __name__ == "__main__":
    sys.exit(main())
