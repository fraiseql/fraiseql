#!/usr/bin/env python3
"""Every Cargo.lock records each path package at the version its manifest declares.

The hole this closes. The 2.16.0 release commit bumped every first-party manifest to
2.16.0 and left the seven fuzz workspaces' lockfiles recording their path packages
(fraiseql-core, fraiseql-db, the fuzz crate itself, …) at 2.15.0. The Rust SDK's
nested `fraiseql-client` workspace was worse: its lockfile recorded 2.3.0 through
thirteen releases. Nothing noticed, for the reason #1225 already measured for the SDK
roots: no build of those workspaces runs `--locked`, so cargo rewrites the stale record
in the build's own checkout and the committed lockfile never has to agree.

`tools/check-sdk-lockfile-freshness.py` closed that hole for each SDK's ROOT lockfile.
This gate is the general rule beneath it, over every Cargo.lock in the tree: a
`[[package]]` with no `source` is a path package, built from a manifest in this
repository, and its recorded version must be that manifest's. Registry and git
packages are not judged — their versions are the resolver's business, and
`--locked` on the legs that carry a toolchain is what covers them.

Discovery walks the filesystem, never `git ls-files`: the Dagger ShellGates container
`git init`s the source with an empty index, so a git-listed gate checks nothing there.

Exit codes are split as in every gate of this family: **1 is a finding, 2 is "this gate
could not run"** — a path package whose manifest it cannot find, two manifests claiming
one name, a version it cannot resolve, or a tree with no lockfile at all.

Runs in preflight and in the Dagger ShellGates leg (python3, stdlib only). Locally:
`make lint-cargo-lock-path-versions`.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import NoReturn

import tomllib

REPO = Path(__file__).resolve().parent.parent

# Directories never holding a committed manifest or lockfile. Pruned so a local build
# (target/), a vendored JS tree or a virtualenv cannot add files CI never sees.
PRUNE = {".git", "target", "node_modules", ".venv"}

ERRORS: list[str] = []


def die(message: str) -> NoReturn:
    """A fault in the gate itself, not a finding: stop rather than report a partial pass."""
    print(f"cargo-lock-path-versions: FATAL — {message}", file=sys.stderr)
    raise SystemExit(2)


def walk(filename: str) -> list[Path]:
    found: list[Path] = []
    for dirpath, dirnames, filenames in os.walk(REPO):
        dirnames[:] = sorted(d for d in dirnames if d not in PRUNE)
        if filename in filenames:
            found.append(Path(dirpath) / filename)
    return found


def read_toml(path: Path) -> dict:
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        die(f"cannot parse {path.relative_to(REPO)}: {exc}")


def inherited_version(manifest: Path) -> str:
    """Resolve `version.workspace = true` against the nearest enclosing workspace."""
    # A manifest can be its own workspace root, so the search starts at its directory.
    directory = manifest.parent
    while True:
        candidate = directory / "Cargo.toml"
        workspace = (
            read_toml(candidate).get("workspace") if candidate.is_file() else None
        )
        if workspace is not None:
            version = workspace.get("package", {}).get("version")
            if not isinstance(version, str):
                die(
                    f"{manifest.relative_to(REPO)} inherits its version, and "
                    f"{candidate.relative_to(REPO)} declares no [workspace.package] version"
                )
            return version
        if directory == REPO:
            break
        directory = directory.parent
    die(
        f"{manifest.relative_to(REPO)} inherits its version from no enclosing workspace"
    )


def manifest_versions() -> dict[str, tuple[Path, str]]:
    """Map every package name declared in this tree to (manifest, version)."""
    index: dict[str, tuple[Path, str]] = {}
    for manifest in walk("Cargo.toml"):
        package = read_toml(manifest).get("package")
        if package is None:
            continue
        name = package.get("name")
        version = package.get("version")
        if not isinstance(name, str):
            die(f"{manifest.relative_to(REPO)} has a [package] with no name")
        if isinstance(version, dict) and version.get("workspace") is True:
            version = inherited_version(manifest)
        if not isinstance(version, str):
            die(f"{manifest.relative_to(REPO)} declares no version this gate can read")
        if name in index and index[name][1] != version:
            die(
                f"package {name!r} is declared by {index[name][0].relative_to(REPO)} "
                f"({index[name][1]}) and {manifest.relative_to(REPO)} ({version}); "
                "a lockfile's path record cannot be matched to one of them"
            )
        index.setdefault(name, (manifest, version))
    return index


def main() -> int:
    locks = walk("Cargo.lock")
    if not locks:
        die("found no Cargo.lock at all; discovery is broken")
    index = manifest_versions()

    checked = 0
    for lock in locks:
        rel = lock.relative_to(REPO)
        packages = read_toml(lock).get("package", [])
        if not isinstance(packages, list):
            die(f"{rel} has no [[package]] array")
        for package in packages:
            if "source" in package:
                continue
            name, recorded = package.get("name"), package.get("version")
            if name not in index:
                die(
                    f"{rel} records path package {name!r}, which no Cargo.toml declares"
                )
            manifest, declared = index[name]
            checked += 1
            if recorded != declared:
                ERRORS.append(
                    f"{rel}: {name} is recorded at {recorded}, "
                    f"{manifest.relative_to(REPO)} declares {declared}"
                )

    if ERRORS:
        print(
            "cargo-lock-path-versions: a lockfile records a path package at a stale version:"
        )
        for error in ERRORS:
            print(f"  ✗ {error}")
        print(
            "\nFix: tools/release.sh bumps these (bump_lockfile_path_packages); outside a "
            "release, `cargo metadata --manifest-path <dir>/Cargo.toml` rewrites them."
        )
        return 1

    print(
        f"OK: {checked} path-package records across {len(locks)} Cargo.lock files "
        "match their manifests."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
