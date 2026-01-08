#!/usr/bin/env python3
"""File size validation for FraiseQL v2.0 organization standards.

This script validates that source and test files comply with size limits:
- Source files: < 1,500 lines (warning at 1,200)
- Test files: < 500 lines (warning at 400)
- __init__.py: < 100 lines

Large files should be refactored into smaller, focused modules.
"""

import sys
from pathlib import Path
from typing import Tuple

ROOT = Path(__file__).parent.parent
SRC = ROOT / "src" / "fraiseql"
TESTS = ROOT / "tests"

# Size limits (in lines)
SOURCE_LIMIT = 1500
SOURCE_WARNING = 1200
TEST_LIMIT = 500
TEST_WARNING = 400
INIT_LIMIT = 100


def count_lines(file_path: Path) -> int:
    """Count non-empty lines in a file."""
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            return sum(1 for line in f if line.strip())
    except Exception:
        return 0


def check_source_files() -> Tuple[bool, list[str]]:
    """Check source file sizes."""
    issues = []
    large_files = []

    for py_file in SRC.rglob("*.py"):
        if "__pycache__" in py_file.parts:
            continue

        lines = count_lines(py_file)
        rel_path = py_file.relative_to(ROOT)

        if lines > SOURCE_LIMIT:
            issues.append(
                f"❌ Source file exceeds {SOURCE_LIMIT} lines: {rel_path} ({lines} lines)"
            )
            large_files.append((rel_path, lines))
        elif lines > SOURCE_WARNING:
            issues.append(
                f"⚠️  Source file exceeds {SOURCE_WARNING} lines (warning): {rel_path} ({lines} lines)"
            )
            large_files.append((rel_path, lines))

    if not issues:
        return True, ["✅ All source files are within size limits"]

    # Sort by size and show top offenders
    large_files.sort(key=lambda x: x[1], reverse=True)
    summary = [f"\n🔴 Top {min(5, len(large_files))} largest files:"]
    for path, lines in large_files[:5]:
        summary.append(f"   {path}: {lines} lines")

    return False, issues + summary


def check_test_files() -> Tuple[bool, list[str]]:
    """Check test file sizes."""
    issues = []
    large_files = []

    for py_file in TESTS.rglob("test_*.py"):
        if "__pycache__" in py_file.parts:
            continue

        lines = count_lines(py_file)
        rel_path = py_file.relative_to(ROOT)

        if lines > TEST_LIMIT:
            issues.append(
                f"❌ Test file exceeds {TEST_LIMIT} lines: {rel_path} ({lines} lines)"
            )
            large_files.append((rel_path, lines))
        elif lines > TEST_WARNING:
            issues.append(
                f"⚠️  Test file exceeds {TEST_WARNING} lines (warning): {rel_path} ({lines} lines)"
            )
            large_files.append((rel_path, lines))

    if not issues:
        return True, ["✅ All test files are within size limits"]

    # Sort by size
    large_files.sort(key=lambda x: x[1], reverse=True)
    summary = [f"\n🔴 Top {min(5, len(large_files))} largest test files:"]
    for path, lines in large_files[:5]:
        summary.append(f"   {path}: {lines} lines")

    return False, issues + summary


def check_init_files() -> Tuple[bool, list[str]]:
    """Check __init__.py file sizes."""
    issues = []

    for init_file in SRC.rglob("__init__.py"):
        if "__pycache__" in init_file.parts:
            continue

        lines = count_lines(init_file)
        rel_path = init_file.relative_to(ROOT)

        if lines > INIT_LIMIT:
            issues.append(
                f"⚠️  __init__.py exceeds {INIT_LIMIT} lines: {rel_path} ({lines} lines)"
            )

    if not issues:
        return True, ["✅ All __init__.py files are lean"]

    return False, issues


def main():
    """Run all size checks."""
    print("\n📏 FraiseQL File Size Validation\n")
    print("=" * 60)

    checks = [
        ("Source Files", check_source_files),
        ("Test Files", check_test_files),
        ("__init__.py Files", check_init_files),
    ]

    all_pass = True
    passed_checks = 0
    total_checks = len(checks)

    for name, check_func in checks:
        passed, messages = check_func()

        print(f"\n{name}:")
        for msg in messages:
            print(f"  {msg}")

        if passed:
            passed_checks += 1
        else:
            all_pass = False

    print("\n" + "=" * 60)
    print(f"\nResults: {passed_checks}/{total_checks} checks passed")

    if all_pass:
        print("✅ All size checks passed!\n")
        return 0
    else:
        print("⚠️  Some files exceed size guidelines. See details above.\n")
        return 0  # Warning mode - don't fail


if __name__ == "__main__":
    sys.exit(main())
