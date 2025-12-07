#!/usr/bin/env python3
"""Check version consistency across FraiseQL package files."""

import re
import sys
from pathlib import Path


def normalize_version(version: str) -> str:
    """Normalize version string for comparison (beta.X → bX)."""
    return version.replace("-beta.", "b").replace("beta.", "b")


def extract_version(file_path: Path, pattern: str) -> str | None:
    """Extract version from file using regex pattern."""
    try:
        content = file_path.read_text()
        match = re.search(pattern, content)
        return match.group(1) if match else None
    except Exception as e:
        print(f"❌ Error reading {file_path}: {e}")
        return None


def main():
    """Check version consistency across all package files."""
    base_path = Path(__file__).parent.parent

    # Version files and their patterns
    version_files = {
        "src/fraiseql/__init__.py": r'__version__\s*=\s*["\']([^"\']+)["\']',
        "pyproject.toml": r'version\s*=\s*["\']([^"\']+)["\']',
        "fraiseql_rs/Cargo.toml": r'version\s*=\s*["\']([^"\']+)["\']',
    }

    versions = {}

    print("=" * 60)
    print("FraiseQL Version Consistency Check")
    print("=" * 60 + "\n")

    for file_rel, pattern in version_files.items():
        file_path = base_path / file_rel
        version = extract_version(file_path, pattern)

        if version:
            versions[file_rel] = version
            print(f"✅ {file_rel}")
            print(f"   Version: {version}\n")
        else:
            print(f"❌ {file_rel}")
            print(f"   Could not extract version\n")
            sys.exit(1)

    # Check all versions match (normalize for comparison)
    normalized_versions = {normalize_version(v) for v in versions.values()}
    unique_versions = set(versions.values())

    print("=" * 60)

    if len(normalized_versions) == 1:
        version = normalized_versions.pop()
        print(f"✅ All versions consistent: {version}")
        print(f"   (Raw versions: {', '.join(sorted(unique_versions))})")
        print("=" * 60)

        # Check if it's a beta version
        if "b" in version or "beta" in version:
            print(f"\n⚠️  Note: This is a beta release ({version})")
            print("   Ensure:")
            print("   - All breaking changes are documented in CHANGELOG.md")
            print("   - error_config.py has correct DEFAULT_ERROR_CONFIG")
            print("   - noop: is in error_prefixes (not error_as_data_prefixes)")

        sys.exit(0)
    else:
        print("❌ Version mismatch detected!\n")
        for file, version in versions.items():
            print(f"   {file}: {version}")
        print("\n" + "=" * 60)
        sys.exit(1)


if __name__ == "__main__":
    main()
