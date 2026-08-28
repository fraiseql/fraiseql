#!/usr/bin/env python3
"""The SDK lockfile-freshness gate: a published SDK's lockfile may not pin a version its
own manifest no longer claims.

The hole this closes (#1225). The 2.15.0 bump edited the SDK *manifests* and left two
*lockfiles* behind. Of the three official SDKs whose lock format records the package's
own version, `fraiseql-typescript` agreed at 2.15.0 while `fraiseql-python`'s uv.lock and
`fraiseql-rust`'s Cargo.lock still said 2.14.1 — measured on scratch copies as
`uv lock --check` rc=1 and `cargo metadata --locked` rc=101, against rc=0 once
regenerated. Nothing noticed, because every `--locked` in this repository is
`cargo install <tool> --locked`: the *tooling* is pinned, an SDK's own lockfile never
checked.

The asymmetry is itself the argument for this gate. TypeScript stayed correct because
`typescript-sdk.yml` runs `npm ci`, which refuses a lockfile disagreeing with its
manifest; Python ran `uv sync` and Rust ran `cargo test`, neither with `--locked`, so
both drifted silently. One SDK was gated by accident of tooling and two were not.

Why a text comparison and not a real resolve. `uv sync --locked` and `cargo build
--locked` are stronger — they also catch *dependency* drift — but they need those
toolchains and a network. The Dagger ShellGates container is bare Ubuntu plus python3
(no uv, no cargo, and famously no cpio), so a resolve-based check could not run in the
shape CI actually uses. This gate therefore takes the property that is pure text —
manifest version vs the version recorded in the lockfile — and runs everywhere, always,
on every push regardless of path filters. Dependency drift is covered separately by
`--locked` on the SDK legs that already carry the toolchains, which fire on their own
path filters. Neither subsumes the other, and this one is the floor.

What it does:

1. **Discover SDKs** from `sdks/official/*` minus NOT_AN_SDK, the same source
   `check-sdk-workflow-coverage.py` and `check-delivery-coverage.py` read — never a
   fourth hand-maintained list, so a new SDK arrives here automatically.

2. **Classify every lockfile, and FAIL on one it cannot classify.** Lock formats split
   into those recording the root package's own version (uv.lock, Cargo.lock,
   package-lock.json) and those recording only dependencies (go.sum, mix.lock,
   composer.lock, pubspec.lock, …). An unrecognised lock-shaped file is FATAL rather than
   skipped: that is Phase 06's A5 rule, because a gate that silently drops what it does
   not understand reports a coverage it never checked.

3. **Compare, for the version-recording formats only.** A mismatch is a finding.

Exit codes are split deliberately, which is the lesson E1 of the delivery-coverage gate
paid for: **1 is a finding, 2 is "this gate could not run"**. A harness asking only
"non-zero?" scores a broken parser as a successful RED proof.

Scans each SDK's top-level directory only — the root package's lockfile is the one that
ships. Nested fixture lockfiles are out of scope and stated as such.

Runs in preflight and in the Dagger ShellGates leg (python3, stdlib only). Locally:
`make lint-sdk-lockfile-freshness`.
"""

from __future__ import annotations

import json
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
OFFICIAL = REPO / "sdks" / "official"

# Matches tools/check-sdk-workflow-coverage.py's NOT_AN_SDK — sdks/official holds two
# directories that are not SDKs.
NOT_AN_SDK = {"conformance", "tests"}

# Lock formats that record the ROOT package's own version, mapped to the manifest that
# declares the version of record. These are the only ones this gate can compare.
VERSION_RECORDING = {
    "uv.lock": "pyproject.toml",
    "Cargo.lock": "Cargo.toml",
    "package-lock.json": "package.json",
}

