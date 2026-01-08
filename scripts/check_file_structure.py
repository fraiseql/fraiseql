#!/usr/bin/env python3
"""File structure validation for FraiseQL v2.0 organization standards.

This script validates that the codebase follows the documented organization structure
in docs/ORGANIZATION.md and docs/CODE_ORGANIZATION_STANDARDS.md.

Rules enforced:
1. No new test files at tests/ root level (must be in subdirectories)
2. Proper directory structure (src/, tests/, docs/, etc.)
3. No deeply nested modules (>3 levels from project root)
4. __init__.py files in Python packages
5. No unexpected directories at root level
"""

import os
import sys
from pathlib import Path
from typing import Tuple

# Root directory
ROOT = Path(__file__).parent.parent
SRC = ROOT / "src" / "fraiseql"
TESTS = ROOT / "tests"
DOCS = ROOT / "docs"

# Allowed root-level directories (excluding hidden and common)
ALLOWED_ROOT_DIRS = {
    "src",
    "tests",
    "docs",
    "scripts",
    "examples",
    "fraiseql_rs",
    ".archive",
    ".github",
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "target",  # Rust build
    "deploy",  # Deployment config
    "deployment",  # Deployment code
    "benchmarks",  # Benchmarks
    "benches",  # Benchmarks
    "performance",  # Performance testing
    "frameworks",  # Framework examples
    "templates",  # Templates
    "grafana",  # Grafana dashboards
    "testing",  # Test utilities
    "dev",  # Development utilities
    "migrations",  # Database migrations
    "sql",  # SQL files
    "reports",  # Generated reports
    "site",  # Documentation site
    "COMPLIANCE",  # Compliance docs
    "archive",  # Archive (legacy)
    "v2-plan",  # v2 planning docs
}

# Allowed test root subdirectories
ALLOWED_TEST_SUBDIRS = {
    "integration",
    "unit",
    "system",
    "chaos",
    "federation",
    "fixtures",
    "grafana",
    "regression",
    "storage",
    "performance",
    "mocks",
    "test_mutations",
    "mutations",
    "v2_e2e",
    "v2_unit",
    "v2_integration",
    "helpers",
    "examples",
    "scripts",
    "starlette",
    "config",
    "core",
    "monitoring",
    "middleware",
    "utils",
    "routing",
    "types",
    "patterns",
}


def check_root_structure() -> Tuple[bool, list[str]]:
    """Check root directory contains only allowed subdirectories."""
    errors = []

    for item in ROOT.iterdir():
        if item.name.startswith("."):
            if item.name not in {".git", ".github", ".archive", ".gitignore", ".env", ".pre-commit-config.yaml", ".pytest_cache"}:
                continue

        if not item.is_dir():
            continue

        if item.name not in ALLOWED_ROOT_DIRS and not item.name.startswith("."):
            errors.append(f"❌ Unexpected directory at root: {item.name}")

    if not errors:
        return True, ["✅ Root directory structure is valid"]
    return False, errors


def check_test_structure() -> Tuple[bool, list[str]]:
    """Check tests directory structure."""
    errors = []
    warnings = []

    if not TESTS.exists():
        return False, ["❌ tests/ directory not found"]

    # Check for test files at root level
    for item in TESTS.iterdir():
        if item.name.startswith("test_") and item.is_file() and item.suffix == ".py":
            errors.append(f"❌ Test file at root level (should be in subdirectory): {item.name}")

        if item.is_dir() and item.name not in ALLOWED_TEST_SUBDIRS and not item.name.startswith("."):
            warnings.append(f"⚠️  Unexpected test subdirectory: {item.name}")

    # Check for proper subdirectories
    for subdir in [TESTS / "integration", TESTS / "unit"]:
        if subdir.exists():
            for item in subdir.iterdir():
                if item.is_dir() and item.name.startswith("test_"):
                    errors.append(f"❌ Test subdirectory named like test file: {item}")

    if errors:
        return False, errors + warnings

    return True, ["✅ Test structure is valid"] + (warnings if warnings else [])


def check_src_structure() -> Tuple[bool, list[str]]:
    """Check src/fraiseql directory structure."""
    errors = []

    if not SRC.exists():
        return False, ["❌ src/fraiseql/ directory not found"]

    # Check nesting depth in src
    for root, dirs, files in os.walk(SRC):
        depth = len(Path(root).relative_to(SRC).parts)
        if depth > 4:  # src/fraiseql/[module]/[submodule]/[submodule]
            rel_path = Path(root).relative_to(ROOT)
            errors.append(f"❌ Module nesting too deep ({depth} levels): {rel_path}")

    if errors:
        return False, errors

    return True, ["✅ Source structure depth is valid"]


def check_python_packages() -> Tuple[bool, list[str]]:
    """Check that Python packages have __init__.py files."""
    errors = []
    warnings = []

    # Directories that should be Python packages
    for root, dirs, files in os.walk(SRC):
        # Skip __pycache__ and hidden directories
        dirs[:] = [d for d in dirs if not d.startswith(".") and d != "__pycache__"]

        # Skip test files
        if "test" in root or "tests" in root:
            continue

        # If directory has Python files, it should be a package
        py_files = [f for f in files if f.endswith(".py") and not f.startswith(".")]
        root_path = Path(root)
        if py_files and not (root_path / "__init__.py").exists():
            rel_path = root_path.relative_to(ROOT)
            errors.append(f"❌ Python package missing __init__.py: {rel_path}")

    if errors:
        return False, errors

    return True, ["✅ Python packages have __init__.py files"]


def main():
    """Run all structure checks."""
    print("\n🔍 FraiseQL File Structure Validation\n")
    print("=" * 60)

    all_pass = True
    total_checks = 0
    passed_checks = 0

    checks = [
        ("Root Directory", check_root_structure),
        ("Test Structure", check_test_structure),
        ("Source Structure", check_src_structure),
        ("Python Packages", check_python_packages),
    ]

    for name, check_func in checks:
        total_checks += 1
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
        print("✅ All structure checks passed!\n")
        return 0
    else:
        print("❌ Some structure checks failed. See details above.\n")
        return 1


if __name__ == "__main__":
    sys.exit(main())
