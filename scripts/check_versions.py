#!/usr/bin/env python3
"""Check version consistency across all project files."""

import re
import sys
from pathlib import Path


def get_versions() -> dict[str, str]:
    """Extract versions from all known locations."""
    versions = {}
    root = Path(__file__).parent.parent

    # pyproject.toml
    pyproject = root / "pyproject.toml"
    if pyproject.exists():
        match = re.search(r'^version\s*=\s*"([^"]+)"', pyproject.read_text(), re.M)
        if match:
            versions["pyproject.toml"] = match.group(1)

    # Cargo.toml
    cargo = root / "fraiseql_rs" / "Cargo.toml"
    if cargo.exists():
        match = re.search(r'^version\s*=\s*"([^"]+)"', cargo.read_text(), re.M)
        if match:
            versions["Cargo.toml"] = match.group(1)

    # Python __version__
    init = root / "src" / "fraiseql" / "__init__.py"
    if init.exists():
        match = re.search(r'^__version__\s*=\s*"([^"]+)"', init.read_text(), re.M)
        if match:
            versions["__init__.py"] = match.group(1)

    return versions


def main() -> int:
    versions = get_versions()

    print("Version check:")
    for file, version in versions.items():
        print(f"  {file}: {version}")

    unique = set(versions.values())
    if len(unique) == 1:
        print(f"\n✅ All versions consistent: {unique.pop()}")
        return 0
    else:
        print(f"\n❌ Version mismatch detected!")
        print(f"   Found versions: {unique}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
