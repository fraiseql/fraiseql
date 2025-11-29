#!/usr/bin/env python3
"""
Test Marker Audit Script for FraiseQL CI/CD Optimization

This script audits test files to ensure that tests using database fixtures
have proper isolation markers (database, integration, e2e, forked, slow, enterprise).
Tests without these markers skip expensive registry clearing, which can cause
issues in CI/CD pipelines.
"""

import re
import subprocess
import sys
from pathlib import Path


# Database-related patterns to search for in test files
DB_PATTERNS = [
    r"db_session",
    r"database_url",
    r"postgres",
    r"async_engine",
    r"AsyncConnection",
    r"psycopg",
]

# Isolation markers that indicate proper test isolation
ISOLATION_MARKERS = {
    "database",
    "integration",
    "e2e",
    "forked",
    "slow",
    "enterprise",
}


def find_test_files() -> list[Path]:
    """Find all Python test files in the tests directory."""
    test_dir = Path("tests")
    if not test_dir.exists():
        print(f"Error: tests directory not found at {test_dir.absolute()}")
        sys.exit(1)

    return list(test_dir.rglob("test_*.py")) + list(test_dir.rglob("*_test.py"))


def contains_db_patterns(file_path: Path) -> bool:
    """Check if a file contains database-related patterns."""
    try:
        content = file_path.read_text(encoding="utf-8")
        for pattern in DB_PATTERNS:
            if re.search(pattern, content, re.IGNORECASE):
                return True
    except Exception as e:
        print(f"Warning: Could not read {file_path}: {e}")
    return False


def get_all_test_markers() -> dict[Path, set[str]]:
    """Get all markers for all test files using a single pytest --collect-only run."""
    try:
        result = subprocess.run(
            ["uv", "run", "python", "-m", "pytest", "--collect-only", "tests/"],
            capture_output=True,
            text=True,
            timeout=120,  # Increased timeout for all tests
        )

        file_markers: dict[Path, set[str]] = {}

        # Parse pytest output for markers per file
        for line in result.stdout.splitlines():
            # Look for test item lines like: tests/test_file.py::TestClass::test_method[marker1-marker2]
            if "::" in line and "[" in line:
                # Extract file path and markers
                parts = line.split("::", 1)
                if len(parts) >= 1:
                    file_part = parts[0].strip()
                    if file_part.startswith("tests/"):
                        file_path = Path(file_part)
                        marker_match = re.search(r"\[([^\]]+)\]", line)
                        if marker_match:
                            # Markers are separated by hyphens in pytest output
                            markers = set(marker_match.group(1).split("-"))
                            if file_path not in file_markers:
                                file_markers[file_path] = set()
                            file_markers[file_path].update(markers)

        return file_markers

    except subprocess.TimeoutExpired:
        print("Warning: Timeout collecting markers for all tests")
        return {}
    except Exception as e:
        print(f"Warning: Could not collect markers: {e}")
        return {}


def main() -> None:
    """Main audit function."""
    print("🔍 Auditing test files for database marker compliance...")

    test_files = find_test_files()
    if not test_files:
        print("No test files found.")
        sys.exit(0)

    print(f"📊 Found {len(test_files)} test files, collecting markers...")

    # Get markers for all test files in one go
    all_markers = get_all_test_markers()

    problematic_files: list[tuple[Path, set[str]]] = []

    for test_file in test_files:
        if contains_db_patterns(test_file):
            markers = all_markers.get(test_file, set())
            # Check if any isolation markers are present
            has_isolation_marker = bool(ISOLATION_MARKERS & markers)

            if not has_isolation_marker:
                problematic_files.append((test_file, markers))

    if problematic_files:
        print("\n❌ Found test files using database patterns but missing isolation markers:")
        print("=" * 80)

        for file_path, markers in problematic_files:
            print(f"\n📁 {file_path}")
            if markers:
                print(f"   Current markers: {', '.join(sorted(markers))}")
            else:
                print("   No markers found")
            print("   Required: At least one of: " + ", ".join(sorted(ISOLATION_MARKERS)))

        print(f"\n🚨 Total problematic files: {len(problematic_files)}")
        print("\n💡 Fix: Add appropriate isolation markers to these test files.")
        print("   Use @pytest.mark.database, @pytest.mark.integration, etc.")

        sys.exit(1)
    else:
        print("\n✅ All database-using test files have proper isolation markers!")
        sys.exit(0)


if __name__ == "__main__":
    main()
