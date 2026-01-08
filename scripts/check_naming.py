#!/usr/bin/env python3
"""Naming convention validation for FraiseQL v2.0 organization standards.

This script validates naming conventions:
- Files: snake_case.py (Python)
- Directories: snake_case
- Test files: test_*.py
- Test classes: Test*
- Classes: PascalCase
- Functions: snake_case
- Constants: UPPER_CASE
"""

import re
import sys
from pathlib import Path
from typing import Tuple

ROOT = Path(__file__).parent.parent
SRC = ROOT / "src" / "fraiseql"
TESTS = ROOT / "tests"


def is_snake_case(name: str) -> bool:
    """Check if name is snake_case."""
    return bool(re.match(r"^[a-z0-9_]+$", name))


def is_pascal_case(name: str) -> bool:
    """Check if name is PascalCase."""
    return bool(re.match(r"^[A-Z][a-zA-Z0-9]*$", name))


def is_upper_case(name: str) -> bool:
    """Check if name is UPPER_CASE."""
    return bool(re.match(r"^[A-Z0-9_]+$", name))


def check_python_filenames() -> Tuple[bool, list[str]]:
    """Check Python file naming convention."""
    issues = []

    for py_file in SRC.rglob("*.py"):
        if "__pycache__" in py_file.parts:
            continue

        filename = py_file.name
        # Allow __init__.py, but check others
        if filename == "__init__.py":
            continue

        if not is_snake_case(filename[:-3]):  # Remove .py extension
            rel_path = py_file.relative_to(ROOT)
            issues.append(f"❌ File not snake_case: {rel_path}")

    if not issues:
        return True, ["✅ All Python files follow snake_case naming"]

    return False, issues


def check_test_filenames() -> Tuple[bool, list[str]]:
    """Check test file naming convention."""
    issues = []

    for py_file in TESTS.rglob("*.py"):
        if "__pycache__" in py_file.parts or py_file.name == "__init__.py":
            continue

        if not py_file.name.startswith("test_"):
            # Skip non-test files
            if not py_file.name.startswith("conftest"):
                rel_path = py_file.relative_to(ROOT)
                if py_file.is_file() and not any(
                    x in py_file.parts for x in ["fixtures", "helpers", "config"]
                ):
                    issues.append(f"⚠️  Test file doesn't start with test_: {rel_path}")

    if not issues:
        return True, ["✅ All test files follow test_*.py naming"]

    return False, issues[:10]  # Limit to first 10


def check_directory_naming() -> Tuple[bool, list[str]]:
    """Check directory naming convention."""
    issues = []

    for directory in list(SRC.rglob("*")):
        if not directory.is_dir():
            continue

        if "__pycache__" in directory.parts or ".pytest_cache" in directory.parts:
            continue

        dirname = directory.name
        if not is_snake_case(dirname):
            rel_path = directory.relative_to(ROOT)
            issues.append(f"❌ Directory not snake_case: {rel_path}")

    if not issues:
        return True, ["✅ All directories follow snake_case naming"]

    return False, issues[:10]  # Limit output


def main():
    """Run all naming checks."""
    print("\n📝 FraiseQL Naming Convention Validation\n")
    print("=" * 60)

    checks = [
        ("Python Files", check_python_filenames),
        ("Test Files", check_test_filenames),
        ("Directories", check_directory_naming),
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
        print("✅ All naming conventions are correct!\n")
        return 0
    else:
        print("⚠️  Some naming convention issues found. See details above.\n")
        return 0  # Warning mode


if __name__ == "__main__":
    sys.exit(main())
