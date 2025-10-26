#!/usr/bin/env python3
"""Find functions missing parameter type annotations."""

import ast
import os
import sys
from pathlib import Path


def has_type_annotation(arg: ast.arg) -> bool:
    """Check if an argument has a type annotation."""
    return arg.annotation is not None


def get_missing_param_types(file_path: str) -> list[tuple[int, str]]:
    """Find functions with missing parameter type annotations in a file."""
    missing = []

    try:
        with open(file_path, "r", encoding="utf-8") as f:
            content = f.read()

        tree = ast.parse(content, filename=file_path)

        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef):
                # Skip if function has *args or **kwargs without type annotations
                # But still check other parameters
                for arg in node.args.args:
                    if not has_type_annotation(arg):
                        # Skip 'self' and 'cls' parameters in methods
                        if arg.arg not in ("self", "cls"):
                            missing.append((node.lineno, arg.arg))

                # Check *args if present
                if node.args.vararg and not node.args.vararg.annotation:
                    missing.append((node.lineno, f"*{node.args.vararg.arg}"))

                # Check **kwargs if present
                if node.args.kwarg and not node.args.kwarg.annotation:
                    missing.append((node.lineno, f"**{node.args.kwarg.arg}"))

    except Exception as e:
        print(f"Error processing {file_path}: {e}", file=sys.stderr)

    return missing


def main():
    """Main entry point."""
    if len(sys.argv) != 2:
        print("Usage: python find_missing_param_types.py <directory>")
        sys.exit(1)

    directory = sys.argv[1]

    if not os.path.isdir(directory):
        print(f"Error: {directory} is not a directory")
        sys.exit(1)

    total_missing = 0
    files_with_missing = 0

    # Walk through all Python files
    for root, dirs, files in os.walk(directory):
        # Skip certain directories
        dirs[:] = [
            d for d in dirs if not d.startswith(".") and d not in ("__pycache__", "node_modules")
        ]

        for file in files:
            if file.endswith(".py"):
                file_path = os.path.join(root, file)
                missing = get_missing_param_types(file_path)

                if missing:
                    files_with_missing += 1
                    total_missing += len(missing)

                    # Convert to relative path
                    rel_path = os.path.relpath(file_path, directory)
                    print(f"{rel_path} ({len(missing)} missing):")

                    for line_no, param in missing:
                        print(f"  Line {line_no}: {param}")

                    print()

    print(
        f"Found {total_missing} parameters missing type annotations in {files_with_missing} files:"
    )


if __name__ == "__main__":
    main()