# Lock formats that record dependencies only. Listed explicitly so that "this SDK has a
# lockfile and is not checked" is a stated classification rather than an omission.
DEPENDENCY_ONLY = {
    # bun.lock records the root workspace's `name` but no `version` (measured against
    # fraiseql-typescript's, whose workspaces[""] keys are dependencies, devDependencies,
    # name, optionalPeers, peerDependencies) — so it cannot go stale on version. It sits
    # beside package-lock.json and no CI leg reads it.
    "bun.lock",
    "go.sum",
    "go.mod",
    "mix.lock",
    "composer.lock",
    "pubspec.lock",
    "Gemfile.lock",
    "poetry.lock",
    "yarn.lock",
    "pnpm-lock.yaml",
    "packages.lock.json",
    "paket.lock",
}

ERRORS: list[str] = []


def die(message: str) -> None:
    """A fault in the gate itself, not a finding: stop rather than report a partial pass."""
    print(f"sdk-lockfile-freshness: FATAL — {message}", file=sys.stderr)
    raise SystemExit(2)


def err(message: str) -> None:
    ERRORS.append(message)


def looks_like_a_lockfile(name: str) -> bool:
    """Shape test used only to catch a format nobody has classified yet.

    Deliberately broader than the two tables above: a new lock format must arrive as a
    FATAL demanding a decision, not as a file this gate silently walks past.
    """
    return (
        name.endswith(".lock")
        or name.endswith(".sum")
        or (name.endswith(".json") and "lock" in name)
        or name.endswith("-lock.yaml")
    )


def read_toml(path: Path) -> dict:
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        die(f"cannot parse {path.relative_to(REPO)}: {exc}")
        raise  # unreachable; keeps the type checker honest


def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        die(f"cannot parse {path.relative_to(REPO)}: {exc}")
        raise  # unreachable


def manifest_version(sdk: str, manifest: Path) -> tuple[str, str]:
    """Return (package name, declared version) from an SDK's manifest.

    A version this gate cannot read in isolation — PEP 621 `dynamic`, or Cargo's
    `version.workspace = true` — is FATAL, never assumed equal.
    """
    rel = manifest.relative_to(REPO)
    if manifest.name == "pyproject.toml":
        data = read_toml(manifest)
        project = data.get("project", {})
        version = project.get("version")
        name = project.get("name")
        if version is None:
            if "version" in project.get("dynamic", []):
                die(f"{rel} declares a dynamic version; this gate cannot resolve it")
            die(f"{rel} has no [project] version")
        if not name:
            die(f"{rel} has no [project] name")
        return str(name), str(version)

    if manifest.name == "Cargo.toml":
        data = read_toml(manifest)
        package = data.get("package", {})
        version = package.get("version")
        name = package.get("name")
        if isinstance(version, dict):
            die(f"{rel} inherits its version from a workspace; this gate cannot resolve it")
        if version is None:
            die(f"{rel} has no [package] version")
        if not name:
            die(f"{rel} has no [package] name")
        return str(name), str(version)

    if manifest.name == "package.json":
        data = read_json(manifest)
        version = data.get("version")
        name = data.get("name")
        if version is None:
            die(f"{rel} has no top-level version")
        if not name:
            die(f"{rel} has no top-level name")
        return str(name), str(version)

    die(f"no manifest reader for {rel} (SDK {sdk})")
    raise  # unreachable


