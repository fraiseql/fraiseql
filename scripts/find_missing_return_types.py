#!/usr/bin/env python3
"""
Script to find functions missing return type annotations.

Usage:
    python scripts/find_missing_return_types.py [--path PATH] [--limit N]
"""

import ast
import sys
from pathlib import Path


class ReturnTypeChecker(ast.NodeVisitor):
    """AST visitor to find functions without return type annotations."""

    def __init__(self):
        self.missing_return_types = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        # Skip __init__ methods and private methods starting with _
        if node.name.startswith("_") or node.name == "__init__":
            return

        # Check if function has return type annotation
        if node.returns is None:
            self.missing_return_types.append((node.name, node.lineno))

        # Visit nested functions
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        # Skip __init__ methods and private methods starting with _
        if node.name.startswith("_") or node.name == "__init__":
            return

        # Check if function has return type annotation
        if node.returns is None:
            self.missing_return_types.append((node.name, node.lineno))

        # Visit nested functions
        self.generic_visit(node)


def analyze_file(file_path: Path) -> list[tuple[str, int]]:
    """Analyze a Python file for missing return type annotations."""
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            content = f.read()

        tree = ast.parse(content, filename=str(file_path))
        checker = ReturnTypeChecker()
        checker.visit(tree)

        return [(name, lineno) for name, lineno in checker.missing_return_types]

    except Exception as e:
        print(f"Error analyzing {file_path}: {e}", file=sys.stderr)
        return []


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

    parser = argparse.ArgumentParser(description="Find functions missing return type annotations")
    parser.add_argument("--path", default="src/fraiseql", help="Path to analyze")
    parser.add_argument("--limit", type=int, default=50, help="Maximum number of results to show")

    args = parser.parse_args()

    path = Path(args.path)
    if not path.exists():
        print(f"Path {path} does not exist")
        return

    python_files = find_python_files([path])

    total_missing = 0
    results = []

    for file_path in sorted(python_files):
        missing = analyze_file(file_path)
        if missing:
            results.append((file_path, missing))
            total_missing += len(missing)

    # Sort by number of missing functions (most missing first)
    results.sort(key=lambda x: len(x[1]), reverse=True)

    print(
        f"Found {total_missing} functions missing return type annotations in {len(results)} files:"
    )
    print()

    count = 0
    for file_path, missing in results:
        if count >= args.limit:
            break

        print(f"{file_path} ({len(missing)} missing):")
        for func_name, lineno in missing[:10]:  # Show first 10 per file
            print(f"  Line {lineno}: {func_name}()")
        if len(missing) > 10:
            print(f"  ... and {len(missing) - 10} more")
        print()
        count += 1

    if len(results) > args.limit:
        print(f"... and {len(results) - args.limit} more files")


if __name__ == "__main__":
    main()
