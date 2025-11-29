#!/usr/bin/env python3
"""
Script to add missing pytest markers to test files based on their directory location.

Usage:
    python scripts/ci-cd/fix_test_markers.py [--dry-run]

Reads file paths from /tmp/unmarked_tests.txt and adds appropriate pytest.mark.X
markers at module level using pytestmark variable.
"""

import sys
from pathlib import Path
from typing import List


def get_marker(path: str) -> str | None:
    """
    Determine the pytest marker based on the file path.

    Args:
        path: The file path to check

    Returns:
        The marker name or None if no marker should be added
    """
    if "tests/unit/" in path:
        return None
    if "tests/integration/database/" in path:
        return "database"
    elif "tests/integration/enterprise/" in path:
        return "enterprise"
    elif "tests/integration/" in path:
        return "integration"
    elif "tests/system/" in path:
        return "integration"
    elif "tests/regression/" in path:
        return "integration"
    elif "tests/grafana/" in path:
        return "integration"
    elif "tests/storage/" in path:
        return "integration"
    elif "tests/monitoring/" in path:
        return "integration"
    elif "tests/middleware/" in path:
        return "integration"
    elif "tests/routing/" in path:
        return "integration"
    elif "tests/fixtures/" in path:
        return "integration"
    elif "tests/config/" in path:
        return "integration"
    elif "tests/core/" in path:
        return "integration"  # Assuming DB patterns for core tests
    elif path.startswith("tests/") and path.endswith(".py"):
        return "integration"
    return None


def has_marker(content: str, marker: str) -> bool:
    """
    Check if the file already has the specified marker.

    Args:
        content: The file content
        marker: The marker name to check for

    Returns:
        True if the marker is already present
    """
    return f"pytestmark = pytest.mark.{marker}" in content or f"@pytest.mark.{marker}" in content


def add_marker_to_file(file_path: str, marker: str, dry_run: bool = False) -> bool:
    """
    Add the pytest marker to the file.

    Args:
        file_path: Path to the file
        marker: The marker to add
        dry_run: If True, only print what would be done

    Returns:
        True if the file was modified (or would be modified in dry run)
    """
    with open(file_path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    content = "".join(lines)

    if has_marker(content, marker):
        return False

    # Check if import pytest is present
    has_import = any("import pytest" in line for line in lines)

    if not has_import:
        # Add import at the top
        lines.insert(0, "import pytest\n\n")

    # Find position after imports and blank lines
    insert_pos = 0
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("import ") or stripped.startswith("from ") or stripped == "":
            continue
        insert_pos = i
        break

    # If no code found, insert at end
    if insert_pos == 0:
        insert_pos = len(lines)

    # Add the marker
    marker_line = f"pytestmark = pytest.mark.{marker}\n\n"
    lines.insert(insert_pos, marker_line)

    new_content = "".join(lines)

    if dry_run:
        print(f"Would modify {file_path}:")
        # Print diff or just indicate
        print(f"  - Added import pytest (if missing)")
        print(f"  - Added pytestmark = pytest.mark.{marker}")
        return True
    else:
        with open(file_path, "w", encoding="utf-8") as f:
            f.write(new_content)
        return True


def main() -> None:
    """Main entry point."""
    dry_run = "--dry-run" in sys.argv

    input_file = "/tmp/unmarked_tests.txt"

    try:
        with open(input_file, "r", encoding="utf-8") as f:
            files = [line.strip() for line in f if line.strip()]
    except FileNotFoundError:
        print(f"Error: Input file {input_file} not found")
        sys.exit(1)

    modified = 0
    skipped = 0

    for file_path in files:
        marker = get_marker(file_path)
        if marker is None:
            skipped += 1
            continue

        if not Path(file_path).exists():
            print(f"Warning: File not found: {file_path}")
            skipped += 1
            continue

        if add_marker_to_file(file_path, marker, dry_run):
            modified += 1
            if not dry_run:
                print(f"Added marker to {file_path}")
        else:
            skipped += 1

    print(f"Summary: {modified} files modified, {skipped} files skipped")


if __name__ == "__main__":
    main()
