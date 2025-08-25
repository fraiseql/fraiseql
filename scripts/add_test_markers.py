#!/usr/bin/env python3
"""Script to add pytest markers to existing test files based on directory structure."""

import re
from pathlib import Path


def get_markers_for_path(file_path: Path) -> list[str]:
    """Determine appropriate markers based on file path."""
    markers = []

    path_str = str(file_path)

    # Directory-based markers
    if "/unit/" in path_str or "/utils/" in path_str or "/types/" in path_str:
        markers.append("unit")
    elif "/integration/" in path_str or "/database/" in path_str:
        markers.append("integration")
        if "/database/" in path_str:
            markers.append("database")
    elif "/e2e/" in path_str:
        markers.append("e2e")
        markers.append("database")

    # Component-specific markers
    if "/mutations/" in path_str:
        markers.append("unit" if "unit" not in markers else None)
    elif "/security/" in path_str:
        markers.append("security")
    elif "/performance/" in path_str:
        markers.append("performance")
        markers.append("slow")
    elif "/auth/" in path_str:
        markers.append("auth")
    elif "/turbo/" in path_str:
        markers.append("turbo")
    elif "/field_threshold/" in path_str and "camelforge" in path_str.lower():
        markers.append("camelforge")

    # E2E specific
    if "e2e" in file_path.name:
        markers.append("e2e")
        markers.append("database")

    # Performance
    if "performance" in file_path.name or "benchmark" in file_path.name:
        markers.append("performance")
        markers.append("slow")

    # Blog demo markers - these are blueprint examples for other projects
    if "blog_demo" in path_str.lower():
        markers.append("blog_demo")
        if "simple" in path_str.lower():
            markers.append("blog_demo_simple")
        elif "enterprise" in path_str.lower():
            markers.append("blog_demo_enterprise")

    # Remove None values and defaults
    markers = [m for m in markers if m is not None]
    if not markers:
        markers.append("unit")  # Default to unit tests

    return list(set(markers))  # Remove duplicates


def add_markers_to_file(file_path: Path, markers: list[str]) -> bool:
    """Add markers to a test file if not already present."""
    try:
        content = file_path.read_text()

        # Check if markers already exist
        if "@pytest.mark." in content:
            return False  # Already has markers

        # Find first import or class/function definition
        lines = content.splitlines()
        insert_index = 0

        # Find where to insert markers (after imports, before first test)
        import_done = False
        for i, line in enumerate(lines):
            if line.strip().startswith("import ") or line.strip().startswith("from "):
                import_done = True
                continue
            elif import_done and (line.strip().startswith("class ") or line.strip().startswith("def test_") or line.strip().startswith("@")):
                insert_index = i
                break

        # Insert markers
        marker_lines = [f"@pytest.mark.{marker}" for marker in markers]
        marker_text = "\n".join(marker_lines) + "\n"

        # Add pytest import if not present
        if "import pytest" not in content:
            # Find where to add import
            import_insert = 0
            for i, line in enumerate(lines):
                if line.strip().startswith("import ") or line.strip().startswith("from "):
                    import_insert = i + 1
                elif line.strip() == "":
                    continue
                else:
                    break
            lines.insert(import_insert, "import pytest")
            lines.insert(import_insert + 1, "")
            insert_index += 2

        # Insert markers
        for j, marker_line in enumerate(reversed(marker_lines)):
            lines.insert(insert_index, marker_line)
        lines.insert(insert_index, "")

        # Write back
        file_path.write_text("\n".join(lines))
        return True

    except Exception as e:
        print(f"Error processing {file_path}: {e}")
        return False


def main():
    """Add markers to all test files."""
    test_dir = Path(__file__).parent.parent / "tests"

    # Find all test files (excluding e2e which we already updated)
    test_files = list(test_dir.rglob("test_*.py"))
    test_files = [f for f in test_files if "/e2e/" not in str(f)]

    updated_count = 0

    for test_file in test_files:
        markers = get_markers_for_path(test_file)
        if add_markers_to_file(test_file, markers):
            print(f"Added markers {markers} to {test_file.relative_to(test_dir)}")
            updated_count += 1

    print(f"\n✅ Updated {updated_count} test files with markers")


if __name__ == "__main__":
    main()
