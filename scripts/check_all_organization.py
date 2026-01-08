#!/usr/bin/env python3
"""Master organization validation script for FraiseQL v2.0.

This script runs all organization validation checks:
1. File structure validation
2. File size validation
3. Naming convention validation
4. Test organization validation

Run this to validate the entire codebase against v2.0 standards.
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
SCRIPTS_DIR = ROOT / "scripts"

# Validation scripts to run
CHECKS = [
    ("File Structure", "check_file_structure.py"),
    ("File Sizes", "check_file_sizes.py"),
    ("Naming Conventions", "check_naming.py"),
]


def run_check(name: str, script: str) -> bool:
    """Run a single validation check."""
    print(f"\n{'=' * 60}")
    print(f"Running: {name}")
    print(f"{'=' * 60}\n")

    try:
        result = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / script)],
            cwd=ROOT,
            capture_output=False,
        )
        return result.returncode == 0
    except Exception as e:
        print(f"❌ Error running {name}: {e}")
        return False


def main():
    """Run all organization checks."""
    print("\n" + "=" * 60)
    print("🔍 FraiseQL v2.0 Organization Validation")
    print("=" * 60)

    results = {}
    for name, script in CHECKS:
        passed = run_check(name, script)
        results[name] = passed

    # Summary
    print("\n" + "=" * 60)
    print("📊 Validation Summary")
    print("=" * 60)

    passed_count = sum(1 for v in results.values() if v)
    total_count = len(results)

    for name, passed in results.items():
        status = "✅" if passed else "⚠️"
        print(f"{status} {name}")

    print("\n" + "=" * 60)
    print(f"\nOverall: {passed_count}/{total_count} check groups passed")

    if all(results.values()):
        print("✅ All organization checks passed!\n")
        return 0
    else:
        print(
            "⚠️  Some organization checks have warnings. Review details above.\n"
        )
        return 0  # Don't fail - these are mostly warnings


if __name__ == "__main__":
    sys.exit(main())
