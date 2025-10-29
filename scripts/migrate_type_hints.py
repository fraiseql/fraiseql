#!/usr/bin/env python3
r"""
Automated migration script for Python 3.10+ type hinting.

Converts old typing imports to modern builtins:
- Dict -> dict
- List -> list
- Tuple -> tuple
- Set -> set

Usage:
    python scripts/migrate_type_hints.py [--dry-run] [--files FILE1 FILE2 ...]

Examples:
    # Dry run on all Python files
    python scripts/migrate_type_hints.py --dry-run

    # Migrate specific files
    python scripts/migrate_type_hints.py src/fraiseql/core/db.py src/fraiseql/decorators.py

    # Migrate all Python files in src/
    find src/ -name "*.py" -exec python scripts/migrate_type_hints.py {} \;
"""

import re
import sys
from pathlib import Path


def migrate_file(file_path: Path, dry_run: bool = False) -> bool:
    """Migrate a single Python file to modern type hints."""
    try:
        content = file_path.read_text(encoding="utf-8")
        original_content = content

        # Step 1: Remove old imports from typing
        # Simple line-by-line approach
        lines = content.split("\n")
        new_lines = []

        for line in lines:
            if "from typing import" in line:
                # Remove the old generics from this line
                original_line = line
                line = re.sub(
                    r"\b(Dict|List|Tuple|Set),\s*", "", line
                )  # Remove with trailing comma
                line = re.sub(r",\s*(Dict|List|Tuple|Set)\b", "", line)  # Remove with leading comma
                line = re.sub(r"\b(Dict|List|Tuple|Set)\b", "", line)  # Remove standalone

                # Clean up extra commas and spaces
                line = re.sub(r",\s*,", ",", line)
                line = re.sub(r",\s*\)", ")", line)
                line = re.sub(r"\(\s*,", "(", line)
                line = re.sub(r"\(\s*\)", "", line)  # Remove empty parentheses

                # If the import line is now empty or just whitespace, skip it
                if re.search(r"from typing import\s*$", line):
                    continue

                # Clean up trailing commas before closing paren
                line = re.sub(r",\s*\)", ")", line)

            new_lines.append(line)

        content = "\n".join(new_lines)

        # Step 2: Replace usage patterns
        # dict[ -> dict[
        content = re.sub(r"\bDict\[", "dict[", content)
        # list[ -> list[
        content = re.sub(r"\bList\[", "list[", content)
        # tuple[ -> tuple[
        content = re.sub(r"\bTuple\[", "tuple[", content)
        # set[ -> set[
        content = re.sub(r"\bSet\[", "set[", content)

        # Step 3: Handle type aliases like dict[str, Any] -> dict[str, Any]
        # (This is already handled by the above regex, but let's be thorough)

        if content != original_content:
            if dry_run:
                print(f"Would migrate: {file_path}")
                # Show diff
                lines_orig = original_content.splitlines()
                lines_new = content.splitlines()
                for i, (orig, new) in enumerate(zip(lines_orig, lines_new)):
                    if orig != new:
                        print(f"  Line {i + 1}:")
                        print(f"    - {orig}")
                        print(f"    + {new}")
                print()
            else:
                file_path.write_text(content, encoding="utf-8")
                print(f"Migrated: {file_path}")
            return True
        else:
            if dry_run:
                print(f"No changes needed: {file_path}")
            return False

    except Exception as e:
        print(f"Error processing {file_path}: {e}", file=sys.stderr)
        return False


def find_python_files(paths: list[Path]) -> set[Path]:
    """Find all Python files in the given paths."""
    python_files = set()

    for path in paths:
        if path.is_file() and path.suffix in (".py", ".pyi"):
            python_files.add(path)
        elif path.is_dir():
            # Find all .py and .pyi files recursively
            for py_file in path.rglob("*.py"):
                python_files.add(py_file)
            for pyi_file in path.rglob("*.pyi"):
                python_files.add(pyi_file)

    return python_files


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Migrate Python files to modern type hints")
    parser.add_argument(
        "--dry-run", action="store_true", help="Show what would be changed without making changes"
    )
    parser.add_argument("files", nargs="*", help="Specific files or directories to process")

    args = parser.parse_args()

    if not args.files:
        # Default to common directories
        paths = [
            Path("src"),
            Path("examples"),
            Path("scripts"),
            Path("tests"),
            Path("benchmarks"),
            Path("frameworks"),
        ]
        # Filter to existing paths
        paths = [p for p in paths if p.exists()]
    else:
        paths = [Path(f) for f in args.files]

    python_files = find_python_files(paths)

    if not python_files:
        print("No Python files found to process")
        return

    print(f"Found {len(python_files)} Python files to process")

    migrated_count = 0
    for file_path in sorted(python_files):
        if migrate_file(file_path, args.dry_run):
            migrated_count += 1

    if args.dry_run:
        print(f"\nDry run complete. Would migrate {migrated_count} files.")
    else:
        print(f"\nMigration complete. Migrated {migrated_count} files.")


if __name__ == "__main__":
    main()
