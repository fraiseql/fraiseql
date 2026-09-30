#!/usr/bin/env python3
"""Merge the per-crate CycloneDX SBOMs of the published crates into one release SBOM.

`cargo cyclonedx` (0.5.x) writes one BOM per workspace crate, next to that crate's
Cargo.toml; it has no whole-workspace output. A release ships the published crates, so
the release SBOM is the union of their BOMs: every published crate as a component, every
dependency of any of them once (deduplicated by `bom-ref`), and every dependency edge,
under one product component, `fraiseql` at the release version.

The published crates are read from `cargo metadata` (a package whose `publish` is not
`[]`), so there is no second hand-kept list to drift from the workspace.

Usage:
    cargo cyclonedx --format json --all --spec-version 1.5 --override-filename <part>
    python3 tools/merge-sbom.py --part <part> --version 2.15.0 --output fraiseql-2.15.0-sbom.json
"""

import argparse
import json
import subprocess
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path


def published_packages() -> list[tuple[str, Path]]:
    """(name, manifest directory) of every workspace member cargo may publish."""
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )
    members = set(meta["workspace_members"])
    return sorted(
        (pkg["name"], Path(pkg["manifest_path"]).parent)
        for pkg in meta["packages"]
        if pkg["id"] in members and pkg.get("publish") != []
    )


def merge(boms: list[dict], version: str) -> dict:
    components: dict[str, dict] = {}
    edges: dict[str, set[str]] = {}
    spec = {b.get("specVersion") for b in boms}
    if len(spec) != 1:
        sys.exit(f"merge-sbom: the per-crate BOMs disagree on specVersion: {sorted(spec)}")

    def add(component: dict) -> None:
        ref = component.get("bom-ref") or component.get("purl")
        if not ref:
            sys.exit(f"merge-sbom: a component has neither bom-ref nor purl: {component.get('name')}")
        components.setdefault(ref, component)

    for bom in boms:
        crate = bom.get("metadata", {}).get("component")
        if crate:
            # The crate itself is part of what ships: a library component of the release.
            crate = dict(crate)
            crate["type"] = "library"
            crate.pop("components", None)
            add(crate)
        for component in bom.get("components", []):
            add(component)
        for dep in bom.get("dependencies", []):
            edges.setdefault(dep["ref"], set()).update(dep.get("dependsOn", []))

    return {
        "bomFormat": "CycloneDX",
        "specVersion": spec.pop(),
        "serialNumber": f"urn:uuid:{uuid.uuid4()}",
        "version": 1,
        "metadata": {
            "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "component": {
                "type": "application",
                "bom-ref": f"fraiseql@{version}",
                "name": "fraiseql",
                "version": version,
                "purl": f"pkg:cargo/fraiseql@{version}",
            },
        },
        "components": sorted(components.values(), key=lambda c: (c.get("name", ""), c.get("version", ""))),
        "dependencies": [
            {"ref": ref, "dependsOn": sorted(deps)} for ref, deps in sorted(edges.items())
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--part", required=True, help="the --override-filename given to cargo cyclonedx")
    parser.add_argument("--version", required=True, help="release version, e.g. 2.15.0")
    parser.add_argument("--output", required=True, help="path of the merged SBOM")
    args = parser.parse_args()

    packages = published_packages()
    boms, missing = [], []
    for name, directory in packages:
        path = directory / f"{args.part}.json"
        if path.is_file():
            boms.append(json.loads(path.read_text()))
        else:
            missing.append(f"{name} ({path})")
    if missing:
        sys.exit("merge-sbom: no per-crate SBOM for published crate(s): " + ", ".join(missing))

    merged = merge(boms, args.version)
    Path(args.output).write_text(json.dumps(merged, indent=2) + "\n")
    print(
        f"merge-sbom: {len(packages)} published crates → {len(merged['components'])} components, "
        f"{len(merged['dependencies'])} dependency entries → {args.output}"
    )


if __name__ == "__main__":
    main()