def locked_versions(sdk: str, lock: Path, pkg_name: str) -> list[tuple[str, str]]:
    """Return [(where, version)] — every place this lock records the root's own version.

    More than one site is deliberate: package-lock.json carries the version twice, and a
    bump that updates one and not the other is exactly the drift being gated.
    """
    rel = lock.relative_to(REPO)

    if lock.name == "uv.lock":
        data = read_toml(lock)
        roots = [
            p
            for p in data.get("package", [])
            if isinstance(p.get("source"), dict) and p["source"].get("editable") == "."
        ]
        if not roots:
            die(f"{rel} records no editable root package; cannot locate {pkg_name}")
        if len(roots) > 1:
            die(f"{rel} records {len(roots)} editable root packages; expected exactly 1")
        root = roots[0]
        if "version" not in root:
            die(f"{rel}'s editable root package has no version")
        return [(f"{rel} [[package]] {root.get('name', '?')}", str(root["version"]))]

    if lock.name == "Cargo.lock":
        data = read_toml(lock)
        roots = [p for p in data.get("package", []) if p.get("name") == pkg_name]
        if not roots:
            die(f"{rel} does not record its own root package {pkg_name!r}")
        if len(roots) > 1:
            die(f"{rel} records {pkg_name!r} {len(roots)} times; expected exactly 1")
        root = roots[0]
        if "version" not in root:
            die(f"{rel}'s [[package]] {pkg_name} has no version")
        return [(f"{rel} [[package]] {pkg_name}", str(root["version"]))]

    if lock.name == "package-lock.json":
        data = read_json(lock)
        sites: list[tuple[str, str]] = []
        if "version" in data:
            sites.append((f"{rel} .version", str(data["version"])))
        root_entry = data.get("packages", {}).get("")
        if isinstance(root_entry, dict) and "version" in root_entry:
            sites.append((f'{rel} .packages[""].version', str(root_entry["version"])))
        if not sites:
            die(f"{rel} records the root package version in neither known site")
        return sites

    die(f"no lock reader for {rel} (SDK {sdk})")
    raise  # unreachable


def main() -> int:
    if not OFFICIAL.is_dir():
        die(f"{OFFICIAL} is not a directory")

    sdks = sorted(
        d.name for d in OFFICIAL.iterdir() if d.is_dir() and d.name not in NOT_AN_SDK
    )
    if not sdks:
        die("zero official SDKs found under sdks/official")

    checked = 0
    dependency_only = 0
    no_lockfile = 0

    for sdk in sdks:
        sdk_dir = OFFICIAL / sdk
        locks = sorted(
            f.name
            for f in sdk_dir.iterdir()
            if f.is_file() and (f.name in VERSION_RECORDING or f.name in DEPENDENCY_ONLY or looks_like_a_lockfile(f.name))
        )

        unclassified = [
            n for n in locks if n not in VERSION_RECORDING and n not in DEPENDENCY_ONLY
        ]
        if unclassified:
            die(
                f"sdks/official/{sdk} carries unclassified lock-shaped file(s) "
                f"{', '.join(unclassified)} — add each to VERSION_RECORDING (with the "
                f"manifest declaring its version) or to DEPENDENCY_ONLY, so that "
                f"'not checked' is a decision rather than an omission"
            )

        recording = [n for n in locks if n in VERSION_RECORDING]
        if not recording:
            if locks:
                dependency_only += 1
            else:
                no_lockfile += 1
            continue

        for lock_name in recording:
            manifest = sdk_dir / VERSION_RECORDING[lock_name]
            if not manifest.is_file():
                die(
                    f"sdks/official/{sdk} has {lock_name} but no "
                    f"{VERSION_RECORDING[lock_name]} to declare the version of record"
                )
            pkg_name, declared = manifest_version(sdk, manifest)
            for where, found in locked_versions(sdk, sdk_dir / lock_name, pkg_name):
                checked += 1
                if found != declared:
                    err(
                        f"sdks/official/{sdk}: {VERSION_RECORDING[lock_name]} declares "
                        f"{declared}, but {where} records {found}. Regenerate the "
                        f"lockfile — a published SDK whose lock pins a version its "
                        f"manifest no longer claims fails any reproducible install "
                        f"(#1225)."
                    )

    if ERRORS:
        print(
            f"sdk-lockfile-freshness: FAIL — {len(ERRORS)} stale lockfile record(s):",
            file=sys.stderr,
        )
        for e in ERRORS:
            print(f"  {e}", file=sys.stderr)
        return 1

    print(
        f"sdk-lockfile-freshness: OK — {len(sdks)} official SDK(s); "
        f"{checked} recorded version(s) agree with their manifest, "
        f"{dependency_only} dependency-only lockfile(s), "
        f"{no_lockfile} without a lockfile."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
