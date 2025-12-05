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


def normalize_version(version: str) -> str:
    """Normalize version strings to handle Python vs Rust pre-release formats.

    Python (PEP 440): 1.8.0a1
    Rust (SemVer 2.0): 1.8.0-alpha.1

    Both are normalized to: 1.8.0-alpha.1 for comparison.
    """
    # Convert Python alpha format (1.8.0a1) to Rust format (1.8.0-alpha.1)
    version = re.sub(r'(\d+\.\d+\.\d+)a(\d+)', r'\1-alpha.\2', version)
    # Convert Python beta format (1.8.0b1) to Rust format (1.8.0-beta.1)
    version = re.sub(r'(\d+\.\d+\.\d+)b(\d+)', r'\1-beta.\2', version)
    # Convert Python rc format (1.8.0rc1) to Rust format (1.8.0-rc.1)
    version = re.sub(r'(\d+\.\d+\.\d+)rc(\d+)', r'\1-rc.\2', version)
    return version


def main() -> int:
    versions = get_versions()

    print("Version check:")
    for file, version in versions.items():
        print(f"  {file}: {version}")

    # Normalize versions for comparison
    normalized = {normalize_version(v) for v in versions.values()}

    if len(normalized) == 1:
        print(f"\n✅ All versions consistent: {list(versions.values())[0]}")
        print(f"   (Normalized: {normalized.pop()})")
        return 0
    else:
        print(f"\n❌ Version mismatch detected!")
        print(f"   Found versions: {set(versions.values())}")
        print(f"   Normalized: {normalized}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
