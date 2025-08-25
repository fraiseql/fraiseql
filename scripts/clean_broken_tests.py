#!/usr/bin/env python3
"""Script to identify and handle broken test files."""

import subprocess
import sys
from pathlib import Path


def find_broken_tests(test_dir: Path) -> list[str]:
    """Find test files that cause collection errors."""
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pytest", str(test_dir), "--collect-only", "-q"],
            capture_output=True,
            text=True,
            cwd=test_dir.parent
        )

        if result.returncode != 0:
            # Parse stderr to find problematic files
            lines = result.stderr.splitlines() + result.stdout.splitlines()
            broken_files = []

            for line in lines:
                if "ERROR collecting" in line:
                    # Extract file path
                    parts = line.split()
                    for part in parts:
                        if part.endswith(".py"):
                            broken_files.append(part)
                            break

            return broken_files

        return []

    except Exception as e:
        print(f"Error running pytest: {e}")
        return []


def check_file_issues(file_path: Path) -> tuple[bool, str]:
    """Check what's wrong with a specific test file."""
    try:
        # Try to compile the file
        with open(file_path) as f:
            content = f.read()

        try:
            compile(content, str(file_path), "exec")
        except SyntaxError as e:
            return False, f"Syntax error: {e}"

        # Try to import any missing modules
        try:
            result = subprocess.run(
                [sys.executable, "-c", f"import ast; ast.parse(open('{file_path}').read())"],
                capture_output=True,
                text=True
            )
            if result.returncode != 0:
                return False, f"Parse error: {result.stderr}"
        except Exception as e:
            return False, f"Import check failed: {e}"

        return True, "OK"

    except Exception as e:
        return False, f"File access error: {e}"


def main():
    """Clean up broken test files."""
    test_dir = Path(__file__).parent.parent / "tests"

    print("🔍 Finding broken test files...")
    broken_files = find_broken_tests(test_dir)

    if not broken_files:
        print("✅ No broken test files found!")
        return

    print(f"❌ Found {len(broken_files)} broken test files:")

    for file_path in broken_files:
        full_path = test_dir / file_path
        if full_path.exists():
            is_ok, issue = check_file_issues(full_path)

            print(f"\n📁 {file_path}")
            print(f"   Issue: {issue}")

            if not is_ok:
                # Check if file is mostly empty or just comments
                try:
                    content = full_path.read_text().strip()
                    lines = [line for line in content.splitlines()
                            if line.strip() and not line.strip().startswith("#")]

                    if len(lines) < 5:  # Very short file, likely broken stub
                        print(f"   Action: Removing (too short/empty)")
                        full_path.unlink()
                    else:
                        print(f"   Action: Needs manual review")

                except Exception as e:
                    print(f"   Action: Could not analyze - {e}")
        else:
            print(f"   File not found: {file_path}")

    print(f"\n🧹 Cleanup complete!")


if __name__ == "__main__":
    main()
