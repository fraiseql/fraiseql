#!/usr/bin/env python3
"""Version manager for FraiseQL - handles all 8 version files.

This script manages version bumping across:
- src/fraiseql/__init__.py (__version__)
- pyproject.toml (version)
- Cargo.toml (Rust workspace version)
- fraiseql_rs/Cargo.toml (Rust extension version)
- README.md (version badges)
- docs/strategic/version-status.md (current stable)
- CHANGELOG.md (release notes header)

Usage:
    python scripts/version_manager.py patch     # Bump patch version
    python scripts/version_manager.py minor     # Bump minor version
    python scripts/version_manager.py major     # Bump major version
    python scripts/version_manager.py show      # Show current version
    python scripts/version_manager.py --dry-run patch  # Preview changes
"""

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Literal

# Version files mapping
VERSION_FILES = {
    "src/fraiseql/__init__.py": {
        "pattern": r'__version__ = "([0-9.]+)"',
        "replacement": '__version__ = "{version}"',
    },
    "pyproject.toml": {
        "pattern": r'version = "([0-9.]+)"',
        "replacement": 'version = "{version}"',
        "first_only": True,
    },
    "Cargo.toml": {
        "pattern": r'version = "([0-9.]+)"',
        "replacement": 'version = "{version}"',
        "first_only": True,
    },
    "fraiseql_rs/Cargo.toml": {
        "pattern": r'version = "([0-9.]+)"',
        "replacement": 'version = "{version}"',
        "first_only": True,
    },
    "README.md": {
        "pattern": r"v([0-9.]+)",
        "replacement": "v{version}",
        "line_match": r"v\d+\.\d+\.\d+",
    },
    "docs/strategic/version-status.md": {
        "pattern": r"\*\*Current Stable\*\*: v([0-9.]+)",
        "replacement": "**Current Stable**: v{version}",
        "first_only": True,
    },
    "CHANGELOG.md": {
        "pattern": r"## \[([0-9.]+)\]",
        "replacement": "## [{version}]",
        "first_only": True,
    },
}


class VersionManager:
    """Manages FraiseQL version across all files."""

    def __init__(self, project_root: Path = None):
        self.project_root = project_root or Path(__file__).parent.parent
        self.dry_run = False

    def parse_version(self, version_string: str) -> tuple[int, int, int]:
        """Parse semantic version string to (major, minor, patch)."""
        parts = version_string.split(".")
        if len(parts) != 3:
            raise ValueError(f"Invalid version format: {version_string}")
        try:
            return (int(parts[0]), int(parts[1]), int(parts[2]))
        except ValueError:
            raise ValueError(f"Invalid version format: {version_string}")

    def format_version(self, major: int, minor: int, patch: int) -> str:
        """Format version tuple to string."""
        return f"{major}.{minor}.{patch}"

    def get_current_version(self) -> str:
        """Get current version from __init__.py."""
        version_file = self.project_root / "src/fraiseql/__init__.py"
        content = version_file.read_text()
        match = re.search(r'__version__ = "([0-9.]+)"', content)
        if not match:
            raise ValueError("Could not find current version in __init__.py")
        return match.group(1)

    def bump_version(
        self, bump_type: Literal["patch", "minor", "major"]
    ) -> tuple[str, str]:
        """Calculate new version based on bump type."""
        current = self.get_current_version()
        major, minor, patch = self.parse_version(current)

        if bump_type == "patch":
            patch += 1
        elif bump_type == "minor":
            minor += 1
            patch = 0
        elif bump_type == "major":
            major += 1
            minor = 0
            patch = 0

        new_version = self.format_version(major, minor, patch)
        return current, new_version

    def update_file(self, file_path: Path, current_version: str, new_version: str, file_name: str = None):
        """Update version in a single file."""
        if not file_path.exists():
            print(f"⚠️  File not found: {file_path}")
            return False

        if file_name is None:
            file_name = str(file_path.relative_to(self.project_root))

        content = file_path.read_text()
        config = VERSION_FILES[file_name]

        pattern = config["pattern"]
        replacement_template = config["replacement"]

        # Verify version is present
        if not re.search(pattern, content):
            print(f"⚠️  Pattern not found in {file_name}")
            return False

        # Replace version
        if config.get("first_only"):
            new_content = re.sub(pattern, replacement_template.format(version=new_version), content, count=1)
        else:
            new_content = re.sub(pattern, replacement_template.format(version=new_version), content)

        if not self.dry_run:
            file_path.write_text(new_content)
            print(f"✅ {file_name}: {current_version} → {new_version}")
        else:
            print(f"🔍 {file_name}: {current_version} → {new_version} (dry run)")

        return True

    def update_all_versions(self, current_version: str, new_version: str) -> bool:
        """Update all version files."""
        print(f"\n📦 Updating version files: {current_version} → {new_version}")
        print("=" * 60)

        all_success = True
        for file_name in VERSION_FILES:
            file_path = self.project_root / file_name
            if not self.update_file(file_path, current_version, new_version, file_name):
                all_success = False

        return all_success

    def show_version(self):
        """Display current version information."""
        version = self.get_current_version()
        major, minor, patch = self.parse_version(version)

        print(f"\n📊 FraiseQL Version Information")
        print("=" * 60)
        print(f"Current Version: v{version}")
        print(f"Version Parts: Major={major}, Minor={minor}, Patch={patch}")
        print()
        print("Next versions:")
        print(f"  • Patch: v{major}.{minor}.{patch + 1}")
        print(f"  • Minor: v{major}.{minor + 1}.0")
        print(f"  • Major: v{major + 1}.0.0")

    def run(self, bump_type: Literal["patch", "minor", "major", "show"] = "show", dry_run: bool = False):
        """Execute version management."""
        self.dry_run = dry_run

        if bump_type == "show":
            self.show_version()
            return True

        current, new = self.bump_version(bump_type)
        self.update_all_versions(current, new)

        if not dry_run:
            print("\n" + "=" * 60)
            print(f"✅ Version bumped to {new}")
            print("=" * 60)

        return True


def main():
    """CLI entry point."""
    import argparse

    parser = argparse.ArgumentParser(description="FraiseQL Version Manager")
    parser.add_argument(
        "action",
        nargs="?",
        default="show",
        choices=["patch", "minor", "major", "show"],
        help="Version bump type or action (default: show)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview changes without writing files",
    )

    args = parser.parse_args()

    try:
        manager = VersionManager()
        success = manager.run(args.action, dry_run=args.dry_run)
        sys.exit(0 if success else 1)
    except Exception as e:
        print(f"❌ Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
