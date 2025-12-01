#!/usr/bin/env python3
"""Script to fix async test functions that need @pytest.mark.asyncio decorators.

This script scans Python test files and adds @pytest.mark.asyncio decorators
to async test functions that don't already have them.
"""

import os
import re
import sys
from pathlib import Path
from typing import List, Tuple


def find_test_files(directory: str = "tests") -> List[Path]:
    """Find all Python test files in the given directory."""
    test_files = []
    for root, _dirs, files in os.walk(directory):
        for file in files:
            if file.endswith(".py") and ("test_" in file or "_test.py" in file):
                test_files.append(Path(root) / file)
    return test_files


def read_file_content(file_path: Path) -> str:
    """Read the content of a file."""
    with file_path.open(encoding="utf-8") as f:
        return f.read()


def write_file_content(file_path: Path, content: str) -> None:
    """Write content to a file."""
    with file_path.open("w", encoding="utf-8") as f:
        f.write(content)


def find_async_test_functions(content: str) -> List[Tuple[int, str, str]]:
    """Find async test functions that need @pytest.mark.asyncio.

    Returns list of tuples: (line_number, function_name, full_match)
    """
    # Pattern to match async def test_ functions
    pattern = r"^(\s*)async def (test_\w+)\("

    functions_to_fix = []
    lines = content.split("\n")

    for i, line in enumerate(lines):
        match = re.match(pattern, line)
        if match:
            _indent, func_name = match.groups()

            # Check if @pytest.mark.asyncio is already present in the previous lines
            has_decorator = False
            for j in range(max(0, i - 5), i):  # Check up to 5 lines before
                if "@pytest.mark.asyncio" in lines[j]:
                    has_decorator = True
                    break

            if not has_decorator:
                functions_to_fix.append((i, func_name, line))

    return functions_to_fix


def add_asyncio_decorator(content: str, line_number: int, indent: str) -> str:
    """Add @pytest.mark.asyncio decorator before the function."""
    lines = content.split("\n")

    # Insert the decorator before the function definition
    decorator_line = f"{indent}@pytest.mark.asyncio"
    lines.insert(line_number, decorator_line)

    return "\n".join(lines)


def fix_file(file_path: Path) -> int:
    """Fix a single test file by adding missing @pytest.mark.asyncio decorators."""
    content = read_file_content(file_path)
    original_content = content

    functions_to_fix = find_async_test_functions(content)

    if not functions_to_fix:
        return 0

    # Process in reverse order to maintain line numbers
    for line_num, _func_name, line_match in reversed(functions_to_fix):
        indent_match = re.match(r"^(\s*)", line_match)
        indent = indent_match.group(1) if indent_match else ""
        content = add_asyncio_decorator(content, line_num, indent)

    if content != original_content:
        write_file_content(file_path, content)
        return len(functions_to_fix)
    return 0


def main() -> None:
    """Main function to run the script."""
    if len(sys.argv) > 1:
        test_dir = sys.argv[1]
    else:
        test_dir = "tests"

    if not Path(test_dir).exists():
        sys.exit(1)

    test_files = find_test_files(test_dir)

    total_fixes = 0
    files_fixed = 0

    for test_file in test_files:
        fixes = fix_file(test_file)
        total_fixes += fixes
        if fixes > 0:
            files_fixed += 1

    # Return non-zero exit code if fixes were made
    if total_fixes > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
