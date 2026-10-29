#!/usr/bin/env python3
"""The crates this workspace publishes to crates.io, and the order to publish them in.

Both are derived, never written down. A crate is published when its manifest does not
say `publish = false`; it is published after every workspace crate it depends on.

    python3 tools/publish-order.py                  # the order, one crate per line
    python3 tools/publish-order.py --metadata F     # the order from a saved
                                                    # `cargo metadata --no-deps` JSON

A dependency counts whatever its kind, except a dev-dependency. An *optional*
dependency counts too: crates.io resolves every dependency a manifest declares, so
`fraiseql-server`'s optional `fraiseql-cdc-sinks` must already be published when
`fraiseql-server` is. A dev-dependency never reaches the registry's resolver and
would close cycles (a crate's tests depending on a crate that depends on it).

Ties are broken by name, so the order is the same on every run.

`publishable_workspace_crates` reads the manifests with `tomllib` instead of running
cargo, for the gates in the toolchain-free ShellGates container.
"""

from __future__ import annotations

import argparse
import heapq
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def is_publishable(publish: object) -> bool:
    """`publish` as a manifest or `cargo metadata` spells it: absent/None/true, or a registry list."""
    return publish is None or publish is True or (isinstance(publish, list) and bool(publish))


def publishable_workspace_crates(root: Path) -> list[str]:
    """Every workspace member whose manifest does not opt out of publishing."""
    members = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    found = []
    for pattern in members.get("workspace", {}).get("members", []):
        dirs = sorted(root.glob(pattern)) if "*" in pattern else [root / pattern]
        for d in dirs:
            manifest = d / "Cargo.toml"
            if not manifest.is_file():
                sys.exit(f"ERROR: workspace member {d} has no Cargo.toml")
            package = tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {})
            if "name" not in package:
                sys.exit(f"ERROR: {manifest} has no [package] name")
            if is_publishable(package.get("publish")):
                found.append(package["name"])
    return found


def publish_order(metadata: dict) -> list[str]:
    """The publishable packages of a `cargo metadata --no-deps` document, dependencies first."""
    packages = {p["name"]: p for p in metadata["packages"] if is_publishable(p.get("publish"))}
    needs = {
        name: {
            d["name"]
            for d in p["dependencies"]
            if d.get("kind") != "dev" and d["name"] in packages
        }
        for name, p in packages.items()
    }
    dependents: dict[str, list[str]] = {name: [] for name in packages}
    for name, deps in needs.items():
        for d in deps:
            dependents[d].append(name)
    waiting = {name: len(deps) for name, deps in needs.items()}
    ready = [name for name, n in waiting.items() if n == 0]
    heapq.heapify(ready)
    order = []
    while ready:
        name = heapq.heappop(ready)
        order.append(name)
        for dependent in dependents[name]:
            waiting[dependent] -= 1
            if waiting[dependent] == 0:
                heapq.heappush(ready, dependent)
    if len(order) != len(packages):
        stuck = sorted(name for name, n in waiting.items() if n > 0)
        sys.exit(f"ERROR: the publishable crates depend on each other in a cycle: {stuck}")
    return order


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--metadata", type=Path, help="a saved `cargo metadata --no-deps` JSON")
    args = parser.parse_args()
    if args.metadata:
        text = args.metadata.read_text(encoding="utf-8")
    else:
        text = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    print("\n".join(publish_order(json.loads(text))))


if __name__ == "__main__":
    main()
