#!/usr/bin/env python3
"""
Script to add missing pytest markers to test files based on their directory location.

Usage:
    python scripts/ci-cd/fix_test_markers.py [--dry-run]

Reads file paths from /tmp/unmarked_tests.txt and adds appropriate pytest.mark.X
markers at module level using pytestmark variable.
"""

import ast
import sys
from pathlib import Path


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


def find_insert_line(source: str) -> int:
    """
    Use AST to find the correct line number to insert pytestmark.

    Returns the line number (1-indexed) where pytestmark should be inserted,
    which is after all imports and module docstring.
    """
    try:
        tree = ast.parse(source)
    except SyntaxError:
        # If AST parsing fails, return 1 (insert at beginning)
        return 1

    last_import_end = 0

    for node in ast.iter_child_nodes(tree):
        # Skip module docstring (first Expr with string value)
        if isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant):
            if isinstance(node.value.value, str) and node.lineno == 1:
                last_import_end = max(last_import_end, node.end_lineno or node.lineno)
                continue

        # Track imports
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            last_import_end = max(last_import_end, node.end_lineno or node.lineno)
        else:
            # First non-import, non-docstring node - stop here
            break

    # Insert after the last import (or at line 1 if no imports)
    return last_import_end + 1 if last_import_end > 0 else 1


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
        content = f.read()

    if has_marker(content, marker):
        return False

    lines = content.splitlines(keepends=True)

    # Check if import pytest is present
    has_import = "import pytest" in content

    # Find insert position using AST
    insert_line = find_insert_line(content)

    # Build the lines to insert
    lines_to_insert = []

    if not has_import:
        lines_to_insert.append("import pytest\n")

    # Add blank line before pytestmark if there's content after
    if insert_line <= len(lines):
        lines_to_insert.append("\n")

    lines_to_insert.append(f"pytestmark = pytest.mark.{marker}\n")

    # Add blank line after pytestmark if there's content after
    if insert_line <= len(lines) and lines[insert_line - 1].strip():
        lines_to_insert.append("\n")

    # Insert the lines
    insert_idx = insert_line - 1  # Convert to 0-indexed

    # If we're inserting at the end and there's no trailing newline, add one
    if insert_idx >= len(lines):
        if lines and not lines[-1].endswith("\n"):
            lines[-1] += "\n"
        lines.extend(lines_to_insert)
    else:
        # Insert at position
        for i, line in enumerate(lines_to_insert):
            lines.insert(insert_idx + i, line)

    new_content = "".join(lines)

    if dry_run:
        print(f"Would modify {file_path}:")
        print(f"  - Insert at line {insert_line}")
        if not has_import:
            print("  - Added import pytest")
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

        try:
            if add_marker_to_file(file_path, marker, dry_run):
                modified += 1
                if not dry_run:
                    print(f"Added marker to {file_path}")
            else:
                skipped += 1
        except Exception as e:
            print(f"Error processing {file_path}: {e}")
            skipped += 1

    print(f"Summary: {modified} files modified, {skipped} files skipped")


if __name__ == "__main__":
    main()
